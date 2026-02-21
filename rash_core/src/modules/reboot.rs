/// ANCHOR: module
/// # reboot
///
/// Manage system reboots.
///
/// This module provides functionality to reboot systems, schedule delayed reboots,
/// and check if a reboot is required. Useful for IoT devices, container hosts,
/// and configuration management scenarios.
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
/// - name: Reboot system immediately
///   reboot:
///
/// - name: Reboot with a message
///   reboot:
///     msg: System rebooting for maintenance
///
/// - name: Schedule reboot in 5 minutes
///   reboot:
///     delay: 300
///     msg: System rebooting for maintenance in 5 minutes
///
/// - name: Check if reboot is required
///   reboot:
///     check_required: true
///   register: reboot_status
///
/// - name: Reboot if required
///   reboot:
///   when: reboot_status.reboot_required
///
/// - name: Reboot using systemctl
///   reboot:
///     method: systemctl
///
/// - name: Cancel scheduled reboot
///   reboot:
///     cancel: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::fs;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum RebootMethod {
    #[default]
    Auto,
    Systemctl,
    Reboot,
    Shutdown,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Message to display before rebooting.
    #[serde(default)]
    msg: Option<String>,

    /// Seconds to wait before rebooting.
    /// Set to 0 for immediate reboot.
    #[serde(default)]
    delay: Option<u64>,

    /// Check if a reboot is required without actually rebooting.
    /// Returns reboot_required in the result.
    #[serde(default)]
    check_required: bool,

    /// Cancel a scheduled reboot.
    #[serde(default)]
    cancel: bool,

    /// Method to use for rebooting.
    /// Options: auto (default), systemctl, reboot, shutdown.
    #[serde(default)]
    method: Option<RebootMethod>,
}

#[derive(Debug)]
pub struct Reboot;

