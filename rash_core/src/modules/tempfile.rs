/// ANCHOR: module
/// # tempfile
///
/// Create temporary files and directories.
///
/// This module creates temporary files or directories with optional prefix,
/// suffix, and permission settings. The created path is returned in the
/// output and can be registered for use in subsequent tasks.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Create a temporary directory
///   tempfile:
///     state: directory
///     prefix: myapp_
///   register: temp_dir
///
/// - name: Create a temporary file
///   tempfile:
///     state: file
///     suffix: .txt
///   register: temp_file
///
/// - name: Create temp file with custom mode
///   tempfile:
///     state: file
///     path: /var/tmp
///     mode: "0600"
///   register: secure_temp
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{create_dir_all, set_permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};
use tempfile::{Builder, NamedTempFile, TempDir};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The type of temporary object to create.
    state: State,
    /// The parent directory where the temporary object will be created.
    path: Option<String>,
    /// Prefix for the temporary name.
    prefix: Option<String>,
    /// Suffix for the temporary name (only valid for files).
    suffix: Option<String>,
    /// Permissions of the temporary file or directory.
    mode: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    File,
    Directory,
}

fn create_temp_directory(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let prefix = params.prefix.as_deref().unwrap_or("");

    if check_mode {
        let base_path = match &params.path {
            Some(p) => p.clone(),
            None => std::env::temp_dir().to_string_lossy().to_string(),
        };
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("{}/{}tmp", base_path, prefix)),
            extra: None,
        });
    }

    let temp_dir: TempDir = match params.path {
        Some(path) => {
            let parent = PathBuf::from(&path);
            if !parent.exists() {
                create_dir_all(&parent)?;
            }
            Builder::new()
                .prefix(prefix)
                .tempdir_in(&parent)
                .map_err(|e| Error::new(ErrorKind::IOError, e))?
        }
        None => Builder::new()
            .prefix(prefix)
            .tempdir()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?,
    };

    let temp_path = temp_dir.path().to_path_buf();

    if let Some(mode) = &params.mode {
        let octal_mode = parse_octal(mode)?;
        let mut permissions = temp_path.metadata()?.permissions();
        permissions.set_mode(octal_mode);
        set_permissions(&temp_path, permissions)?;
    }

    let path_str = temp_path.to_string_lossy().to_string();

    let _ = temp_dir.keep();

    Ok(ModuleResult {
        changed: true,
        output: Some(path_str),
        extra: None,
    })
}

fn create_temp_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let prefix = params.prefix.as_deref().unwrap_or("");
    let suffix = params.suffix.as_deref().unwrap_or("");

    if check_mode {
        let base_path = match &params.path {
            Some(p) => p.clone(),
            None => std::env::temp_dir().to_string_lossy().to_string(),
        };
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("{}/{}{}tmp", base_path, prefix, suffix)),
            extra: None,
        });
    }

    let temp_file: NamedTempFile = match params.path {
        Some(path) => {
            let parent = PathBuf::from(&path);
            if !parent.exists() {
                create_dir_all(&parent)?;
            }
            Builder::new()
                .prefix(prefix)
                .suffix(suffix)
                .tempfile_in(&parent)
                .map_err(|e| Error::new(ErrorKind::IOError, e))?
        }
        None => Builder::new()
            .prefix(prefix)
            .suffix(suffix)
            .tempfile()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?,
    };

    let temp_path = temp_file.path().to_path_buf();

    if let Some(mode) = &params.mode {
        let octal_mode = parse_octal(mode)?;
        let mut permissions = temp_path.metadata()?.permissions();
        permissions.set_mode(octal_mode);
        set_permissions(&temp_path, permissions)?;
    }

    let path_str = temp_path.to_string_lossy().to_string();

    temp_file
        .keep()
        .map_err(|e| Error::new(ErrorKind::IOError, e))?;

    Ok(ModuleResult {
        changed: true,
        output: Some(path_str),
        extra: None,
    })
}

fn create_tempfile(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Directory => create_temp_directory(params, check_mode),
        State::File => create_temp_file(params, check_mode),
    }
}

#[derive(Debug)]
pub struct Tempfile;

