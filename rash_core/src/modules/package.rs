/// ANCHOR: module
/// # package
///
/// Generic package manager module that auto-detects the system's package manager.
///
/// This module provides a unified interface for package management across different
/// Linux distributions. It automatically detects the appropriate package manager
/// (apk, apt, dnf, pacman, or zypper) based on the system.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Install packages using auto-detected package manager
///   package:
///     name:
///       - curl
///       - jq
///     state: present
///
/// - name: Remove a package
///   package:
///     name: vim
///     state: absent
///
/// - name: Update all packages
///   package:
///     upgrade: true
///
/// - name: Install from specific package manager
///   package:
///     name: nginx
///     use_manager: apt
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum PackageManager {
    Apk,
    Apt,
    Dnf,
    Pacman,
    Zypper,
}

fn detect_package_manager() -> Option<PackageManager> {
    if Path::new("/etc/alpine-release").exists() || which("apk") {
        return Some(PackageManager::Apk);
    }
    if Path::new("/etc/debian_version").exists() || which("apt-get") {
        return Some(PackageManager::Apt);
    }
    if Path::new("/etc/fedora-release").exists()
        || Path::new("/etc/redhat-release").exists()
        || which("dnf")
    {
        return Some(PackageManager::Dnf);
    }
    if Path::new("/etc/arch-release").exists() || which("pacman") {
        return Some(PackageManager::Pacman);
    }
    if Path::new("/etc/SuSE-release").exists() || Path::new("/etc/zypp").exists() || which("zypper")
    {
        return Some(PackageManager::Zypper);
    }
    None
}

fn which(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name or list of names of the package(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), remove (`absent`), or ensure latest version (`latest`).
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Whether to update the package cache before installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Whether to upgrade all packages to the latest version available.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    upgrade: Option<bool>,
    /// Force a specific package manager to be used instead of auto-detection.
    /// If not specified, the module will auto-detect the system's package manager.
    use_manager: Option<PackageManager>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            upgrade: Some(false),
            use_manager: None,
        }
    }
}

struct PackageClient {
    manager: PackageManager,
    check_mode: bool,
}

impl PackageClient {
    fn new(manager: PackageManager, check_mode: bool) -> Self {
        PackageClient {
            manager,
            check_mode,
        }
    }

    fn get_install_cmd(&self, packages: &[String]) -> Command {
        match self.manager {
            PackageManager::Apk => {
                let mut cmd = Command::new("apk");
                cmd.arg("add").args(packages);
                cmd
            }
            PackageManager::Apt => {
                let mut cmd = Command::new("apt-get");
                cmd.arg("install").arg("-y").args(packages);
                cmd
            }
            PackageManager::Dnf => {
                let mut cmd = Command::new("dnf");
                cmd.arg("install").arg("-y").args(packages);
                cmd
            }
            PackageManager::Pacman => {
                let mut cmd = Command::new("pacman");
                cmd.arg("-S").arg("--noconfirm").args(packages);
                cmd
            }
            PackageManager::Zypper => {
                let mut cmd = Command::new("zypper");
                cmd.arg("install").arg("-y").args(packages);
                cmd
            }
        }
    }

    fn get_remove_cmd(&self, packages: &[String]) -> Command {
        match self.manager {
            PackageManager::Apk => {
                let mut cmd = Command::new("apk");
                cmd.arg("del").args(packages);
                cmd
            }
            PackageManager::Apt => {
                let mut cmd = Command::new("apt-get");
                cmd.arg("remove").arg("-y").args(packages);
                cmd
            }
            PackageManager::Dnf => {
                let mut cmd = Command::new("dnf");
                cmd.arg("remove").arg("-y").args(packages);
                cmd
            }
            PackageManager::Pacman => {
                let mut cmd = Command::new("pacman");
                cmd.arg("-R").arg("--noconfirm").args(packages);
                cmd
            }
            PackageManager::Zypper => {
                let mut cmd = Command::new("zypper");
                cmd.arg("remove").arg("-y").args(packages);
                cmd
            }
        }
    }

    fn get_update_cache_cmd(&self) -> Command {
        match self.manager {
            PackageManager::Apk => {
                let mut cmd = Command::new("apk");
                cmd.arg("update");
                cmd
            }
            PackageManager::Apt => {
                let mut cmd = Command::new("apt-get");
                cmd.arg("update");
                cmd
            }
            PackageManager::Dnf => {
                let mut cmd = Command::new("dnf");
                cmd.arg("makecache");
                cmd
            }
            PackageManager::Pacman => {
                let mut cmd = Command::new("pacman");
                cmd.arg("-Sy");
                cmd
            }
            PackageManager::Zypper => {
                let mut cmd = Command::new("zypper");
                cmd.arg("refresh");
                cmd
            }
        }
    }

