/// ANCHOR: module
/// # apk
///
/// Manage packages with the apk package manager, which is used by Alpine Linux.
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
///   apk:
///     update_cache: yes
///
/// - name: Install packages
///   apk:
///     name:
///       - curl
///       - jq
///       - postgresql-client
///     state: present
///
/// - name: Install specific version
///   apk:
///     name: nginx=1.24.0-r0
///     state: present
///
/// - name: Remove package
///   apk:
///     name: vim
///     state: absent
///
/// - name: Update all packages to latest versions
///   apk:
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
    Some("apk".to_owned())
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
    /// **[default: `"apk"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to apk.
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
            executable: Some("apk".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            upgrade: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Apk;

impl Module for Apk {
    fn get_name(&self) -> &str {
        "apk"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((apk(parse_params(optional_params)?, check_mode)?, None))
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

struct ApkClient {
    executable: PathBuf,
    extra_args: Option<String>,
    check_mode: bool,
}

impl ApkClient {
    pub fn new(executable: &Path, extra_args: Option<String>, check_mode: bool) -> Result<Self> {
        Ok(ApkClient {
            executable: executable.to_path_buf(),
            extra_args,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("--quiet");
        cmd
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
        output_string.lines().map(String::from).collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--info");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(ApkClient::parse_installed(output.stdout))
    }

    pub fn get_outdated(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--version").arg("-l").arg("<");

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
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--add")
            .arg("--no-cache")
            .arg("--no-progress")
            .args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--del")
            .arg("--no-cache")
            .arg("--no-progress")
            .args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--update").arg("--no-progress");

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade(&self) -> Result<IsChanged> {
        let mut query_cmd = self.get_cmd();
        query_cmd.arg("--version").arg("-l").arg("<");

        let query_output = self.exec_cmd(&mut query_cmd, false)?;

        let stdout = String::from_utf8_lossy(&query_output.stdout);
        let has_upgrades = stdout.lines().any(|line| !line.trim().is_empty());

        if !has_upgrades || self.check_mode {
            return Ok(has_upgrades && !self.check_mode);
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--upgrade").arg("--no-cache").arg("--no-progress");

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
}

fn apk(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = ApkClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args,
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
            let outdated = client.get_outdated()?;

            let p_to_install: Vec<String> = packages
                .difference(&installed)
                .cloned()
                .chain(packages.intersection(&outdated).cloned())
                .collect();
            let p_to_remove: Vec<String> = packages
                .intersection(&installed)
                .filter(|p| !packages.contains(*p))
                .cloned()
                .collect();
            (p_to_install, p_to_remove)
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
            executable: /sbin/apk
            extra_args: "--no-network"
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
                executable: Some("/sbin/apk".to_owned()),
                extra_args: Some("--no-network".to_owned()),
                name: vec!["curl".to_owned(), "jq".to_owned()],
                state: Some(State::Latest),
                update_cache: Some(true),
                upgrade: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_version_pinning() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx=1.24.0-r0
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["nginx=1.24.0-r0".to_owned()],
                state: Some(State::Present),
                ..Default::default()
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
    fn test_apk_client_parse_installed() {
        let stdout = r#"musl
busybox
alpine-baselayout
apk-tools
libc-utils
"#
        .as_bytes();
        let parsed = ApkClient::parse_installed(stdout.to_vec());

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
    fn test_apk_client_new_with_nonexistent_executable() {
        use std::path::Path;
        let result = ApkClient::new(Path::new("definitely-not-a-real-executable"), None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_apk_client_exec_cmd_with_nonexistent_executable() {
        use std::process::Command;
        let client =
            ApkClient::new(Path::new("definitely-not-a-real-executable"), None, false).unwrap();
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
