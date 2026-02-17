/// ANCHOR: module
/// # fetch
///
/// Fetch files from remote systems to local.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Fetch application logs
///   fetch:
///     src: /var/log/app.log
///     dest: ./logs/
///
/// - name: Fetch config file with flat structure
///   fetch:
///     src: /etc/app/config.yaml
///     dest: ./configs/config.yaml
///     flat: true
///
/// - name: Fetch file with checksum validation
///   fetch:
///     src: /data/backup.tar.gz
///     dest: ./backups/
///     validate_checksum: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff_files;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, create_dir_all, metadata, set_permissions};
use std::io::prelude::*;
use std::io::{BufReader, BufWriter, Result as IoResult};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use sha2::{Digest, Sha256};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The file on the remote system to fetch.
    pub src: String,
    /// A directory to save the file into.
    pub dest: String,
    /// If set to true, the file will be fetched directly to the dest path without
    /// adding the hostname and source path directory structure.
    /// [default: false]
    #[serde(default)]
    pub flat: bool,
    /// Verify that the source and destination checksums match after the transfer.
    /// [default: false]
    #[serde(default)]
    pub validate_checksum: bool,
    /// When set to true, the task will fail if the source file is missing.
    /// When set to false, the task will succeed even if the source file is missing.
    /// [default: true]
    #[serde(default = "default_fail_on_missing")]
    pub fail_on_missing: bool,
}

fn default_fail_on_missing() -> bool {
    true
}

fn calculate_checksum(path: &Path) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn files_are_identical(src: &Path, dest: &Path) -> Result<bool> {
    let src_metadata = metadata(src)?;
    let dest_metadata = metadata(dest)?;

    if src_metadata.len() != dest_metadata.len() {
        return Ok(false);
    }

    let src_file = File::open(src)?;
    let dest_file = File::open(dest)?;

    let mut src_reader = BufReader::new(src_file);
    let mut dest_reader = BufReader::new(dest_file);

    let mut src_buffer = [0u8; 8192];
    let mut dest_buffer = [0u8; 8192];

    loop {
        let src_read = src_reader.read(&mut src_buffer)?;
        let dest_read = dest_reader.read(&mut dest_buffer)?;

        if src_read == 0 && dest_read == 0 {
            return Ok(true);
        }

        if src_read != dest_read || src_buffer[..src_read] != dest_buffer[..dest_read] {
            return Ok(false);
        }
    }
}

fn copy_file_with_permissions(src: &Path, dest: &Path) -> Result<()> {
    let src_file = File::open(src)?;
    let src_metadata = src_file.metadata()?;
    let src_permissions = src_metadata.permissions();

    let dest_file = File::create(dest)?;
    {
        let mut reader = BufReader::new(src_file);
        let mut writer = BufWriter::new(&dest_file);

        let mut buffer = [0u8; 8192];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buffer[..n])?;
        }
        writer.flush()?;
    }

    let mut dest_permissions = dest_file.metadata()?.permissions();
    dest_permissions.set_mode(src_permissions.mode() & 0o7777);
    set_permissions(dest, dest_permissions)?;

    Ok(())
}