impl Module for Tempfile {
    fn get_name(&self) -> &str {
        "tempfile"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            create_tempfile(parse_params(optional_params)?, check_mode)?,
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

    use std::fs::metadata;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_parse_params_directory() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: directory
            prefix: test_
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                state: State::Directory,
                path: None,
                prefix: Some("test_".to_owned()),
                suffix: None,
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: file
            suffix: .txt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                state: State::File,
                path: None,
                prefix: None,
                suffix: Some(".txt".to_owned()),
                mode: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_path_and_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: file
            path: /var/tmp
            mode: "0600"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                state: State::File,
                path: Some("/var/tmp".to_owned()),
                prefix: None,
                suffix: None,
                mode: Some("0600".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_missing_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            prefix: test_
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_create_temp_directory() {
        let params = Params {
            state: State::Directory,
            path: None,
            prefix: Some("test_dir_".to_owned()),
            suffix: None,
            mode: None,
        };

        let result = create_tempfile(params, false).unwrap();
        assert!(result.changed);
        assert!(result.output.is_some());

        let path = result.output.unwrap();
        let meta = metadata(&path).unwrap();
        assert!(meta.is_dir());

        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn test_create_temp_file() {
        let params = Params {
            state: State::File,
            path: None,
            prefix: Some("test_file_".to_owned()),
            suffix: Some(".txt".to_owned()),
            mode: None,
        };

        let result = create_tempfile(params, false).unwrap();
        assert!(result.changed);
        assert!(result.output.is_some());

        let path = result.output.unwrap();
        assert!(path.ends_with(".txt"));

        let meta = metadata(&path).unwrap();
        assert!(meta.is_file());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_create_temp_directory_with_mode() {
        let params = Params {
            state: State::Directory,
            path: None,
            prefix: Some("test_mode_".to_owned()),
            suffix: None,
            mode: Some("0700".to_owned()),
        };

        let result = create_tempfile(params, false).unwrap();
        let path = result.output.unwrap();

        let meta = metadata(&path).unwrap();
        let permissions = meta.permissions();
        assert_eq!(permissions.mode() & 0o7777, 0o700);

        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn test_create_temp_file_with_mode() {
        let params = Params {
            state: State::File,
            path: None,
            prefix: Some("test_file_mode_".to_owned()),
            suffix: None,
            mode: Some("0600".to_owned()),
        };

        let result = create_tempfile(params, false).unwrap();
        let path = result.output.unwrap();

        let meta = metadata(&path).unwrap();
        let permissions = meta.permissions();
        assert_eq!(permissions.mode() & 0o7777, 0o600);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_create_temp_directory_check_mode() {
        let params = Params {
            state: State::Directory,
            path: None,
            prefix: Some("check_test_".to_owned()),
            suffix: None,
            mode: None,
        };

        let result = create_tempfile(params, true).unwrap();
        assert!(result.changed);
        assert!(result.output.is_some());

        let path = result.output.unwrap();
        let meta = metadata(&path);
        assert!(meta.is_err());
    }

    #[test]
    fn test_create_temp_file_check_mode() {
        let params = Params {
            state: State::File,
            path: None,
            prefix: Some("check_file_".to_owned()),
            suffix: Some(".txt".to_owned()),
            mode: None,
        };

        let result = create_tempfile(params, true).unwrap();
        assert!(result.changed);
        assert!(result.output.is_some());

        let path = result.output.unwrap();
        let meta = metadata(&path);
        assert!(meta.is_err());
    }

    #[test]
    fn test_create_temp_directory_in_custom_path() {
        let temp_parent = tempfile::tempdir().unwrap();
        let custom_path = temp_parent.path().to_string_lossy().to_string();

        let params = Params {
            state: State::Directory,
            path: Some(custom_path.clone()),
            prefix: Some("custom_".to_owned()),
            suffix: None,
            mode: None,
        };

        let result = create_tempfile(params, false).unwrap();
        let path = result.output.unwrap();

        assert!(path.starts_with(&custom_path));
        let meta = metadata(&path).unwrap();
        assert!(meta.is_dir());

        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn test_create_temp_file_in_custom_path() {
        let temp_parent = tempfile::tempdir().unwrap();
        let custom_path = temp_parent.path().to_string_lossy().to_string();

        let params = Params {
            state: State::File,
            path: Some(custom_path.clone()),
            prefix: Some("custom_file_".to_owned()),
            suffix: Some(".log".to_owned()),
            mode: None,
        };

        let result = create_tempfile(params, false).unwrap();
        let path = result.output.unwrap();

        assert!(path.starts_with(&custom_path));
        assert!(path.ends_with(".log"));
        let meta = metadata(&path).unwrap();
        assert!(meta.is_file());

        std::fs::remove_file(&path).unwrap();
    }
}
