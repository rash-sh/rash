/// ANCHOR: module
/// # openrc
///
/// Control OpenRC services. This module is designed for Alpine Linux and
/// other OpenRC-based systems.
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
/// - name: Start service nginx
///   openrc:
///     name: nginx
///     state: started
///
/// - name: Stop service nginx
///   openrc:
///     name: nginx
///     state: stopped
///
/// - name: Restart service nginx
///   openrc:
///     name: nginx
///     state: restarted
///
/// - name: Reload service nginx
///   openrc:
///     name: nginx
///     state: reloaded
///
/// - name: Enable service nginx and ensure it is started
///   openrc:
///     name: nginx
///     enabled: true
///     state: started
///
/// - name: Enable service nginx at boot in default runlevel
///   openrc:
///     name: nginx
///     enabled: true
///
/// - name: Enable service nginx in boot runlevel
///   openrc:
///     name: nginx
///     enabled: true
///     runlevel: boot
///
/// - name: Disable service nginx at boot
///   openrc:
///     name: nginx
///     enabled: false
///
/// - name: Check if nginx is running
///   openrc:
///     name: nginx
///     state: started
///   check_mode: true
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Reloaded,
    Restarted,
    Started,
    Stopped,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Runlevel {
    #[default]
    Default,
    Boot,
    Sysinit,
    Shutdown,
    Single,
}

impl Runlevel {
    fn as_str(&self) -> &'static str {
        match self {
            Runlevel::Default => "default",
            Runlevel::Boot => "boot",
            Runlevel::Sysinit => "sysinit",
            Runlevel::Shutdown => "shutdown",
            Runlevel::Single => "single",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the service to manage.
    name: String,
    /// Whether the service should be enabled on boot.
    enabled: Option<bool>,
    /// State of the service.
    state: Option<State>,
    /// Runlevel for the service. **[default: `default`]**
    #[serde(default)]
    runlevel: Runlevel,
}

#[derive(Debug)]
pub struct OpenRc;

impl Module for OpenRc {
    fn get_name(&self) -> &str {
        "openrc"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((openrc(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct OpenRcClient {
    check_mode: bool,
}

struct OpenRcResult {
    changed: bool,
    output: Option<String>,
}

impl OpenRcResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        OpenRcResult { changed, output }
    }

    fn no_change() -> Self {
        OpenRcResult {
            changed: false,
            output: None,
        }
    }
}

impl OpenRcClient {
    fn new(check_mode: bool) -> Self {
        OpenRcClient { check_mode }
    }

    fn exec_rc_service(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("rc-service")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `rc-service {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing rc-service: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn exec_rc_update(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("rc-update")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `rc-update {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing rc-update: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn execute_service_command(&self, service: &str, action: &str) -> Result<OpenRcResult> {
        if self.check_mode {
            return Ok(OpenRcResult::new(true, None));
        }

        let output = self.exec_rc_service(&[service, action], true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let output_str = if stdout.trim().is_empty() && stderr.trim().is_empty() {
            None
        } else if !stdout.trim().is_empty() {
            Some(stdout.trim().to_string())
        } else {
            Some(stderr.trim().to_string())
        };
        Ok(OpenRcResult::new(true, output_str))
    }

    fn execute_rc_update(&self, args: &[&str]) -> Result<OpenRcResult> {
        if self.check_mode {
            return Ok(OpenRcResult::new(true, None));
        }

        let output = self.exec_rc_update(args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(OpenRcResult::new(true, output_str))
    }

    fn is_active(&self, service: &str) -> Result<bool> {
        let output = self.exec_rc_service(&[service, "status"], false)?;
        Ok(output.status.success())
    }

    fn is_enabled(&self, service: &str, runlevel: &Runlevel) -> Result<bool> {
        let output = self.exec_rc_update(&["show", runlevel.as_str()], false)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 2 {
                let svc = parts[0].trim();
                let rl = parts[1].trim();
                if svc == service && rl == runlevel.as_str() {
                    return Ok(true);
                }
            } else if line.trim().starts_with(service) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn start(&self, service: &str) -> Result<OpenRcResult> {
        let is_currently_active = self.is_active(service)?;
        if is_currently_active {
            return Ok(OpenRcResult::no_change());
        }
        self.execute_service_command(service, "start")
    }

    fn stop(&self, service: &str) -> Result<OpenRcResult> {
        let is_currently_active = self.is_active(service)?;
        if !is_currently_active {
            return Ok(OpenRcResult::no_change());
        }
        self.execute_service_command(service, "stop")
    }

    fn restart(&self, service: &str) -> Result<OpenRcResult> {
        self.execute_service_command(service, "restart")
    }

    fn reload(&self, service: &str) -> Result<OpenRcResult> {
        let is_currently_active = self.is_active(service)?;
        if !is_currently_active {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Service {} is not running, cannot reload", service),
            ));
        }
        self.execute_service_command(service, "reload")
    }

    fn enable(&self, service: &str, runlevel: &Runlevel) -> Result<OpenRcResult> {
        if self.is_enabled(service, runlevel)? {
            return Ok(OpenRcResult::no_change());
        }
        self.execute_rc_update(&["add", service, runlevel.as_str()])
    }

    fn disable(&self, service: &str, runlevel: &Runlevel) -> Result<OpenRcResult> {
        if !self.is_enabled(service, runlevel)? {
            return Ok(OpenRcResult::no_change());
        }
        self.execute_rc_update(&["delete", service, runlevel.as_str()])
    }
}

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

    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name contains invalid characters",
        ));
    }

    if name.chars().any(|c| c.is_control()) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Service name contains control characters",
        ));
    }

