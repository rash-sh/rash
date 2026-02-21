/// ANCHOR: module
/// # service
///
/// Manage services on target hosts. This module is a wrapper for service
/// management on different init systems (systemd, sysvinit, openrc).
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
///   service:
///     name: httpd
///     state: started
///
/// - name: Stop service httpd
///   service:
///     name: httpd
///     state: stopped
///
/// - name: Restart service httpd
///   service:
///     name: httpd
///     state: restarted
///
/// - name: Reload service httpd
///   service:
///     name: httpd
///     state: reloaded
///
/// - name: Enable service httpd and ensure it is started
///   service:
///     name: httpd
///     enabled: true
///     state: started
///
/// - name: Enable service httpd on boot
///   service:
///     name: httpd
///     enabled: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;

use std::path::Path;
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum ServiceManager {
    Systemd,
    Openrc,
    Sysvinit,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct Params {
    /// Name of the service to manage.
    name: String,
    /// Whether the service should be enabled, disabled, or neither.
    enabled: Option<bool>,
    /// State of the service.
    state: Option<State>,
    /// The service manager to use. If not specified, it will be auto-detected.
    #[serde(rename = "use")]
    service_manager: Option<ServiceManager>,
}

#[derive(Debug)]
pub struct Service;

impl Module for Service {
    fn get_name(&self) -> &str {
        "service"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((service(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct ServiceResult {
    changed: bool,
    output: Option<String>,
}

impl ServiceResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        ServiceResult { changed, output }
    }

    fn no_change() -> Self {
        ServiceResult {
            changed: false,
            output: None,
        }
    }
}

trait ServiceClient {
    fn is_active(&self, service: &str) -> Result<bool>;
    fn is_enabled(&self, service: &str) -> Result<bool>;
    fn start(&self, service: &str) -> Result<ServiceResult>;
    fn stop(&self, service: &str) -> Result<ServiceResult>;
    fn restart(&self, service: &str) -> Result<ServiceResult>;
    fn reload(&self, service: &str) -> Result<ServiceResult>;
    fn enable(&self, service: &str) -> Result<ServiceResult>;
    fn disable(&self, service: &str) -> Result<ServiceResult>;
}

struct SystemdClient {
    check_mode: bool,
}

impl SystemdClient {
    fn new(check_mode: bool) -> Self {
        SystemdClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("systemctl")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `systemctl {:?}`", args);
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

    fn execute_command_with_output(&self, args: &[&str]) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        let output = self.exec_cmd(args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }
}

impl ServiceClient for SystemdClient {
    fn is_active(&self, service: &str) -> Result<bool> {
        let output = self.exec_cmd(&["is-active", service], false)?;
        Ok(output.status.success())
    }

    fn is_enabled(&self, service: &str) -> Result<bool> {
        let output = self.exec_cmd(&["is-enabled", service], false)?;
        Ok(output.status.success())
    }

    fn start(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&["start", service])
    }

    fn stop(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if !is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&["stop", service])
    }

    fn restart(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(&["restart", service])
    }

    fn reload(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(&["reload", service])
    }

    fn enable(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_enabled = self.is_enabled(service)?;
        if is_currently_enabled {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&["enable", service])
    }

    fn disable(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_enabled = self.is_enabled(service)?;
        if !is_currently_enabled {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&["disable", service])
    }
}

struct SysvinitClient {
    check_mode: bool,
}

impl SysvinitClient {
    fn new(check_mode: bool) -> Self {
        SysvinitClient { check_mode }
    }

    fn service_path(service: &str) -> String {
        format!("/etc/init.d/{}", service)
    }

    fn exec_cmd(&self, service: &str, action: &str, check_success: bool) -> Result<Output> {
        let service_path = Self::service_path(service);
        let output = Command::new(&service_path)
            .arg(action)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{} {}`", service_path, action);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing {}: {}",
                    service_path,
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn execute_command_with_output(&self, service: &str, action: &str) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        let output = self.exec_cmd(service, action, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }
}

impl ServiceClient for SysvinitClient {
    fn is_active(&self, service: &str) -> Result<bool> {
        let output = self.exec_cmd(service, "status", false)?;
        Ok(output.status.success())
    }

    fn is_enabled(&self, service: &str) -> Result<bool> {
        for rc_dir in &["/etc/rc2.d", "/etc/rc3.d", "/etc/rc5.d"] {
            let rc_path = Path::new(*rc_dir);
            if rc_path.exists()
                && let Ok(entries) = std::fs::read_dir(rc_path)
            {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string()
                        && name.starts_with('S')
                        && name.contains(service)
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn start(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(service, "start")
    }

    fn stop(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if !is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(service, "stop")
    }

    fn restart(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(service, "restart")
    }

    fn reload(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(service, "reload")
    }

    fn enable(&self, service: &str) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        if self.is_enabled(service)? {
            return Ok(ServiceResult::no_change());
        }

        let output = Command::new("update-rc.d")
            .args([service, "defaults"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error enabling service: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }

    fn disable(&self, service: &str) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        if !self.is_enabled(service)? {
            return Ok(ServiceResult::no_change());
        }

        let output = Command::new("update-rc.d")
            .args(["-f", service, "remove"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error disabling service: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }
}

struct OpenRcClient {
    check_mode: bool,
}

impl OpenRcClient {
    fn new(check_mode: bool) -> Self {
        OpenRcClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
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

    fn execute_command_with_output(&self, args: &[&str]) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        let output = self.exec_cmd(args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }

    fn rc_update(&self, args: &[&str]) -> Result<ServiceResult> {
        if self.check_mode {
            return Ok(ServiceResult::new(true, None));
        }

        let output = Command::new("rc-update")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing rc-update: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(ServiceResult::new(true, output_str))
    }
}

impl ServiceClient for OpenRcClient {
    fn is_active(&self, service: &str) -> Result<bool> {
        let output = self.exec_cmd(&[service, "status"], false)?;
        Ok(output.status.success())
    }

    fn is_enabled(&self, service: &str) -> Result<bool> {
        let output = Command::new("rc-update")
            .args(["show", "default"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with(service) || line.contains(&format!(" | {}", service)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn start(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&[service, "start"])
    }

    fn stop(&self, service: &str) -> Result<ServiceResult> {
        let is_currently_active = self.is_active(service)?;
        if !is_currently_active {
            return Ok(ServiceResult::no_change());
        }
        self.execute_command_with_output(&[service, "stop"])
    }

    fn restart(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(&[service, "restart"])
    }

    fn reload(&self, service: &str) -> Result<ServiceResult> {
        self.execute_command_with_output(&[service, "reload"])
    }

    fn enable(&self, service: &str) -> Result<ServiceResult> {
        if self.is_enabled(service)? {
            return Ok(ServiceResult::no_change());
        }
        self.rc_update(&["add", service, "default"])
    }

    fn disable(&self, service: &str) -> Result<ServiceResult> {
        if !self.is_enabled(service)? {
            return Ok(ServiceResult::no_change());
        }
        self.rc_update(&["delete", service, "default"])
    }
}

fn detect_service_manager() -> Result<ServiceManager> {
    if Path::new("/run/systemd/system").exists() {
        return Ok(ServiceManager::Systemd);
    }

    if Command::new("rc-service").arg("--version").output().is_ok() {
        let output = Command::new("rc-status").output();
        if output.is_ok() && output.unwrap().status.success() {
            return Ok(ServiceManager::Openrc);
        }
    }

    if Path::new("/etc/init.d").exists() {
        return Ok(ServiceManager::Sysvinit);
    }

    Err(Error::new(
        ErrorKind::InvalidData,
        "Could not detect service manager. Supported: systemd, openrc, sysvinit",
    ))
}

fn get_client(manager: &ServiceManager, check_mode: bool) -> Box<dyn ServiceClient> {
    match manager {
        ServiceManager::Systemd => Box::new(SystemdClient::new(check_mode)),
        ServiceManager::Openrc => Box::new(OpenRcClient::new(check_mode)),
        ServiceManager::Sysvinit => Box::new(SysvinitClient::new(check_mode)),
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

fn service(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_service_name(&params.name)?;

    let manager = match params.service_manager {
        Some(ref m) => m.clone(),
        None => detect_service_manager()?,
    };

    let client = get_client(&manager, check_mode);

    let mut changed = false;
    let mut output_messages = Vec::new();

    if let Some(should_be_enabled) = params.enabled {
        if should_be_enabled {
            let enable_result = client.enable(&params.name)?;
            if enable_result.changed {
                diff("enabled: false".to_string(), "enabled: true".to_string());
                if let Some(output) = enable_result.output {
                    output_messages.push(output);
                }
            }
            changed |= enable_result.changed;
        } else {
            let disable_result = client.disable(&params.name)?;
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
    let is_enabled = client.is_enabled(&params.name)?;

    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert("active".to_string(), serde_json::Value::Bool(is_active));
    extra.insert("enabled".to_string(), serde_json::Value::Bool(is_enabled));
    extra.insert(
        "service_manager".to_string(),
        serde_json::Value::String(format!("{:?}", manager).to_lowercase()),
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
                name: "httpd".to_owned(),
                state: Some(State::Started),
                enabled: Some(true),
                service_manager: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_use() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: httpd
            state: started
            use: systemd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "httpd".to_owned(),
                state: Some(State::Started),
                enabled: None,
                service_manager: Some(ServiceManager::Systemd),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
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
        assert!(validate_service_name("httpd").is_ok());
        assert!(validate_service_name("my-service").is_ok());
        assert!(validate_service_name("another.service").is_ok());

        assert!(validate_service_name("").is_err());
        assert!(validate_service_name("a".repeat(256).as_str()).is_err());
        assert!(validate_service_name("invalid/name").is_err());
        assert!(validate_service_name("invalid\\name").is_err());
        assert!(validate_service_name("invalid\0name").is_err());
        assert!(validate_service_name("invalid\x1Fname").is_err());
    }
}
