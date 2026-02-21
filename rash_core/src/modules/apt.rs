/// ANCHOR: module
/// # apt
///
/// Manage packages with the apt package manager, which is used by Debian, Ubuntu and their variants.
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
/// - name: Update apt cache
///   apt:
///     update_cache: yes
///     cache_valid_time: 86400
///
/// - name: Install packages
///   apt:
///     name:
///       - curl
///       - gnupg
///       - lsb-release
///     state: present
///
/// - name: Install specific version
///   apt:
///     name: nginx=1.18.0-0ubuntu1
///     state: present
///
/// - name: Install from .deb file
///   apt:
///     deb: /tmp/package.deb
///
/// - name: Remove package
///   apt:
///     name: vim
///     state: absent
///     purge: yes
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
    Some("apt-get".to_owned())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Latest,
    BuildDep,
    Fixed,
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
    /// **[default: `"apt-get"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional option to pass to executable.
    extra_args: Option<String>,
    /// If `yes`, force apt to install only recommended packages.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    install_recommends: Option<bool>,
    /// If `yes`, install suggested packages as well as recommended packages.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    install_suggests: Option<bool>,
    /// Name or list of names of the package(s) or file(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), remove (`absent`), update to latest (`latest`),
    /// install build dependencies (`build-dep`), or fix a broken installation (`fixed`).
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Whether to purge packages or not. Only used when `state: absent`.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    purge: Option<bool>,
    /// Whether to refresh the package lists.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Update the cache only if it is older than this many seconds.
    /// Only has effect when `update_cache` is `true`.
    /// **[default: `0`]**
    #[serde(default)]
    cache_valid_time: Option<u64>,
    /// Whether to upgrade the whole system.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    upgrade: Option<bool>,
    /// Path to a .deb package to install.
    deb: Option<String>,
    /// Use this to pin a specific version of a package.
    default_release: Option<String>,
    /// Corresponds to the `--allow-downgrades` option for apt.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    allow_downgrade: Option<bool>,
    /// Corresponds to the `--allow-change-held-packages` option for apt.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    allow_unauthenticated: Option<bool>,
}

