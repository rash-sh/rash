/// ANCHOR: module
/// # fetch
///
/// This module copies a file from a source path to a local destination path.
/// Useful for retrieving files such as configurations, logs, and backups.
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
///     src: /etc/app/config.yaml
///     dest: /backup/config.yaml
///     flat: true
///     validate_checksum: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;
use sha2::{Digest as Sha2Digest, Sha256};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The file to fetch from the source path.
    src: String,
    /// The destination path where the file should be saved.
    /// If `flat` is false and dest ends with `/`, the file is saved preserving
    /// the source directory structure under dest.
    dest: String,
    /// If true, stores the file directly at dest without hostname-based subdirectory structure.
    /// **[default: `false`]**
    #[serde(default)]
    flat: bool,
    /// Whether to validate that the source and destination file checksums match after copy.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    validate_checksum: bool,
    /// If true, the task will fail when the source file is missing.
    /// If false, a warning is printed and the task succeeds with changed=false.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    fail_on_missing: bool,
}

fn default_true() -> bool {
    true
}

fn calculate_checksum(path: &Path) -> Result<String> {
    let contents = fs::read(path).map_err(|e| {
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

#[derive(Debug, Serialize)]
struct FetchResult {
    dest: String,
    src: String,
    checksum: String,
    size: u64,
    changed: bool,
}

fn fetch_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let src_path = Path::new(&params.src);

    if !src_path.exists() {
        if !params.fail_on_missing {
            debug!("Source file '{}' not found, skipping", params.src);
            return Ok(ModuleResult::new(
                false,
                None,
                Some(params.dest.clone()),
            ));
        }
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Source file '{}' not found", params.src),
        ));
    }

    if !src_path.is_file() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Source '{}' is not a regular file", params.src),
        ));
    }

    let dest_path = if params.flat {
        Path::new(&params.dest).to_path_buf()
    } else if params.dest.ends_with('/') {
        let src_relative = params.src.trim_start_matches('/');
        Path::new(&params.dest).join(src_relative)
    } else {
        Path::new(&params.dest).to_path_buf()
    };

    let dest_exists = dest_path.exists();

    if dest_exists {
        let src_checksum = calculate_checksum(src_path)?;
        let dest_checksum = calculate_checksum(&dest_path)?;
        if src_checksum == dest_checksum {
            let src_meta = fs::metadata(src_path)?;
            let extra = serde_norway::to_value(FetchResult {
                dest: dest_path.to_str().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path")
                })?
                .to_owned(),
                src: params.src.clone(),
                checksum: src_checksum,
                size: src_meta.len(),
                changed: false,
            })
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            return Ok(ModuleResult::new(false, Some(extra), None));
        }
    }

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(src_path, &dest_path)?;

    if params.validate_checksum {
        let src_checksum = calculate_checksum(src_path)?;
        let dest_checksum = calculate_checksum(&dest_path)?;
        if src_checksum != dest_checksum {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Checksum mismatch after copy: src={} dest={}",
                    src_checksum, dest_checksum
                ),
            ));
        }
    }

    let src_meta = fs::metadata(src_path)?;
    let checksum = calculate_checksum(&dest_path)?;

    let extra = serde_norway::to_value(FetchResult {
        dest: dest_path.to_str().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "Invalid UTF-8 in destination path")
        })?
        .to_owned(),
        src: params.src.clone(),
        checksum,
        size: src_meta.len(),
        changed: true,
    })
    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    Ok(ModuleResult::new(true, Some(extra), None))
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
        Ok((fetch_file(parse_params(optional_params)?, check_mode)?, None))
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
            src: /etc/hosts
            dest: /backup/hosts
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/hosts".to_owned(),
                dest: "/backup/hosts".to_owned(),
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
            src: /etc/hosts
            dest: /backup/hosts
            flat: true
            validate_checksum: false
            fail_on_missing: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "/etc/hosts".to_owned(),
                dest: "/backup/hosts".to_owned(),
                flat: true,
                validate_checksum: false,
                fail_on_missing: false,
            }
        );
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: /etc/hosts
            dest: /backup/hosts
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_fetch_file_basic() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        write!(file, "hello world").unwrap();

        let dest_file = dest_dir.path().join("fetched.txt");

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

        let content = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "hello world");

        let extra = result.get_extra().unwrap();
        let result_map: serde_norway::Mapping = extra.as_mapping().unwrap().clone();
        assert!(result_map
            .get(&YamlValue::String("checksum".to_owned()))
            .unwrap()
            .as_str()
            .unwrap()
            .len()
            == 64);
    }

    #[test]
    fn test_fetch_file_no_change() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("test.txt");
        let mut file = File::create(&file_path).unwrap();
        write!(file, "hello world").unwrap();

        let dest_path = dir.path().join("copy.txt");
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

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        write!(file, "hello world").unwrap();

        let dest_file = dest_dir.path().join("fetched.txt");

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
    fn test_fetch_file_missing_source_fail() {
        let result = fetch_file(
            Params {
                src: "/nonexistent/file.txt".to_owned(),
                dest: "/tmp/dest.txt".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_fetch_file_missing_source_no_fail() {
        let result = fetch_file(
            Params {
                src: "/nonexistent/file.txt".to_owned(),
                dest: "/tmp/dest.txt".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: false,
            },
            false,
        )
        .unwrap();

        assert!(!result.get_changed());
    }

    #[test]
    fn test_fetch_file_flat_false_with_directory_dest() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        write!(file, "hello").unwrap();

        let dest_dir_path = dest_dir.path().join("output/");

        let result = fetch_file(
            Params {
                src: src_file.to_str().unwrap().to_owned(),
                dest: dest_dir_path.to_str().unwrap().to_owned(),
                flat: false,
                validate_checksum: false,
                fail_on_missing: true,
            },
            false,
        )
        .unwrap();

        assert!(result.get_changed());

        let expected_dest = dest_dir.path().join("output").join(&src_file);
        assert!(expected_dest.exists());
        let content = fs::read_to_string(&expected_dest).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_fetch_file_creates_dest_dirs() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("test.txt");
        let mut file = File::create(&src_file).unwrap();
        write!(file, "hello").unwrap();

        let dest_file = dest_dir.path().join("a/b/c/test.txt");

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
        let content = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_fetch_file_source_is_directory() {
        let dir = tempdir().unwrap();

        let result = fetch_file(
            Params {
                src: dir.path().to_str().unwrap().to_owned(),
                dest: "/tmp/dest.txt".to_owned(),
                flat: true,
                validate_checksum: true,
                fail_on_missing: true,
            },
            false,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_fetch_file_binary() {
        let src_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();

        let src_file = src_dir.path().join("binary.dat");
        let binary_data: &[u8] = &[0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD];
        fs::write(&src_file, binary_data).unwrap();

        let dest_file = dest_dir.path().join("binary.dat");

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

        let content = fs::read(&dest_file).unwrap();
        assert_eq!(content, binary_data);
    }

    #[test]
    fn test_fetch_file_overwrites_existing() {
        let dir = tempdir().unwrap();

        let src_file = dir.path().join("src.txt");
        let mut file = File::create(&src_file).unwrap();
        write!(file, "new content").unwrap();

        let dest_file = dir.path().join("dest.txt");
        let mut file = File::create(&dest_file).unwrap();
        write!(file, "old content").unwrap();

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
        let content = fs::read_to_string(&dest_file).unwrap();
        assert_eq!(content, "new content");
    }
}