    fn get_upgrade_cmd(&self) -> Command {
        match self.manager {
            PackageManager::Apk => {
                let mut cmd = Command::new("apk");
                cmd.arg("upgrade");
                cmd
            }
            PackageManager::Apt => {
                let mut cmd = Command::new("apt-get");
                cmd.arg("upgrade").arg("-y");
                cmd
            }
            PackageManager::Dnf => {
                let mut cmd = Command::new("dnf");
                cmd.arg("upgrade").arg("-y");
                cmd
            }
            PackageManager::Pacman => {
                let mut cmd = Command::new("pacman");
                cmd.arg("-Su").arg("--noconfirm");
                cmd
            }
            PackageManager::Zypper => {
                let mut cmd = Command::new("zypper");
                cmd.arg("update").arg("-y");
                cmd
            }
        }
    }

    fn exec_cmd(&self, mut cmd: Command) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute command: {e}"),
            )
        })?;

        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    fn update_cache(&self) -> Result<()> {
        let cmd = self.get_update_cache_cmd();
        self.exec_cmd(cmd)
    }

    fn install(&self, packages: &[String]) -> Result<()> {
        let cmd = self.get_install_cmd(packages);
        self.exec_cmd(cmd)
    }

    fn remove(&self, packages: &[String]) -> Result<()> {
        let cmd = self.get_remove_cmd(packages);
        self.exec_cmd(cmd)
    }

    fn upgrade(&self) -> Result<()> {
        let cmd = self.get_upgrade_cmd();
        self.exec_cmd(cmd)
    }
}

fn package(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let manager = params.use_manager.unwrap_or_else(|| {
        detect_package_manager().unwrap_or_else(|| {
            panic!("Could not detect package manager. Please specify 'use' parameter.");
        })
    });

    let client = PackageClient::new(manager.clone(), check_mode);

    if params.update_cache.unwrap() {
        client.update_cache()?;
    }

    if params.upgrade.unwrap() {
        logger::add(&["all packages".to_string()]);
        client.upgrade()?;
        return Ok(ModuleResult {
            changed: true,
            output: None,
            extra: Some(value::to_value(
                json!({"upgraded": true, "manager": format!("{:?}", manager)}),
            )?),
        });
    }

    if params.name.is_empty() {
        return Ok(ModuleResult {
            changed: false,
            output: None,
            extra: Some(value::to_value(
                json!({"manager": format!("{:?}", manager)}),
            )?),
        });
    }

    match params.state.unwrap() {
        State::Present | State::Latest => {
            logger::add(&params.name);
            client.install(&params.name)?;
            Ok(ModuleResult {
                changed: true,
                output: None,
                extra: Some(value::to_value(
                    json!({"installed": params.name, "manager": format!("{:?}", manager)}),
                )?),
            })
        }
        State::Absent => {
            logger::remove(&params.name);
            client.remove(&params.name)?;
            Ok(ModuleResult {
                changed: true,
                output: None,
                extra: Some(value::to_value(
                    json!({"removed": params.name, "manager": format!("{:?}", manager)}),
                )?),
            })
        }
    }
}

#[derive(Debug)]
pub struct Package;

impl Module for Package {
    fn get_name(&self) -> &str {
        "package"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((package(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
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
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - curl
              - jq
            state: latest
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["curl".to_owned(), "jq".to_owned()],
                state: Some(State::Latest),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_with_manager() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            use_manager: apt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.use_manager, Some(PackageManager::Apt));
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: Vec::new(),
                state: Some(State::Present),
                update_cache: Some(false),
                upgrade: Some(false),
                use_manager: None,
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: curl
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_package_client_install_cmd() {
        let client = PackageClient::new(PackageManager::Apt, false);
        let cmd = client.get_install_cmd(&["curl".to_string()]);
        assert_eq!(cmd.get_program(), "apt-get");
    }

    #[test]
    fn test_package_client_remove_cmd() {
        let client = PackageClient::new(PackageManager::Apk, false);
        let cmd = client.get_remove_cmd(&["vim".to_string()]);
        assert_eq!(cmd.get_program(), "apk");
    }
}
