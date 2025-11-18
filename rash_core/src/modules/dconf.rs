/// ANCHOR: module
/// # dconf
///
/// Modify and read dconf database.
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
/// - name: Configure available keyboard layouts in Gnome
///   dconf:
///     key: "/org/gnome/desktop/input-sources/sources"
///     value: "[('xkb', 'us'), ('xkb', 'se')]"
///     state: present
///
/// - name: Read currently available keyboard layouts in Gnome
///   dconf:
///     key: "/org/gnome/desktop/input-sources/sources"
///     state: read
///   register: keyboard_layouts
///
/// - name: Reset the available keyboard layouts in Gnome
///   dconf:
///     key: "/org/gnome/desktop/input-sources/sources"
///     state: absent
///
/// - name: Set string value
///   dconf:
///     key: "/org/gnome/desktop/background/picture-uri"
///     value: "'file:///usr/share/backgrounds/gnome/adwaita-day.jpg'"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Set the key to the specified value
    #[default]
    Present,
    /// Read the current value of the key
    Read,
    /// Remove/reset the key
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The dconf key path (e.g., "/org/gnome/desktop/input-sources/sources")
    pub key: String,
    /// The value to set for the key. Uses GVariant syntax, so strings need single quotes like "'myvalue'"
    pub value: Option<String>,
    /// The desired state for the key (present, read, or absent). Defaults to present.
    #[serde(default)]
    pub state: State,
}

fn dconf_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let key = params.key.trim();

    if key.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "key cannot be empty"));
    }

    match params.state {
        State::Read => {
            // Read operation - get current value
            let output = Command::new("dconf")
                .arg("read")
                .arg(key)
                .output()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute dconf: {}", e),
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("dconf read failed: {}", stderr),
                ));
            }

            let current_value = String::from_utf8_lossy(&output.stdout).trim().to_string();

            let extra = Some(value::to_value(json!({
                "value": if current_value.is_empty() { None::<String> } else { Some(current_value.clone()) },
            }))?);

            Ok(ModuleResult {
                changed: false,
                output: if current_value.is_empty() {
                    Some(format!("Key '{}' is not set", key))
                } else {
                    Some(format!("Key '{}' = {}", key, current_value))
                },
                extra,
            })
        }
        State::Present => {
            // Write operation - set value
            let value = params.value.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "value is required when state is present",
                )
            })?;

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would set key '{}' to {}", key, value)),
                    extra: None,
                });
            }

            // First, read the current value to check if we need to change it
            let read_output = Command::new("dconf")
                .arg("read")
                .arg(key)
                .output()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute dconf: {}", e),
                    )
                })?;

            let current_value = if read_output.status.success() {
                String::from_utf8_lossy(&read_output.stdout)
                    .trim()
                    .to_string()
            } else {
                String::new()
            };

            // Check if value is already set to the desired value
            if current_value == value {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Key '{}' already set to {}", key, value)),
                    extra: None,
                });
            }

            // Set the new value
            let output = Command::new("dconf")
                .arg("write")
                .arg(key)
                .arg(&value)
                .output()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute dconf: {}", e),
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("dconf write failed: {}", stderr),
                ));
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Set key '{}' to {}", key, value)),
                extra: None,
            })
        }
        State::Absent => {
            // Reset/remove operation
            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would reset key '{}'", key)),
                    extra: None,
                });
            }

            // First check if the key exists
            let read_output = Command::new("dconf")
                .arg("read")
                .arg(key)
                .output()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute dconf: {}", e),
                    )
                })?;

            let current_value = if read_output.status.success() {
                String::from_utf8_lossy(&read_output.stdout)
                    .trim()
                    .to_string()
            } else {
                String::new()
            };

            // If key is already not set, no change needed
            if current_value.is_empty() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Key '{}' is already not set", key)),
                    extra: None,
                });
            }

            // Reset the key
            let output = Command::new("dconf")
                .arg("reset")
                .arg(key)
                .output()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute dconf: {}", e),
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("dconf reset failed: {}", stderr),
                ));
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Reset key '{}'", key)),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct Dconf;

impl Module for Dconf {
    fn get_name(&self) -> &str {
        "dconf"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((dconf_impl(params, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_read() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: "/org/gnome/desktop/interface/clock-format"
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                key: "/org/gnome/desktop/interface/clock-format".to_string(),
                value: None,
                state: State::Read,
            }
        );
    }

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: "/org/gnome/desktop/interface/clock-format"
            value: "'12h'"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                key: "/org/gnome/desktop/interface/clock-format".to_string(),
                value: Some("'12h'".to_string()),
                state: State::Present,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: "/org/gnome/desktop/interface/clock-format"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                key: "/org/gnome/desktop/interface/clock-format".to_string(),
                value: None,
                state: State::Absent,
            }
        );
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: "/org/gnome/desktop/interface/clock-format"
            value: "'24h'"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                key: "/org/gnome/desktop/interface/clock-format".to_string(),
                value: Some("'24h'".to_string()),
                state: State::Present,
            }
        );
    }

    #[test]
    fn test_parse_params_empty_key() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: ""
            value: "'24h'"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, "");
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: "/test/key"
            value: "'test'"
            unknown: "field"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
