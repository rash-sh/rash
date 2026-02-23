/// ANCHOR: module
/// # zypper
///
/// Manage packages with the zypper package manager, which is used by openSUSE and SLES.
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
/// - name: Install package nginx
///   zypper:
///     name: nginx
///     state: present
///
/// - name: Install packages
///   zypper:
///     name:
///       - nginx
///       - postgresql-server
///     state: present
///
/// - name: Install a pattern
///   zypper:
///     name: lamp_server
///     type: pattern
///     state: present
///
/// - name: Remove packages
///   zypper:
///     name:
///       - nginx
///       - postgresql-server
///     state: absent
///
/// - name: Update all packages
///   zypper:
///     name: "*"
///     state: latest
///     update_cache: true
///
/// - name: Refresh repositories and install package
///   zypper:
///     name: curl
///     state: present
///     update_cache: true
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
    Product,
    Srcpackage,
    Application,
}

fn default_type() -> Option<PackageType> {
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
    /// Additional option to pass to executable.
    extra_args: Option<String>,
    /// Name or list of names of the package(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`, `installed`), remove (`absent`, `removed`),
    /// or update to latest (`latest`).
    /// `present` and `installed` will simply ensure that a desired package is installed.
    /// `absent` and `removed` will remove the specified package.
    /// `latest` will update the specified package to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Run the equivalent of `zypper refresh` before the operation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,
    /// Disable GPG signature checking during package installation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_gpg_check: Option<bool>,
    /// The type of package to be manipulated.
    /// **[default: `"package"`]**
    #[serde(default = "default_type", rename = "type")]
    pkg_type: Option<PackageType>,
    /// Whether to clean the cache before installation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    clean_cache: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("zypper".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            disable_gpg_check: Some(false),
            pkg_type: Some(PackageType::Package),
            clean_cache: Some(false),
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

struct ZypperClient {
    executable: PathBuf,
    extra_args: Option<String>,
    disable_gpg_check: bool,
    clean_cache: bool,
    check_mode: bool,
}

impl ZypperClient {
    pub fn new(
        executable: &Path,
        extra_args: Option<String>,
        disable_gpg_check: bool,
        clean_cache: bool,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(ZypperClient {
            executable: executable.to_path_buf(),
            extra_args,
            disable_gpg_check,
            clean_cache,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("--non-interactive");
        cmd.arg("--quiet");
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
        let mut cmd = self.get_cmd();
        cmd.arg("--no-refresh");
        cmd.arg("search");
        cmd.arg("--installed-only");
        cmd.arg("--match-exact");
        cmd.arg("-t");
        cmd.arg("package");
        cmd.arg("--");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        Ok(ZypperClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String], pkg_type: &PackageType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();

        if self.clean_cache {
            cmd.arg("clean");
            self.exec_cmd(&mut cmd, true)?;
            let _cmd = self.get_cmd();
        }

        cmd.arg("install");
        cmd.arg("--auto-agree-with-licenses");

        if self.disable_gpg_check {
            cmd.arg("--no-gpg-checks");
        }

        cmd.arg("-t");
        cmd.arg(Self::type_to_string(pkg_type));

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String], pkg_type: &PackageType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("remove");
        cmd.arg("--clean-deps");

        cmd.arg("-t");
        cmd.arg(Self::type_to_string(pkg_type));

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update(&self, packages: &[String], pkg_type: &PackageType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("update");
        cmd.arg("--auto-agree-with-licenses");

        if self.disable_gpg_check {
            cmd.arg("--no-gpg-checks");
        }

        cmd.arg("-t");
        cmd.arg(Self::type_to_string(pkg_type));

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
        if self.clean_cache {
            cmd.arg("--force");
        }
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    fn type_to_string(pkg_type: &PackageType) -> &'static str {
        match pkg_type {
            PackageType::Package => "package",
            PackageType::Pattern => "pattern",
            PackageType::Patch => "patch",
            PackageType::Product => "product",
            PackageType::Srcpackage => "srcpackage",
            PackageType::Application => "application",
        }
    }

    pub fn get_upgradable(&self) -> Result<Vec<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--no-refresh");
        cmd.arg("list-updates");
        cmd.arg("-t");
        cmd.arg("package");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: Vec<String> = stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .filter(|s| !s.is_empty() && !s.starts_with('-'))
            .collect();
        Ok(packages)
    }
}

fn zypper(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = ZypperClient::new(
        Path::new(params.executable.as_ref().unwrap()),
        params.extra_args.clone(),
        params.disable_gpg_check.unwrap(),
        params.clean_cache.unwrap(),
        check_mode,
    )?;

    if params.update_cache.unwrap() {
        client.update_cache()?;
    }

    let pkg_type = params.pkg_type.as_ref().unwrap();

    let (p_to_install, p_to_remove, p_to_upgrade) = match params.state.as_ref().unwrap() {
        State::Present | State::Installed => {
            let installed = client.get_installed()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            (p_to_install, Vec::new(), Vec::new())
        }
        State::Absent | State::Removed => {
            let installed = client.get_installed()?;
            let p_to_remove: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p_to_remove, Vec::new())
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let upgradable = client.get_upgradable()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            let p_to_upgrade: Vec<String> = packages
                .intersection(&upgradable.iter().cloned().collect::<BTreeSet<String>>())
                .cloned()
                .collect();
            (p_to_install, Vec::new(), p_to_upgrade)
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install, pkg_type)?;
        true
    } else {
        false
    };

    let upgrade_changed = if !p_to_upgrade.is_empty() {
        logger::add(&p_to_upgrade);
        client.update(&p_to_upgrade, pkg_type)?;
        true
    } else {
        false
    };

    let remove_changed = if !p_to_remove.is_empty() {
        logger::remove(&p_to_remove);
        client.remove(&p_to_remove, pkg_type)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: install_changed || upgrade_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "upgraded_packages": p_to_upgrade}),
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
            executable: zypper
            extra_args: "--no-recommends"
            name:
              - nginx
              - postgresql-server
            state: present
            update_cache: true
            disable_gpg_check: true
            type: package
            clean_cache: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("zypper".to_owned()),
                extra_args: Some("--no-recommends".to_owned()),
                name: vec!["nginx".to_owned(), "postgresql-server".to_owned()],
                state: Some(State::Present),
                update_cache: Some(true),
                disable_gpg_check: Some(true),
                pkg_type: Some(PackageType::Package),
                clean_cache: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_pattern() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: lamp_server
            type: pattern
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["lamp_server".to_owned()],
                state: Some(State::Present),
                pkg_type: Some(PackageType::Pattern),
                ..Default::default()
            }
        );
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
    fn test_zypper_client_parse_installed() {
        let stdout = "nginx\npostgresql-server\ncurl\nwget\n".as_bytes();
        let parsed = ZypperClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "nginx".to_owned(),
                "postgresql-server".to_owned(),
                "curl".to_owned(),
                "wget".to_owned(),
            ])
        );
    }

    #[test]
    fn test_zypper_client_new() {
        let result = ZypperClient::new(Path::new("zypper"), None, false, false, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.executable, PathBuf::from("zypper"));
        assert!(!client.disable_gpg_check);
        assert!(!client.clean_cache);
        assert!(!client.check_mode);
    }

    #[test]
    fn test_type_to_string() {
        assert_eq!(
            ZypperClient::type_to_string(&PackageType::Package),
            "package"
        );
        assert_eq!(
            ZypperClient::type_to_string(&PackageType::Pattern),
            "pattern"
        );
        assert_eq!(ZypperClient::type_to_string(&PackageType::Patch), "patch");
        assert_eq!(
            ZypperClient::type_to_string(&PackageType::Product),
            "product"
        );
        assert_eq!(
            ZypperClient::type_to_string(&PackageType::Srcpackage),
            "srcpackage"
        );
        assert_eq!(
            ZypperClient::type_to_string(&PackageType::Application),
            "application"
        );
    }
}
