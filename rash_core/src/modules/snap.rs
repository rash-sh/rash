/// ANCHOR: module
/// # snap
///
/// Manage Snap packages.
///
/// Snap is a universal package manager developed by Canonical, primarily used
/// in Ubuntu-based systems. This module enables management of Snap packages
/// for VM and desktop environments.
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
/// - name: Install snap packages
///   snap:
///     name:
///       - code
///       - slack
///     state: present
///
/// - name: Install a classic-confined snap
///   snap:
///     name: code
///     classic: yes
///
/// - name: Install snap from a specific channel
///   snap:
///     name: code
///     channel: edge
///
/// - name: Remove a snap package
///   snap:
///     name: code
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
use shlex::split;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("snap".to_owned())
}

fn default_channel() -> Option<String> {
    Some("stable".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name or list of names of the Snap package(s) to install or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`) or remove (`absent`) a Snap package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Install the snap with classic confinement.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    classic: Option<bool>,
    /// The channel to install the snap from (e.g., `stable`, `edge`, `beta`, `candidate`).
    /// **[default: `"stable"`]**
    #[serde(default = "default_channel")]
    channel: Option<String>,
    /// Path of the snap binary to use.
    /// **[default: `"snap"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to snap.
    extra_args: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            classic: Some(false),
            channel: Some("stable".to_owned()),
            executable: Some("snap".to_owned()),
            extra_args: None,
        }
    }
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
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((snap(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct SnapClient {
    executable: PathBuf,
    classic: bool,
    channel: String,
    extra_args: Option<String>,
    check_mode: bool,
}

impl SnapClient {
    pub fn new(
        executable: &Path,
        classic: bool,
        channel: &str,
        extra_args: Option<String>,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(SnapClient {
            executable: executable.to_path_buf(),
            classic,
            channel: channel.to_string(),
            extra_args,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        Command::new(self.executable.clone())
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        if let Some(extra_args) = &self.extra_args {
            cmd.args(split(extra_args).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid extra_args: {extra_args}"),
                )
            })?);
        };
        let output = cmd
            .output()
            .map_err(|e| Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            ))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(output)
    }

    #[inline]
    fn parse_installed(stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .skip(1)
            .filter_map(|line| {
                let name = line.split_whitespace().next()?;
                Some(name.to_string())
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list");
        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(SnapClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if self.classic {
            cmd.arg("--classic");
        }

        cmd.arg(format!("--channel={}", self.channel));

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("remove").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn snap(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = SnapClient::new(
        Path::new(&params.executable.unwrap()),
        params.classic.unwrap(),
        &params.channel.unwrap(),
        params.extra_args,
        check_mode,
    )?;

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            let p: Vec<String> = packages.difference(&installed).cloned().collect();
            (p, Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed()?;
            let p: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p)
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install)?;
        true
    } else {
        false
    };

    let remove_changed = if !p_to_remove.is_empty() {
        logger::remove(&p_to_remove);
        client.remove(&p_to_remove)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove}),
        )?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: code
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["code".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - code
              - slack
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["code".to_owned(), "slack".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_with_classic() {
        let yaml: YamlValue = serde_norway::from_str(
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
    fn test_parse_params_with_channel() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: code
            channel: edge
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.channel, Some("edge".to_owned()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: code
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/snap
            extra_args: "--verbose"
            name:
              - code
              - slack
            state: present
            classic: true
            channel: beta
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/snap".to_owned()),
                extra_args: Some("--verbose".to_owned()),
                name: vec!["code".to_owned(), "slack".to_owned()],
                state: Some(State::Present),
                classic: Some(true),
                channel: Some("beta".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: code
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_snap_client_parse_installed() {
        let stdout =
            "Name                       Version           Rev    Tracking         Publisher   Notes
core18                     20231219          2812   latest/stable    canonical**  base
code                       1.85.1            152    latest/stable    msasci✓     classic
slack                      4.38.121          119    latest/stable    slack✓      -
"
            .as_bytes();
        let parsed = SnapClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from(["core18".to_owned(), "code".to_owned(), "slack".to_owned(),])
        );
    }

    #[test]
    fn test_snap_client_new_with_nonexistent_executable() {
        let result = SnapClient::new(
            Path::new("definitely-not-a-real-executable"),
            false,
            "stable",
            None,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_snap_client_exec_cmd_with_nonexistent_executable() {
        let client = SnapClient::new(
            Path::new("definitely-not-a-real-executable"),
            false,
            "stable",
            None,
            false,
        )
        .unwrap();
        let mut cmd = Command::new(&client.executable);
        let result = client.exec_cmd(&mut cmd, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Failed to execute"));
        assert!(msg.contains("definitely-not-a-real-executable"));
        assert!(msg.contains("not in the PATH"));
    }
}
