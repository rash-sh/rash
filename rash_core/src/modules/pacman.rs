/// ANCHOR: module
/// # pacman
///
/// Manage packages with the pacman package manager, which is used by Arch Linux and its variants.
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
/// - name: Install package rustup from repo
///   pacman:
///     name: rustup
///     state: present
///
/// - pacman:
///     executable: yay
///     name:
///       - rash
///       - timer-rs
///     state: present
///
/// - pacman:
///    upgrade: true
///    update_cache: true
///    name:
///      - rustup
///      - bpftrace
///      - linux61-zfs
///    state: sync
///    register: packages
///
/// - pacman:
///    upgrade: true
///    update_cache: true
///    force: true
///    name: linux-nvidia
///    state: absent
///    register: packages
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
use serde_with::{OneOrMany, serde_as};
use serde_yaml::{Value as YamlValue, value};
use shlex::split;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("pacman".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
    Sync,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the binary to use. This can either be `pacman` or a pacman compatible AUR helper.
    /// **[default: `"pacman"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional option to pass to executable.
    extra_args: Option<String>,
    /// When removing packages, forcefully remove them, without any checks.
    /// Same as extra_args=”--nodeps --nodeps”.
    /// When combined with `update_cache` force a refresh of all package databases.
    /// Same as update_cache_extra_args=”--refresh --refresh”.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    force: Option<bool>,
    /// Name or list of names of the package(s) or file(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), or remove (`absent`) a package.
    /// Also, supports the `sync` which will keep explicit packages accord with packages defined.
    /// Explicit packages are packages installed were literally passed to a generic
    /// `pacman` `-S` or `-U` command. You can list them with: `pacman -Qe`
    /// `present` will simply ensure that a desired package is installed.
    /// `absent` will remove the specified package.
    /// `sync` will install or remove packages to be in sync with explicit package list.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,

    /// Whether or not to refresh the master package lists.
    /// This can be run as part of a package installation or as a separate step.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    update_cache: Option<bool>,

    /// Whether or not to upgrade the whole system.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    upgrade: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("pacman".to_owned()),
            extra_args: None,
            force: Some(false),
            name: Vec::new(),
            state: Some(State::Present),
            update_cache: Some(false),
            upgrade: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Pacman;

impl Module for Pacman {
    fn get_name(&self) -> &str {
        "pacman"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        vars: Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        Ok((pacman(parse_params(optional_params)?, check_mode)?, vars))
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

struct PacmanClient {
    executable: PathBuf,
    force: bool,
    extra_args: Option<String>,
    check_mode: bool,
}

impl PacmanClient {
    pub fn new(
        executable: &Path,
        force: bool,
        extra_args: Option<String>,
        check_mode: bool,
    ) -> Self {
        PacmanClient {
            executable: executable.to_path_buf(),
            force,
            extra_args,
            check_mode,
        }
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
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
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
        cmd.arg("--query");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(PacmanClient::parse_installed(output.stdout))
    }

    pub fn get_explicit_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--query").arg("--explicit");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(PacmanClient::parse_installed(output.stdout))
    }

    pub fn install(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--noconfirm")
            .arg("--noprogressbar")
            .arg("--needed")
            .arg("--sync")
            .args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();

        if self.force {
            cmd.arg("--nodeps").arg("--nodeps");
        };

        cmd.arg("--noconfirm")
            .arg("--noprogressbar")
            .arg("--remove")
            .args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update_cache(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--sync").arg("--refresh").arg("--noprogressbar");

        if self.force {
            cmd.arg("--refresh");
        };

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade(&self) -> Result<IsChanged> {
        let mut query_cmd = self.get_cmd();
        query_cmd
            .arg("--noconfirm")
            .arg("--noprogressbar")
            .arg("--query")
            .arg("--upgrades");

        let query_output = self.exec_cmd(&mut query_cmd, false)?;

        let exit_code = query_output
            .status
            .code()
            .ok_or_else(|| Error::new(ErrorKind::SubprocessFail, "Process terminated by signal"))?;

        if exit_code == 1 || self.check_mode {
            return Ok(false);
        };

        let mut cmd = self.get_cmd();
        cmd.arg("--noconfirm")
            .arg("--noprogressbar")
            .arg("--sync")
            .arg("--sysupgrade");
        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let last_line = stdout
            .lines()
            .last()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("No last line: {stdout}")))?;
        Ok(last_line != " there is nothing to do")
    }
}

fn pacman(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = PacmanClient::new(
        // safe unwrap: params is already parsed and it has default values
        Path::new(&params.executable.unwrap()),
        params.force.unwrap(),
        params.extra_args,
        check_mode,
    );

    if params.update_cache.unwrap() {
        client.update_cache()?;
    };

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
        State::Sync => {
            let explicit_installed = &client.get_explicit_installed()?;

            let p_to_install: Vec<String> =
                packages.difference(explicit_installed).cloned().collect();
            let p_to_remove: Vec<String> =
                explicit_installed.difference(&packages).cloned().collect();
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
        changed: upgrade_changed || install_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "upgraded": upgrade_changed}),
        )?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            name: rustup
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["rustup".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            executable: yay
            extra_args: "--nodeps --nodeps"
            force: true
            name:
              - rustup
              - bpftrace
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("yay".to_owned()),
                extra_args: Some("--nodeps --nodeps".to_owned()),
                force: Some(true),
                name: vec!["rustup".to_owned(), "bpftrace".to_owned()],
                state: Some(State::Present),
                update_cache: Some(false),
                upgrade: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            name: rustup
            foo: yea
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_pacman_client_parse_installed() {
        let stdout = r#"linux-api-headers
linux-firmware
linux-firmware-whence
linux61
linux61-nvidia
linux61-zfs
"#
        .as_bytes();
        let parsed = PacmanClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "linux-api-headers".to_owned(),
                "linux-firmware".to_owned(),
                "linux-firmware-whence".to_owned(),
                "linux61".to_owned(),
                "linux61-nvidia".to_owned(),
                "linux61-zfs".to_owned(),
            ])
        );
    }
    // PacmanClient cannot be tested because it needs rash for run a mock of pacman
}
