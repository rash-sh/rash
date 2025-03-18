/// ANCHOR: module
/// # systemd
///
/// Control systemd services.
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
/// - name: Start service httpd
///   systemd:
///     name: httpd
///     state: started
///
/// - name: Stop service httpd
///   systemd:
///     name: httpd
///     state: stopped
///
/// - name: Restart service httpd
///   systemd:
///     name: httpd
///     state: restarted
///
/// - name: Reload service httpd
///   systemd:
///     name: httpd
///     state: reloaded
///
/// - name: Enable service httpd and ensure it is started
///   systemd:
///     name: httpd
///     enabled: true
///     state: started
///
/// - name: Enable service httpd on boot
///   systemd:
///     name: httpd
///     enabled: true
///
/// - name: Reload systemd daemon
///   systemd:
///     daemon_reload: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;

use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_yaml::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

/// State options for systemd services
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    /// Reload the service configuration without restarting
    Reloaded,
    /// Restart the service (stop then start)
    Restarted,
    /// Start the service if it's not running
    Started,
    /// Stop the service if it's running
    Stopped,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Scope {
    System,
    User,
    Global,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the service to manage.
    name: Option<String>,
    /// Whether the service should be enabled, disabled, or neither.
    enabled: Option<bool>,
    /// Whether to override existing symlinks.
    force: Option<bool>,
    /// Whether the unit should be masked or not. A masked unit is impossible to start.
    /// if set, requires `name`.
    masked: Option<bool>,
    /// Run daemon-reexec before doing any other operations, to make sure systemd has read any changes.
    #[serde(default = "default_false")]
    daemon_reexec: Option<bool>,
    /// Run daemon-reload before doing any other operations, to make sure systemd has read any changes.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    daemon_reload: Option<bool>,
    /// State of the service.
    state: Option<State>,
    /// Run systemctl within a given service manager scope, either as the default system scope system, the current user’s scope user, or the scope of all users global.
    /// For systemd to work with user, the executing user must have its own instance of dbus started and accessible (systemd requirement).
    /// The user dbus process is normally started during normal login, but not during the run of Ansible tasks. Otherwise you will probably get a ‘Failed to connect to bus: no such file or directory’ error.
    /// The user must have access, normally given via setting the XDG_RUNTIME_DIR variable, see the example below.
    scope: Option<Scope>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: None,
            enabled: None, // Fixed: Match production behavior where enabled is None by default
            daemon_reload: Some(false),
            state: None,
            scope: None,
            force: None,
            masked: None,
            daemon_reexec: Some(false), // Fixed: Match the default_false behavior
        }
    }
}

#[derive(Debug)]
pub struct Systemd;

impl Module for Systemd {
    fn get_name(&self) -> &str {
        "systemd"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        vars: Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        Ok((systemd(parse_params(optional_params)?, check_mode)?, vars))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct SystemdClient {
    check_mode: bool,
    scope: Option<Scope>,
}

impl SystemdClient {
    pub fn new(scope: Option<Scope>, check_mode: bool) -> Self {
        SystemdClient { check_mode, scope }
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new("systemctl");
        if let Some(scope) = &self.scope {
            match scope {
                Scope::User => cmd.arg("--user"),
                Scope::System => cmd.arg("--system"),
                Scope::Global => cmd.arg("--global"),
            };
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing systemctl: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn daemon_reload(&self) -> Result<bool> {
        if self.check_mode {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("daemon-reload");
        self.exec_cmd(&mut cmd, true)?;
        // daemon-reload is a refresh operation, not a state change
        // so we don't report it as "changed" unless there's an error
        Ok(false)
    }

    pub fn is_active(&self, service: &str) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.args(["is-active", service]);

        let output = self.exec_cmd(&mut cmd, false)?;
        Ok(output.status.success())
    }

    pub fn is_enabled(&self, service: &str) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.args(["is-enabled", service]);

        let output = self.exec_cmd(&mut cmd, false)?;
        Ok(output.status.success())
    }

    pub fn start(&self, service: &str) -> Result<SystemdResult> {
        let is_currently_active = self.is_active(service)?;

        if is_currently_active {
            return Ok(SystemdResult::no_change());
        }

        self.execute_command_with_output(&["start", service])
    }

    pub fn stop(&self, service: &str) -> Result<SystemdResult> {
        let is_currently_active = self.is_active(service)?;

        if !is_currently_active {
            return Ok(SystemdResult::no_change());
        }

        self.execute_command_with_output(&["stop", service])
    }

    pub fn restart(&self, service: &str) -> Result<SystemdResult> {
        self.execute_command_with_output(&["restart", service])
    }

    pub fn reload(&self, service: &str) -> Result<SystemdResult> {
        self.execute_command_with_output(&["reload", service])
    }

    pub fn enable(&self, service: &str) -> Result<SystemdResult> {
        let is_currently_enabled = self.is_enabled(service)?;

        if is_currently_enabled {
            return Ok(SystemdResult::no_change());
        }

        self.execute_command_with_output(&["enable", service])
    }

    pub fn disable(&self, service: &str) -> Result<SystemdResult> {
        let is_currently_enabled = self.is_enabled(service)?;

        if !is_currently_enabled {
            return Ok(SystemdResult::no_change());
        }

        self.execute_command_with_output(&["disable", service])
    }

    /// Helper method to execute a systemctl command and process its output
    fn execute_command_with_output(&self, args: &[&str]) -> Result<SystemdResult> {
        if self.check_mode {
            return Ok(SystemdResult::new(true, None));
        }

        let mut cmd = self.get_cmd();
        cmd.args(args);
        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(SystemdResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct SystemdResult {
    changed: bool,
    output: Option<String>,
}

impl SystemdResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        SystemdResult { changed, output }
    }

    fn no_change() -> Self {
        SystemdResult {
            changed: false,
            output: None,
        }
    }
}

/// Validates a service name to ensure it's safe to use with systemctl
fn validate_service_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name cannot be empty",
        ));
    }

