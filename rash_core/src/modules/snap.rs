/// ANCHOR: module
/// # snap
///
/// Manage packages with the snap package manager (Ubuntu).
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
/// - name: Install a snap package
///   snap:
///     name: firefox
///     state: present
///
/// - name: Install a snap package with classic confinement
///   snap:
///     name: code
///     classic: true
///     state: present
///
/// - name: Install a snap from a specific channel
///   snap:
///     name: firefox
///     channel: beta
///     state: present
///
/// - name: Install multiple snap packages
///   snap:
///     name:
///       - firefox
///       - thunderbird
///     state: present
///
/// - name: Remove a snap package
///   snap:
///     name: firefox
///     state: absent
///
/// - name: Enable a snap
///   snap:
///     name: firefox
///     state: enabled
///
/// - name: Disable a snap
///   snap:
///     name: firefox
///     state: disabled
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::collections::BTreeSet;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_with::{OneOrMany, serde_as};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("snap".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Enabled,
    Disabled,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the binary to use.
    /// **[default: `"snap"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Name or list of names of the snap package(s).
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Desired state of the snap package.
    /// `present` will ensure the snap is installed.
    /// `absent` will remove the snap.
    /// `enabled` will enable the snap.
    /// `disabled` will disable the snap.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Install snap with classic confinement.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    classic: Option<bool>,
    /// Install from a specific channel (stable, candidate, beta, edge).
    channel: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("snap".to_owned()),
            name: Vec::new(),
            state: Some(State::Present),
            classic: Some(false),
            channel: None,
        }
    }
}

fn run_command(cmd: &mut Command) -> Result<Output> {
    trace!("running command: {:?}", cmd);
    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    trace!("command output: {:?}", output);
    Ok(output)
}

fn get_installed_snaps(executable: &str) -> Result<BTreeSet<String>> {
    let output = run_command(Command::new(executable).args(["list", "--color=never"]))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut snaps = BTreeSet::new();

    for line in stdout.lines().skip(1) {
        if let Some(name) = line.split_whitespace().next() {
            snaps.insert(name.to_string());
        }
    }

    Ok(snaps)
}

fn get_snap_status(executable: &str, name: &str) -> Result<Option<String>> {
    let output = run_command(Command::new(executable).args(["info", name]))?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.starts_with("status:") {
            return Ok(Some(
                line.split(':')
                    .nth(1)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default(),
            ));
        }
    }

    Ok(None)
}

fn is_snap_enabled(executable: &str, name: &str) -> Result<bool> {
    let status = get_snap_status(executable, name)?;
    Ok(status.map(|s| s == "active").unwrap_or(false))
}

fn install_snap(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let installed = get_installed_snaps(params.executable.as_ref().unwrap())?;
    let mut changed = false;

    for name in &params.name {
        if !installed.contains(name) {
            if check_mode {
                logger::diff(
                    format!("snap {} absent", name),
                    format!("snap {} present", name),
                );
                changed = true;
                continue;
            }

            let mut cmd = Command::new(params.executable.as_ref().unwrap());
            cmd.args(["install"]);

            if params.classic.unwrap_or(false) {
                cmd.arg("--classic");
            }

            if let Some(channel) = &params.channel {
                cmd.arg("--channel").arg(channel);
            }

            cmd.arg(name);

            let output = run_command(&mut cmd)?;
            if output.status.success() {
                logger::diff(
                    format!("snap {} absent", name),
                    format!("snap {} present", name),
                );
                changed = true;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to install snap {}: {}", name, stderr),
                ));
            }
        }
    }

    Ok((changed, None))
}

fn remove_snap(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let installed = get_installed_snaps(params.executable.as_ref().unwrap())?;
    let mut changed = false;

    for name in &params.name {
        if installed.contains(name) {
            if check_mode {
                logger::diff(
                    format!("snap {} present", name),
                    format!("snap {} absent", name),
                );
                changed = true;
                continue;
            }

            let output = run_command(
                Command::new(params.executable.as_ref().unwrap()).args(["remove", name]),
            )?;

            if output.status.success() {
                logger::diff(
                    format!("snap {} present", name),
                    format!("snap {} absent", name),
                );
                changed = true;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to remove snap {}: {}", name, stderr),
                ));
            }
        }
    }

    Ok((changed, None))
}

fn enable_snap(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let mut changed = false;

    for name in &params.name {
        let is_enabled = is_snap_enabled(params.executable.as_ref().unwrap(), name)?;

        if !is_enabled {
            if check_mode {
                logger::diff(
                    format!("snap {} disabled", name),
                    format!("snap {} enabled", name),
                );
                changed = true;
                continue;
            }

            let output = run_command(
                Command::new(params.executable.as_ref().unwrap()).args(["enable", name]),
            )?;

            if output.status.success() {
                logger::diff(
                    format!("snap {} disabled", name),
                    format!("snap {} enabled", name),
                );
                changed = true;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to enable snap {}: {}", name, stderr),
                ));
            }
        }
    }

    Ok((changed, None))
}

fn disable_snap(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let mut changed = false;

    for name in &params.name {
        let is_enabled = is_snap_enabled(params.executable.as_ref().unwrap(), name)?;

        if is_enabled {
            if check_mode {
                logger::diff(
                    format!("snap {} enabled", name),
                    format!("snap {} disabled", name),
                );
                changed = true;
                continue;
            }

            let output = run_command(
                Command::new(params.executable.as_ref().unwrap()).args(["disable", name]),
            )?;

            if output.status.success() {
                logger::diff(
                    format!("snap {} enabled", name),
                    format!("snap {} disabled", name),
                );
                changed = true;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to disable snap {}: {}", name, stderr),
                ));
            }
        }
    }

    Ok((changed, None))
}

#[derive(Debug)]
pub struct Snap;

impl Module for Snap {
    fn get_name(&self) -> &str {
        "snap"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        trace!("exec snap module");
        let params: Params = parse_params(params)?;

        if params.name.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "name parameter is required",
            ));
        }

        let (changed, extra) = match params.state.as_ref().unwrap_or(&State::Present) {
            State::Present => install_snap(&params, check_mode)?,
            State::Absent => remove_snap(&params, check_mode)?,
            State::Enabled => enable_snap(&params, check_mode)?,
            State::Disabled => disable_snap(&params, check_mode)?,
        };

        Ok((ModuleResult::new(changed, extra, None), None))
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
    fn test_parse_params_present() {
        let yaml = serde_norway::from_str(
            r#"
name: firefox
state: present
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["firefox"]);
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = serde_norway::from_str(
            r#"
name: firefox
state: absent
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["firefox"]);
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_classic() {
        let yaml = serde_norway::from_str(
            r#"
name: code
classic: true
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.classic, Some(true));
    }

    #[test]
    fn test_parse_params_channel() {
        let yaml = serde_norway::from_str(
            r#"
name: firefox
channel: beta
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.channel, Some("beta".to_owned()));
    }

    #[test]
    fn test_parse_params_multiple_names() {
        let yaml = serde_norway::from_str(
            r#"
name:
  - firefox
  - thunderbird
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["firefox", "thunderbird"]);
    }

    #[test]
    fn test_parse_params_enabled() {
        let yaml = serde_norway::from_str(
            r#"
name: firefox
state: enabled
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Enabled));
    }

    #[test]
    fn test_parse_params_disabled() {
        let yaml = serde_norway::from_str(
            r#"
name: firefox
state: disabled
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Disabled));
    }
}
