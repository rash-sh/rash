/// ANCHOR: module
/// # distro_package
///
/// Auto-detect the distribution's package manager and install/remove packages
/// using the appropriate backend (apk, apt, dnf, pacman, zypper, opkg).
///
/// This module provides a unified, idempotent interface for package management
/// across different Linux distributions. It automatically detects the appropriate
/// package manager based on the system and performs the requested operation.
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
/// - name: Install packages using auto-detected package manager
///   distro_package:
///     name:
///       - curl
///       - vim
///       - git
///     state: present
///     update_cache: true
///
/// - name: Remove a package
///   distro_package:
///     name: nginx
///     state: absent
///
/// - name: Ensure latest version of packages
///   distro_package:
///     name:
///       - curl
///       - jq
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
use std::path::Path;
use std::process::{Command, Output};

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

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum PackageManager {
    Apk,
    Apt,
    Dnf,
    Pacman,
    Zypper,
    Opkg,
}

fn detect_package_manager() -> Result<PackageManager> {
    if Path::new("/etc/alpine-release").exists() || which("apk") {
        return Ok(PackageManager::Apk);
    }
    if Path::new("/etc/debian_version").exists() || which("apt-get") {
        return Ok(PackageManager::Apt);
    }
    if Path::new("/etc/fedora-release").exists()
        || Path::new("/etc/redhat-release").exists()
        || which("dnf")
    {
        return Ok(PackageManager::Dnf);
    }
    if Path::new("/etc/arch-release").exists() || which("pacman") {
        return Ok(PackageManager::Pacman);
    }
    if Path::new("/etc/SuSE-release").exists() || Path::new("/etc/zypp").exists() || which("zypper")
    {
        return Ok(PackageManager::Zypper);
    }
    if which("opkg") {
        return Ok(PackageManager::Opkg);
    }
    Err(Error::new(
        ErrorKind::InvalidData,
        "Could not detect package manager. Supported managers: apk, apt, dnf, pacman, zypper, opkg",
    ))
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
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
        }
    }
}

struct DistroPackageClient {
    manager: PackageManager,
    check_mode: bool,
}

impl DistroPackageClient {
    fn new(manager: PackageManager, check_mode: bool) -> Self {
        DistroPackageClient {
            manager,
            check_mode,
        }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    cmd.get_program().to_string_lossy()
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = match self.manager {
            PackageManager::Apk => {
                let mut c = Command::new("apk");
                c.arg("info").arg("-q");
                c
            }
            PackageManager::Apt => {
                let mut c = Command::new("dpkg-query");
                c.arg("--show").arg("--showformat=${Package}\n");
                c
            }
            PackageManager::Dnf => {
                let mut c = Command::new("rpm");
                c.arg("-qa").arg("--queryformat=%{NAME}\n");
                c
            }
            PackageManager::Pacman => {
                let mut c = Command::new("pacman");
                c.arg("-Q");
                c
            }
            PackageManager::Zypper => {
                let mut c = Command::new("rpm");
                c.arg("-qa").arg("--queryformat=%{NAME}\n");
                c
            }
            PackageManager::Opkg => {
                let mut c = Command::new("opkg");
                c.arg("list-installed");
                c
            }
        };

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(self.parse_installed(output.stdout))
    }

    fn parse_installed(&self, stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect()
    }

    fn parse_outdated(&self, stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .filter_map(|line| {
                let first = line.split_whitespace().next()?;
                match self.manager {
                    PackageManager::Apt => {
                        Some(first.split('/').next().unwrap_or(first).to_string())
                    }
                    PackageManager::Dnf => {
                        if let Some(pos) = first.rfind('.') {
                            Some(first[..pos].to_string())
                        } else {
                            Some(first.to_string())
                        }
                    }
                    _ => Some(first.to_string()),
                }
            })
            .collect()
    }