impl Module for Reboot {
    fn get_name(&self) -> &str {
        "reboot"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((reboot(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn is_reboot_required() -> bool {
    let paths = [
        "/var/run/reboot-required",
        "/run/reboot-required",
        "/var/run/reboot-required.pkgs",
    ];

    for path in &paths {
        if Path::new(path).exists() {
            trace!("Reboot required indicator found at {}", path);
            return true;
        }
    }

    if Path::new("/etc").exists()
        && let Ok(entries) = fs::read_dir("/etc")
    {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("reboot-required") || name_str.contains("reboot-required") {
                trace!("Reboot required indicator found: {:?}", entry.path());
                return true;
            }
        }
    }

    false
}

fn has_systemctl() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cancel_scheduled_reboot(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let mut cancelled = false;
    let mut messages = Vec::new();

    if has_systemctl() {
        let output = Command::new("shutdown")
            .arg("-c")
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if output.status.success() {
            cancelled = true;
            messages.push("Cancelled scheduled shutdown/reboot via shutdown -c".to_string());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("no scheduled shutdown") {
                trace!("shutdown -c output: {}", stderr);
            }
        }
    }

    if !cancelled {
        let output = Command::new("at").args(["-l"]).output();

        if let Ok(output) = output
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("reboot")
                    && let Some(job_id) = line.split_whitespace().next()
                {
                    let _ = Command::new("at").args(["-r", job_id]).output();
                    cancelled = true;
                    messages.push(format!("Cancelled at job {}", job_id));
                }
            }
        }
    }

    let extra = if cancelled {
        let mut map = serde_json::Map::new();
        map.insert("cancelled".to_string(), serde_json::Value::Bool(cancelled));
        map.insert(
            "message".to_string(),
            serde_json::Value::String(messages.join("\n")),
        );
        Some(serde_norway::value::to_value(map)?)
    } else {
        None
    };

    Ok(ModuleResult::new(
        cancelled,
        extra,
        if messages.is_empty() {
            None
        } else {
            Some(messages.join("\n"))
        },
    ))
}

fn execute_reboot(method: &RebootMethod, msg: &Option<String>, delay: u64) -> Result<()> {
    let msg_arg = msg
        .as_ref()
        .map(|m| format!("'{}'", m))
        .unwrap_or_else(|| "'System rebooting'".to_string());

    let actual_method = if matches!(method, RebootMethod::Auto) {
        if has_systemctl() {
            RebootMethod::Systemctl
        } else {
            RebootMethod::Reboot
        }
    } else {
        method.clone()
    };

    match actual_method {
        RebootMethod::Systemctl => {
            let delay_str = if delay > 0 {
                format!("--when=+{}", delay / 60 + 1)
            } else {
                "--now".to_string()
            };

            let result = if delay > 0 {
                Command::new("systemctl")
                    .args(["reboot", &delay_str])
                    .arg("--message")
                    .arg(&msg_arg)
                    .status()
            } else {
                Command::new("systemctl")
                    .args(["reboot", &delay_str])
                    .status()
            };

            result.map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }
        RebootMethod::Reboot => {
            if delay > 0 {
                let sleep_output = Command::new("sleep")
                    .arg(delay.to_string())
                    .status()
                    .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

                if !sleep_output.success() {
                    return Err(Error::new(
                        ErrorKind::SubprocessFail,
                        "Failed to execute sleep before reboot",
                    ));
                }
            }

            Command::new("reboot")
                .arg("-f")
                .status()
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }
        RebootMethod::Shutdown => {
            let time_arg = if delay > 0 {
                format!("+{}", delay.div_ceil(60))
            } else {
                "now".to_string()
            };

            let result = if msg.is_some() {
                Command::new("shutdown")
                    .args(["-r", &time_arg])
                    .arg(&msg_arg)
                    .status()
            } else {
                Command::new("shutdown").args(["-r", &time_arg]).status()
            };

            result.map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }
        RebootMethod::Auto => unreachable!(),
    }

    Ok(())
}

fn reboot(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.cancel {
        return cancel_scheduled_reboot(check_mode);
    }

    if params.check_required {
        let required = is_reboot_required();
        let mut map = serde_json::Map::new();
        map.insert(
            "reboot_required".to_string(),
            serde_json::Value::Bool(required),
        );
        return Ok(ModuleResult::new(
            false,
            Some(serde_norway::value::to_value(map)?),
            None,
        ));
    }

    let delay = params.delay.unwrap_or(0);
    let method = params.method.unwrap_or_default();

    if check_mode {
        let mut map = serde_json::Map::new();
        map.insert(
            "reboot_initiated".to_string(),
            serde_json::Value::Bool(false),
        );
        map.insert("check_mode".to_string(), serde_json::Value::Bool(true));
        map.insert("delay".to_string(), serde_json::Value::Number(delay.into()));
        map.insert(
            "method".to_string(),
            serde_json::Value::String(format!("{:?}", method)),
        );
        return Ok(ModuleResult::new(
            true,
            Some(serde_norway::value::to_value(map)?),
            Some(format!(
                "Would reboot system with method {:?}{}{}",
                method,
                if delay > 0 {
                    format!(" after {} seconds delay", delay)
                } else {
                    String::new()
                },
                params
                    .msg
                    .as_ref()
                    .map(|m| format!(" with message: '{}'", m))
                    .unwrap_or_default()
            )),
        ));
    }

    execute_reboot(&method, &params.msg, delay)?;

    let mut map = serde_json::Map::new();
    map.insert(
        "reboot_initiated".to_string(),
        serde_json::Value::Bool(true),
    );
    map.insert("delay".to_string(), serde_json::Value::Number(delay.into()));
    Ok(ModuleResult::new(
        true,
        Some(serde_norway::value::to_value(map)?),
        Some(format!(
            "Rebooting system{}{}",
            if delay > 0 {
                format!(" in {} seconds", delay)
            } else {
                String::new()
            },
            params
                .msg
                .as_ref()
                .map(|m| format!(": {}", m))
                .unwrap_or_default()
        )),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, None);
        assert_eq!(params.delay, None);
        assert!(!params.check_required);
        assert!(!params.cancel);
        assert_eq!(params.method, None);
    }

    #[test]
    fn test_parse_params_with_msg() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "System rebooting for maintenance"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.msg,
            Some("System rebooting for maintenance".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_delay() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            delay: 300
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.delay, Some(300));
    }

    #[test]
    fn test_parse_params_check_required() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            check_required: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.check_required);
    }

    #[test]
    fn test_parse_params_cancel() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cancel: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.cancel);
    }

    #[test]
    fn test_parse_params_method() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            method: systemctl
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.method, Some(RebootMethod::Systemctl));
    }

    #[test]
    fn test_parse_params_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "Rebooting"
            delay: 60
            method: reboot
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, Some("Rebooting".to_string()));
        assert_eq!(params.delay, Some(60));
        assert_eq!(params.method, Some(RebootMethod::Reboot));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "test"
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_reboot_check_required() {
        let params = Params {
            msg: None,
            delay: None,
            check_required: true,
            cancel: false,
            method: None,
        };
        let result = reboot(params, false).unwrap();
        assert!(!result.get_changed());
        let extra = result.get_extra().unwrap();
        assert!(extra.get("reboot_required").is_some());
    }

    #[test]
    fn test_reboot_check_mode() {
        let params = Params {
            msg: Some("Test message".to_string()),
            delay: Some(10),
            check_required: false,
            cancel: false,
            method: Some(RebootMethod::Reboot),
        };
        let result = reboot(params, true).unwrap();
        assert!(result.get_changed());
        let output = result.get_output().unwrap();
        assert!(output.contains("Would reboot"));
        assert!(output.contains("Test message"));
    }

    #[test]
    fn test_reboot_check_required_returns_extra() {
        let params = Params {
            msg: None,
            delay: None,
            check_required: true,
            cancel: false,
            method: None,
        };
        let result = reboot(params, false).unwrap();
        assert!(!result.get_changed());
    }
}
