/// ANCHOR: module
/// # gem
///
/// Manage Ruby gems with the gem package manager.
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
/// - name: Install a gem
///   gem:
///     name: bundler
///     state: present
///
/// - name: Install specific version of a gem
///   gem:
///     name: rails
///     version: "7.0.0"
///     state: present
///
/// - name: Install gem with version constraint
///   gem:
///     name: rake
///     version: ">= 13.0"
///     state: present
///
/// - name: Install gem to user directory
///   gem:
///     name: rubocop
///     user_install: true
///
/// - name: Install pre-release version
///   gem:
///     name: some_gem
///     pre_release: true
///
/// - name: Install from specific source
///   gem:
///     name: private_gem
///     gem_source: https://gems.example.com
///
/// - name: Install gems from Gemfile
///   gem:
///     bundler: true
///     chdir: /app
///
/// - name: Remove a gem
///   gem:
///     name: rails
///     state: absent
///
/// - name: Update gem to latest version
///   gem:
///     name: bundler
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
    Some("gem".to_owned())
}

fn default_user_install() -> Option<bool> {
    Some(true)
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
    /// **[default: `"gem"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to gem.
    extra_args: Option<String>,
    /// Name or list of names of the gem(s) to install, upgrade, or remove.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// Whether to install (`present`), remove (`absent`), or ensure latest version (`latest`).
    /// `present` will simply ensure that a desired gem is installed.
    /// `absent` will remove the specified gem.
    /// `latest` will update the specified gem to the latest version.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Version of the gem to install/upgrade.
    /// Can be a specific version or a constraint (e.g., ">= 1.0", "~> 2.0").
    version: Option<String>,
    /// Install gem in user's home directory.
    /// When false, installs to system-wide gem directory (may require root).
    /// **[default: `true`]**
    #[serde(default = "default_user_install")]
    user_install: Option<bool>,
    /// Allow installation of pre-release versions.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    pre_release: Option<bool>,
    /// Custom gem source (repository URL).
    gem_source: Option<String>,
    /// Use bundler instead of gem command.
    /// When true, runs bundle install for Gemfile management.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    bundler: Option<bool>,
    /// Directory to run bundler from (for Gemfile location).
    /// Only used when bundler is true.
    chdir: Option<String>,
    /// Include dependencies when installing.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    include_dependencies: Option<bool>,
    /// Custom installation directory for gems.
    install_dir: Option<String>,
}

fn default_true() -> Option<bool> {
    Some(true)
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("gem".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            version: None,
            user_install: Some(true),
            pre_release: Some(false),
            gem_source: None,
            bundler: Some(false),
            chdir: None,
            include_dependencies: Some(true),
            install_dir: None,
        }
    }
}

#[derive(Debug)]
pub struct Gem;

