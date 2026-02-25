/// ANCHOR: module
/// # npm
///
/// Manage Node.js packages with npm, Node.js package manager.
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
/// - name: Install a package
///   npm:
///     name: typescript
///     state: present
///
/// - name: Install specific version of a package
///   npm:
///     name: express
///     version: "4.18.0"
///     state: present
///
/// - name: Install package globally
///   npm:
///     name: typescript
///     global: true
///     state: latest
///
/// - name: Install dependencies from package.json
///   npm:
///     path: /app
///
/// - name: Install production dependencies only
///   npm:
///     path: /app
///     production: true
///
/// - name: Install using npm ci (clean install)
///   npm:
///     path: /app
///     ci: true
///
/// - name: Remove a package
///   npm:
///     name: typescript
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
use std::path::PathBuf;
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
    Some("npm".to_owned())
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
    /// Path of the npm binary to use.
    /// **[default: `"npm"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to npm.
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
    /// The version of the package to install.
    /// Only used with `state: present`.
    version: Option<String>,
    /// Install the package globally.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    global: Option<bool>,
    /// The directory containing package.json for npm operations.
    /// Equivalent to running npm with `--prefix` flag.
    path: Option<String>,
    /// Install only production dependencies.
    /// Only used when no package name is specified.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    production: Option<bool>,
    /// Use npm ci instead of npm install for clean installs.
    /// Runs `npm ci` which installs from package-lock.json.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    ci: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("npm".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            version: None,
            global: Some(false),
            path: None,
            production: Some(false),
            ci: Some(false),
        }
    }
}

#[derive(Debug)]
pub struct Npm;

impl Module for Npm {
    fn get_name(&self) -> &str {
        "npm"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((npm(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct NpmClient {
    executable: PathBuf,
    extra_args: Option<String>,
    global: bool,
    path: Option<String>,
    check_mode: bool,
}

impl NpmClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(NpmClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            global: params.global.unwrap(),
            path: params.path.clone(),
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        if let Some(ref path) = self.path {
            cmd.arg("--prefix").arg(path);
        }
        if self.global {
            cmd.arg("--global");
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

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--depth=0").arg("--json");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or(serde_json::json!({}));

        let mut packages = BTreeSet::new();
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for name in deps.keys() {
                packages.insert(name.clone());
            }
        }

        Ok(packages)
    }

    pub fn is_package_outdated(&self, package_name: &str) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.arg("outdated").arg("--json").arg(package_name);

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).unwrap_or(serde_json::json!({}));

        Ok(parsed.get(package_name).is_some())
    }

    pub fn install(&self, packages: &[String], version: Option<&str>) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        for package in packages {
            if let Some(v) = version {
                cmd.arg(format!("{}@{}", package, v));
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
        cmd.arg("update").args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn install_deps(&self, production: bool, ci: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();

        if ci {
            cmd.arg("ci");
        } else {
            cmd.arg("install");
        }

        if production {
            cmd.arg("--production");
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(stdout.contains("added")
            || stdout.contains("updated")
            || stderr.contains("added")
            || stderr.contains("updated"))
    }

    pub fn check_install_needed(&self) -> Result<bool> {
        let mut cmd = self.get_cmd();
        cmd.arg("ls");

        let output = self.exec_cmd(&mut cmd, false);
        Ok(output.is_err() || !output.unwrap().status.success())
    }
}

fn npm(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = NpmClient::new(&params, check_mode)?;

    if params.name.is_empty() {
        return install_deps(&params, &client);
    }

    let packages: BTreeSet<String> = params.name.iter().cloned().collect();

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
                .filter(|p| client.is_package_outdated(p).unwrap_or(false))
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
            "installed_packages": p_to_install,
            "updated_packages": p_to_update,
            "removed_packages": p_to_remove,
        }))?),
    })
}

fn install_deps(params: &Params, client: &NpmClient) -> Result<ModuleResult> {
    if client.check_mode {
        let needs_install = client.check_install_needed()?;
        return Ok(ModuleResult {
            changed: needs_install,
            output: None,
            extra: Some(value::to_value(
                json!({"path": params.path, "production": params.production, "ci": params.ci}),
            )?),
        });
    }

    let changed = client.install_deps(params.production.unwrap(), params.ci.unwrap())?;

    Ok(ModuleResult {
        changed,
        output: None,
        extra: Some(value::to_value(
            json!({"path": params.path, "production": params.production, "ci": params.ci}),
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
            name: typescript
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["typescript".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/local/bin/npm
            extra_args: "--verbose"
            name:
              - typescript
              - express
            state: latest
            version: "4.18.0"
            global: true
            path: /app
            production: true
            ci: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/local/bin/npm".to_owned()),
                extra_args: Some("--verbose".to_owned()),
                name: vec!["typescript".to_owned(), "express".to_owned()],
                state: Some(State::Latest),
                version: Some("4.18.0".to_owned()),
                global: Some(true),
                path: Some("/app".to_owned()),
                production: Some(true),
                ci: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_path_only() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /app
            production: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: Some("/app".to_owned()),
                production: Some(true),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_ci() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /app
            ci: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: Some("/app".to_owned()),
                ci: Some(true),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: typescript
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_npm_parse_json_output() {
        let stdout = r#"{
  "dependencies": {
    "express": { "version": "4.18.0" },
    "lodash": { "version": "4.17.21" },
    "typescript": { "version": "5.0.0" }
  }
}"#;
        let parsed: serde_json::Value = serde_json::from_str(stdout).unwrap();
        let mut packages = BTreeSet::new();
        if let Some(deps) = parsed.get("dependencies").and_then(|d| d.as_object()) {
            for name in deps.keys() {
                packages.insert(name.clone());
            }
        }

        assert_eq!(
            packages,
            BTreeSet::from([
                "express".to_owned(),
                "lodash".to_owned(),
                "typescript".to_owned(),
            ])
        );
    }

    #[test]
    fn test_npm_client_new() {
        let params = Params::default();
        let result = NpmClient::new(&params, false);
        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.executable, PathBuf::from("npm"));
        assert!(!client.global);
    }

    #[test]
    fn test_npm_client_exec_cmd_with_nonexistent_executable() {
        let params = Params {
            executable: Some("definitely-not-a-real-executable".to_owned()),
            ..Default::default()
        };
        let client = NpmClient::new(&params, false).unwrap();
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
