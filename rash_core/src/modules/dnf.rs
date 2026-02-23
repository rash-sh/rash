/// ANCHOR: module
/// # dnf
///
/// Manage packages with the dnf package manager, which is used by Fedora and RHEL.
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
/// - name: Update package cache
///   dnf:
///     update_cache: yes
///
/// - name: Install packages
///   dnf:
///     name:
///       - nginx
///       - postgresql-server
///     state: present
///
/// - name: Install specific version
///   dnf:
///     name: nginx-1.24.0
///     state: present
///
/// - name: Remove package
///   dnf:
///     name: vim
///     state: absent
///
/// - name: Install package from specific repo
///   dnf:
///     name: nginx
///     enablerepo: epel
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

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Latest,
    Removed,
    Installed,
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
    /// Additional options to pass to dnf.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`/`installed`), remove (`absent`/`removed`), or
    /// ensure latest version (`latest`).
    /// `present` and `installed` will simply ensure that a desired package is installed.
    /// `absent` and `removed` will remove the specified package.
    /// `latest` will update the specified package to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Whether or not to refresh the package index.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Enable a specific repository.
    enablerepo: Option<String>,
    /// Disable a specific repository.
    disablerepo: Option<String>,
    /// Disable all repositories.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_gpg_check: Option<bool>,
    /// Skip packages with broken dependencies.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    skip_broken: Option<bool>,
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
            enablerepo: None,
            disablerepo: None,
            disable_gpg_check: Some(false),
            skip_broken: Some(false),
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
    enablerepo: Option<String>,
    disablerepo: Option<String>,
    disable_gpg_check: bool,
    skip_broken: bool,
    check_mode: bool,
}

impl DnfClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(DnfClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            enablerepo: params.enablerepo.clone(),
            disablerepo: params.disablerepo.clone(),
            disable_gpg_check: params.disable_gpg_check.unwrap(),
            skip_broken: params.skip_broken.unwrap(),
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("-y");
        cmd.arg("--quiet");
        if let Some(ref repo) = self.disablerepo {
            cmd.arg(format!("--disablerepo={}", repo));
        }
        if let Some(ref repo) = self.enablerepo {
            cmd.arg(format!("--enablerepo={}", repo));
        }
        if self.disable_gpg_check {
            cmd.arg("--nogpgcheck");
        }
        if self.skip_broken {
            cmd.arg("--skip-broken");
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
        output_string.lines().map(String::from).collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = Command::new("rpm");
        cmd.arg("-qa").arg("--queryformat=%{NAME}\n");

        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute 'rpm': {e}. The executable may not be installed or not in the PATH."
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{:?}", output);

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(DnfClient::parse_installed(output.stdout))
    }

    pub fn get_upgradable(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("check-update").arg("--quiet");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .collect();
        Ok(packages)
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
        cmd.arg("makecache");

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

    let cache_updated = if params.update_cache.unwrap() {
        client.update_cache()?;
        true
    } else {
        false
    };

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present | State::Installed => {
            let p: Vec<String> = packages
                .difference(&client.get_installed()?)
                .cloned()
                .collect();
            (p, Vec::new())
        }
        State::Absent | State::Removed => {
            let p: Vec<String> = packages
                .intersection(&client.get_installed()?)
                .cloned()
                .collect();
            (Vec::new(), p)
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let upgradable = client.get_upgradable()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            let p_to_upgrade: Vec<String> = packages.intersection(&upgradable).cloned().collect();

            if !p_to_upgrade.is_empty() {
                logger::add(&p_to_upgrade);
                client.upgrade(&p_to_upgrade)?;
            }

            (p_to_install, Vec::new())
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
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "cache_updated": cache_updated}),
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
            name: nginx
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["nginx".to_owned()],
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
            extra_args: "--setopt=strict=0"
            name:
              - nginx
              - postgresql-server
            state: latest
            update_cache: true
            enablerepo: epel
            disablerepo: fedora-modular
            disable_gpg_check: true
            skip_broken: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/dnf".to_owned()),
                extra_args: Some("--setopt=strict=0".to_owned()),
                name: vec!["nginx".to_owned(), "postgresql-server".to_owned()],
                state: Some(State::Latest),
                update_cache: Some(true),
                enablerepo: Some("epel".to_owned()),
                disablerepo: Some("fedora-modular".to_owned()),
                disable_gpg_check: Some(true),
                skip_broken: Some(true),
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
    fn test_parse_params_state_installed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: installed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Installed));
    }

    #[test]
    fn test_parse_params_state_removed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
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
            name: nginx
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_dnf_client_parse_installed() {
        let stdout = r#"bash
coreutils
curl
dnf
fedora-release
glibc
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
                "fedora-release".to_owned(),
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
        assert!(!client.skip_broken);
    }
}
