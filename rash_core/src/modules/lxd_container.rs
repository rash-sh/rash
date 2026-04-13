/// ANCHOR: module
/// # lxd_container
///
/// Manage LXD containers.
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
/// - name: Create and start a container
///   lxd_container:
///     name: webserver
///     state: started
///     source:
///       type: image
///       alias: ubuntu/22.04
///
/// - name: Create container with custom config
///   lxd_container:
///     name: myapp
///     state: started
///     source:
///       type: image
///       alias: alpine/3.18
///     config:
///       limits.cpu: "2"
///       limits.memory: 2GB
///
/// - name: Create container with profiles
///   lxd_container:
///     name: profiled
///     state: started
///     source:
///       type: image
///       alias: ubuntu/22.04
///     profiles:
///       - default
///       - custom-profile
///
/// - name: Create container with devices
///   lxd_container:
///     name: devcontainer
///     state: started
///     source:
///       type: image
///       alias: ubuntu/22.04
///     devices:
///       eth0:
///         type: nic
///         nictype: bridged
///         parent: lxdbr0
///
/// - name: Stop a container
///   lxd_container:
///     name: webserver
///     state: stopped
///
/// - name: Freeze a container
///   lxd_container:
///     name: webserver
///     state: frozen
///
/// - name: Delete a container
///   lxd_container:
///     name: webserver
///     state: absent
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
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Present,
    Started,
    Stopped,
    Frozen,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct Source {
    /// Type of source (image, migration, copy, none).
    #[serde(default = "default_source_type")]
    source_type: String,
    /// Image alias or fingerprint.
    alias: Option<String>,
    /// Image server URL.
    server: Option<String>,
    /// Image protocol (simplestreams, lxd, dir).
    protocol: Option<String>,
    /// Image secret for private images.
    secret: Option<String>,
    /// Certificate fingerprint for remote servers.
    certificate: Option<String>,
}

fn default_source_type() -> String {
    "image".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the container.
    name: String,
    /// State of the container.
    #[serde(default = "default_state")]
    state: State,
    /// Image source configuration.
    source: Option<Source>,
    /// Container configuration key-value pairs.
    config: Option<HashMap<String, String>>,
    /// Device configuration.
    devices: Option<HashMap<String, HashMap<String, String>>>,
    /// Profiles to apply to the container.
    profiles: Option<Vec<String>>,
    /// Force operation (for stop/delete).
    #[serde(default)]
    force: bool,
    /// Wait for operation to complete.
    #[serde(default = "default_true")]
    wait: bool,
    /// Timeout for operations (seconds).
    timeout: Option<u32>,
    /// Target remote LXD server.
    target: Option<String>,
    /// Project name.
    project: Option<String>,
}

fn default_state() -> State {
    State::Started
}

#[derive(Debug)]
pub struct LxdContainer;

#[derive(Debug, Clone)]
struct ContainerInfo {
    name: String,
    status: String,
    state: String,
}

