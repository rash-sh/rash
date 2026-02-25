/// ANCHOR: module
/// # composer
///
/// Dependency Manager for PHP.
///
/// Composer is a tool for dependency management in PHP. It allows you to declare
/// the dependent libraries your project needs and it installs them in your project for you.
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
/// - name: Install dependencies from composer.lock
///   composer:
///     command: install
///     working_dir: /path/to/project
///
/// - name: Install dependencies without dev packages
///   composer:
///     command: install
///     working_dir: /path/to/project
///     no_dev: true
///
/// - name: Install a new package
///   composer:
///     command: require
///     arguments: my/package
///     working_dir: /path/to/project
///
/// - name: Install a package globally
///   composer:
///     command: require
///     global_command: true
///     arguments: my/package
///
/// - name: Update all dependencies
///   composer:
///     command: update
///     working_dir: /path/to/project
///
/// - name: Create a new project
///   composer:
///     command: create-project
///     arguments: package/package /path/to/project ~1.0
///     working_dir: /tmp
///     prefer_dist: true
///
/// - name: Optimize autoloader for production
///   composer:
///     command: dump-autoload
///     working_dir: /path/to/project
///     optimize_autoloader: true
///     no_dev: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use shlex::split;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("composer".to_owned())
}

fn default_command() -> Option<String> {
    Some("install".to_owned())
}

fn default_arguments() -> Option<String> {
    Some(String::new())
}

fn default_true() -> Option<bool> {
    Some(true)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "kebab-case")]
