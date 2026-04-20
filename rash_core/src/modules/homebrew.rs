/// ANCHOR: module
/// # homebrew
///
/// Manage packages with Homebrew, the macOS package manager.
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
/// - name: Update Homebrew
///   homebrew:
///     update_homebrew: true
///
/// - name: Install packages
///   homebrew:
///     name:
///       - git
///       - curl
///       - jq
///     state: present
///
/// - name: Install a cask package
///   homebrew:
///     name: visual-studio-code
///     state: present
///     cask: true
///
/// - name: Remove package
///   homebrew:
///     name: node
///     state: absent
///
/// - name: Upgrade all packages
///   homebrew:
///     upgrade_all: true
///
/// - name: Ensure latest version of a package
///   homebrew:
///     name: git
///     state: latest
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
    Some("brew".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Latest,
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
    /// **[default: `"brew"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to brew.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), remove (`absent`), or ensure latest version (`latest`).
    /// `present` will simply ensure that a desired package is installed.
    /// `absent` will remove the specified package.
    /// `latest` will update the specified package to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Whether to manage a Homebrew cask instead of a formula.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    cask: Option<bool>,
    /// Whether to update Homebrew itself.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_homebrew: Option<bool>,
    /// Whether to upgrade all Homebrew packages.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    upgrade_all: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("brew".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            cask: Some(false),
            update_homebrew: Some(false),
            upgrade_all: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Homebrew;

impl Module for Homebrew {
    fn get_name(&self) -> &str {
        "homebrew"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((homebrew(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

type IsChanged = bool;

struct HomebrewClient {
    executable: PathBuf,
    extra_args: Option<String>,
    is_cask: bool,
    check_mode: bool,
}

impl HomebrewClient {
    pub fn new(
        executable: &Path,
        extra_args: Option<String>,
        is_cask: bool,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(HomebrewClient {
            executable: executable.to_path_buf(),
            extra_args,
            is_cask,
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
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            )
        })?;
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
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let name = line.split_whitespace().next().unwrap_or(line);
                name.to_string()
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        if self.is_cask {
            cmd.arg("list").arg("--cask").arg("-1");
        } else {
            cmd.arg("list").arg("--formula").arg("-1");
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        Ok(HomebrewClient::parse_installed(output.stdout))
    }

    pub fn get_outdated(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        if self.is_cask {
            cmd.arg("outdated").arg("--cask");
        } else {
            cmd.arg("outdated").arg("--formula");
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = stdout
            .lines()
            .filter_map(|line| {
                let name = line.split_whitespace().next();
                name.map(String::from)
            })
            .collect();
        Ok(packages)
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        if self.is_cask {
            cmd.arg("install").arg("--cask");
        } else {
            cmd.arg("install");
        }
        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        if self.is_cask {
            cmd.arg("uninstall").arg("--cask");
        } else {
            cmd.arg("uninstall");
        }
        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade_package(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        if self.is_cask {
            cmd.arg("upgrade").arg("--cask");
        } else {
            cmd.arg("upgrade");
        }
        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_homebrew(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("update");
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade_all(&self) -> Result<IsChanged> {
        let mut query_cmd = self.get_cmd();
        query_cmd.arg("outdated");

        let query_output = self.exec_cmd(&mut query_cmd, false)?;

        let stdout = String::from_utf8_lossy(&query_output.stdout);
        let has_upgrades = stdout.lines().any(|line| !line.trim().is_empty());

        if !has_upgrades || self.check_mode {
            return Ok(has_upgrades && !self.check_mode);
        };

        let mut cmd = self.get_cmd();
        cmd.arg("upgrade");
        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
}

fn homebrew(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let is_cask = params.cask.unwrap();
    let client = HomebrewClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args,
        is_cask,
        check_mode,
    )?;

    if params.update_homebrew.unwrap() {
        client.update_homebrew()?;
    };

    let homebrew_updated = params.update_homebrew.unwrap();

    let (p_to_install, p_to_remove, p_to_upgrade) = match params.state.unwrap() {
        State::Present => {
            let p: Vec<String> = packages
                .difference(&client.get_installed()?)
                .cloned()
                .collect();
            (p, Vec::new(), Vec::new())
        }
        State::Absent => {
            let p: Vec<String> = packages
                .intersection(&client.get_installed()?)
                .cloned()
                .collect();
            (Vec::new(), p, Vec::new())
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let outdated = client.get_outdated()?;

            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            let p_to_upgrade: Vec<String> = packages.intersection(&outdated).cloned().collect();
            let p_to_remove: Vec<String> = Vec::new();
            (p_to_install, p_to_remove, p_to_upgrade)
        }
    };

    let upgrade_all_changed = params.upgrade_all.unwrap() && client.upgrade_all()?;

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

    let upgrade_changed = if !p_to_upgrade.is_empty() {
        logger::add(&p_to_upgrade);
        client.upgrade_package(&p_to_upgrade)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: homebrew_updated
            || upgrade_all_changed
            || install_changed
            || remove_changed
            || upgrade_changed,
        output: None,
        extra: Some(value::to_value(json!({
            "installed_packages": p_to_install,
            "removed_packages": p_to_remove,
            "upgraded_packages": p_to_upgrade,
            "upgraded_all": upgrade_all_changed,
            "homebrew_updated": homebrew_updated
        }))?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: git
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["git".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /opt/homebrew/bin/brew
            extra_args: "--verbose"
            name:
              - git
              - curl
            state: latest
            cask: true
            update_homebrew: true
            upgrade_all: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/opt/homebrew/bin/brew".to_owned()),
                extra_args: Some("--verbose".to_owned()),
                name: vec!["git".to_owned(), "curl".to_owned()],
                state: Some(State::Latest),
                cask: Some(true),
                update_homebrew: Some(true),
                upgrade_all: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_cask() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: visual-studio-code
            cask: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["visual-studio-code".to_owned()],
                cask: Some(true),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: git
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_homebrew_client_parse_installed() {
        let stdout = r#"git
curl
jq
wget
openssl
"#
        .as_bytes();
        let parsed = HomebrewClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "git".to_owned(),
                "curl".to_owned(),
                "jq".to_owned(),
                "wget".to_owned(),
                "openssl".to_owned(),
            ])
        );
    }

    #[test]
    fn test_homebrew_client_parse_installed_with_versions() {
        let stdout = r#"git 2.43.0
curl 8.4.0
"#
        .as_bytes();
        let parsed = HomebrewClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from(["git".to_owned(), "curl".to_owned(),])
        );
    }

    #[test]
    fn test_homebrew_client_parse_installed_empty() {
        let stdout = "".as_bytes();
        let parsed = HomebrewClient::parse_installed(stdout.to_vec());
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_homebrew_client_new_with_nonexistent_executable() {
        let result = HomebrewClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_homebrew_client_exec_cmd_with_nonexistent_executable() {
        let client = HomebrewClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
            false,
        )
        .unwrap();
        let mut cmd = std::process::Command::new(&client.executable);
        let result = client.exec_cmd(&mut cmd, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Failed to execute"));
        assert!(msg.contains("definitely-not-a-real-executable"));
        assert!(msg.contains("not in the PATH"));
    }
}