impl Module for LxdContainer {
    fn get_name(&self) -> &str {
        "lxd_container"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            lxd_container(parse_params(optional_params)?, check_mode)?,
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

struct LxdClient {
    check_mode: bool,
    target: Option<String>,
    project: Option<String>,
}

impl LxdClient {
    fn new(check_mode: bool, target: Option<String>, project: Option<String>) -> Self {
        LxdClient {
            check_mode,
            target,
            project,
        }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let mut full_args: Vec<String> = Vec::new();

        if let Some(ref target) = self.target {
            full_args.push(target.clone());
        }

        for arg in args {
            full_args.push(arg.to_string());
        }

        if let Some(ref project) = self.project {
            full_args.push("--project".to_string());
            full_args.push(project.clone());
        }

        let output = Command::new("lxc")
            .args(&full_args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `lxc {:?}`", full_args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing lxc: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn container_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(&["list", "--format", "csv", "--columns", "n", name], false)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    fn get_container_info(&self, name: &str) -> Result<Option<ContainerInfo>> {
        let output = self.exec_cmd(
            &["list", "--format", "json", "--columns", "ns", name],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        for container in containers {
            if let Some(container_name) = container.get("name").and_then(|n| n.as_str())
                && container_name == name
            {
                let status = container
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let state = match status.as_str() {
                    "Running" => "started",
                    "Stopped" => "stopped",
                    "Frozen" => "frozen",
                    _ => "unknown",
                }
                .to_string();

                return Ok(Some(ContainerInfo {
                    name: container_name.to_string(),
                    status,
                    state,
                }));
            }
        }

        Ok(None)
    }

    fn is_running(&self, name: &str) -> Result<bool> {
        let info = self.get_container_info(name)?;
        Ok(info.is_some_and(|i| i.status == "Running"))
    }

    fn is_stopped(&self, name: &str) -> Result<bool> {
        let info = self.get_container_info(name)?;
        Ok(info.is_some_and(|i| i.status == "Stopped"))
    }

    fn is_frozen(&self, name: &str) -> Result<bool> {
        let info = self.get_container_info(name)?;
        Ok(info.is_some_and(|i| i.status == "Frozen"))
    }

    fn launch_container(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let source = params.source.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "source is required when creating a container",
            )
        })?;

        let mut args: Vec<String> = vec!["launch".to_string()];

        let image_ref = match (&source.alias, &source.server) {
            (Some(alias), Some(server)) => format!("{}:{}", server, alias),
            (Some(alias), None) => alias.clone(),
            (None, Some(server)) => format!("{}:", server),
            (None, None) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "source.alias or source.server is required",
                ));
            }
        };

        if let Some(ref protocol) = source.protocol {
            args.push("--protocol".to_string());
            args.push(protocol.clone());
        }

        if let Some(ref secret) = source.secret {
            args.push("--secret".to_string());
            args.push(secret.clone());
        }

        if let Some(ref certificate) = source.certificate {
            args.push("--certificate".to_string());
            args.push(certificate.clone());
        }

        args.push(image_ref);
        args.push(params.name.clone());

        if let Some(ref profiles) = params.profiles {
            if profiles.is_empty() {
                args.push("--no-profiles".to_string());
            } else {
                for profile in profiles {
                    args.push("--profile".to_string());
                    args.push(profile.clone());
                }
            }
        }

        if let Some(ref config) = params.config {
            for (key, value) in config {
                args.push("--config".to_string());
                args.push(format!("{}={}", key, value));
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

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn start_container(&self, name: &str, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_running(name)? {
            return Ok(false);
        }

        let mut args: Vec<String> = vec!["start".to_string(), name.to_string()];

        if params.wait {
            args.push("--force".to_string());
        }

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_cmd(&args_refs, true)?;
        Ok(true)
    }

    fn stop_container(&self, name: &str, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_stopped(name)? || self.is_frozen(name)? && !params.force {
            if self.is_frozen(name)? {
                self.exec_cmd(&["unfreeze", name], true)?;
            }
            if self.is_stopped(name)? {
                return Ok(false);
            }
        }

        let mut args: Vec<String> = vec!["stop".to_string(), name.to_string()];

        if params.force {
            args.push("--force".to_string());
        }

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_cmd(&args_refs, true)?;
        Ok(true)
    }

    fn freeze_container(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_frozen(name)? {
            return Ok(false);
        }

        if !self.is_running(name)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Container '{}' must be running to freeze", name),
            ));
        }

        self.exec_cmd(&["freeze", name], true)?;
        Ok(true)
    }

    fn unfreeze_container(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_frozen(name)? {
            return Ok(false);
        }

        self.exec_cmd(&["unfreeze", name], true)?;
        Ok(true)
    }

    fn delete_container(&self, name: &str, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.container_exists(name)? {
            return Ok(false);
        }

        let mut args: Vec<String> = vec!["delete".to_string(), name.to_string()];

        if params.force {
            args.push("--force".to_string());
        }

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_cmd(&args_refs, true)?;
        Ok(true)
    }

    fn set_config(&self, name: &str, key: &str, value: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        self.exec_cmd(&["config", "set", name, key, value], true)?;
        Ok(())
    }

    fn get_container_state(
        &self,
        name: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_container_info(name)? {
            let is_running = info.status == "Running";
            let is_frozen = info.status == "Frozen";
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("status".to_string(), serde_json::Value::String(info.status));
            result.insert("state".to_string(), serde_json::Value::String(info.state));
            result.insert("running".to_string(), serde_json::Value::Bool(is_running));
            result.insert("frozen".to_string(), serde_json::Value::Bool(is_frozen));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name cannot be empty",
        ));
    }

    if name.len() > 63 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name too long (max 63 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name contains invalid characters (only [a-zA-Z0-9-_] allowed)",
        ));
    }

    if name.starts_with('-') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name cannot start with '-'",
        ));
    }

    Ok(())
}