    fn get_outdated(&self) -> Result<BTreeSet<String>> {
        let mut cmd = match self.manager {
            PackageManager::Apk => {
                let mut c = Command::new("apk");
                c.arg("version").arg("-l").arg("<");
                c
            }
            PackageManager::Apt => {
                let mut c = Command::new("apt");
                c.arg("list").arg("--upgradable");
                c
            }
            PackageManager::Dnf => {
                let mut c = Command::new("dnf");
                c.arg("check-update").arg("--quiet");
                c
            }
            PackageManager::Pacman => {
                let mut c = Command::new("pacman");
                c.arg("-Qu");
                c
            }
            PackageManager::Zypper => {
                let mut c = Command::new("zypper");
                c.arg("--quiet")
                    .arg("--non-interactive")
                    .arg("--no-refresh")
                    .arg("list-updates")
                    .arg("--type")
                    .arg("package");
                c
            }
            PackageManager::Opkg => {
                let mut c = Command::new("opkg");
                c.arg("list-upgradable");
                c
            }
        };

        let output = self.exec_cmd(&mut cmd)?;

        Ok(self.parse_outdated(output.stdout))
    }

    fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = match self.manager {
            PackageManager::Apk => {
                let mut c = Command::new("apk");
                c.arg("update").arg("--no-progress");
                c
            }
            PackageManager::Apt => {
                let mut c = Command::new("apt-get");
                c.arg("update");
                c
            }
            PackageManager::Dnf => {
                let mut c = Command::new("dnf");
                c.arg("makecache");
                c
            }
            PackageManager::Pacman => {
                let mut c = Command::new("pacman");
                c.arg("-Sy").arg("--noconfirm");
                c
            }
            PackageManager::Zypper => {
                let mut c = Command::new("zypper");
                c.arg("--quiet").arg("--non-interactive").arg("refresh");
                c
            }
            PackageManager::Opkg => {
                let mut c = Command::new("opkg");
                c.arg("update");
                c
            }
        };

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = match self.manager {
            PackageManager::Apk => {
                let mut c = Command::new("apk");
                c.arg("add").arg("--no-progress").args(packages);
                c
            }
            PackageManager::Apt => {
                let mut c = Command::new("apt-get");
                c.arg("install").arg("-y").args(packages);
                c
            }
            PackageManager::Dnf => {
                let mut c = Command::new("dnf");
                c.arg("install").arg("-y").args(packages);
                c
            }
            PackageManager::Pacman => {
                let mut c = Command::new("pacman");
                c.arg("-S")
                    .arg("--noconfirm")
                    .arg("--needed")
                    .args(packages);
                c
            }
            PackageManager::Zypper => {
                let mut c = Command::new("zypper");
                c.arg("--quiet")
                    .arg("--non-interactive")
                    .arg("install")
                    .arg("--auto-agree-with-licenses")
                    .args(packages);
                c
            }
            PackageManager::Opkg => {
                let mut c = Command::new("opkg");
                c.arg("install").args(packages);
                c
            }
        };

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = match self.manager {
            PackageManager::Apk => {
                let mut c = Command::new("apk");
                c.arg("del").arg("--no-progress").args(packages);
                c
            }
            PackageManager::Apt => {
                let mut c = Command::new("apt-get");
                c.arg("remove").arg("-y").args(packages);
                c
            }
            PackageManager::Dnf => {
                let mut c = Command::new("dnf");
                c.arg("remove").arg("-y").args(packages);
                c
            }
            PackageManager::Pacman => {
                let mut c = Command::new("pacman");
                c.arg("-R").arg("--noconfirm").args(packages);
                c
            }
            PackageManager::Zypper => {
                let mut c = Command::new("zypper");
                c.arg("--quiet")
                    .arg("--non-interactive")
                    .arg("remove")
                    .args(packages);
                c
            }
            PackageManager::Opkg => {
                let mut c = Command::new("opkg");
                c.arg("remove").args(packages);
                c
            }
        };

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }
}

fn distro_package(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let manager = detect_package_manager()?;
    let client = DistroPackageClient::new(manager, check_mode);
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();

    let cache_updated = if params.update_cache.unwrap() {
        client.update_cache()?;
        true
    } else {
        false
    };

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            let to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            (to_install, Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed()?;
            let to_remove: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), to_remove)
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let outdated = client.get_outdated()?;
            let to_install: Vec<String> = packages
                .difference(&installed)
                .cloned()
                .chain(packages.intersection(&outdated).cloned())
                .collect();
            (to_install, Vec::new())
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
        changed: cache_updated || install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(json!({
            "installed_packages": p_to_install,
            "removed_packages": p_to_remove,
            "cache_updated": cache_updated,
            "manager": format!("{:?}", manager).to_lowercase(),
        }))?),
    })
}

