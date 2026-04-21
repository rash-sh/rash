/// ANCHOR: module
/// # snap
///
/// Manage Snap packages.
///
/// Snap is a universal package manager used extensively in Ubuntu-based
/// containers and IoT devices. This module enables management of Snap packages.
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
/// - name: Install a Snap package
///   snap:
///     name: hello-world
///     state: present
///
/// - name: Install a Snap package from a specific channel
///   snap:
///     name: hello-world
///     channel: beta
///
/// - name: Install multiple Snap packages
///   snap:
///     name:
///       - hello-world
///       - jq
///     state: present
///
/// - name: Install a Snap with classic confinement
///   snap:
///     name: code
///     classic: true
///
/// - name: Remove a Snap package
///   snap:
///     name: obsolete-app
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
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
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
    /// Whether to install (`present`), or remove (`absent`) a Snap package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The channel to install the Snap from: `stable`, `candidate`, `beta`, or `edge`.
    /// **[default: `"stable"`]**
    #[serde(default = "default_channel")]
    channel: Option<String>,
    /// Install the Snap with classic confinement.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    classic: Option<bool>,
    /// Path of the snap binary to use.
    /// **[default: `"snap"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            channel: Some("stable".to_owned()),
            classic: Some(false),
            executable: Some("snap".to_owned()),
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
    executable: String,
    channel: String,
    classic: bool,
    check_mode: bool,
}

impl SnapClient {
    pub fn new(executable: &str, channel: &str, classic: bool, check_mode: bool) -> Self {
        SnapClient {
            executable: executable.to_string(),
            channel: channel.to_string(),
            classic,
            check_mode,
        }
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable
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

    fn parse_installed(stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .skip(1)
            .filter_map(|line| line.split_whitespace().next().map(String::from))
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("list");
        let output = self.exec_cmd(&mut cmd, false)?;
        Ok(SnapClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = Command::new(&self.executable);
        cmd.arg("install");

        if self.classic {
            cmd.arg("--classic");
        }

        if self.channel != "stable" {
            cmd.arg(format!("--channel={}", self.channel));
        }

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = Command::new(&self.executable);
        cmd.arg("remove").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn snap(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = SnapClient::new(
        &params.executable.unwrap(),
        &params.channel.unwrap(),
        params.classic.unwrap(),
        check_mode,
    );

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
            name: hello-world
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["hello-world".to_owned()],
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
              - hello-world
              - jq
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["hello-world".to_owned(), "jq".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_with_channel() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: hello-world
            channel: beta
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.channel, Some("beta".to_owned()));
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
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: hello-world
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: hello-world
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_snap_client_parse_installed() {
        let stdout = r#"Name        Version    Rev    Tracking       Publisher   Notes
core        16-2.61     16928  latest/stable  canonical*  core
hello-world 6.4        29     latest/stable  canonical*  -
jq          1.7.1      1129   latest/stable  jqlang*     -
"#
        .as_bytes();
        let parsed = SnapClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from(["core".to_owned(), "hello-world".to_owned(), "jq".to_owned(),])
        );
    }

    #[test]
    fn test_snap_client_parse_installed_empty() {
        let stdout = b"";
        let parsed = SnapClient::parse_installed(stdout.to_vec());
        assert!(parsed.is_empty());
    }
}