fn default_true() -> Option<bool> {
    Some(true)
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("apt-get".to_owned()),
            extra_args: None,
            install_recommends: Some(true),
            install_suggests: Some(false),
            name: Vec::new(),
            state: Some(State::Present),
            purge: Some(false),
            update_cache: Some(false),
            cache_valid_time: None,
            upgrade: Some(false),
            deb: None,
            default_release: None,
            allow_downgrade: Some(false),
            allow_unauthenticated: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Apt;

impl Module for Apt {
    fn get_name(&self) -> &str {
        "apt"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((apt(parse_params(optional_params)?, check_mode)?, None))
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

struct AptClient {
    executable: PathBuf,
    extra_args: Option<String>,
    install_recommends: bool,
    install_suggests: bool,
    purge: bool,
    check_mode: bool,
    default_release: Option<String>,
    allow_downgrade: bool,
    allow_unauthenticated: bool,
}

impl AptClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(AptClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            install_recommends: params.install_recommends.unwrap(),
            install_suggests: params.install_suggests.unwrap(),
            purge: params.purge.unwrap(),
            check_mode,
            default_release: params.default_release.clone(),
            allow_downgrade: params.allow_downgrade.unwrap(),
            allow_unauthenticated: params.allow_unauthenticated.unwrap(),
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("-y");
        cmd.arg("-q");
        if self.install_recommends {
            cmd.arg("--install-recommends");
        } else {
            cmd.arg("--no-install-recommends");
        }
        if self.install_suggests {
            cmd.arg("--install-suggests");
        }
        if let Some(ref release) = self.default_release {
            cmd.arg(format!("--default-release={}", release));
        }
        if self.allow_downgrade {
            cmd.arg("--allow-downgrades");
        }
        if self.allow_unauthenticated {
            cmd.arg("--allow-unauthenticated");
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        if let Some(ref extra_args) = self.extra_args {
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
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = Command::new("dpkg-query");
        cmd.arg("-W").arg("-f=${Package}\n");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(AptClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn install_deb(&self, deb_path: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install").arg(deb_path);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("remove");
        if self.purge {
            cmd.arg("--purge");
        }
        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("update");
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn needs_cache_update(&self, cache_valid_time: u64) -> Result<bool> {
        if cache_valid_time == 0 {
            return Ok(true);
        }

        let list_path = Path::new("/var/lib/apt/lists");
        if !list_path.exists() {
            return Ok(true);
        }

        let mut cmd = Command::new("find");
        cmd.arg(list_path)
            .arg("-name")
            .arg("*.lock")
            .arg("-mtime")
            .arg(format!("-{}", cache_valid_time / 86400));

        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to check cache age: {e}"),
            )
        })?;

        Ok(output.stdout.is_empty())
    }

    pub fn upgrade(&self) -> Result<IsChanged> {
        if self.check_mode {
            return Ok(false);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("dist-upgrade");
        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.contains("0 upgraded, 0 newly installed"))
    }

    pub fn install_build_deps(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("build-dep").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn fix_broken(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("-f").arg("install");
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn get_upgradable(&self) -> Result<Vec<String>> {
        let mut cmd = Command::new("apt");
        cmd.arg("list").arg("--upgradable");

        let output = self.exec_cmd(&mut cmd, false)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: Vec<String> = stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('/').collect();
                parts.first().map(|s| s.to_string())
            })
            .collect();
        Ok(packages)
    }
}

fn apt(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = AptClient::new(&params, check_mode)?;

    if let Some(deb_path) = &params.deb {
        logger::add(std::slice::from_ref(deb_path));
        client.install_deb(deb_path)?;
        return Ok(ModuleResult {
            changed: true,
            output: None,
            extra: Some(value::to_value(json!({"deb": deb_path}))?),
        });
    }

    let cache_valid_time = params.cache_valid_time.unwrap_or(0);
    if params.update_cache.unwrap() && client.needs_cache_update(cache_valid_time)? {
        client.update_cache()?;
    }

    let (p_to_install, p_to_remove, p_to_upgrade) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            let p_to_install: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('=').next().unwrap_or(p);
                    !installed.contains(pkg_name)
                })
                .cloned()
                .collect();
            (p_to_install, Vec::new(), Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed()?;
            let p_to_remove: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('=').next().unwrap_or(p);
                    installed.contains(pkg_name)
                })
                .cloned()
                .collect();
            (Vec::new(), p_to_remove, Vec::new())
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let upgradable = client.get_upgradable()?;
            let p_to_install: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('=').next().unwrap_or(p);
                    !installed.contains(pkg_name)
                })
                .cloned()
                .collect();
            let p_to_upgrade: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('=').next().unwrap_or(p);
                    upgradable.iter().any(|u| u == pkg_name)
                })
                .cloned()
                .collect();
            (p_to_install, Vec::new(), p_to_upgrade)
        }
        State::BuildDep => (packages.iter().cloned().collect(), Vec::new(), Vec::new()),
        State::Fixed => (Vec::new(), Vec::new(), Vec::new()),
    };

    let upgrade_changed = if params.upgrade.unwrap() {
        client.upgrade()?
    } else {
        false
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        match params.state.unwrap() {
            State::BuildDep => client.install_build_deps(&p_to_install)?,
            State::Fixed => client.fix_broken()?,
            _ => client.install(&p_to_install)?,
        }
        true
    } else if params.state.unwrap() == State::Fixed {
        client.fix_broken()?;
        true
    } else {
        false
    };

    let upgrade_packages_changed = if !p_to_upgrade.is_empty() {
        logger::add(&p_to_upgrade);
        client.install(&p_to_upgrade)?;
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
        changed: upgrade_changed || install_changed || upgrade_packages_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "upgraded_packages": p_to_upgrade, "upgraded": upgrade_changed}),
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
            executable: apt-get
            extra_args: "--allow-downgrades"
            install_recommends: false
            install_suggests: true
            name:
              - curl
              - gnupg
            state: present
            purge: true
            update_cache: true
            cache_valid_time: 86400
            upgrade: false
            default_release: focal
            allow_downgrade: true
            allow_unauthenticated: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("apt-get".to_owned()),
                extra_args: Some("--allow-downgrades".to_owned()),
                install_recommends: Some(false),
                install_suggests: Some(true),
                name: vec!["curl".to_owned(), "gnupg".to_owned()],
                state: Some(State::Present),
                purge: Some(true),
                update_cache: Some(true),
                cache_valid_time: Some(86400),
                upgrade: Some(false),
                deb: None,
                default_release: Some("focal".to_owned()),
                allow_downgrade: Some(true),
                allow_unauthenticated: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_deb() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            deb: /tmp/package.deb
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.deb, Some("/tmp/package.deb".to_owned()));
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
    fn test_apt_client_parse_installed() {
        let stdout = r#"bash
coreutils
curl
dpkg
libc6
"#
        .as_bytes();
        let parsed = AptClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "bash".to_owned(),
                "coreutils".to_owned(),
                "curl".to_owned(),
                "dpkg".to_owned(),
                "libc6".to_owned(),
            ])
        );
    }

    #[test]
    fn test_apt_client_new() {
        let params = Params::default();
        let result = AptClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.executable, PathBuf::from("apt-get"));
        assert!(client.install_recommends);
        assert!(!client.install_suggests);
        assert!(!client.purge);
    }
}
