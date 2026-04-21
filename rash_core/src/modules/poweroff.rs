/// ANCHOR: module
/// # poweroff
///
/// Manage system power state (shutdown, poweroff, halt).
///
/// This module provides functionality to power off, shut down, or halt systems.
/// Supports scheduling delayed actions, custom messages, and forced shutdowns.
/// Useful for IoT devices, container hosts, and automated maintenance scenarios.
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
/// - name: Power off system immediately
///   poweroff:
///
/// - name: Power off with a message
///   poweroff:
///     msg: System powering off for maintenance
///
/// - name: Schedule shutdown in 5 minutes
///   poweroff:
///     state: shutdown
///     delay: 300
///     msg: System shutting down for maintenance in 5 minutes
///
/// - name: Halt the system
///   poweroff:
///     state: halt
///
/// - name: Force immediate poweroff
///   poweroff:
///     force: true
///
/// - name: Cancel scheduled poweroff
///   poweroff:
///     cancel: true
///
/// - name: Reboot from poweroff module
///   poweroff:
///     state: reboot
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
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum PowerState {
    #[default]
    Poweroff,
    Shutdown,
    Halt,
    Reboot,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(default)]
    msg: Option<String>,

    #[serde(default)]
    delay: Option<u64>,

    #[serde(default)]
    state: Option<PowerState>,

    #[serde(default)]
    force: bool,

    #[serde(default)]
    cancel: bool,
}

#[derive(Debug)]
pub struct Poweroff;

