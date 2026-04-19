/// ANCHOR: module
/// # incus
///
/// Manage Incus/LXD containers and virtual machines.
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
/// - name: Create and start Incus container
///   incus:
///     name: webapp
///     state: started
///     image: images:alpine/3.19
///     type: container
///
/// - name: Stop a container
///   incus:
///     name: webapp
///     state: stopped
///
/// - name: Restart a container
///   incus:
///     name: webapp
///     state: restarted
///
/// - name: Remove a container
///   incus:
///     name: webapp
///     state: absent
///
/// - name: Create a virtual machine
///   incus:
///     name: vmapp
///     state: started
///     image: images:ubuntu/22.04
///     type: virtual-machine
///
/// - name: Create container with config
///   incus:
///     name: configured_app
///     image: images:alpine/3.19
///     state: started
///     config:
///       limits.memory: 512MB
///       boot.autostart: true
///
/// - name: Create container with devices
///   incus:
///     name: device_app
///     image: images:alpine/3.19
///     state: started
///     devices:
///       root:
///         path: /
///         pool: default
///         type: disk
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::collections::HashMap;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Present,
    Restarted,
    Started,
    Stopped,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "kebab-case")]
enum InstanceType {
    Container,
    VirtualMachine,
}

fn default_state() -> State {
    State::Started
}

