/// ANCHOR: module
/// # xattr
///
/// Manage extended file attributes (xattrs).
///
/// Extended attributes are key-value metadata stored on filesystems that support them.
/// They are useful for security labeling, custom metadata, and container/overlay filesystem
/// configurations.
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
/// - xattr:
///     path: /etc/app/config.json
///     key: user.comment
///     value: "Application configuration"
///
/// - xattr:
///     path: /data/file.txt
///     key: user.backup_status
///     state: absent
///
/// - xattr:
///     path: /var/log/app.log
///     key: security.label
///     value: "confidential"
///     namespace: security
///
/// - name: Get all xattrs for a file
///   xattr:
///     path: /etc/app/config.json
///     state: all
///   register: file_xattrs
///
/// - debug:
///     var: file_xattrs.xattrs
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
    All,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Namespace {
    User,
    Trusted,
    System,
    Security,
}

impl Namespace {
    fn prefix(&self) -> &'static str {
        match self {
            Namespace::User => "user.",
            Namespace::Trusted => "trusted.",
            Namespace::System => "system.",
            Namespace::Security => "security.",
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The full path to the file or directory.
    path: String,
    /// The name of the extended attribute key.
    /// Required for state=present and state=absent.
    key: Option<String>,
    /// The value to set for the extended attribute.
    /// Required for state=present.
    value: Option<String>,
    /// The namespace for the attribute.
    /// **[default: `"user"`]**
    #[serde(default)]
    namespace: Option<Namespace>,
    /// Whether to set/get/remove the attribute (present), remove it (absent), or
    /// list all attributes (all).
    /// **[default: `"present"`]**
    #[serde(default)]
    state: Option<State>,
    /// Whether to follow symlinks.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    follow: bool,
}

fn get_full_key(key: &str, namespace: &Namespace) -> String {
    if key.starts_with(namespace.prefix()) {
        key.to_string()
    } else {
        format!("{}{}", namespace.prefix(), key)
    }
}

fn check_path_exists(path: &str) -> Result<()> {
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("path '{}' does not exist", path),
        ));
    }
    Ok(())
}

fn get_xattr_value(path: &str, key: &str, follow: bool) -> Result<Option<String>> {
    let value = if follow {
        xattr::get(path, key)
    } else {
        xattr::get_deref(path, key)
    };

    match value {
        Ok(Some(v)) => Ok(Some(String::from_utf8_lossy(&v).to_string())),
        Ok(None) => Ok(None),
        Err(e) => Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("failed to get xattr '{}': {}", key, e),
        )),
    }
}

fn set_xattr_value(path: &str, key: &str, value: &str, follow: bool) -> Result<()> {
    let result = if follow {
        xattr::set(path, key, value.as_bytes())
    } else {
        xattr::set_deref(path, key, value.as_bytes())
    };

    result.map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("failed to set xattr '{}': {}", key, e),
        )
    })
}

fn remove_xattr(path: &str, key: &str, follow: bool) -> Result<()> {
    let result = if follow {
        xattr::remove(path, key)
    } else {
        xattr::remove_deref(path, key)
    };

    result.map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("failed to remove xattr '{}': {}", key, e),
        )
    })
}

fn list_xattrs(path: &str, follow: bool) -> Result<Vec<String>> {
    let xattrs = if follow {
        xattr::list(path)
    } else {
        xattr::list_deref(path)
    };

    xattrs
        .map(|iter| {
            iter.map(|k| String::from_utf8_lossy(k.as_bytes()).to_string())
                .collect()
        })
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("failed to list xattrs: {}", e),
            )
        })
}

fn handle_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let key = params
        .key
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "key is required for state=present"))?;

    let value = params.value.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "value is required for state=present",
        )
    })?;

    let namespace = params.namespace.as_ref().unwrap_or(&Namespace::User);
    let full_key = get_full_key(key, namespace);

    check_path_exists(&params.path)?;

    let current_value = get_xattr_value(&params.path, &full_key, params.follow)?;

    match current_value {
        Some(current) if current == *value => Ok(ModuleResult {
            changed: false,
            output: Some(params.path.clone()),
            extra: None,
        }),
        Some(current) => {
            debug!(
                "xattr '{}' changing from '{}' to '{}'",
                full_key, current, value
            );
            if !check_mode {
                set_xattr_value(&params.path, &full_key, value, params.follow)?;
            }
            Ok(ModuleResult {
                changed: true,
                output: Some(params.path.clone()),
                extra: None,
            })
        }
        None => {
            debug!("xattr '{}' setting to '{}'", full_key, value);
            if !check_mode {
                set_xattr_value(&params.path, &full_key, value, params.follow)?;
            }
            Ok(ModuleResult {
                changed: true,
                output: Some(params.path.clone()),
                extra: None,
            })
        }
    }
}

fn handle_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let key = params
        .key
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "key is required for state=absent"))?;

    let namespace = params.namespace.as_ref().unwrap_or(&Namespace::User);
    let full_key = get_full_key(key, namespace);

    check_path_exists(&params.path)?;

    let current_value = get_xattr_value(&params.path, &full_key, params.follow)?;

    match current_value {
        Some(_) => {
            debug!("removing xattr '{}'", full_key);
            if !check_mode {
                remove_xattr(&params.path, &full_key, params.follow)?;
            }
            Ok(ModuleResult {
                changed: true,
                output: Some(params.path.clone()),
                extra: None,
            })
        }
        None => Ok(ModuleResult {
            changed: false,
            output: Some(params.path.clone()),
            extra: None,
        }),
    }
}