impl Module for Poweroff {
    fn get_name(&self) -> &str {
        "poweroff"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((poweroff(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn has_systemctl() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cancel_scheduled_poweroff(check_mode: bool) -> Result<ModuleResult> {
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
            messages.push("Cancelled scheduled shutdown via shutdown -c".to_string());
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
                if (line.contains("shutdown") || line.contains("poweroff") || line.contains("halt"))
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

fn run_cmd(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().to_string();
    let status = cmd
        .status()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    if !status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "{} failed with exit code {}",
                program,
                status
                    .code()
                    .map_or("unknown".to_string(), |c| c.to_string())
            ),
        ));
    }
    Ok(())
}

fn execute_systemctl_action(
    action: &str,
    msg: &Option<String>,
    delay: u64,
    force: bool,
) -> Result<()> {
    let msg_arg = msg
        .as_ref()
        .map(|m| format!("'{}'", m))
        .unwrap_or_else(|| "'System powering off'".to_string());

    if delay > 0 {
        let delay_str = format!("--when=+{}", delay / 60 + 1);
        run_cmd(
            Command::new("systemctl")
                .args([action, &delay_str])
                .arg("--message")
                .arg(&msg_arg),
        )
    } else if force {
        run_cmd(Command::new("systemctl").args([action, "--force"]))
    } else {
        run_cmd(Command::new("systemctl").arg(action))
    }
}

fn execute_poweroff(
    state: &PowerState,
    msg: &Option<String>,
    delay: u64,
    force: bool,
) -> Result<()> {
    let msg_arg = msg
        .as_ref()
        .map(|m| format!("'{}'", m))
        .unwrap_or_else(|| "'System powering off'".to_string());

    match state {
        PowerState::Poweroff => {
            if has_systemctl() {
                execute_systemctl_action("poweroff", msg, delay, force)
            } else if force {
                run_cmd(Command::new("poweroff").arg("-f"))
            } else if delay > 0 {
                let time_arg = format!("+{}", delay.div_ceil(60));
                run_cmd(
                    Command::new("shutdown")
                        .arg("-h")
                        .arg(&time_arg)
                        .arg(&msg_arg),
                )
            } else {
                run_cmd(Command::new("shutdown").args(["-h", "now"]).arg(&msg_arg))
            }
        }
        PowerState::Shutdown => {
            if delay > 0 {
                let time_arg = format!("+{}", delay.div_ceil(60));
                run_cmd(
                    Command::new("shutdown")
                        .arg("-h")
                        .arg(&time_arg)
                        .arg(&msg_arg),
                )
            } else if force {
                run_cmd(
                    Command::new("shutdown")
                        .args(["-f", "-h", "now"])
                        .arg(&msg_arg),
                )
            } else {
                run_cmd(Command::new("shutdown").args(["-h", "now"]).arg(&msg_arg))
            }
        }
        PowerState::Halt => {
            if has_systemctl() {
                execute_systemctl_action("halt", msg, delay, force)
            } else if force {
                run_cmd(Command::new("halt").arg("-f"))
            } else {
                run_cmd(&mut Command::new("halt"))
            }
        }
        PowerState::Reboot => {
            if has_systemctl() {
                execute_systemctl_action("reboot", msg, delay, force)
            } else if force {
                run_cmd(Command::new("reboot").arg("-f"))
            } else if delay > 0 {
                let time_arg = format!("+{}", delay.div_ceil(60));
                run_cmd(
                    Command::new("shutdown")
                        .arg("-r")
                        .arg(&time_arg)
                        .arg(&msg_arg),
                )
            } else {
                run_cmd(&mut Command::new("reboot"))
            }
        }
    }
}

fn poweroff(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.cancel {
        return cancel_scheduled_poweroff(check_mode);
    }

    let delay = params.delay.unwrap_or(0);
    let state = params.state.unwrap_or_default();

    if check_mode {
        let mut map = serde_json::Map::new();
        map.insert(
            "poweroff_initiated".to_string(),
            serde_json::Value::Bool(false),
        );
        map.insert("check_mode".to_string(), serde_json::Value::Bool(true));
        map.insert("delay".to_string(), serde_json::Value::Number(delay.into()));
        map.insert(
            "state".to_string(),
            serde_json::Value::String(format!("{:?}", state)),
        );
        map.insert("force".to_string(), serde_json::Value::Bool(params.force));
        return Ok(ModuleResult::new(
            true,
            Some(serde_norway::value::to_value(map)?),
            Some(format!(
                "Would {:?} system{}{}{}",
                state,
                if params.force { " (forced)" } else { "" },
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

    execute_poweroff(&state, &params.msg, delay, params.force)?;

    let mut map = serde_json::Map::new();
    map.insert(
        "poweroff_initiated".to_string(),
        serde_json::Value::Bool(true),
    );
    map.insert("delay".to_string(), serde_json::Value::Number(delay.into()));
    map.insert(
        "state".to_string(),
        serde_json::Value::String(format!("{:?}", state)),
    );
    Ok(ModuleResult::new(
        true,
        Some(serde_norway::value::to_value(map)?),
        Some(format!(
            "{:?} system{}{}",
            state,
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
        assert_eq!(params.state, None);
        assert!(!params.force);
        assert!(!params.cancel);
    }

    #[test]
    fn test_parse_params_with_msg() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "Powering off for maintenance"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, Some("Powering off for maintenance".to_string()));
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
    fn test_parse_params_state_poweroff() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: poweroff
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(PowerState::Poweroff));
    }

    #[test]
    fn test_parse_params_state_shutdown() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: shutdown
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(PowerState::Shutdown));
    }

    #[test]
    fn test_parse_params_state_halt() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: halt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(PowerState::Halt));
    }

    #[test]
    fn test_parse_params_state_reboot() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: reboot
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(PowerState::Reboot));
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
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
    fn test_parse_params_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "Goodbye"
            delay: 60
            state: halt
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.msg, Some("Goodbye".to_string()));
        assert_eq!(params.delay, Some(60));
        assert_eq!(params.state, Some(PowerState::Halt));
        assert!(params.force);
        assert!(!params.cancel);
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
    fn test_poweroff_check_mode_default() {
        let params = Params {
            msg: None,
            delay: None,
            state: None,
            force: false,
            cancel: false,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
        let output = result.get_output().unwrap();
        assert!(output.contains("Would Poweroff"));
    }

    #[test]
    fn test_poweroff_check_mode_with_options() {
        let params = Params {
            msg: Some("Test message".to_string()),
            delay: Some(10),
            state: Some(PowerState::Shutdown),
            force: true,
            cancel: false,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
        let output = result.get_output().unwrap();
        assert!(output.contains("Would Shutdown"));
        assert!(output.contains("(forced)"));
        assert!(output.contains("10 seconds delay"));
        assert!(output.contains("Test message"));
    }

    #[test]
    fn test_poweroff_check_mode_halt() {
        let params = Params {
            msg: None,
            delay: None,
            state: Some(PowerState::Halt),
            force: false,
            cancel: false,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
        let output = result.get_output().unwrap();
        assert!(output.contains("Would Halt"));
    }

    #[test]
    fn test_poweroff_check_mode_reboot() {
        let params = Params {
            msg: Some("Restarting".to_string()),
            delay: None,
            state: Some(PowerState::Reboot),
            force: false,
            cancel: false,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
        let output = result.get_output().unwrap();
        assert!(output.contains("Would Reboot"));
        assert!(output.contains("Restarting"));
    }

    #[test]
    fn test_poweroff_check_mode_cancel() {
        let params = Params {
            msg: None,
            delay: None,
            state: None,
            force: false,
            cancel: true,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_poweroff_check_mode_returns_extra() {
        let params = Params {
            msg: None,
            delay: Some(30),
            state: Some(PowerState::Poweroff),
            force: false,
            cancel: false,
        };
        let result = poweroff(params, true).unwrap();
        assert!(result.get_changed());
        let extra = result.get_extra().unwrap();
        assert!(extra.get("poweroff_initiated").is_some());
        assert!(extra.get("check_mode").is_some());
        assert!(extra.get("delay").is_some());
        assert!(extra.get("state").is_some());
    }
}