#[derive(Debug)]
pub struct DistroPackage;

impl Module for DistroPackage {
    fn get_name(&self) -> &str {
        "distro_package"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            distro_package(parse_params(optional_params)?, check_mode)?,
            None,
        ))
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
              - vim
              - git
            state: present
            update_cache: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["curl".to_owned(), "vim".to_owned(), "git".to_owned(),],
                state: Some(State::Present),
                update_cache: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_state_latest() {
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
        assert_eq!(params.state, Some(State::Latest));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
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
    fn test_parse_installed() {
        let client = DistroPackageClient::new(PackageManager::Apk, false);
        let stdout = r#"musl
busybox
alpine-baselayout
apk-tools
libc-utils
"#
        .as_bytes();
        let parsed = client.parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "musl".to_owned(),
                "busybox".to_owned(),
                "alpine-baselayout".to_owned(),
                "apk-tools".to_owned(),
                "libc-utils".to_owned(),
            ])
        );
    }

    #[test]
    fn test_parse_installed_pacman_format() {
        let client = DistroPackageClient::new(PackageManager::Pacman, false);
        let stdout = r#"linux-api-headers
linux-firmware
linux61
linux61-nvidia
"#
        .as_bytes();
        let parsed = client.parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "linux-api-headers".to_owned(),
                "linux-firmware".to_owned(),
                "linux61".to_owned(),
                "linux61-nvidia".to_owned(),
            ])
        );
    }

    #[test]
    fn test_parse_installed_opkg_format() {
        let client = DistroPackageClient::new(PackageManager::Opkg, false);
        let stdout = r#"curl - 8.4.0-1
jq - 1.6-3
libcurl4 - 8.4.0-1
"#
        .as_bytes();
        let parsed = client.parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from(["curl".to_owned(), "jq".to_owned(), "libcurl4".to_owned(),])
        );
    }

    #[test]
    fn test_parse_installed_empty() {
        let client = DistroPackageClient::new(PackageManager::Apk, false);
        let parsed = client.parse_installed(Vec::new());
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_parse_outdated_apt_format() {
        let client = DistroPackageClient::new(PackageManager::Apt, false);
        let stdout =
            r#"curl/focal-updates 7.68.0-1ubuntu2.22 amd64 [upgradable from: 7.68.0-1ubuntu2.21]
jq/stable 1.6-2.1 amd64 [upgradable from: 1.6-2]
"#
            .as_bytes();
        let parsed = client.parse_outdated(stdout.to_vec());
        assert_eq!(
            parsed,
            BTreeSet::from(["curl".to_owned(), "jq".to_owned(),])
        );
    }

    #[test]
    fn test_parse_outdated_dnf_format() {
        let client = DistroPackageClient::new(PackageManager::Dnf, false);
        let stdout = r#"curl.x86_64              7.68.0-1.fc37           updates
jq.i686                  1.6-2.fc37              updates
python3.11.x86_64        3.11.1-1.fc37           updates
"#
        .as_bytes();
        let parsed = client.parse_outdated(stdout.to_vec());
        assert_eq!(
            parsed,
            BTreeSet::from(["curl".to_owned(), "jq".to_owned(), "python3.11".to_owned(),])
        );
    }

    #[test]
    fn test_parse_outdated_apk_format() {
        let client = DistroPackageClient::new(PackageManager::Apk, false);
        let stdout = r#"curl
jq
"#
        .as_bytes();
        let parsed = client.parse_outdated(stdout.to_vec());
        assert_eq!(
            parsed,
            BTreeSet::from(["curl".to_owned(), "jq".to_owned(),])
        );
    }

    #[test]
    fn test_parse_outdated_empty() {
        let client = DistroPackageClient::new(PackageManager::Dnf, false);
        let parsed = client.parse_outdated(Vec::new());
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_which_nonexistent_command() {
        assert!(!which("definitely-not-a-real-command-12345"));
    }
}