#[allow(dead_code)]
enum ComposerCommand {
    Install,
    Update,
    Require,
    Remove,
    CreateProject,
    DumpAutoload,
    ClearCache,
    SelfUpdate,
    Show,
    Init,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to composer executable on the remote host, if composer is not in PATH.
    /// **[default: `"composer"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Path to PHP executable on the remote host, if PHP is not in PATH.
    executable_php: Option<String>,
    /// Composer command to run.
    /// **[default: `"install"`]**
    #[serde(default = "default_command")]
    command: Option<String>,
    /// Composer arguments like required package, version and so on.
    /// **[default: `""`]**
    #[serde(default = "default_arguments")]
    arguments: Option<String>,
    /// Directory of your project (see --working-dir).
    /// This is required when the command is not run globally.
    working_dir: Option<String>,
    /// Runs the specified command globally.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    global_command: Option<bool>,
    /// Disables installation of require-dev packages.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    no_dev: Option<bool>,
    /// Optimize autoloader during autoloader dump.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    optimize_autoloader: Option<bool>,
    /// Autoload classes from classmap only.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    classmap_authoritative: Option<bool>,
    /// Uses APCu to cache found/not-found classes.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    apcu_autoloader: Option<bool>,
    /// Forces installation from package dist even for dev versions.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    prefer_dist: Option<bool>,
    /// Forces installation from package sources when possible.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    prefer_source: Option<bool>,
    /// Ignore php, hhvm, lib-* and ext-* requirements.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    ignore_platform_reqs: Option<bool>,
    /// Skips the execution of all scripts defined in composer.json.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_scripts: Option<bool>,
    /// Disables all plugins.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_plugins: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("composer".to_owned()),
            executable_php: None,
            command: Some("install".to_owned()),
            arguments: Some(String::new()),
            working_dir: None,
            global_command: Some(false),
            no_dev: Some(true),
            optimize_autoloader: Some(true),
            classmap_authoritative: Some(false),
            apcu_autoloader: Some(false),
            prefer_dist: Some(false),
            prefer_source: Some(false),
            ignore_platform_reqs: Some(false),
            no_scripts: Some(false),
            no_plugins: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Composer;

impl Module for Composer {
    fn get_name(&self) -> &str {
        "composer"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((composer(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct ComposerClient {
    composer_executable: PathBuf,
    php_executable: Option<PathBuf>,
    check_mode: bool,
}

impl ComposerClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(ComposerClient {
            composer_executable: PathBuf::from(params.executable.as_ref().unwrap()),
            php_executable: params.executable_php.as_ref().map(PathBuf::from),
            check_mode,
        })
    }

    fn get_cmd(&self) -> ProcessCommand {
        if let Some(ref php) = self.php_executable {
            let mut cmd = ProcessCommand::new(php);
            cmd.arg(&self.composer_executable);
            cmd
        } else {
            ProcessCommand::new(&self.composer_executable)
        }
    }

    fn exec_cmd(&self, cmd: &mut ProcessCommand) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.composer_executable.display()
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn run_command(&self, params: &Params) -> Result<(bool, String)> {
        if self.check_mode {
            return Ok((false, String::new()));
        }

        let mut cmd = self.get_cmd();

        cmd.arg("--no-ansi");
        cmd.arg("--no-interaction");
        cmd.arg("--no-progress");

        let command = params.command.as_ref().unwrap();
        cmd.arg(command);

        if let Some(ref working_dir) = params.working_dir
            && !params.global_command.unwrap()
        {
            cmd.arg("--working-dir").arg(working_dir);
        }

        if params.global_command.unwrap() {
            cmd.arg("--global");
        }

        if params.no_dev.unwrap() {
            cmd.arg("--no-dev");
        }

        if params.optimize_autoloader.unwrap() {
            cmd.arg("--optimize-autoloader");
        }

        if params.classmap_authoritative.unwrap() {
            cmd.arg("--classmap-authoritative");
        }

        if params.apcu_autoloader.unwrap() {
            cmd.arg("--apcu-autoloader");
        }

        if params.prefer_dist.unwrap() {
            cmd.arg("--prefer-dist");
        }

        if params.prefer_source.unwrap() {
            cmd.arg("--prefer-source");
        }

        if params.ignore_platform_reqs.unwrap() {
            cmd.arg("--ignore-platform-reqs");
        }

        if params.no_scripts.unwrap() {
            cmd.arg("--no-scripts");
        }

        if params.no_plugins.unwrap() {
            cmd.arg("--no-plugins");
        }

        if let Some(ref arguments) = params.arguments
            && !arguments.is_empty()
            && let Some(args) = split(arguments)
        {
            cmd.args(&args);
        }

        let output = self.exec_cmd(&mut cmd)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Composer command failed: {}", stderr.trim()),
            ));
        }

        let changed = !stdout.contains("Nothing to install, update or remove")
            && !stdout.contains("Lock file is not being updated")
            && !stdout.contains("Generating autoload files")
            || command == "update"
            || command == "self-update";

        Ok((changed, stdout))
    }
}

fn composer(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.working_dir.is_none() && !params.global_command.unwrap() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "working_dir is required when not using global_command",
        ));
    }

    let client = ComposerClient::new(&params, check_mode)?;

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let (changed, stdout) = client.run_command(&params)?;

    Ok(ModuleResult::new(changed, None, Some(stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: install
            working_dir: /path/to/project
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Some("install".to_owned()));
        assert_eq!(params.working_dir, Some("/path/to/project".to_owned()));
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/local/bin/composer
            executable_php: /usr/bin/php
            command: require
            arguments: my/package
            working_dir: /path/to/project
            global_command: false
            no_dev: true
            optimize_autoloader: true
            classmap_authoritative: true
            apcu_autoloader: true
            prefer_dist: true
            prefer_source: false
            ignore_platform_reqs: true
            no_scripts: true
            no_plugins: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.executable,
            Some("/usr/local/bin/composer".to_owned())
        );
        assert_eq!(params.executable_php, Some("/usr/bin/php".to_owned()));
        assert_eq!(params.command, Some("require".to_owned()));
        assert_eq!(params.arguments, Some("my/package".to_owned()));
        assert_eq!(params.working_dir, Some("/path/to/project".to_owned()));
        assert!(!params.global_command.unwrap());
        assert!(params.no_dev.unwrap());
        assert!(params.optimize_autoloader.unwrap());
        assert!(params.classmap_authoritative.unwrap());
        assert!(params.apcu_autoloader.unwrap());
        assert!(params.prefer_dist.unwrap());
        assert!(!params.prefer_source.unwrap());
        assert!(params.ignore_platform_reqs.unwrap());
        assert!(params.no_scripts.unwrap());
        assert!(params.no_plugins.unwrap());
    }

    #[test]
    fn test_parse_params_global_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: require
            global_command: true
            arguments: my/package
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.global_command.unwrap());
        assert!(params.working_dir.is_none());
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: install
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_composer_missing_working_dir() {
        let params = Params {
            working_dir: None,
            global_command: Some(false),
            ..Default::default()
        };
        let result = composer(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_composer_check_mode() {
        let params = Params {
            working_dir: Some("/tmp".to_owned()),
            global_command: Some(false),
            ..Default::default()
        };
        let result = composer(params, true).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_composer_client_new() {
        let params = Params::default();
        let result = ComposerClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.composer_executable, PathBuf::from("composer"));
        assert!(client.php_executable.is_none());
    }

    #[test]
    fn test_composer_client_with_php() {
        let params = Params {
            executable_php: Some("/usr/bin/php8.2".to_owned()),
            ..Default::default()
        };
        let result = ComposerClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(
            client.php_executable,
            Some(PathBuf::from("/usr/bin/php8.2"))
        );
    }
}
