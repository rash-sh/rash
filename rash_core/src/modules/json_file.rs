/// ANCHOR: module
/// # json_file
///
/// Manage settings in JSON files.
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
/// - json_file:
///     path: /etc/app/config.json
///     key: server.port
///     value: 8080
///
/// - json_file:
///     path: /etc/app/config.json
///     key: database.connection.timeout
///     value: 30
///
/// - json_file:
///     path: /etc/app/config.json
///     key: debug.enabled
///     state: absent
///
/// - json_file:
///     path: /etc/app/config.json
///     key: server.host
///     value: "0.0.0.0"
///     backup: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The absolute path to the JSON file to modify.
    pub path: String,
    /// The JSON key path using dot notation (e.g., `server.port`).
    pub key: String,
    /// The value to set for the key. Required if state=present.
    pub value: Option<JsonValue>,
    /// Whether the key should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Create a backup of the file before modifying.
    /// **[default: `false`]**
    pub backup: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn parse_key_path(key: &str) -> Vec<String> {
    key.split('.').map(|s| s.to_string()).collect()
}

#[allow(dead_code)]
fn get_value_at_path<'a>(json: &'a JsonValue, path: &[String]) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(json);
    }

    match json {
        JsonValue::Object(map) => {
            if let Some(value) = map.get(&path[0]) {
                get_value_at_path(value, &path[1..])
            } else {
                None
            }
        }
        JsonValue::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if idx < arr.len() {
                    get_value_at_path(&arr[idx], &path[1..])
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn set_value_at_path(json: &mut JsonValue, path: &[String], value: JsonValue) -> bool {
    if path.is_empty() {
        *json = value;
        return true;
    }

    match json {
        JsonValue::Object(map) => {
            let key = &path[0];
            if path.len() == 1 {
                if let Some(existing) = map.get(key)
                    && existing == &value
                {
                    return false;
                }
                map.insert(key.clone(), value);
                true
            } else {
                if !map.contains_key(key) {
                    map.insert(key.clone(), JsonValue::Object(serde_json::Map::new()));
                }
                if let Some(child) = map.get_mut(key) {
                    set_value_at_path(child, &path[1..], value)
                } else {
                    false
                }
            }
        }
        JsonValue::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if idx < arr.len() {
                    if path.len() == 1 {
                        if arr[idx] == value {
                            return false;
                        }
                        arr[idx] = value;
                        true
                    } else {
                        set_value_at_path(&mut arr[idx], &path[1..], value)
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn remove_key_at_path(json: &mut JsonValue, path: &[String]) -> bool {
    if path.is_empty() {
        return false;
    }

    match json {
        JsonValue::Object(map) => {
            if path.len() == 1 {
                map.remove(&path[0]).is_some()
            } else if let Some(child) = map.get_mut(&path[0]) {
                remove_key_at_path(child, &path[1..])
            } else {
                false
            }
        }
        JsonValue::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if idx < arr.len() {
                    if path.len() == 1 {
                        arr.remove(idx);
                        true
                    } else {
                        remove_key_at_path(&mut arr[idx], &path[1..])
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn create_backup(path: &Path) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new(ErrorKind::Other, e))?
        .as_secs();
    let backup_path = format!("{}.{}.bak", path.display(), timestamp);
    fs::copy(path, &backup_path)?;
    Ok(())
}

pub fn json_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let path = Path::new(&params.path);
    let path_str = params.path.clone();

    let (mut json, original_content) = if path.exists() {
        let content = fs::read_to_string(path)?;
        let json: JsonValue = if content.trim().is_empty() {
            JsonValue::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&content).map_err(|e| Error::new(ErrorKind::InvalidData, e))?
        };
        (json, content)
    } else {
        (JsonValue::Object(serde_json::Map::new()), String::new())
    };

    let key_path = parse_key_path(&params.key);

    let changed = match state {
        State::Present => {
            let value = params.value.as_ref().unwrap().clone();
            set_value_at_path(&mut json, &key_path, value)
        }
        State::Absent => remove_key_at_path(&mut json, &key_path),
    };

    if changed {
        let new_content =
            serde_json::to_string_pretty(&json).map_err(|e| Error::new(ErrorKind::Other, e))?;
        let new_content_with_newline = format!("{new_content}\n");

        diff(&original_content, &new_content_with_newline);

        if !check_mode {
            if params.backup.unwrap_or(false) && path.exists() {
                create_backup(path)?;
            }

            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content_with_newline.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(path_str),
        extra: None,
    })
}

#[derive(Debug)]
pub struct JsonFile;

impl Module for JsonFile {
    fn get_name(&self) -> &str {
        "json_file"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((json_file(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "/etc/config.json"
            key: "server.port"
            value: 8080
            state: "present"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/etc/config.json".to_owned(),
                key: "server.port".to_owned(),
                value: Some(json!(8080)),
                state: Some(State::Present),
                backup: None,
            }
        );
    }

    #[test]
    fn test_parse_key_path() {
        assert_eq!(parse_key_path("server.port"), vec!["server", "port"]);
        assert_eq!(
            parse_key_path("database.connection.timeout"),
            vec!["database", "connection", "timeout"]
        );
        assert_eq!(parse_key_path("key"), vec!["key"]);
    }

    #[test]
    fn test_get_value_at_path() {
        let json: JsonValue = serde_json::from_str(r#"{"server": {"port": 8080}}"#).unwrap();
        let result = get_value_at_path(&json, &["server".to_string(), "port".to_string()]);
        assert_eq!(result, Some(&JsonValue::Number(8080.into())));
    }

    #[test]
    fn test_set_value_at_path_new_key() {
        let mut json = JsonValue::Object(serde_json::Map::new());
        let changed = set_value_at_path(&mut json, &["server".to_string()], json!({"port": 8080}));
        assert!(changed);
        assert_eq!(json, json!({"server": {"port": 8080}}));
    }

    #[test]
    fn test_set_value_at_path_nested() {
        let mut json = json!({});
        let changed = set_value_at_path(
            &mut json,
            &["server".to_string(), "port".to_string()],
            json!(8080),
        );
        assert!(changed);
        assert_eq!(json, json!({"server": {"port": 8080}}));
    }

    #[test]
    fn test_set_value_at_path_no_change() {
        let mut json = json!({"server": {"port": 8080}});
        let changed = set_value_at_path(
            &mut json,
            &["server".to_string(), "port".to_string()],
            json!(8080),
        );
        assert!(!changed);
    }

    #[test]
    fn test_remove_key_at_path() {
        let mut json = json!({"server": {"port": 8080, "host": "localhost"}});
        let removed = remove_key_at_path(&mut json, &["server".to_string(), "port".to_string()]);
        assert!(removed);
        assert_eq!(json, json!({"server": {"host": "localhost"}}));
    }

    #[test]
    fn test_remove_key_at_path_not_found() {
        let mut json = json!({"server": {"port": 8080}});
        let removed = remove_key_at_path(&mut json, &["server".to_string(), "host".to_string()]);
        assert!(!removed);
    }

    #[test]
    fn test_json_file_add_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{"server": {}}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: Some(json!(8080)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"server": {"port": 8080}}));
    }

    #[test]
    fn test_json_file_modify_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{"server": {"port": 3000}}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: Some(json!(8080)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"server": {"port": 8080}}));
    }

    #[test]
    fn test_json_file_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{"server": {"port": 8080}}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: Some(json!(8080)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_json_file_remove_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(
            &file_path,
            r#"{"server": {"port": 8080, "host": "localhost"}}"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: None,
            state: Some(State::Absent),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"server": {"host": "localhost"}}));
    }

    #[test]
    fn test_json_file_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: Some(json!(8080)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"server": {"port": 8080}}));
    }

    #[test]
    fn test_json_file_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{"server": {"port": 3000}}"#).unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.port".to_string(),
            value: Some(json!(8080)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_json_file_missing_value_for_present() {
        let params = Params {
            path: "/tmp/test.json".to_string(),
            key: "server.port".to_string(),
            value: None,
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_json_file_string_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "server.host".to_string(),
            value: Some(json!("0.0.0.0")),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"server": {"host": "0.0.0.0"}}));
    }

    #[test]
    fn test_json_file_deeply_nested() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "database.connection.timeout".to_string(),
            value: Some(json!(30)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"database": {"connection": {"timeout": 30}}}));
    }

    #[test]
    fn test_json_file_boolean_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");

        fs::write(&file_path, r#"{}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: "debug.enabled".to_string(),
            value: Some(json!(true)),
            state: Some(State::Present),
            backup: None,
        };

        let result = json_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"debug": {"enabled": true}}));
    }
}
