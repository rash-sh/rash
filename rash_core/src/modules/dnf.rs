/// ANCHOR: module
/// # dnf
///
/// Manage packages with the dnf package manager, which is used by Fedora, RHEL, CentOS, and related distributions.
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
/// - name: Install packages
///   dnf:
///     name:
///       - curl
///       - git
///       - vim-enhanced
///     state: present
///
/// - name: Install specific version
///   dnf:
///     name: nginx-1.24.0
///     state: present
///
/// - name: Remove package
///   dnf:
///     name: vim-enhanced
///     state: absent
///
/// - name: Update all packages
///   dnf:
///     name: "*"
///     state: latest
///
/// - name: Update package cache
///   dnf:
///     update_cache: yes
///
/// - name: Install package from specific repo
///   dnf:
///     name: nginx
///     enablerepo: epel
///     state: present
///
/// - name: Install package excluding some
///   dnf:
///     name: nginx
///     exclude:
///       - nginx-filesystem
///     state: present
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
use std::path::PathBuf;
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
    Some("dnf".to_owned())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Latest,
    Installed,
    Removed,
}

impl State {
    #[cfg(test)]
    fn is_install_variant(&self) -> bool {
        matches!(self, State::Present | State::Latest | State::Installed)
    }
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
    /// **[default: `"dnf"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional option to pass to executable.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) or file(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present` or `installed`), remove (`absent` or `removed`), or update to latest (`latest`).
    /// `present` and `installed` will simply ensure that a desired package is installed.
    /// `absent` and `removed` will remove the specified package.
    /// `latest` will update the specified package to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Whether or not to refresh the package metadata cache.
    /// This can be run as part of a package installation or as a separate step.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Disable GPG signature checking.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_gpg_check: Option<bool>,
    /// Enable specific repositories.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    enablerepo: Vec<String>,
    /// Disable specific repositories.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    disablerepo: Vec<String>,
    /// Packages to exclude from the transaction.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    exclude: Vec<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("dnf".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            disable_gpg_check: Some(false),
            enablerepo: Vec::new(),
            disablerepo: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Dnf;

impl Module for Dnf {
    fn get_name(&self) -> &str {
        "dnf"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((dnf(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct DnfClient {
    executable: PathBuf,
    extra_args: Option<String>,
    check_mode: bool,
    disable_gpg_check: bool,
    enablerepo: Vec<String>,
    disablerepo: Vec<String>,
    exclude: Vec<String>,
}

impl DnfClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(DnfClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            check_mode,
            disable_gpg_check: params.disable_gpg_check.unwrap(),
            enablerepo: params.enablerepo.clone(),
            disablerepo: params.disablerepo.clone(),
            exclude: params.exclude.clone(),
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("-y");
        if self.check_mode {
            cmd.arg("--downloadonly");
        }
        for repo in &self.enablerepo {
            cmd.arg(format!("--enablerepo={}", repo));
        }
        for repo in &self.disablerepo {
            cmd.arg(format!("--disablerepo={}", repo));
        }
        if self.disable_gpg_check {
            cmd.arg("--nogpgcheck");
        }
        for pkg in &self.exclude {
            cmd.arg("--exclude").arg(pkg);
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
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                parts.first().map(|s| {
                    let pkg_full = s.to_string();
                    pkg_full.split('.').next().unwrap_or(&pkg_full).to_string()
                })
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.args(["list", "installed", "--quiet"]);

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(DnfClient::parse_installed(output.stdout))
    }

    pub fn get_upgradable(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.args(["list", "upgrades", "--quiet"]);

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        Ok(DnfClient::parse_installed(output.stdout))
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

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("remove").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.args(["makecache", "--timer"]);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("upgrade").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn dnf(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = DnfClient::new(&params, check_mode)?;

    if params.update_cache.unwrap() {
        client.update_cache()?;
    }

    let state = params.state.unwrap();

    let (p_to_install, p_to_remove, p_to_upgrade) = match state {
        State::Present | State::Installed => {
            let installed = client.get_installed()?;
            let p_to_install: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('-').next().unwrap_or(p);
                    !installed.contains(pkg_name) && !installed.contains(*p)
                })
                .cloned()
                .collect();
            (p_to_install, Vec::new(), Vec::new())
        }
        State::Absent | State::Removed => {
            let installed = client.get_installed()?;
            let p_to_remove: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('-').next().unwrap_or(p);
                    installed.contains(pkg_name) || installed.contains(*p)
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
                    let pkg_name = p.split('-').next().unwrap_or(p);
                    !installed.contains(pkg_name) && !installed.contains(*p)
                })
                .cloned()
                .collect();
            let p_to_upgrade: Vec<String> = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split('-').next().unwrap_or(p);
                    upgradable.contains(pkg_name) || upgradable.contains(*p)
                })
                .cloned()
                .collect();
            (p_to_install, Vec::new(), p_to_upgrade)
        }
    };

    let cache_updated = params.update_cache.unwrap();

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install)?;
        true
    } else {
        false
    };

    let upgrade_changed = if !p_to_upgrade.is_empty() {
        logger::add(&p_to_upgrade);
        client.upgrade(&p_to_upgrade)?;
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
        changed: cache_updated || install_changed || upgrade_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "upgraded_packages": p_to_upgrade, "cache_updated": cache_updated}),
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
            executable: /usr/bin/dnf
            extra_args: "--skip-broken"
            name:
              - curl
              - git
            state: latest
            update_cache: true
            disable_gpg_check: true
            enablerepo:
              - epel
            disablerepo:
              - updates
            exclude:
              - kernel*
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/dnf".to_owned()),
                extra_args: Some("--skip-broken".to_owned()),
                name: vec!["curl".to_owned(), "git".to_owned()],
                state: Some(State::Latest),
                update_cache: Some(true),
                disable_gpg_check: Some(true),
                enablerepo: vec!["epel".to_owned()],
                disablerepo: vec!["updates".to_owned()],
                exclude: vec!["kernel*".to_owned()],
            }
        );
    }

    #[test]
    fn test_parse_params_version_pinning() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx-1.24.0
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["nginx-1.24.0".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_state_aliases() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: curl
            state: installed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Installed));

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: curl
            state: removed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Removed));
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
    fn test_dnf_client_parse_installed() {
        let stdout =
            r#"bash.x86_64                         5.1.8-3.fc38                   @koji-override-0
coreutils.x86_64                    9.3-1.fc38                     @koji-override-0
curl.x86_64                         8.2.1-1.fc38                   @updates
dnf.noarch                          4.18.0-1.fc38                  @koji-override-0
glibc.x86_64                        2.37-9.fc38                    @koji-override-0
"#
            .as_bytes();
        let parsed = DnfClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "bash".to_owned(),
                "coreutils".to_owned(),
                "curl".to_owned(),
                "dnf".to_owned(),
                "glibc".to_owned(),
            ])
        );
    }

    #[test]
    fn test_dnf_client_new() {
        let params = Params::default();
        let result = DnfClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.executable, PathBuf::from("dnf"));
        assert!(!client.disable_gpg_check);
        assert!(client.enablerepo.is_empty());
        assert!(client.disablerepo.is_empty());
        assert!(client.exclude.is_empty());
    }

    #[test]
    fn test_state_is_install_variant() {
        assert!(State::Present.is_install_variant());
        assert!(State::Installed.is_install_variant());
        assert!(State::Latest.is_install_variant());
        assert!(!State::Absent.is_install_variant());
        assert!(!State::Removed.is_install_variant());
    }
}
