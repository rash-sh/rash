/// ANCHOR: module
/// # zypper
///
/// Manage packages on openSUSE and SUSE Linux Enterprise Server.
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
///   zypper:
///     update_cache: yes
///
/// - name: Install packages
///   zypper:
///     name:
///       - curl
///       - jq
///       - postgresql-client
///     state: present
///
/// - name: Install specific version
///   zypper:
///     name: nginx=1.24.0
///     state: present
///
/// - name: Remove package
///   zypper:
///     name: vim
///     state: absent
///
/// - name: Install a pattern
///   zypper:
///     name: devel_basis
///     type: pattern
///     state: present
///
/// - name: Update all packages
///   zypper:
///     name: '*'
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
    Some("zypper".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
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

fn default_state() -> Option<State> {
    Some(State::default())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum PackageType {
    #[default]
    Package,
    Pattern,
    Patch,
    Srcpackage,
}

fn default_package_type() -> Option<PackageType> {
    Some(PackageType::default())
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the binary to use.
    /// **[default: `"zypper"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to zypper.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`, `installed`), remove (`absent`, `removed`), or ensure latest version (`latest`).
    /// `present` and `installed` will simply ensure that a desired package is installed.
    /// `absent` and `removed` will remove the specified package.
    /// `latest` will update the specified package to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The type of package to operate on.
    /// **[default: `"package"`]**
    #[serde(default = "default_package_type", rename = "type")]
    package_type: Option<PackageType>,
    /// Whether or not to refresh the package index.
    /// This can be run as part of a package installation or as a separate step.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Whether to disable GPG signature checking.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_gpg_check: Option<bool>,
    /// Whether to disable installing recommended packages.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_recommends: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("zypper".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            package_type: Some(PackageType::Package),
            update_cache: Some(false),
            disable_gpg_check: Some(false),
            disable_recommends: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Zypper;

impl Module for Zypper {
    fn get_name(&self) -> &str {
        "zypper"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((zypper(parse_params(optional_params)?, check_mode)?, None))
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

struct ZypperClient {
    executable: PathBuf,
    extra_args: Option<String>,
    disable_gpg_check: bool,
    disable_recommends: bool,
    check_mode: bool,
}

impl ZypperClient {
    pub fn new(
        executable: &Path,
        extra_args: Option<String>,
        disable_gpg_check: bool,
        disable_recommends: bool,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(ZypperClient {
            executable: executable.to_path_buf(),
            extra_args,
            disable_gpg_check,
            disable_recommends,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("--quiet");
        cmd.arg("--non-interactive");
        if self.disable_gpg_check {
            cmd.arg("--no-gpg-checks");
        }
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
        output_string
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--no-refresh");
        cmd.arg("se");
        cmd.arg("--installed-only");
        cmd.arg("--type").arg("package");

        let output = self.exec_cmd(&mut cmd, false)?;
        Ok(ZypperClient::parse_installed(output.stdout))
    }

    pub fn get_outdated(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--no-refresh");
        cmd.arg("list-updates");
        cmd.arg("--type").arg("package");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = stdout
            .lines()
            .skip(3)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .collect();
        Ok(packages)
    }

    pub fn install(&self, packages: &[String], package_type: &PackageType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");
        cmd.arg("--auto-agree-with-licenses");

        if self.disable_recommends {
            cmd.arg("--no-recommends");
        }

        match package_type {
            PackageType::Pattern => {
                cmd.arg("--type").arg("pattern");
            }
            PackageType::Patch => {
                cmd.arg("--type").arg("patch");
            }
            PackageType::Srcpackage => {
                cmd.arg("--type").arg("srcpackage");
            }
            PackageType::Package => {}
        }

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String], package_type: &PackageType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("remove");

        match package_type {
            PackageType::Pattern => {
                cmd.arg("--type").arg("pattern");
            }
            PackageType::Patch => {
                cmd.arg("--type").arg("patch");
            }
            PackageType::Srcpackage => {
                cmd.arg("--type").arg("srcpackage");
            }
            PackageType::Package => {}
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
        cmd.arg("refresh");

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_all(&self) -> Result<IsChanged> {
        let mut query_cmd = self.get_cmd();
        query_cmd.arg("--no-refresh");
        query_cmd.arg("list-updates");
        query_cmd.arg("--type").arg("package");

        let query_output = self.exec_cmd(&mut query_cmd, false)?;

        let stdout = String::from_utf8_lossy(&query_output.stdout);
        let has_upgrades = stdout.lines().skip(3).any(|line| !line.trim().is_empty());

        if !has_upgrades || self.check_mode {
            return Ok(has_upgrades);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("update");
        cmd.arg("--auto-agree-with-licenses");

        if self.disable_recommends {
            cmd.arg("--no-recommends");
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }
}

fn zypper(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = ZypperClient::new(
        Path::new(&params.executable.unwrap()),
        params.extra_args,
        params.disable_gpg_check.unwrap(),
        params.disable_recommends.unwrap(),
        check_mode,
    )?;

    let cache_updated = if params.update_cache.unwrap() {
        client.update_cache()?;
        true
    } else {
        false
    };

    let package_type = params.package_type.unwrap();

    let is_update_all = packages.contains("*");

    let (p_to_install, p_to_remove, update_all_changed) = match params.state.unwrap() {
        State::Present | State::Installed => {
            if is_update_all {
                (Vec::new(), Vec::new(), false)
            } else {
                let p: Vec<String> = packages
                    .difference(&client.get_installed()?)
                    .cloned()
                    .collect();
                (p, Vec::new(), false)
            }
        }
        State::Absent | State::Removed => {
            if is_update_all {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Cannot use '*' with state=absent",
                ));
            }
            let p: Vec<String> = packages
                .intersection(&client.get_installed()?)
                .cloned()
                .collect();
            (Vec::new(), p, false)
        }
        State::Latest => {
            if is_update_all {
                let changed = client.update_all()?;
                (Vec::new(), Vec::new(), changed)
            } else {
                let installed = client.get_installed()?;
                let outdated = client.get_outdated()?;

                let p_to_install: Vec<String> = packages
                    .difference(&installed)
                    .cloned()
                    .chain(packages.intersection(&outdated).cloned())
                    .collect();
                (p_to_install, Vec::new(), false)
            }
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install, &package_type)?;
        true
    } else {
        false
    };

    let remove_changed = if !p_to_remove.is_empty() {
        logger::remove(&p_to_remove);
        client.remove(&p_to_remove, &package_type)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: cache_updated || update_all_changed || install_changed || remove_changed,
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
            executable: /usr/bin/zypper
            extra_args: "--no-refresh"
            name:
              - curl
              - jq
            state: latest
            type: pattern
            update_cache: true
            disable_gpg_check: true
            disable_recommends: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/zypper".to_owned()),
                extra_args: Some("--no-refresh".to_owned()),
                name: vec!["curl".to_owned(), "jq".to_owned()],
                state: Some(State::Latest),
                package_type: Some(PackageType::Pattern),
                update_cache: Some(true),
                disable_gpg_check: Some(true),
                disable_recommends: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_state_installed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: curl
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
            name: curl
            state: removed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Removed));
    }

    #[test]
    fn test_parse_params_type_patch() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: openSUSE-2024-1
            type: patch
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.package_type, Some(PackageType::Patch));
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
    fn test_zypper_client_parse_installed() {
        let stdout = r#"glibc
bash
coreutils
curl
zypper
"#
        .as_bytes();
        let parsed = ZypperClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "glibc".to_owned(),
                "bash".to_owned(),
                "coreutils".to_owned(),
                "curl".to_owned(),
                "zypper".to_owned(),
            ])
        );
    }

    #[test]
    fn test_zypper_client_new_with_nonexistent_executable() {
        let result = ZypperClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
            false,
            false,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_zypper_client_exec_cmd_with_nonexistent_executable() {
        let client = ZypperClient::new(
            Path::new("definitely-not-a-real-executable"),
            None,
            false,
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
