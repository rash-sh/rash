/// ANCHOR: module
/// # opkg
///
/// Manage packages with the opkg package manager, which is used by OpenWrt.
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
/// - name: Update package lists
///   opkg:
///     update_cache: yes
///
/// - name: Install packages
///   opkg:
///     name:
///       - curl
///       - jq
///     state: present
///
/// - name: Remove package
///   opkg:
///     name: vim
///     state: absent
///
/// - name: Upgrade all packages
///   opkg:
///     upgrade: yes
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
    Some("opkg".to_owned())
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
    /// **[default: `"opkg"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to opkg.
    extra_args: Option<String>,
    /// Force removal of package and its dependencies.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    force: Option<bool>,
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
    /// Whether or not to refresh the package index.
    /// This can be run as part of a package installation or as a separate step.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Whether or not to upgrade all packages to the latest version available.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    upgrade: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("opkg".to_owned()),
            extra_args: None,
            force: Some(false),
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            upgrade: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Opkg;

impl Module for Opkg {
    fn get_name(&self) -> &str {
        "opkg"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((opkg(parse_params(optional_params)?, check_mode)?, None))
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

struct OpkgClient {
    executable: PathBuf,
    extra_args: Option<String>,
    force: bool,
    check_mode: bool,
}

impl OpkgClient {
    pub fn new(
        executable: &Path,
        extra_args: Option<String>,
        force: bool,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(OpkgClient {
            executable: executable.to_path_buf(),
            extra_args,
            force,
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
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list-installed");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(OpkgClient::parse_installed(output.stdout))
    }

    pub fn get_upgradable(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list-upgradable");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        Ok(OpkgClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("install").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();

        if self.force {
            cmd.arg("--force-removal-of-dependent-packages");
        };

        cmd.arg("remove").args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("update");

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade(&self) -> Result<IsChanged> {
        let upgradable = self.get_upgradable()?;

        if upgradable.is_empty() || self.check_mode {
            return Ok(!upgradable.is_empty() && !self.check_mode);
        };

        let mut cmd = self.get_cmd();
        cmd.arg("upgrade");

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
}

fn opkg(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = OpkgClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args,
        params.force.unwrap(),
        check_mode,
    )?;

    if params.update_cache.unwrap() {
        client.update_cache()?;
    };

    let cache_updated = params.update_cache.unwrap();

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present => {
            let p: Vec<String> = packages
                .difference(&client.get_installed()?)
                .cloned()
                .collect();
            (p, Vec::new())
        }
        State::Absent => {
            let p: Vec<String> = packages
                .intersection(&client.get_installed()?)
                .cloned()
                .collect();
            (Vec::new(), p)
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let upgradable = client.get_upgradable()?;

            let p_to_install: Vec<String> = packages
                .difference(&installed)
                .cloned()
                .chain(packages.intersection(&upgradable).cloned())
                .collect();
            (p_to_install, Vec::new())
        }
    };

    let upgrade_changed = params.upgrade.unwrap() && client.upgrade()?;

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
        changed: cache_updated || upgrade_changed || install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "upgraded": upgrade_changed, "cache_updated": cache_updated}),
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
            name: curl
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["curl".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/opkg
            extra_args: "--tmp-dir /tmp"
            force: true
            name:
              - curl
              - jq
            state: latest
            update_cache: true
            upgrade: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/opkg".to_owned()),
                extra_args: Some("--tmp-dir /tmp".to_owned()),
                force: Some(true),
                name: vec!["curl".to_owned(), "jq".to_owned()],
                state: Some(State::Latest),
                update_cache: Some(true),
                upgrade: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: curl
            foo: yea
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_opkg_client_parse_installed() {
        let stdout = r#"curl - 8.4.0-1
jq - 1.6-3
libcurl4 - 8.4.0-1
libc - 1.2.4-1
"#
        .as_bytes();
        let parsed = OpkgClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "curl".to_owned(),
                "jq".to_owned(),
                "libcurl4".to_owned(),
                "libc".to_owned(),
            ])
        );
    }

    #[test]
    fn test_opkg_client_new_with_nonexistent_executable() {
        use std::path::Path;
        let result = OpkgClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_opkg_client_exec_cmd_with_nonexistent_executable() {
        use std::process::Command;
        let client = OpkgClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
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
