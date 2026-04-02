/// ANCHOR: module
/// # snap
///
/// Manage Snap packages on Ubuntu and other Snap-supported systems.
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
///     name: hello-world
///
/// - name: Install multiple snap packages
///   snap:
///     name:
///       - vlc
///       - chromium
///     state: present
///
/// - name: Install a snap from the edge channel
///   snap:
///     name: lxd
///     channel: edge
///
/// - name: Install a classic snap
///   snap:
///     name: code
///     classic: true
///
/// - name: Remove a snap package
///   snap:
///     name: vlc
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{parse_params, Module, ModuleResult};
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
use serde_norway::{value, Value as YamlValue};
use serde_with::{serde_as, OneOrMany};
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
    /// Path of the binary to use.
    /// **[default: `"snap"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to snap.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) to install or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`) or remove (`absent`) the package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Channel to install from (stable, candidate, beta, edge).
    /// **[default: `"stable"`]**
    #[serde(default = "default_channel")]
    channel: Option<String>,
    /// Confinement mode to use (strict, classic, devmode).
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    classic: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("snap".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            channel: Some("stable".to_owned()),
            classic: Some(false),
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
    extra_args: Option<String>,
    check_mode: bool,
}

impl SnapClient {
    pub fn new(executable: &Path, extra_args: Option<String>, check_mode: bool) -> Result<Self> {
        Ok(SnapClient {
            executable: executable.to_path_buf(),
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

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--all");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = stdout
            .lines()
            .skip(1)
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect();
        Ok(packages)
    }

    pub fn install(&self, packages: &[String], channel: &str, classic: bool) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if classic {
            cmd.arg("--classic");
        }

        cmd.arg(format!("--channel={}", channel));
        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("remove");
        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn snap(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = SnapClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args,
        check_mode,
    )?;

    let installed = client.get_installed()?;

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present => {
            let p: Vec<String> = packages.difference(&installed).cloned().collect();
            (p, Vec::new())
        }
        State::Absent => {
            let p: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p)
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(
            &p_to_install,
            &params.channel.unwrap(),
            params.classic.unwrap(),
        )?;
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
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/snap
            extra_args: "--no-wait"
            name:
              - vlc
              - chromium
            state: present
            channel: edge
            classic: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/snap".to_owned()),
                extra_args: Some("--no-wait".to_owned()),
                name: vec!["vlc".to_owned(), "chromium".to_owned()],
                state: Some(State::Present),
                channel: Some("edge".to_owned()),
                classic: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_multiple_names() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - code
              - docker
            state: present
            classic: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["code".to_owned(), "docker".to_owned()],
                state: Some(State::Present),
                classic: Some(true),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vlc
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["vlc".to_owned()],
                state: Some(State::Absent),
                ..Default::default()
            }
        );
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
}
