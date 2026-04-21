/// ANCHOR: module
/// # sysfs
///
/// Manage sysfs attributes for kernel and device configuration.
/// Essential for IoT devices and embedded systems where hardware parameters
/// need to be tuned at runtime.
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
/// - name: Set MTU for network interface
///   sysfs:
///     path: /sys/class/net/eth0/mtu
///     value: "9000"
///
/// - name: Configure GPIO pin direction
///   sysfs:
///     path: /sys/class/gpio/gpio17/direction
///     value: "out"
///
/// - name: Enable IP forwarding via sysfs
///   sysfs:
///     path: /proc/sys/net/ipv4/ip_forward
///     value: "1"
///
/// - name: Set CPU governor
///   sysfs:
///     path: /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
///     value: "performance"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// sysfs attribute path.
    pub path: String,
    /// Desired value of the sysfs attribute. Required when state=present.
    pub value: Option<String>,
    /// Whether the attribute should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn read_sysfs_attribute(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read sysfs attribute {}: {e}", path.display()),
        )
    })?;
    Ok(content.trim().to_string())
}

fn write_sysfs_attribute(path: &Path, value: &str) -> Result<()> {
    fs::write(path, value).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write sysfs attribute {}: {e}", path.display()),
        )
    })
}

pub fn sysfs(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let path = Path::new(&params.path);

    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("sysfs path not found: {}", params.path),
        ));
    }

    match state {
        State::Present => {
            let value = params.value.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::OmitParam,
                    "value parameter is required when state=present",
                )
            })?;

            let current = read_sysfs_attribute(path)?;

            if current == *value {
                return Ok(ModuleResult::new(false, None, Some(params.path)));
            }

            diff(&current, value);

            if !check_mode {
                write_sysfs_attribute(path, value)?;
            }

            Ok(ModuleResult::new(true, None, Some(params.path)))
        }
        State::Absent => {
            let current = read_sysfs_attribute(path)?;

            if let Some(ref value) = params.value {
                if current != *value {
                    return Ok(ModuleResult::new(false, None, Some(params.path)));
                }
            }

            diff(&current, "");

            if !check_mode {
                write_sysfs_attribute(path, "")?;
            }

            Ok(ModuleResult::new(true, None, Some(params.path)))
        }
    }
}

#[derive(Debug)]
pub struct Sysfs;

impl Module for Sysfs {
    fn get_name(&self) -> &str {
        "sysfs"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((sysfs(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /sys/class/net/eth0/mtu
            value: "9000"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/sys/class/net/eth0/mtu".to_owned(),
                value: Some("9000".to_owned()),
                state: Some(State::Present),
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /sys/class/net/eth0/mtu
            value: "9000"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/sys/class/net/eth0/mtu");
        assert_eq!(params.value, Some("9000".to_owned()));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /sys/class/gpio/gpio17/direction
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
        assert_eq!(params.value, None);
    }

    #[test]
    fn test_sysfs_set_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "1500").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("9000".to_string()),
            state: Some(State::Present),
        };

        let result = sysfs(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "9000");
    }

    #[test]
    fn test_sysfs_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "9000").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("9000".to_string()),
            state: Some(State::Present),
        };

        let result = sysfs(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_sysfs_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "1500").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("9000".to_string()),
            state: Some(State::Present),
        };

        let result = sysfs(params, true).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "1500");
    }

    #[test]
    fn test_sysfs_missing_value_for_present() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "1500").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: None,
            state: Some(State::Present),
        };

        let result = sysfs(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_sysfs_path_not_found() {
        let params = Params {
            path: "/sys/class/net/nonexistent/mtu".to_string(),
            value: Some("9000".to_string()),
            state: Some(State::Present),
        };

        let result = sysfs(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("sysfs path not found")
        );
    }

    #[test]
    fn test_sysfs_absent_with_matching_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("direction");
        fs::write(&file_path, "out").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("out".to_string()),
            state: Some(State::Absent),
        };

        let result = sysfs(params, false).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_sysfs_absent_with_non_matching_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("direction");
        fs::write(&file_path, "in").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("out".to_string()),
            state: Some(State::Absent),
        };

        let result = sysfs(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_sysfs_trims_whitespace() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "1500\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("1500".to_string()),
            state: Some(State::Present),
        };

        let result = sysfs(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_sysfs_default_state_is_present() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("mtu");
        fs::write(&file_path, "1500").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            value: Some("9000".to_string()),
            state: None,
        };

        let result = sysfs(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "9000");
    }
}