    Ok(())
}

fn openrc(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_service_name(&params.name)?;

    let client = OpenRcClient::new(check_mode);

    let mut changed = false;
    let mut output_messages = Vec::new();

    if let Some(should_be_enabled) = params.enabled {
        if should_be_enabled {
            let enable_result = client.enable(&params.name, &params.runlevel)?;
            if enable_result.changed {
                diff("enabled: false".to_string(), "enabled: true".to_string());
                if let Some(output) = enable_result.output {
                    output_messages.push(output);
                }
            }
            changed |= enable_result.changed;
        } else {
            let disable_result = client.disable(&params.name, &params.runlevel)?;
            if disable_result.changed {
                diff("enabled: true".to_string(), "enabled: false".to_string());
                if let Some(output) = disable_result.output {
                    output_messages.push(output);
                }
            }
            changed |= disable_result.changed;
        }
    }

    match params.state {
        Some(State::Started) => {
            let start_result = client.start(&params.name)?;
            if start_result.changed {
                diff("state: stopped".to_string(), "state: started".to_string());
                if let Some(output) = start_result.output {
                    output_messages.push(output);
                }
            }
            changed |= start_result.changed;
        }
        Some(State::Stopped) => {
            let stop_result = client.stop(&params.name)?;
            if stop_result.changed {
                diff("state: started".to_string(), "state: stopped".to_string());
                if let Some(output) = stop_result.output {
                    output_messages.push(output);
                }
            }
            changed |= stop_result.changed;
        }
        Some(State::Restarted) => {
            let restart_result = client.restart(&params.name)?;
            if restart_result.changed
                && let Some(output) = restart_result.output
            {
                output_messages.push(output);
            }
            changed |= restart_result.changed;
        }
        Some(State::Reloaded) => {
            let reload_result = client.reload(&params.name)?;
            if reload_result.changed
                && let Some(output) = reload_result.output
            {
                output_messages.push(output);
            }
            changed |= reload_result.changed;
        }
        None => {}
    }

    let mut extra = serde_json::Map::new();
    let is_active = client.is_active(&params.name)?;
    let is_enabled = client.is_enabled(&params.name, &params.runlevel)?;

    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert("active".to_string(), serde_json::Value::Bool(is_active));
    extra.insert("enabled".to_string(), serde_json::Value::Bool(is_enabled));
    extra.insert(
        "runlevel".to_string(),
        serde_json::Value::String(params.runlevel.as_str().to_string()),
    );

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
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: started
            enabled: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "nginx".to_owned(),
                state: Some(State::Started),
                enabled: Some(true),
                runlevel: Runlevel::Default,
            }
        );
    }

    #[test]
    fn test_parse_params_with_runlevel() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            enabled: true
            runlevel: boot
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "nginx".to_owned(),
                state: None,
                enabled: Some(true),
                runlevel: Runlevel::Boot,
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
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
        assert!(validate_service_name("nginx").is_ok());
        assert!(validate_service_name("my-service").is_ok());
        assert!(validate_service_name("sshd").is_ok());

        assert!(validate_service_name("").is_err());
        assert!(validate_service_name("a".repeat(256).as_str()).is_err());
        assert!(validate_service_name("invalid/name").is_err());
        assert!(validate_service_name("invalid\\name").is_err());
        assert!(validate_service_name("invalid\0name").is_err());
        assert!(validate_service_name("invalid\x1Fname").is_err());
    }

    #[test]
    fn test_runlevel_as_str() {
        assert_eq!(Runlevel::Default.as_str(), "default");
        assert_eq!(Runlevel::Boot.as_str(), "boot");
        assert_eq!(Runlevel::Sysinit.as_str(), "sysinit");
        assert_eq!(Runlevel::Shutdown.as_str(), "shutdown");
        assert_eq!(Runlevel::Single.as_str(), "single");
    }
}