    if name.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name too long (max 255 characters)",
        ));
    }

    // Check for path separators and other potentially dangerous characters
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name contains invalid characters",
        ));
    }

    // Check for control characters
    if name.chars().any(|c| c.is_control()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name contains control characters",
        ));
    }

    Ok(())
}

fn systemd(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.name.is_none() && !params.daemon_reload.unwrap_or(false) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Either name or daemon_reload is required",
        ));
    }

    let client = SystemdClient::new(params.scope, check_mode);

    let mut changed = false;
    let mut output_messages = Vec::new();

    // Handle daemon-reload first
    if params.daemon_reload.unwrap_or(false) {
        changed |= client.daemon_reload()?;
    }

    // Skip service operations if no name is provided
    let service_name = match params.name {
        Some(ref name) => {
            validate_service_name(name)?; // Add validation
            name
        }
        None => {
            return Ok(ModuleResult {
                changed,
                output: None,
                extra: None,
            });
        }
    };

    // Validate the service name
    validate_service_name(service_name)?;

    // Handle enabled state
    if let Some(should_be_enabled) = params.enabled {
        if should_be_enabled {
            let enable_result = client.enable(service_name)?;
            if enable_result.changed {
                diff(
                    "enabled: false -> true".to_string(),
                    "enabled: true".to_string(),
                );
                if let Some(output) = enable_result.output {
                    output_messages.push(output);
                }
            }
            changed |= enable_result.changed;
        } else {
            let disable_result = client.disable(service_name)?;
            if disable_result.changed {
                diff(
                    "enabled: true -> false".to_string(),
                    "enabled: false".to_string(),
                );
                if let Some(output) = disable_result.output {
                    output_messages.push(output);
                }
            }
            changed |= disable_result.changed;
        }
    }

    // Handle service state
    match params.state {
        Some(State::Started) => {
            let start_result = client.start(service_name)?;
            if start_result.changed {
                diff(
                    "state: stopped -> started".to_string(),
                    "state: started".to_string(),
                );
                if let Some(output) = start_result.output {
                    output_messages.push(output);
                }
            }
            changed |= start_result.changed;
        }
        Some(State::Stopped) => {
            let stop_result = client.stop(service_name)?;
            if stop_result.changed {
                diff(
                    "state: started -> stopped".to_string(),
                    "state: stopped".to_string(),
                );
                if let Some(output) = stop_result.output {
                    output_messages.push(output);
                }
            }
            changed |= stop_result.changed;
        }
        Some(State::Restarted) => {
            let restart_result = client.restart(service_name)?;
            if restart_result.changed {
                diff(
                    "state: restarted".to_string(),
                    "state: restarted".to_string(),
                );
                if let Some(output) = restart_result.output {
                    output_messages.push(output);
                }
            }
            changed |= restart_result.changed;
        }
        Some(State::Reloaded) => {
            let reload_result = client.reload(service_name)?;
            if reload_result.changed {
                diff("state: reloaded".to_string(), "state: reloaded".to_string());
                if let Some(output) = reload_result.output {
                    output_messages.push(output);
                }
            }
            changed |= reload_result.changed;
        }
        None => {}
    }

    // Build extra info
    let mut extra = serde_json::Map::new();
    if let Some(name) = &params.name {
        let is_active = client.is_active(name)?;
        let is_enabled = client.is_enabled(name)?;

        extra.insert("name".to_string(), serde_json::Value::String(name.clone()));
        extra.insert("active".to_string(), serde_json::Value::Bool(is_active));
        extra.insert("enabled".to_string(), serde_json::Value::Bool(is_enabled));
    }

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
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            name: httpd
            state: started
            enabled: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: Some("httpd".to_owned()),
                state: Some(State::Started),
                enabled: Some(true),
                scope: None,
                force: None,
                masked: None,
                daemon_reexec: Some(false),
                daemon_reload: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_daemon_reload() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            daemon_reload: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: None,
                state: None,
                enabled: None,
                scope: None,
                force: None,
                masked: None,
                daemon_reexec: Some(false),
                daemon_reload: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            name: httpd
            state: started
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_service_name() {
        // Valid names
        assert!(validate_service_name("httpd").is_ok());
        assert!(validate_service_name("my-service").is_ok());
        assert!(validate_service_name("another.service").is_ok());

        // Invalid names
        assert!(validate_service_name("").is_err());
        assert!(validate_service_name("a".repeat(256).as_str()).is_err());
        assert!(validate_service_name("invalid/name").is_err());
        assert!(validate_service_name("invalid\\name").is_err());
        assert!(validate_service_name("invalid\0name").is_err());
        assert!(validate_service_name("invalid\x1Fname").is_err());
    }
}
