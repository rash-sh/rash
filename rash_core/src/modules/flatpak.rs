/// ANCHOR: module
/// # flatpak
///
/// Manage Flatpak packages.
///
/// Flatpak is a universal package format for Linux desktop applications.
/// This module enables management of Flatpak packages in desktop and container environments.
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
/// - name: Install a Flatpak package
///   flatpak:
///     name: org.gnome.Calendar
///     state: present
///
/// - name: Install a Flatpak from a specific remote
///   flatpak:
///     name: org.gnome.Calendar
///     remote: flathub
///     state: present
///
/// - name: Install a Flatpak for user installation
///   flatpak:
///     name: org.gnome.Calendar
///     method: user
///     state: present
///
/// - name: Install multiple Flatpaks
///   flatpak:
///     name:
///       - org.gnome.Calendar
///       - org.gnome.Todo
///     state: present
///
/// - name: Install a Flatpak without dependencies
///   flatpak:
///     name: org.gnome.Calendar
///     no_deps: true
///     state: present
///
/// - name: Remove a Flatpak package
///   flatpak:
///     name: org.gnome.Calendar
///     state: absent
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
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::{Value as YamlValue, value};
use serde_with::{OneOrMany, serde_as};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("flatpak".to_owned())
}

fn default_remote() -> Option<String> {
    Some("flathub".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Method {
    System,
    User,
}

fn default_method() -> Option<Method> {
    Some(Method::System)
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name or list of names of the Flatpak package(s) to install or remove.
    /// Package IDs are preferred (e.g., `org.gnome.Calendar`).
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), or remove (`absent`) a Flatpak package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The Flatpak remote to use for installation.
    /// **[default: `"flathub"`]**
    #[serde(default = "default_remote")]
    remote: Option<String>,
    /// The installation method to use. `system` installs for all users,
    /// `user` installs for the current user only.
    /// **[default: `"system"`]**
    #[serde(default = "default_method")]
    method: Option<Method>,
    /// Whether to install without dependencies.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_deps: Option<bool>,
    /// Path of the flatpak binary to use.
    /// **[default: `"flatpak"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            remote: Some("flathub".to_owned()),
            method: Some(Method::System),
            no_deps: Some(false),
            executable: Some("flatpak".to_owned()),
        }
    }
}

#[derive(Debug)]
pub struct Flatpak;

impl Module for Flatpak {
    fn get_name(&self) -> &str {
        "flatpak"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((flatpak(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct FlatpakClient {
    executable: String,
    method: Method,
    remote: String,
    no_deps: bool,
    check_mode: bool,
}

impl FlatpakClient {
    pub fn new(
        executable: &str,
        method: Method,
        remote: &str,
        no_deps: bool,
        check_mode: bool,
    ) -> Self {
        FlatpakClient {
            executable: executable.to_string(),
            method,
            remote: remote.to_string(),
            no_deps,
            check_mode,
        }
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(&self.executable);
        if self.method == Method::User {
            cmd.arg("--user");
        } else {
            cmd.arg("--system");
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable
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

    fn parse_installed(stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--app").arg("list");
        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(FlatpakClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--no-interaction");

        if self.no_deps {
            cmd.arg("--no-deps");
        };

        cmd.arg("install").arg(&self.remote).args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn flatpak(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = FlatpakClient::new(
        &params.executable.unwrap(),
        params.method.unwrap(),
        &params.remote.unwrap(),
        params.no_deps.unwrap(),
        check_mode,
    );

    let (p_to_install, p_to_remove) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            let p: Vec<String> = packages.difference(&installed).cloned().collect();
            (p, Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed()?;
            let p: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p)
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
        changed: install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove}),
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
            name: org.gnome.Calendar
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["org.gnome.Calendar".to_owned()],
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
              - org.gnome.Calendar
              - org.gnome.Todo
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["org.gnome.Calendar".to_owned(), "org.gnome.Todo".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_with_remote() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: org.gnome.Calendar
            remote: flathub-beta
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.remote, Some("flathub-beta".to_owned()));
    }

    #[test]
    fn test_parse_params_with_method() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: org.gnome.Calendar
            method: user
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.method, Some(Method::User));
    }

    #[test]
    fn test_parse_params_with_no_deps() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: org.gnome.Calendar
            no_deps: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.no_deps, Some(true));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: org.gnome.Calendar
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: org.gnome.Calendar
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_flatpak_client_parse_installed() {
        let stdout = r#"Name       Application ID           Version     Branch      Installation
Calendar   org.gnome.Calendar       stable      system
Todo       org.gnome.Todo           stable      system
"#
        .as_bytes();
        let parsed = FlatpakClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from(["org.gnome.Calendar".to_owned(), "org.gnome.Todo".to_owned(),])
        );
    }
}
