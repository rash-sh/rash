/// ANCHOR: module
/// # fetch
///
/// This module copies a file from a source path on the local host to a destination path.
/// It is useful for configuration management, backup operations, and auditing.
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
/// - name: Fetch configuration file
///   fetch:
///     src: /etc/app/config.yaml
///     dest: /backup/config.yaml
///     flat: true
///
/// - name: Fetch logs for analysis
///   fetch:
///     src: /var/log/app.log
///     dest: /backup/logs/
///     fail_on_missing: false
///
/// - name: Fetch with checksum validation
///   fetch:
///     src: /etc/important.conf
///     dest: /backup/important.conf
///     flat: true
///     validate_checksum: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff_files;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Read;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use sha2::{Digest as Sha2Digest, Sha256};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The file to fetch. Must be an absolute path.
    pub src: String,
    /// The directory or file path where the file should be fetched to.
    pub dest: String,
    /// Allows you to override the default behavior of appending hostname/path/to/file
    /// to the destination. If dest ends with '/', it will use the basename of the source file.
    /// **[default: `false`]**
    #[serde(default)]
    pub flat: bool,
    /// Verify that the source and destination checksums match after the copy.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub validate_checksum: bool,
    /// When set to true, the task will fail if the source file is missing.
    /// When set to false, the task will silently skip if the source file is missing.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub fail_on_missing: bool,
}

fn default_true() -> bool {
    true
}

fn calculate_checksum(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to open file for checksum: {e}"),
        )
    })?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read file for checksum: {e}"),
        )
    })?;
    let mut hasher = Sha256::new();
    Sha2Digest::update(&mut hasher, &contents);
    let hash = hasher.finalize();
    Ok(hash.iter().map(|b| format!("{:02x}", b)).collect())
}

fn resolve_dest_path(src: &str, dest: &str, flat: bool) -> Result<String> {
    let src_path = Path::new(src);
    let dest_path = Path::new(dest);

    if flat {
        if dest.ends_with('/') {
            let src_filename = src_path
                .file_name()
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Cannot extract filename from src: {}", src),
                    )
                })?
                .to_str()
                .ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in source filename")
                })?;
            let resolved = dest_path.join(src_filename);
            return Ok(resolved
                .to_str()
                .ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path")
                })?
                .to_owned());
        }
        return Ok(dest.to_owned());
    }

    if dest.ends_with('/') {
        let src_filename = src_path
            .file_name()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Cannot extract filename from src: {}", src),
                )
            })?
            .to_str()
            .ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in source filename")
            })?;
        let resolved = dest_path.join(src_filename);
        return Ok(resolved
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path"))?
            .to_owned());
    }

    Ok(dest.to_owned())
}

fn read_file_contents(path: &Path) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    Ok(contents)
}