impl Module for Gem {
    fn get_name(&self) -> &str {
        "gem"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((gem(parse_params(optional_params)?, check_mode)?, None))
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

struct GemClient {
    executable: PathBuf,
    extra_args: Option<String>,
    user_install: bool,
    pre_release: bool,
    gem_source: Option<String>,
    include_dependencies: bool,
    install_dir: Option<String>,
    check_mode: bool,
}

impl GemClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(GemClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            user_install: params.user_install.unwrap(),
            pre_release: params.pre_release.unwrap(),
            gem_source: params.gem_source.clone(),
            include_dependencies: params.include_dependencies.unwrap(),
            install_dir: params.install_dir.clone(),
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("--no-document");
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
        cmd.arg("list").arg("--no-versions");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(GemClient::parse_installed(output.stdout))
    }

    pub fn is_gem_outdated(&self, gem_name: &str) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.arg("outdated");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.starts_with(gem_name)))
    }

    pub fn install(&self, packages: &[String], version: Option<&str>) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if !self.user_install {
            cmd.arg("--no-user-install");
        }

        if self.pre_release {
            cmd.arg("--pre");
        }

        if let Some(source) = &self.gem_source {
            cmd.arg("--source").arg(source);
        }

        if !self.include_dependencies {
            cmd.arg("--no-dependencies");
        }

        if let Some(dir) = &self.install_dir {
            cmd.arg("--install-dir").arg(dir);
        }

        for package in packages {
            if let Some(v) = version {
                cmd.arg(format!("{}:{}", package, v));
            } else {
                cmd.arg(package);
            }
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("update");

        if !self.user_install {
            cmd.arg("--no-user-install");
        }

        if self.pre_release {
            cmd.arg("--pre");
        }

        if let Some(source) = &self.gem_source {
            cmd.arg("--source").arg(source);
        }

        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").arg("--all").arg("--executables");

        if !self.user_install {
            cmd.arg("--no-user-install");
        }

        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

struct BundlerClient {
    executable: PathBuf,
    chdir: Option<String>,
    check_mode: bool,
}

impl BundlerClient {
    pub fn new(check_mode: bool, chdir: Option<String>) -> Result<Self> {
        Ok(BundlerClient {
            executable: PathBuf::from("bundle"),
            chdir,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        if let Some(dir) = &self.chdir {
            cmd.current_dir(dir);
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute 'bundle': {e}. Bundler may not be installed or not in the PATH."
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

    pub fn install(&self) -> Result<IsChanged> {
        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("--quiet");

        let output = self.exec_cmd(&mut cmd, false)?;

        if self.check_mode {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(!stdout.contains("Bundle complete")
            || stderr.contains("Installing")
            || stderr.contains("updated"))
    }

    pub fn check(&self) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.arg("check");

        let output = self.exec_cmd(&mut cmd, false);
        Ok(output.is_ok() && output.unwrap().status.success())
    }
}

fn gem(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.bundler.unwrap() {
        return bundler_install(&params, check_mode);
    }

    let packages: BTreeSet<String> = params.name.iter().cloned().collect();
    let client = GemClient::new(&params, check_mode)?;

    let (p_to_install, p_to_remove, p_to_update) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            (p_to_install, Vec::new(), Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed()?;
            let p_to_remove: Vec<String> = packages.intersection(&installed).cloned().collect();
            (Vec::new(), p_to_remove, Vec::new())
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let p_to_install: Vec<String> = packages.difference(&installed).cloned().collect();
            let p_to_update: Vec<String> = packages
                .intersection(&installed)
                .filter(|p| client.is_gem_outdated(p).unwrap_or(false))
                .cloned()
                .collect();
            (p_to_install, Vec::new(), p_to_update)
        }
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install, params.version.as_deref())?;
        true
    } else {
        false
    };

    let update_changed = if !p_to_update.is_empty() {
        logger::add(&p_to_update);
        client.update(&p_to_update)?;
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
        changed: install_changed || update_changed || remove_changed,
        output: None,
        extra: Some(value::to_value(json!({
            "installed_gems": p_to_install,
            "updated_gems": p_to_update,
            "removed_gems": p_to_remove,
        }))?),
    })
}

fn bundler_install(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = BundlerClient::new(check_mode, params.chdir.clone())?;

    if check_mode {
        let needs_install = !client.check()?;
        return Ok(ModuleResult {
            changed: needs_install,
            output: None,
            extra: Some(value::to_value(
                json!({"bundler": true, "chdir": params.chdir}),
            )?),
        });
    }

    let changed = client.install()?;

    Ok(ModuleResult {
        changed,
        output: None,
        extra: Some(value::to_value(
            json!({"bundler": true, "chdir": params.chdir}),
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
            name: bundler
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["bundler".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/gem
            extra_args: "--verbose"
            name:
              - rails
              - bundler
            state: latest
            version: "7.0.0"
            user_install: false
            pre_release: true
            gem_source: https://gems.example.com
            include_dependencies: false
            install_dir: /opt/gems
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/gem".to_owned()),
                extra_args: Some("--verbose".to_owned()),
                name: vec!["rails".to_owned(), "bundler".to_owned()],
                state: Some(State::Latest),
                version: Some("7.0.0".to_owned()),
                user_install: Some(false),
                pre_release: Some(true),
                gem_source: Some("https://gems.example.com".to_owned()),
                bundler: Some(false),
                chdir: None,
                include_dependencies: Some(false),
                install_dir: Some("/opt/gems".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_version_constraint() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rake
            version: ">= 13.0"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["rake".to_owned()],
                version: Some(">= 13.0".to_owned()),
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_bundler() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            bundler: true
            chdir: /app
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                bundler: Some(true),
                chdir: Some("/app".to_owned()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bundler
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_gem_client_parse_installed() {
        let stdout = r#"bundler (2.4.0)
rails (7.0.0, 6.1.0)
rake (13.0.0)
rbs (2.8.0)
"#
        .as_bytes();
        let parsed = GemClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "bundler".to_owned(),
                "rails".to_owned(),
                "rake".to_owned(),
                "rbs".to_owned(),
            ])
        );
    }

    #[test]
    fn test_gem_client_new() {
        let params = Params::default();
        let result = GemClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.executable, PathBuf::from("gem"));
        assert!(client.user_install);
        assert!(!client.pre_release);
    }

    #[test]
    fn test_gem_client_exec_cmd_with_nonexistent_executable() {
        let params = Params {
            executable: Some("definitely-not-a-real-executable".to_owned()),
            ..Default::default()
        };
        let client = GemClient::new(&params, false).unwrap();
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
