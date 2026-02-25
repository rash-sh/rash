/// ANCHOR: module
/// # cargo
///
/// Manage Rust crates with cargo, Rust's package manager.
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
/// - name: Install crates
///   cargo:
///     name:
///       - ripgrep
///       - fd-find
///       - bat
///     state: present
///
/// - name: Install specific version
///   cargo:
///     name: cargo-edit
///     version: "0.11.9"
///     state: present
///
/// - name: Install crate with features
///   cargo:
///     name: cargo-watch
///     features:
///       - watch
///     state: present
///
/// - name: Install from git
///   cargo:
///     name: my-crate
///     git: https://github.com/user/my-crate.git
///     branch: main
///     state: present
///
/// - name: Remove crate
///   cargo:
///     name: ripgrep
///     state: absent
///
/// - name: Update all packages to latest versions
///   cargo:
///     name:
///       - ripgrep
///       - fd-find
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
use serde_json::json;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
use shlex::split;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("cargo".to_owned())
}

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

#[serde_as]
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the cargo binary to use.
    /// **[default: `"cargo"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to cargo.
    extra_args: Option<String>,
    /// Name or list of names of the crate(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), remove (`absent`), or ensure latest version (`latest`).
    /// `present` will simply ensure that a desired crate is installed.
    /// `absent` will remove the specified crate.
    /// `latest` will update the specified crate to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The version of the crate to install.
    /// Only used with `state: present`.
    version: Option<String>,
    /// Git repository URL to install from.
    git: Option<String>,
    /// Branch to install from when using git.
    branch: Option<String>,
    /// Tag to install from when using git.
    tag: Option<String>,
    /// Specific commit to install from when using git.
    rev: Option<String>,
    /// List of features to install.
    #[serde_as(deserialize_as = "Option<OneOrMany<_>>")]
    #[serde(default)]
    features: Option<Vec<String>>,
    /// Install all available features.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    all_features: Option<bool>,
    /// Do not install default features.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_default_features: Option<bool>,
    /// Use locked dependencies when building.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    locked: Option<bool>,
    /// Force reinstall even if the crate is already installed.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    force: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("cargo".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            version: None,
            git: None,
            branch: None,
            tag: None,
            rev: None,
            features: None,
            all_features: Some(false),
            no_default_features: Some(false),
            locked: Some(false),
            force: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Cargo;

impl Module for Cargo {
    fn get_name(&self) -> &str {
        "cargo"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((cargo(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct CargoClient {
    executable: PathBuf,
    extra_args: Option<String>,
    check_mode: bool,
}

impl CargoClient {
    pub fn new(executable: &Path, extra_args: Option<String>, check_mode: bool) -> Result<Self> {
        Ok(CargoClient {
            executable: executable.to_path_buf(),
            extra_args,
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
    fn parse_installed_crates(stdout: Vec<u8>) -> BTreeSet<String> {
        let stdout = String::from_utf8_lossy(&stdout);
        stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1].starts_with('v') && parts[1].ends_with(':') {
                    parts.first().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_installed_crates(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("--list");

        let output = self.exec_cmd(&mut cmd, false)?;

        Ok(CargoClient::parse_installed_crates(output.stdout))
    }

    pub fn install(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if let Some(version) = &params.version {
            cmd.arg("--version").arg(version);
        }

        if let Some(git) = &params.git {
            cmd.arg("--git").arg(git);
        }

        if let Some(branch) = &params.branch {
            cmd.arg("--branch").arg(branch);
        }

        if let Some(tag) = &params.tag {
            cmd.arg("--tag").arg(tag);
        }

        if let Some(rev) = &params.rev {
            cmd.arg("--rev").arg(rev);
        }

        if let Some(features) = &params.features {
            cmd.arg("--features").arg(features.join(","));
        }

        if params.all_features.unwrap() {
            cmd.arg("--all-features");
        }

        if params.no_default_features.unwrap() {
            cmd.arg("--no-default-features");
        }

        if params.locked.unwrap() {
            cmd.arg("--locked");
        }

        if params.force.unwrap() {
            cmd.arg("--force");
        }

        for name in &params.name {
            cmd.arg(name);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn uninstall(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn cargo(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = CargoClient::new(
        Path::new(&params.executable.clone().unwrap()),
        params.extra_args.clone(),
        check_mode,
    )?;

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present | State::Latest => {
            let installed = client.get_installed_crates()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            let p_to_upgrade: Vec<String> = packages.intersection(&installed).cloned().collect();

            if matches!(params.state.unwrap(), State::Latest) {
                (
                    p_to_install.into_iter().chain(p_to_upgrade).collect(),
                    Vec::new(),
                )
            } else {
                (p_to_install, Vec::new())
            }
        }
        State::Absent => {
            let installed = client.get_installed_crates()?;
            let p_to_remove: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p_to_remove)
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        let install_params = Params {
            name: p_to_install.clone(),
            ..params.clone()
        };
        client.install(&install_params)?;
        true
    } else {
        false
    };

    let remove_changed = if !p_to_remove.is_empty() {
        logger::remove(&p_to_remove);
        client.uninstall(&p_to_remove)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_crates": p_to_install, "removed_crates": p_to_remove}),
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
            name: ripgrep
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["ripgrep".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /home/user/.cargo/bin/cargo
            extra_args: "--verbose"
            name:
              - ripgrep
              - fd-find
            state: latest
            version: "0.11.9"
            features:
              - watch
            all_features: true
            no_default_features: false
            locked: true
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/home/user/.cargo/bin/cargo".to_owned()),
                extra_args: Some("--verbose".to_owned()),
                name: vec!["ripgrep".to_owned(), "fd-find".to_owned()],
                state: Some(State::Latest),
                version: Some("0.11.9".to_owned()),
                git: None,
                branch: None,
                tag: None,
                rev: None,
                features: Some(vec!["watch".to_owned()]),
                all_features: Some(true),
                no_default_features: Some(false),
                locked: Some(true),
                force: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_git() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-crate
            git: https://github.com/user/my-crate.git
            branch: main
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["my-crate".to_owned()],
                git: Some("https://github.com/user/my-crate.git".to_owned()),
                branch: Some("main".to_owned()),
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_git_tag() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-crate
            git: https://github.com/user/my-crate.git
            tag: v1.0.0
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.tag, Some("v1.0.0".to_owned()));
        assert_eq!(params.branch, None);
    }

    #[test]
    fn test_parse_params_git_rev() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-crate
            git: https://github.com/user/my-crate.git
            rev: abc123def456
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rev, Some("abc123def456".to_owned()));
        assert_eq!(params.branch, None);
        assert_eq!(params.tag, None);
    }

    #[test]
    fn test_parse_params_single_feature() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: cargo-watch
            features: watch
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.features, Some(vec!["watch".to_owned()]));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: ripgrep
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_cargo_client_parse_installed() {
        let stdout = r#"ripgrep v14.0.4:
    rg
fd-find v10.1.0:
    fd
bat v0.24.0:
    bat
"#
        .as_bytes();
        let parsed = CargoClient::parse_installed_crates(stdout.to_vec());

        let expected: BTreeSet<String> = ["ripgrep", "fd-find", "bat"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_cargo_client_new_with_nonexistent_executable() {
        let result = CargoClient::new(Path::new("definitely-not-a-real-executable"), None, false);
        assert!(result.is_ok());
    }
}