fn handle_all(params: &Params) -> Result<ModuleResult> {
    check_path_exists(&params.path)?;

    let xattrs = list_xattrs(&params.path, params.follow)?;

    let xattrs_with_values: Vec<serde_json::Value> = xattrs
        .iter()
        .filter_map(|key| {
            let value = get_xattr_value(&params.path, key, params.follow).ok();
            value.flatten().map(|v| {
                json!({
                    "key": key,
                    "value": v
                })
            })
        })
        .collect();

    let extra = serde_norway::to_value(json!({
        "xattrs": xattrs_with_values
    }))
    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    Ok(ModuleResult {
        changed: false,
        output: Some(params.path.clone()),
        extra: Some(extra),
    })
}

pub fn xattr(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let state = params.state.as_ref().unwrap_or(&State::Present);

    match state {
        State::Present => handle_present(&params, check_mode),
        State::Absent => handle_absent(&params, check_mode),
        State::All => handle_all(&params),
    }
}

#[derive(Debug)]
pub struct Xattr;

impl Module for Xattr {
    fn get_name(&self) -> &str {
        "xattr"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((xattr(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            key: user.comment
            value: "test value"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/tmp/test");
        assert_eq!(params.key, Some("user.comment".to_string()));
        assert_eq!(params.value, Some("test value".to_string()));
        assert_eq!(params.state, None);
        assert!(params.follow);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            key: user.comment
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            state: all
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::All));
        assert_eq!(params.key, None);
    }

    #[test]
    fn test_parse_params_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            key: label
            value: "test"
            namespace: security
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.namespace, Some(Namespace::Security));
    }

    #[test]
    fn test_parse_params_no_follow() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /tmp/test
            key: user.comment
            value: "test"
            follow: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.follow);
    }

    #[test]
    fn test_parse_params_no_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: user.comment
            value: "test"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_get_full_key_with_namespace() {
        assert_eq!(get_full_key("comment", &Namespace::User), "user.comment");
        assert_eq!(
            get_full_key("user.comment", &Namespace::User),
            "user.comment"
        );
        assert_eq!(
            get_full_key("label", &Namespace::Security),
            "security.label"
        );
    }

    #[test]
    fn test_xattr_not_exists() {
        let result = xattr(
            Params {
                path: "/nonexistent/path".to_string(),
                key: Some("user.test".to_string()),
                value: Some("test".to_string()),
                namespace: None,
                state: Some(State::Present),
                follow: true,
            },
            false,
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_xattr_present_missing_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: None,
                value: Some("test".to_string()),
                namespace: None,
                state: Some(State::Present),
                follow: true,
            },
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_xattr_present_missing_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("user.test".to_string()),
                value: None,
                namespace: None,
                state: Some(State::Present),
                follow: true,
            },
            false,
        );
        assert!(result.is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_set_and_get() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: Some("test value".to_string()),
                namespace: Some(Namespace::User),
                state: Some(State::Present),
                follow: true,
            },
            false,
        )
        .unwrap();

        assert!(result.changed);

        let value = xattr::get(file_path, "user.comment").unwrap();
        assert_eq!(String::from_utf8_lossy(&value.unwrap()), "test value");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_no_change_when_same() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        xattr::set(&file_path, "user.comment", b"test value").unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: Some("test value".to_string()),
                namespace: Some(Namespace::User),
                state: Some(State::Present),
                follow: true,
            },
            false,
        )
        .unwrap();

        assert!(!result.changed);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_remove() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        xattr::set(&file_path, "user.comment", b"test value").unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: None,
                namespace: Some(Namespace::User),
                state: Some(State::Absent),
                follow: true,
            },
            false,
        )
        .unwrap();

        assert!(result.changed);

        let value = xattr::get(&file_path, "user.comment").unwrap();
        assert!(value.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_remove_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: None,
                namespace: Some(Namespace::User),
                state: Some(State::Absent),
                follow: true,
            },
            false,
        )
        .unwrap();

        assert!(!result.changed);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_list_all() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        xattr::set(&file_path, "user.comment", b"test value").unwrap();
        xattr::set(&file_path, "user.author", b"test author").unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: None,
                value: None,
                namespace: None,
                state: Some(State::All),
                follow: true,
            },
            false,
        )
        .unwrap();

        assert!(!result.changed);
        let extra = result.extra.unwrap();
        let xattrs = extra.get("xattrs").unwrap();
        let empty_vec = vec![];
        let xattrs_seq = xattrs.as_sequence().unwrap_or(&empty_vec);
        assert_eq!(xattrs_seq.len(), 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_check_mode_set() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: Some("test value".to_string()),
                namespace: Some(Namespace::User),
                state: Some(State::Present),
                follow: true,
            },
            true,
        )
        .unwrap();

        assert!(result.changed);

        let value = xattr::get(&file_path, "user.comment").unwrap();
        assert!(value.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_xattr_check_mode_remove() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        xattr::set(&file_path, "user.comment", b"test value").unwrap();

        let result = xattr(
            Params {
                path: file_path.to_str().unwrap().to_string(),
                key: Some("comment".to_string()),
                value: None,
                namespace: Some(Namespace::User),
                state: Some(State::Absent),
                follow: true,
            },
            true,
        )
        .unwrap();

        assert!(result.changed);

        let value = xattr::get(&file_path, "user.comment").unwrap();
        assert!(value.is_some());
    }
}