pub fn fetch_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let src_path = Path::new(&params.src);

    if !src_path.exists() {
        if !params.fail_on_missing {
            return Ok(ModuleResult {
                changed: false,
                output: Some(format!("{} (missing, skipped)", params.src)),
                extra: None,
            });
        }
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source file not found: {}", params.src),
        ));
    }

    if !src_path.is_file() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source is not a regular file: {}", params.src),
        ));
    }

    let resolved_dest = resolve_dest_path(&params.src, &params.dest, params.flat)?;
    let dest_path = Path::new(&resolved_dest);

    let src_contents = read_file_contents(src_path)?;

    let dest_contents = if dest_path.exists() && dest_path.is_file() {
        Some(read_file_contents(dest_path)?)
    } else {
        None
    };

    if dest_contents.as_ref() == Some(&src_contents) {
        if params.validate_checksum {
            let src_checksum = calculate_checksum(src_path)?;
            let dest_checksum = calculate_checksum(dest_path)?;
            if src_checksum != dest_checksum {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Checksum mismatch after copy: source={} dest={}",
                        src_checksum, dest_checksum
                    ),
                ));
            }
        }
        return Ok(ModuleResult {
            changed: false,
            output: Some(resolved_dest),
            extra: None,
        });
    }

    let src_display = String::from_utf8_lossy(&src_contents);
    let dest_display = dest_contents
        .as_ref()
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .unwrap_or_else(|| "(absent)".to_owned());
    diff_files(&dest_display, &*src_display);

    if !check_mode {
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src_path, dest_path).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to copy '{}' to '{}': {}",
                    params.src, resolved_dest, e
                ),
            )
        })?;

        if params.validate_checksum {
            let src_checksum = calculate_checksum(src_path)?;
            let dest_checksum = calculate_checksum(dest_path)?;
            if src_checksum != dest_checksum {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Checksum mismatch after copy: source={} dest={}",
                        src_checksum, dest_checksum
                    ),
                ));
            }
        }
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(resolved_dest),
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

    use std::fs::{self, File};
    use std::io::Write;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/app/config.yaml
            dest: /backup/config.yaml
            flat: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/app/config.yaml".to_owned(),
                dest: "/backup/config.yaml".to_owned(),
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
            src: /var/log/app.log
            dest: /backup/logs/
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/var/log/app.log".to_owned(),
                dest: "/backup/logs/".to_owned(),
                flat: false,
                validate_checksum: true,
                fail_on_missing: true,
            }
        );
    }

    #[test]
    fn test_parse_params_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /var/log/app.log
            dest: /backup/logs/
            flat: false
            validate_checksum: false
            fail_on_missing: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/var/log/app.log".to_owned(),
                dest: "/backup/logs/".to_owned(),
                flat: false,
                validate_checksum: false,
                fail_on_missing: false,
            }
        );
    }

    #[test]
    fn test_fetch_file_flat() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("config.yaml");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "key: value").unwrap();

        let dest_file = dest_dir.path().join("config.yaml");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_file.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        let dest_contents = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(dest_contents, "key: value\n");
    }

    #[test]
    fn test_fetch_file_flat_dest_directory() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("config.yaml");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "key: value").unwrap();

        let dest_subdir = dest_dir.path().join("backup/");
        fs::create_dir_all(&dest_subdir).unwrap();

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_subdir.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        let expected_dest = dest_subdir.join("config.yaml");
        assert!(expected_dest.exists());
        let dest_contents = fs::read_to_string(&expected_dest).unwrap();
        assert_eq!(dest_contents, "key: value\n");
    }

    #[test]
    fn test_fetch_file_no_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("data.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "unchanged").unwrap();

        let dest_path = dir.path().join("dest.txt");
        fs::copy(&file_path, &dest_path).unwrap();

        let result = fetch_file(
            Params {
                src: file_path.to_str().unwrap().to_owned(),
                dest: dest_path.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(!result.get_changed());
    }

    #[test]
    fn test_fetch_file_check_mode() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("config.yaml");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "key: value").unwrap();

        let dest_file = dest_dir.path().join("config.yaml");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_file.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            true,
        )
        .unwrap();

        assert!(result.get_changed());
        assert!(!dest_file.exists());
    }

    #[test]
    fn test_fetch_file_missing_fail() {
        let result = fetch_file(
            Params {
                src: "/nonexistent/file.txt".to_owned(),
                dest: "/tmp/backup.txt".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_file_missing_skip() {
        let result = fetch_file(
            Params {
                src: "/nonexistent/file.txt".to_owned(),
                dest: "/tmp/backup.txt".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: false,
            },
            false,
        )
        .unwrap();

        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("missing, skipped"));
    }

    #[test]
    fn test_fetch_file_dest_directory_not_flat() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("app.log");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "log entry").unwrap();

        let dest_subdir = dest_dir.path().join("backup/");
        fs::create_dir_all(&dest_subdir).unwrap();

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_subdir.to_str().unwrap().to_owned(),
                flat: false,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        let expected_dest = dest_subdir.join("app.log");
        assert!(expected_dest.exists());
    }

    #[test]
    fn test_fetch_file_creates_parent_dirs() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("data.txt");
        let mut file = File::create(&src_file).unwrap();
        writeln!(file, "important").unwrap();

        let dest_file = dest_dir.path().join("a/b/c/data.txt");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_file.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        assert!(dest_file.exists());
        let contents = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(contents, "important\n");
    }

    #[test]
    fn test_fetch_file_validate_checksum_false() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("data.bin");
        let mut file = File::create(&src_file).unwrap();
        file.write_all(&[0x00, 0x01, 0x02, 0xFF]).unwrap();

        let dest_file = dest_dir.path().join("data.bin");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_file.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        let contents = fs::read(&dest_file).unwrap();
        assert_eq!(contents, vec![0x00, 0x01, 0x02, 0xFF]);
    }

    #[test]
    fn test_fetch_file_binary() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let binary_data: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52,
        ];
        let src_file = src_dir.path().join("image.png");
        let mut file = File::create(&src_file).unwrap();
        file.write_all(binary_data).unwrap();

        let dest_file = dest_dir.path().join("image.png");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_file.to_str().unwrap().to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());
        let contents = fs::read(&dest_file).unwrap();
        assert_eq!(contents, binary_data);
    }

    #[test]
    fn test_fetch_file_src_is_directory() {
        let dir = tempdir().unwrap();

        let result = fetch_file(
            Params {
                src: dir.path().to_str().unwrap().to_owned(),
                dest: "/tmp/backup".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_dest_path_flat_file() {
        let result =
            resolve_dest_path("/etc/app/config.yaml", "/backup/config.yaml", true).unwrap();
        assert_eq!(result, "/backup/config.yaml");
    }

    #[test]
    fn test_resolve_dest_path_flat_directory() {
        let result = resolve_dest_path("/etc/app/config.yaml", "/backup/", true).unwrap();
        assert_eq!(result, "/backup/config.yaml");
    }

    #[test]
    fn test_resolve_dest_path_not_flat_directory() {
        let result = resolve_dest_path("/etc/app/config.yaml", "/backup/", false).unwrap();
        assert_eq!(result, "/backup/config.yaml");
    }

    #[test]
    fn test_resolve_dest_path_not_flat_file() {
        let result =
            resolve_dest_path("/etc/app/config.yaml", "/backup/my-config.yaml", false).unwrap();
        assert_eq!(result, "/backup/my-config.yaml");
    }

    #[test]
    fn test_calculate_checksum() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello").unwrap();

        let checksum = calculate_checksum(&file_path).unwrap();
        assert_eq!(checksum.len(), 64);
    }
}
