/// ANCHOR: module
/// # docker_volume
///
/// Manage Docker volumes for persistent container storage.
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
/// - name: Create a volume
///   docker_volume:
///     name: mydata
///     state: present
///
/// - name: Create a volume with specific driver
///   docker_volume:
///     name: mydata
///     driver: local
///     state: present
///
/// - name: Create a volume with driver options
///   docker_volume:
///     name: nfs_volume
///     driver: local
///     driver_options:
///       type: nfs
///       o: addr=192.168.1.1,rw
///       device: ":/export/data"
///     state: present
///
/// - name: Create a volume with labels
///   docker_volume:
///     name: labeled_volume
///     labels:
///       environment: production
///       owner: team-ops
///     state: present
///
/// - name: Remove a volume
///   docker_volume:
///     name: olddata
///     state: absent
///
/// - name: Force remove a volume
///   docker_volume:
///     name: olddata
///     state: absent
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the volume.
    name: String,
    /// State of the volume.
    #[serde(default)]
    state: State,
    /// Volume driver (e.g., local).
    #[serde(default)]
    driver: Option<String>,
    /// Driver-specific options.
    #[serde(default)]
    driver_options: Option<serde_json::Map<String, serde_json::Value>>,
    /// Volume labels.
    #[serde(default)]
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Force removal of volume (for state=absent).
    #[serde(default)]
    force: bool,
}

#[derive(Debug)]
pub struct DockerVolume;

#[derive(Debug, Clone)]
struct VolumeInfo {
    name: String,
    driver: String,
    mountpoint: String,
}

impl Module for DockerVolume {
    fn get_name(&self) -> &str {
        "docker_volume"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_volume(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct DockerClient {
    check_mode: bool,
}

impl DockerClient {
    fn new(check_mode: bool) -> Self {
        DockerClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `docker {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing docker: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn volume_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(&["volume", "inspect", "--format", "{{.Name}}", name], false)?;
        Ok(output.status.success())
    }

    fn get_volume_info(&self, name: &str) -> Result<Option<VolumeInfo>> {
        let output = self.exec_cmd(
            &[
                "volume",
                "inspect",
                "--format",
                "{{.Name}}|{{.Driver}}|{{.Mountpoint}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 3 {
            Ok(Some(VolumeInfo {
                name: parts[0].to_string(),
                driver: parts[1].to_string(),
                mountpoint: parts[2].to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn create_volume(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["volume".to_string(), "create".to_string()];

        args.push("--name".to_string());
        args.push(params.name.clone());

        if let Some(ref driver) = params.driver {
            args.push("--driver".to_string());
            args.push(driver.clone());
        }

        if let Some(ref opts) = params.driver_options {
            for (key, value) in opts {
                let opt_str = match value {
                    serde_json::Value::String(s) => format!("{}={}", key, s),
                    serde_json::Value::Number(n) => format!("{}={}", key, n),
                    serde_json::Value::Bool(b) => format!("{}={}", key, b),
                    _ => format!("{}={}", key, value),
                };
                args.push("--opt".to_string());
                args.push(opt_str);
            }
        }

        if let Some(ref labels) = params.labels {
            for (key, value) in labels {
                let label_str = match value {
                    serde_json::Value::String(s) => format!("{}={}", key, s),
                    serde_json::Value::Number(n) => format!("{}={}", key, n),
                    serde_json::Value::Bool(b) => format!("{}={}", key, b),
                    _ => format!("{}={}", key, value),
                };
                args.push("--label".to_string());
                args.push(label_str);
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn remove_volume(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.volume_exists(name)? {
            return Ok(false);
        }

        let mut args = vec!["volume", "rm"];
        if force {
            args.push("-f");
        }
        args.push(name);

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn get_volume_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_volume_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("driver".to_string(), serde_json::Value::String(info.driver));
            result.insert(
                "mountpoint".to_string(),
                serde_json::Value::String(info.mountpoint),
            );
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn validate_volume_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Volume name cannot be empty",
        ));
    }

    if name.len() > 64 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Volume name too long (max 64 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Volume name contains invalid characters (only [a-zA-Z0-9.-_] allowed)",
        ));
    }

    if name.starts_with('-') || name.starts_with('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Volume name cannot start with '-' or '.'",
        ));
    }

    Ok(())
}

fn docker_volume(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_volume_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            if !client.volume_exists(&params.name)? {
                client.create_volume(&params)?;
                diff("volume: absent".to_string(), "volume: present".to_string());
                output_messages.push(format!("Volume '{}' created", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Volume '{}' already exists", params.name));
            }
        }
        State::Absent => {
            if client.remove_volume(&params.name, params.force)? {
                diff("volume: present".to_string(), "volume: absent".to_string());
                output_messages.push(format!("Volume '{}' removed", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Volume '{}' not found", params.name));
            }
        }
    }

    let extra = client.get_volume_state(&params.name)?;

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mydata
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "mydata");
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_with_driver() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mydata
            driver: local
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "mydata");
        assert_eq!(params.driver, Some("local".to_string()));
    }

    #[test]
    fn test_parse_params_with_driver_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nfs_volume
            driver: local
            driver_options:
              type: nfs
              o: addr=192.168.1.1,rw
              device: ":/export/data"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let opts = params.driver_options.unwrap();
        assert_eq!(
            opts.get("type").unwrap(),
            &serde_json::Value::String("nfs".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_labels() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: labeled_volume
            labels:
              environment: production
              owner: team-ops
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let labels = params.labels.unwrap();
        assert_eq!(
            labels.get("environment").unwrap(),
            &serde_json::Value::String("production".to_string())
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: olddata
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "olddata");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_force_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: olddata
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mydata
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_volume_name() {
        assert!(validate_volume_name("mydata").is_ok());
        assert!(validate_volume_name("my-data").is_ok());
        assert!(validate_volume_name("my_data").is_ok());
        assert!(validate_volume_name("my.data").is_ok());
        assert!(validate_volume_name("mydata123").is_ok());

        assert!(validate_volume_name("").is_err());
        assert!(validate_volume_name(&"a".repeat(65)).is_err());
        assert!(validate_volume_name("-mydata").is_err());
        assert!(validate_volume_name(".mydata").is_err());
        assert!(validate_volume_name("my data").is_err());
        assert!(validate_volume_name("my/data").is_err());
    }
}