pub fn fetch_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let src_path = Path::new(&params.src);
    let dest_path = Path::new(&params.dest);

    if !src_path.exists() {
        if params.fail_on_missing {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Source file {} does not exist", params.src),
            ));
        }
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Source {} not found, skipping", params.src)),
            extra: None,
        });
    }

    if src_path.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Source {} is a directory, fetch only supports files",
                params.src
            ),
        ));
    }

    let final_dest = if params.flat {
        if dest_path.is_dir() {
            let filename = src_path.file_name().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Cannot extract filename from src: {}", params.src),
                )
            })?;
            dest_path.join(filename)
        } else {
            dest_path.to_path_buf()
        }
    } else {
        let dest_base = dest_path.to_path_buf();

        let src_absolute = src_path.canonicalize()?;
        let src_parent = src_absolute.parent().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Cannot get parent directory of {}", params.src),
            )
        })?;

        let src_parent_str = src_parent.to_string_lossy();
        let relative_path = if src_parent_str == "/" {
            src_absolute
                .file_name()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Cannot extract filename from src: {}", params.src),
                    )
                })?
                .to_string_lossy()
                .to_string()
        } else {
            src_absolute
                .strip_prefix(src_parent_str.trim_start_matches('/'))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| {
                    src_absolute
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                })
        };

        dest_base.join(relative_path)
    };

    let dest_str = final_dest.to_string_lossy().to_string();

    if final_dest.exists() {
        let identical = files_are_identical(src_path, &final_dest)?;

        if identical {
            if params.validate_checksum {
                let src_checksum = calculate_checksum(src_path)?;
                let dest_checksum = calculate_checksum(&final_dest)?;
                if src_checksum != dest_checksum {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Checksum mismatch after verification: source {} != destination {}",
                            src_checksum, dest_checksum
                        ),
                    ));
                }
            }
            return Ok(ModuleResult {
                changed: false,
                output: Some(dest_str),
                extra: None,
            });
        }

        let src_content = std::fs::read(src_path)?;
        let dest_content = std::fs::read(&final_dest)?;
        diff_files(
            String::from_utf8_lossy(&dest_content),
            String::from_utf8_lossy(&src_content),
        );

        if !check_mode {
            copy_file_with_permissions(src_path, &final_dest)?;

            if params.validate_checksum {
                let src_checksum = calculate_checksum(src_path)?;
                let dest_checksum = calculate_checksum(&final_dest)?;
                if src_checksum != dest_checksum {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Checksum mismatch after transfer: source {} != destination {}",
                            src_checksum, dest_checksum
                        ),
                    ));
                }
            }
        }

        return Ok(ModuleResult {
            changed: true,
            output: Some(dest_str),
            extra: None,
        });
    }

    if !check_mode {
        if let Some(parent) = final_dest.parent()
            && !parent.exists()
        {
            create_dir_all(parent)?;
        }

        diff_files(
            "(absent)",
            String::from_utf8_lossy(&std::fs::read(src_path)?),
        );

        copy_file_with_permissions(src_path, &final_dest)?;

        if params.validate_checksum {
            let src_checksum = calculate_checksum(src_path)?;
            let dest_checksum = calculate_checksum(&final_dest)?;
            if src_checksum != dest_checksum {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Checksum mismatch after transfer: source {} != destination {}",
                        src_checksum, dest_checksum
                    ),
                ));
            }
        }
    } else {
        diff_files(
            "(absent)",
            String::from_utf8_lossy(&std::fs::read(src_path)?),
        );
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(dest_str),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Fetch;

impl Module for Fetch {
    fn get_name(&self) -> &str {
        "fetch"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            fetch_file(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::{File, create_dir_all, set_permissions};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/var/log/app.log"
            dest: "./logs/"
            flat: true
            validate_checksum: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/var/log/app.log".to_owned(),
                dest: "./logs/".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            }
        );
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/var/log/app.log"
            dest: "./logs/"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/var/log/app.log".to_owned(),
                dest: "./logs/".to_owned(),
                flat: false,
                validate_checksum: false,
                fail_on_missing: true,
            }
        );
    }

    #[test]
    fn test_parse_params_fail_on_missing_false() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: "/var/log/app.log"
            dest: "./logs/"
            fail_on_missing: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/var/log/app.log".to_owned(),
                dest: "./logs/".to_owned(),
                flat: false,
                validate_checksum: false,
                fail_on_missing: false,
            }
        );
    }

    #[test]
    fn test_fetch_file_flat_mode() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("retrieved.log");
        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(dest.exists());
        let contents = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(contents, "test content\n");
        assert!(output.changed);
    }

    #[test]
    fn test_fetch_file_flat_mode_to_directory() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("subdir/");
        create_dir_all(&dest).unwrap();

        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        let expected_dest = dest.join("test.log");
        assert!(expected_dest.exists());
        let contents = std::fs::read_to_string(&expected_dest).unwrap();
        assert_eq!(contents, "test content\n");
        assert!(output.changed);
    }

    #[test]
    fn test_fetch_file_no_change() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("test.log");
        let mut dest_file = File::create(&dest).unwrap();
        writeln!(dest_file, "test content").unwrap();

        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(!output.changed);
    }

    #[test]
    fn test_fetch_file_with_checksum_validation() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content for checksum").unwrap();

        let dest = dest_dir.path().join("test.log");

        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(dest.exists());
        assert!(output.changed);

        let src_checksum = calculate_checksum(&src_file).unwrap();
        let dest_checksum = calculate_checksum(&dest).unwrap();
        assert_eq!(src_checksum, dest_checksum);
    }

    #[test]
    fn test_fetch_file_missing_source_fail() {
        let dest_dir = tempdir().unwrap();

        let result = fetch_file(
            Params {
                src: "/nonexistent/file.log".to_owned(),
                dest: dest_dir.path().to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_fetch_file_missing_source_no_fail() {
        let dest_dir = tempdir().unwrap();

        let output = fetch_file(
            Params {
                src: "/nonexistent/file.log".to_owned(),
                dest: dest_dir.path().to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: false,
            },
            false,
        )
        .unwrap();

        assert!(!output.changed);
    }

    #[test]
    fn test_fetch_file_directory_source_error() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let result = fetch_file(
            Params {
                src: src_dir.path().to_str().unwrap().to_owned(),
                dest: dest_dir.path().to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_file_check_mode() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("test.log");

        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            true,
        )
        .unwrap();

        assert!(output.changed);
        assert!(!dest.exists());
    }

    #[test]
    fn test_fetch_file_preserves_permissions() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o600);
        set_permissions(&src_file, permissions).unwrap();

        let dest = dest_dir.path().join("test.log");

        fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        let dest_metadata = metadata(&dest).unwrap();
        let dest_permissions = dest_metadata.permissions();
        assert_eq!(dest_permissions.mode() & 0o7777, 0o600);
    }

    #[test]
    fn test_fetch_file_creates_parent_directories() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "test content").unwrap();

        let dest = dest_dir.path().join("a/b/c/test.log");

        let output = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(dest.exists());
        assert!(output.changed);
    }

    #[test]
    fn test_calculate_checksum() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello world").unwrap();

        let checksum = calculate_checksum(&file_path).unwrap();

        assert_eq!(
            checksum,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
