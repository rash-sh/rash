/// ANCHOR: module
/// # selinux
///
/// Change SELinux policy and modes.
///
/// This module manages SELinux configuration and state.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Enable SELinux
///   selinux:
///     policy: targeted
///     state: enforcing
///
/// - name: Set SELinux to permissive mode
///   selinux:
///     state: permissive
///
/// - name: Disable SELinux
///   selinux:
///     state: disabled
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

const SELINUX_CONFIG: &str = "/etc/selinux/config";
const SELINUX_ENFORCE: &str = "/sys/fs/selinux/enforce";

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Policy {
    Targeted,
    Minimum,
    Mls,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Enforcing,
    Permissive,
    Disabled,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The SELinux policy to use.
    policy: Option<Policy>,
    /// The SELinux mode.
    state: State,
}

fn get_current_config() -> Result<(Option<String>, Option<String>)> {
    let content = fs::read_to_string(SELINUX_CONFIG).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read SELinux config: {}", e),
        )
    })?;

    let mut current_policy = None;
    let mut current_state = None;

    for line in content.lines() {
        if line.starts_with("SELINUXTYPE=") {
            current_policy = Some(line.split('=').nth(1).unwrap_or("").to_string());
        } else if line.starts_with("SELINUX=") {
            current_state = Some(line.split('=').nth(1).unwrap_or("").to_string());
        }
    }

    Ok((current_policy, current_state))
}

fn is_enforcing() -> Result<bool> {
    if !Path::new(SELINUX_ENFORCE).exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(SELINUX_ENFORCE).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read SELinux enforce status: {}", e),
        )
    })?;

    Ok(content.trim() == "1")
}

fn update_config(policy: Option<&Policy>, state: State) -> Result<()> {
    let content = fs::read_to_string(SELINUX_CONFIG).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read SELinux config: {}", e),
        )
    })?;

    let mut new_content = String::new();
    let state_str = match state {
        State::Enforcing => "enforcing",
        State::Permissive => "permissive",
        State::Disabled => "disabled",
    };

    for line in content.lines() {
        if line.starts_with("SELINUX=") {
            new_content.push_str(&format!("SELINUX={}\n", state_str));
        } else if let Some(p) = policy {
            if line.starts_with("SELINUXTYPE=") {
                let policy_str = match p {
                    Policy::Targeted => "targeted",
                    Policy::Minimum => "minimum",
                    Policy::Mls => "mls",
                };
                new_content.push_str(&format!("SELINUXTYPE={}\n", policy_str));
            } else {
                new_content.push_str(line);
                new_content.push('\n');
            }
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    fs::write(SELINUX_CONFIG, new_content).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write SELinux config: {}", e),
        )
    })?;

    Ok(())
}

fn set_enforce(enforcing: bool) -> Result<()> {
    let value = if enforcing { "1" } else { "0" };
    fs::write(SELINUX_ENFORCE, value).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to set SELinux enforce status: {}", e),
        )
    })?;
    Ok(())
}

pub fn selinux(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let (current_policy, current_state) = get_current_config()?;
    let current_enforcing = is_enforcing()?;

    let policy_changed = if let Some(ref policy) = params.policy {
        let policy_str = match policy {
            Policy::Targeted => "targeted",
            Policy::Minimum => "minimum",
            Policy::Mls => "mls",
        };
        current_policy.as_deref() != Some(policy_str)
    } else {
        false
    };

    let state_str = match params.state {
        State::Enforcing => "enforcing",
        State::Permissive => "permissive",
        State::Disabled => "disabled",
    };
    let state_changed = current_state.as_deref() != Some(state_str);

    let runtime_changed = match params.state {
        State::Enforcing => !current_enforcing,
        State::Permissive => current_enforcing,
        State::Disabled => false,
    };

    let changed = policy_changed || state_changed || runtime_changed;

    if changed && !check_mode {
        update_config(params.policy.as_ref(), params.state)?;

        match params.state {
            State::Enforcing => {
                if !current_enforcing {
                    set_enforce(true)?;
                }
            }
            State::Permissive => {
                if current_enforcing {
                    set_enforce(false)?;
                }
            }
            State::Disabled => {}
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(format!(
            "SELinux {} to {} (policy: {})",
            if changed { "changed" } else { "unchanged" },
            state_str,
            params
                .policy
                .as_ref()
                .map(|p| match p {
                    Policy::Targeted => "targeted",
                    Policy::Minimum => "minimum",
                    Policy::Mls => "mls",
                })
                .unwrap_or("unchanged")
        )),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Selinux;

impl Module for Selinux {
    fn get_name(&self) -> &str {
        "selinux"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((selinux(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policy: targeted
            state: enforcing
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, Some(Policy::Targeted));
        assert_eq!(params.state, State::Enforcing);
    }

    #[test]
    fn test_parse_params_permissive() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: permissive
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, None);
        assert_eq!(params.state, State::Permissive);
    }

    #[test]
    fn test_parse_params_disabled() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: disabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Disabled);
    }

    #[test]
    fn test_parse_params_minimum_policy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policy: minimum
            state: enforcing
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, Some(Policy::Minimum));
    }

    #[test]
    fn test_parse_params_mls_policy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policy: mls
            state: enforcing
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, Some(Policy::Mls));
    }
}
