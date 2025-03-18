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
use crate::modules::{parse_params, Module, ModuleResult};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_yaml::{value, Value as YamlValue};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Reloaded,
    Restarted,
    Started,
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
    /// **[default: `false`]**
    #[serde(default = "default_false")]
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
            enabled: Some(false),
            daemon_reload: Some(false),
            state: None,
            scope: None,
            force: None,
            masked: None,
            daemon_reexec: None,
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
    fn get_json_schema(&self) -> Option<RootSchema> {
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
            cmd.arg(format!("--{}", scope));
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        // Don't execute if in check mode
        if self.check_mode {
            trace!("Check mode - would have run: {:?}", cmd);
            return Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }

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
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("daemon-reload");
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
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

    pub fn start(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(!self.is_active(service)?);
        }

        if self.is_active(service)? {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["start", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn stop(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return self.is_active(service);
        }

        if !self.is_active(service)? {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["stop", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn restart(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["restart", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn reload(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["reload", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn enable(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(!self.is_enabled(service)?);
        }

        if self.is_enabled(service)? {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["enable", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    pub fn disable(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return self.is_enabled(service);
        }

        if !self.is_enabled(service)? {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.args(["disable", service]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
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

    // Handle daemon-reload first
    if params.daemon_reload.unwrap_or(false) {
        changed |= client.daemon_reload()?;
    }

    // Skip service operations if no name is provided
    let service_name = match params.name {
        Some(ref name) => name,
        None => {
            return Ok(ModuleResult {
                changed,
                output: None,
                extra: None,
            })
        }
    };

    // Handle enabled state
    if let Some(should_be_enabled) = params.enabled {
        if should_be_enabled {
            changed |= client.enable(service_name)?;
        } else {
            changed |= client.disable(service_name)?;
        }
    }

    // Handle service state
    match params.state {
        Some(State::Started) => changed |= client.start(service_name)?,
        Some(State::Stopped) => changed |= client.stop(service_name)?,
        Some(State::Restarted) => changed |= client.restart(service_name)?,
        Some(State::Reloaded) => changed |= client.reload(service_name)?,
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

    Ok(ModuleResult {
        changed,
        output: None,
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
                enabled: Some(false),
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
}
