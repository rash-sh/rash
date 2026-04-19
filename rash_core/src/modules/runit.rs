/// ANCHOR: module
/// # runit
///
/// Manage Runit services.
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
/// - name: Start nginx under runit
///   runit:
///     name: nginx
///     state: started
///     enabled: true
///
/// - name: Stop nginx service
///   runit:
///     name: nginx
///     state: stopped
///
/// - name: Restart nginx service
///   runit:
///     name: nginx
///     state: restarted
///
/// - name: Reload nginx service
///   runit:
///     name: nginx
///     state: reloaded
///
/// - name: Enable nginx at boot
///   runit:
///     name: nginx
///     enabled: true
///
/// - name: Disable nginx at boot
///   runit:
///     name: nginx
///     enabled: false
///
/// - name: Use custom service directory
///   runit:
///     name: nginx
///     state: started
///     service_dir: /etc/sv
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;

use std::path::{Path, PathBuf};
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the service to manage.
    name: String,
    /// Whether the service should be started, stopped, restarted, or reloaded.
    state: Option<State>,
    /// Whether the service should be enabled at boot.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    enabled: bool,
    /// Runit service directory where service definitions are stored.
    /// **[default: `/etc/sv`]**
    #[serde(default = "default_service_dir")]
    service_dir: String,
}

fn default_true() -> bool {
    true
}

fn default_service_dir() -> String {
    "/etc/sv".to_string()
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: String::new(),
            state: None,
            enabled: true,
            service_dir: default_service_dir(),
        }
    }
}

#[derive(Debug)]
pub struct Runit;

impl Module for Runit {
    fn get_name(&self) -> &str {
        "runit"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((runit(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct RunitClient {
    check_mode: bool,
    service_dir: PathBuf,
}

impl RunitClient {
    pub fn new(service_dir: &str, check_mode: bool) -> Self {
        RunitClient {
            check_mode,
            service_dir: PathBuf::from(service_dir),
        }
    }

    fn get_active_service_dir() -> PathBuf {
        for dir in &["/var/service", "/run/service", "/service"] {
            let path = Path::new(dir);
            if path.exists() {
                return path.to_path_buf();
            }
        }
        PathBuf::from("/var/service")
    }

    fn exec_sv_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("sv")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `sv {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing sv: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn is_active(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(false);
        }
        let output = self.exec_sv_cmd(&["status", service], false)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("run:") || stdout.contains("up:"))
    }

    pub fn is_enabled(&self, service: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(false);
        }
        let active_dir = Self::get_active_service_dir();
        let service_link = active_dir.join(service);
        Ok(service_link.exists() && service_link.is_symlink())
    }

    fn service_definition_exists(&self, service: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }
        let service_path = self.service_dir.join(service);
        if !service_path.exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Service definition not found at {}", service_path.display()),
            ));
        }
        Ok(())
    }

    pub fn start(&self, service: &str) -> Result<RunitResult> {
        self.service_definition_exists(service)?;
        let is_currently_active = self.is_active(service)?;

        if is_currently_active {
            return Ok(RunitResult::no_change());
        }

        self.execute_command_with_output(&["start", service])
    }

    pub fn stop(&self, service: &str) -> Result<RunitResult> {
        let is_currently_active = self.is_active(service)?;

        if !is_currently_active {
            return Ok(RunitResult::no_change());
        }

        self.execute_command_with_output(&["stop", service])
    }

    pub fn restart(&self, service: &str) -> Result<RunitResult> {
        self.service_definition_exists(service)?;
        self.execute_command_with_output(&["restart", service])
    }

    pub fn reload(&self, service: &str) -> Result<RunitResult> {
        self.service_definition_exists(service)?;
        self.execute_command_with_output(&["reload", service])
    }

    pub fn enable(&self, service: &str) -> Result<RunitResult> {
        self.service_definition_exists(service)?;

        if self.is_enabled(service)? {
            return Ok(RunitResult::no_change());
        }

        if self.check_mode {
            return Ok(RunitResult::new(true, None));
        }

        let active_dir = Self::get_active_service_dir();
        let service_path = self.service_dir.join(service);
        let link_path = active_dir.join(service);

        std::fs::create_dir_all(&active_dir)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        #[cfg(unix)]
        std::os::unix::fs::symlink(&service_path, &link_path)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        Ok(RunitResult::new(
            true,
            Some(format!("Enabled {} at boot", service)),
        ))
    }

    pub fn disable(&self, service: &str) -> Result<RunitResult> {
        if !self.is_enabled(service)? {
            return Ok(RunitResult::no_change());
        }

        if self.check_mode {
            return Ok(RunitResult::new(true, None));
        }

        let active_dir = Self::get_active_service_dir();
        let link_path = active_dir.join(service);

        std::fs::remove_file(&link_path).map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        Ok(RunitResult::new(
            true,
            Some(format!("Disabled {} at boot", service)),
        ))
    }

    fn execute_command_with_output(&self, args: &[&str]) -> Result<RunitResult> {
        if self.check_mode {
            return Ok(RunitResult::new(true, None));
        }

        let output = self.exec_sv_cmd(args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };
        Ok(RunitResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct RunitResult {
    changed: bool,
    output: Option<String>,
}

impl RunitResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        RunitResult { changed, output }
    }

    fn no_change() -> Self {
        RunitResult {
            changed: false,
            output: None,
        }
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

fn runit(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_service_name(&params.name)?;

    let client = RunitClient::new(&params.service_dir, check_mode);

    let mut changed = false;
    let mut output_messages = Vec::new();

    if params.enabled {
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
    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert(
        "state".to_string(),
        serde_json::Value::String(
            match params.state {
                Some(State::Started) => "started",
                Some(State::Stopped) => "stopped",
                Some(State::Restarted) => "restarted",
                Some(State::Reloaded) => "reloaded",
                None => "unknown",
            }
            .to_string(),
        ),
    );
    extra.insert(
        "enabled".to_string(),
        serde_json::Value::Bool(params.enabled),
    );
    extra.insert(
        "service_dir".to_string(),
        serde_json::Value::String(params.service_dir.clone()),
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
                enabled: true,
                service_dir: default_service_dir(),
            }
        );
    }

    #[test]
    fn test_parse_params_with_service_dir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: started
            enabled: true
            service_dir: /etc/runit/sv
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "nginx".to_owned(),
                state: Some(State::Started),
                enabled: true,
                service_dir: "/etc/runit/sv".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "nginx".to_owned(),
                state: None,
                enabled: true,
                service_dir: default_service_dir(),
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
}
