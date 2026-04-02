/// ANCHOR: module
/// # docker_info
///
/// Gather Docker system information for debugging and monitoring.
///
/// Returns Docker version, system info, and availability status.
/// This module never changes system state - it only collects information.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Get Docker info
///   docker_info:
///   register: docker
///
/// - name: Check Docker is available
///   debug:
///     msg: "Docker is available: {{ docker.docker_info.available }}"
///
/// - name: Show Docker version
///   debug:
///     msg: "Docker version: {{ docker.docker_info.version.Version }}"
///
/// - name: Show Docker server info
///   debug:
///     msg: "Server version: {{ docker.docker_info.info.ServerVersion }}"
///
/// - name: Fail if Docker not available
///   fail:
///     msg: "Docker is not available"
///   when: not docker.docker_info.available
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Get Docker version information.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    get_version: bool,
    /// Get Docker system info.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    get_info: bool,
    /// Get Docker disk usage info.
    /// **[default: `false`]**
    #[serde(default)]
    get_disk_usage: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
pub struct DockerInfo;

impl Module for DockerInfo {
    fn get_name(&self) -> &str {
        "docker_info"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((docker_info(parse_params(optional_params)?)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn check_docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn get_docker_version() -> Result<serde_json::Value> {
    let output = Command::new("docker")
        .args(["version", "--format", "{{json .}}"])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("docker version output: {:?}", output);

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to get Docker version: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid JSON: {e}")))
}

fn get_docker_info() -> Result<serde_json::Value> {
    let output = Command::new("docker")
        .args(["info", "--format", "{{json .}}"])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("docker info output: {:?}", output);

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to get Docker info: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid JSON: {e}")))
}

fn get_docker_disk_usage() -> Result<serde_json::Value> {
    let output = Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("docker system df output: {:?}", output);

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to get Docker disk usage: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid JSON: {e}")))
}

fn docker_info(params: Params) -> Result<ModuleResult> {
    let available = check_docker_available();

    let mut info = serde_json::Map::new();
    info.insert("available".to_string(), serde_json::Value::Bool(available));

    if !available {
        let extra = value::to_value(serde_json::json!({"docker_info": info}))?;
        return Ok(ModuleResult {
            changed: false,
            output: Some("Docker is not available".to_string()),
            extra: Some(extra),
        });
    }

    if params.get_version {
        match get_docker_version() {
            Ok(version) => {
                info.insert("version".to_string(), version);
            }
            Err(e) => {
                trace!("Failed to get Docker version: {}", e);
                info.insert("version".to_string(), serde_json::Value::Null);
            }
        }
    }

    if params.get_info {
        match get_docker_info() {
            Ok(docker_info_val) => {
                info.insert("info".to_string(), docker_info_val);
            }
            Err(e) => {
                trace!("Failed to get Docker info: {}", e);
                info.insert("info".to_string(), serde_json::Value::Null);
            }
        }
    }

    if params.get_disk_usage {
        match get_docker_disk_usage() {
            Ok(disk_usage) => {
                info.insert("disk_usage".to_string(), disk_usage);
            }
            Err(e) => {
                trace!("Failed to get Docker disk usage: {}", e);
                info.insert("disk_usage".to_string(), serde_json::Value::Null);
            }
        }
    }

    let extra = value::to_value(serde_json::json!({"docker_info": info}))?;

    Ok(ModuleResult {
        changed: false,
        output: Some("Docker information collected".to_string()),
        extra: Some(extra),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.get_version);
        assert!(params.get_info);
        assert!(!params.get_disk_usage);
    }

    #[test]
    fn test_parse_params_all_false() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            get_version: false
            get_info: false
            get_disk_usage: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.get_version);
        assert!(!params.get_info);
        assert!(params.get_disk_usage);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_module_name() {
        let module = DockerInfo;
        assert_eq!(module.get_name(), "docker_info");
    }
}