fn lxd_container(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_container_name(&params.name)?;

    let client = LxdClient::new(check_mode, params.target.clone(), params.project.clone());
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Absent => {
            if client.delete_container(&params.name, &params)? {
                diff("state: present".to_string(), "state: absent".to_string());
                output_messages.push(format!("Container '{}' deleted", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Container '{}' already absent", params.name));
            }
        }
        State::Present | State::Started => {
            let exists = client.container_exists(&params.name)?;
            let was_running = client.is_running(&params.name)?;
            let was_frozen = client.is_frozen(&params.name)?;

            if !exists {
                client.launch_container(&params)?;
                diff("state: absent".to_string(), "state: present".to_string());
                output_messages.push(format!("Container '{}' created and started", params.name));
                changed = true;
            } else if params.state == State::Started {
                if was_frozen {
                    client.unfreeze_container(&params.name)?;
                    diff("state: frozen".to_string(), "state: started".to_string());
                    output_messages.push(format!("Container '{}' unfrozen", params.name));
                    changed = true;
                } else if !was_running {
                    client.start_container(&params.name, &params)?;
                    diff("state: stopped".to_string(), "state: started".to_string());
                    output_messages.push(format!("Container '{}' started", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!("Container '{}' already running", params.name));
                }
            } else if params.state == State::Present && (was_running || was_frozen) {
                if was_frozen {
                    client.unfreeze_container(&params.name)?;
                }
                if was_running && client.stop_container(&params.name, &params)? {
                    diff("state: started".to_string(), "state: present".to_string());
                    output_messages.push(format!("Container '{}' stopped", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!("Container '{}' exists", params.name));
                }
            } else {
                output_messages.push(format!("Container '{}' exists", params.name));
            }

            if let Some(ref config) = params.config
                && !check_mode
            {
                for (key, value) in config {
                    client.set_config(&params.name, key, value)?;
                }
            }
        }
        State::Stopped => {
            if client.container_exists(&params.name)? {
                if client.stop_container(&params.name, &params)? {
                    diff("state: started".to_string(), "state: stopped".to_string());
                    output_messages.push(format!("Container '{}' stopped", params.name));
                    changed = true;
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Container '{}' does not exist", params.name),
                ));
            }
        }
        State::Frozen => {
            if client.container_exists(&params.name)? {
                if client.freeze_container(&params.name)? {
                    diff("state: started".to_string(), "state: frozen".to_string());
                    output_messages.push(format!("Container '{}' frozen", params.name));
                    changed = true;
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Container '{}' does not exist", params.name),
                ));
            }
        }
    }

    let extra = client.get_container_state(&params.name)?;

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
            name: webserver
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webserver");
        assert_eq!(params.state, State::Started);
    }

    #[test]
    fn test_parse_params_with_source() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            source:
              type: image
              alias: ubuntu/22.04
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webserver");
        assert_eq!(params.state, State::Started);
        let source = params.source.unwrap();
        assert_eq!(source.source_type, "image");
        assert_eq!(source.alias, Some("ubuntu/22.04".to_string()));
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: started
            source:
              type: image
              alias: ubuntu/22.04
              server: https://images.linuxcontainers.org
            config:
              limits.cpu: "2"
              limits.memory: 2GB
            profiles:
              - default
              - custom
            devices:
              eth0:
                type: nic
                nictype: bridged
                parent: lxdbr0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "webserver");
        assert_eq!(params.state, State::Started);
        let source = params.source.unwrap();
        assert_eq!(source.alias, Some("ubuntu/22.04".to_string()));
        assert_eq!(
            source.server,
            Some("https://images.linuxcontainers.org".to_string())
        );
        let config = params.config.unwrap();
        assert_eq!(config.get("limits.cpu").unwrap(), "2");
        assert_eq!(config.get("limits.memory").unwrap(), "2GB");
        assert_eq!(
            params.profiles,
            Some(vec!["default".to_string(), "custom".to_string()])
        );
    }

    #[test]
    fn test_parse_params_state_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: stopped
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
    }

    #[test]
    fn test_parse_params_state_frozen() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: frozen
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Frozen);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_force_and_wait() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            state: stopped
            force: true
            wait: false
            timeout: 30
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
        assert!(!params.wait);
        assert_eq!(params.timeout, Some(30));
    }

    #[test]
    fn test_parse_params_target_and_project() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            target: remote-server
            project: myproject
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.target, Some("remote-server".to_string()));
        assert_eq!(params.project, Some("myproject".to_string()));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: webserver
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_container_name() {
        assert!(validate_container_name("webserver").is_ok());
        assert!(validate_container_name("web-server").is_ok());
        assert!(validate_container_name("web_server").is_ok());
        assert!(validate_container_name("webserver123").is_ok());
        assert!(validate_container_name("WebServer").is_ok());

        assert!(validate_container_name("").is_err());
        assert!(validate_container_name(&"a".repeat(64)).is_err());
        assert!(validate_container_name("-webserver").is_err());
        assert!(validate_container_name("web server").is_err());
        assert!(validate_container_name("web.server").is_err());
    }
}