fn default_instance_type() -> InstanceType {
    InstanceType::Container
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the container/VM.
    name: String,
    /// Image to use for creation (e.g., images:alpine/3.19).
    image: Option<String>,
    /// State of the container/VM.
    #[serde(default = "default_state")]
    state: State,
    /// Type of instance (container or virtual-machine).
    #[serde(default = "default_instance_type")]
    #[serde(rename = "type")]
    instance_type: InstanceType,
    /// Configuration key-value pairs (supports strings, booleans, numbers).
    config: Option<serde_json::Map<String, serde_json::Value>>,
    /// Device configuration.
    devices: Option<HashMap<String, HashMap<String, String>>>,
    /// Force container/VM removal on state=absent.
    #[serde(default)]
    force: bool,
    /// Wait for operation to complete.
    #[serde(default = "default_true")]
    wait: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
pub struct Incus;

#[derive(Debug, Clone)]
struct InstanceInfo {
    name: String,
    status: String,
    image: Option<String>,
    #[allow(dead_code)]
    instance_type: String,
    ip_addresses: Vec<String>,
}

impl Module for Incus {
    fn get_name(&self) -> &str {
        "incus"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            incus_instance(parse_params(optional_params)?, check_mode)?,
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

struct IncusClient {
    check_mode: bool,
}

impl IncusClient {
    fn new(check_mode: bool) -> Self {
        IncusClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("incus")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `incus {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing incus: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn instance_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(&["list", "--format", "json"], false)?;
        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let instances: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(instances.iter().any(|i| {
            i.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|n| n == name)
        }))
    }

    fn get_instance_info(&self, name: &str) -> Result<Option<InstanceInfo>> {
        let output = self.exec_cmd(&["list", name, "--format", "json"], false)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let instances: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        if instances.is_empty() {
            return Ok(None);
        }

        let instance = &instances[0];
        let status = instance
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let instance_type = instance
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("container")
            .to_string();

        let image = instance
            .get("config")
            .and_then(|c| c.get("image.alias"))
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());

        let ip_addresses: Vec<String> = instance
            .get("state")
            .and_then(|s| s.get("network"))
            .and_then(|n| n.as_object())
            .map(|network| {
                network
                    .values()
                    .flat_map(|iface| {
                        iface
                            .get("addresses")
                            .and_then(|a| a.as_array())
                            .map(|addresses| {
                                addresses
                                    .iter()
                                    .filter_map(|addr| addr.get("address").and_then(|a| a.as_str()))
                                    .filter(|addr| !addr.starts_with("fe80"))
                                    .map(|s| s.to_string())
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Some(InstanceInfo {
            name: name.to_string(),
            status,
            image,
            instance_type,
            ip_addresses,
        }))
    }

    fn is_running(&self, name: &str) -> Result<bool> {
        let info = self.get_instance_info(name)?;
        Ok(info.is_some_and(|i| i.status == "Running"))
    }

    fn create_instance(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["init".to_string()];

        let image = params.image.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "image is required when creating an instance",
            )
        })?;

        args.push(image.clone());
        args.push(params.name.clone());

        if params.instance_type == InstanceType::VirtualMachine {
            args.push("--vm".to_string());
        }

        if let Some(ref config) = params.config {
            for (key, value) in config {
                args.push("--config".to_string());
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => value.to_string(),
                };
                args.push(format!("{}={}", key, value_str));
            }
        }

        if let Some(ref devices) = params.devices {
            for (device_name, device_config) in devices {
                for (key, value) in device_config {
                    args.push("--device".to_string());
                    args.push(format!("{},{}={}", device_name, key, value));
                }
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn start_instance(&self, name: &str, wait: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_running(name)? {
            return Ok(false);
        }

        let mut args = vec!["start", name];
        if wait {
            args.push("--wait");
        }

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn stop_instance(&self, name: &str, wait: bool, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_running(name)? {
            return Ok(false);
        }

        let mut args = vec!["stop", name];
        if wait {
            args.push("--wait");
        }
        if force {
            args.push("--force");
        }

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn restart_instance(&self, name: &str, wait: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.instance_exists(name)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Instance '{}' does not exist", name),
            ));
        }

        let mut args = vec!["restart", name];
        if wait {
            args.push("--wait");
        }

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn delete_instance(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.instance_exists(name)? {
            return Ok(false);
        }

        let mut args = vec!["delete", name];
        if force {
            args.push("--force");
        }

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn get_instance_state(&self, name: &str) -> Result<HashMap<String, serde_json::Value>> {
        let mut result = HashMap::new();

        if let Some(info) = self.get_instance_info(name)? {
            let is_running = info.status == "Running";
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("status".to_string(), serde_json::Value::String(info.status));
            result.insert("running".to_string(), serde_json::Value::Bool(is_running));

            if let Some(image) = info.image {
                result.insert("image".to_string(), serde_json::Value::String(image));
            }

            result.insert(
                "ip_addresses".to_string(),
                serde_json::Value::Array(
                    info.ip_addresses
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn validate_instance_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Instance name cannot be empty",
        ));
    }

    if name.len() > 63 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Instance name too long (max 63 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Instance name contains invalid characters (only [a-zA-Z0-9.-_] allowed)",
        ));
    }

    if name.starts_with('-') || name.starts_with('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Instance name cannot start with '-' or '.'",
        ));
    }

    Ok(())
}

fn validate_image_name(image: &str) -> Result<()> {
    if image.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name cannot be empty",
        ));
    }

    if image.len() > 256 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name too long (max 256 characters)",
        ));
    }

    Ok(())
}

fn incus_instance(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_instance_name(&params.name)?;

    let client = IncusClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Absent => {
            let was_running = client.is_running(&params.name)?;
            if client.delete_instance(&params.name, params.force)? {
                diff("state: present".to_string(), "state: absent".to_string());
                output_messages.push(format!("Instance '{}' removed", params.name));
                changed = true;
            } else if was_running {
                output_messages.push(format!("Instance '{}' already absent", params.name));
            }
        }
        State::Present | State::Started => {
            let image = params.image.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "image is required for state 'present' or 'started'",
                )
            })?;
            validate_image_name(image)?;

            let exists = client.instance_exists(&params.name)?;
            let was_running = client.is_running(&params.name)?;

            if !exists {
                client.create_instance(&params)?;
                diff("state: absent".to_string(), "state: present".to_string());
                output_messages.push(format!(
                    "Instance '{}' created from image '{}'",
                    params.name, image
                ));
                changed = true;
            }

            if params.state == State::Started {
                if client.start_instance(&params.name, params.wait)? {
                    diff("state: stopped".to_string(), "state: started".to_string());
                    output_messages.push(format!("Instance '{}' started", params.name));
                    changed = true;
                } else if !was_running && !check_mode {
                    output_messages.push(format!("Instance '{}' already running", params.name));
                }
            } else if params.state == State::Present
                && was_running
                && client.stop_instance(&params.name, params.wait, params.force)?
            {
                diff("state: started".to_string(), "state: present".to_string());
                output_messages.push(format!("Instance '{}' stopped", params.name));
                changed = true;
            }
        }
        State::Stopped => {
            if client.instance_exists(&params.name)? {
                if client.stop_instance(&params.name, params.wait, params.force)? {
                    diff("state: started".to_string(), "state: stopped".to_string());
                    output_messages.push(format!("Instance '{}' stopped", params.name));
                    changed = true;
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Instance '{}' does not exist", params.name),
                ));
            }
        }
        State::Restarted => {
            if !client.instance_exists(&params.name)? {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Instance '{}' does not exist", params.name),
                ));
            }
            client.restart_instance(&params.name, params.wait)?;
            diff("state: running".to_string(), "state: restarted".to_string());
            output_messages.push(format!("Instance '{}' restarted", params.name));
            changed = true;
        }
    }

    let extra = client.get_instance_state(&params.name)?;

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
            name: webapp
            image: images:alpine/3.19
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webapp");
        assert_eq!(params.image, Some("images:alpine/3.19".to_string()));
        assert_eq!(params.state, State::Started);
        assert_eq!(params.instance_type, InstanceType::Container);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            image: images:alpine/3.19
            state: started
            type: container
            config:
              limits.memory: 512MB
              boot.autostart: true
            devices:
              root:
                path: /
                pool: default
                type: disk
            wait: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webapp");
        assert_eq!(params.image, Some("images:alpine/3.19".to_string()));
        assert_eq!(params.state, State::Started);
        assert_eq!(params.instance_type, InstanceType::Container);
        let config = params.config.unwrap();
        assert_eq!(
            config.get("limits.memory"),
            Some(&serde_json::Value::String("512MB".to_string()))
        );
        assert_eq!(
            config.get("boot.autostart"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(params.devices.is_some());
    }

    #[test]
    fn test_parse_params_virtual_machine() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vmapp
            image: images:ubuntu/22.04
            type: virtual-machine
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "vmapp");
        assert_eq!(params.instance_type, InstanceType::VirtualMachine);
    }

    #[test]
    fn test_parse_params_state_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            state: stopped
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_state_restarted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            state: restarted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Restarted);
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webapp
            image: images:alpine/3.19
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_instance_name() {
        assert!(validate_instance_name("webapp").is_ok());
        assert!(validate_instance_name("web-app").is_ok());
        assert!(validate_instance_name("web_app").is_ok());
        assert!(validate_instance_name("web.app").is_ok());
        assert!(validate_instance_name("webapp123").is_ok());
        assert!(validate_instance_name("WebApp").is_ok());

        assert!(validate_instance_name("").is_err());
        assert!(validate_instance_name(&"a".repeat(64)).is_err());
        assert!(validate_instance_name("-webapp").is_err());
        assert!(validate_instance_name(".webapp").is_err());
        assert!(validate_instance_name("web app").is_err());
        assert!(validate_instance_name("web/app").is_err());
    }

    #[test]
    fn test_validate_image_name() {
        assert!(validate_image_name("images:alpine/3.19").is_ok());
        assert!(validate_image_name("ubuntu:22.04").is_ok());
        assert!(validate_image_name("custom-image").is_ok());
        assert!(validate_image_name("registry.example.com/namespace/image:tag").is_ok());

        assert!(validate_image_name("").is_err());
        assert!(validate_image_name(&"a".repeat(257)).is_err());
    }
}
