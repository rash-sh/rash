/// ANCHOR: module
/// # npm
///
/// Manage Node.js packages with npm.
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
/// - name: Install package
///   npm:
///     name: coffee-script
///     path: /app/location
///
/// - name: Install specific version
///   npm:
///     name: coffee-script
///     version: "1.6.1"
///     path: /app/location
///
/// - name: Install package globally
///   npm:
///     name: coffee-script
///     global: true
///
/// - name: Remove package
///   npm:
///     name: coffee-script
///     global: true
///     state: absent
///
/// - name: Install from custom registry
///   npm:
///     name: coffee-script
///     registry: "http://registry.mysite.com"
///
/// - name: Install packages from package.json
///   npm:
///     path: /app/location
///
/// - name: Update packages to latest
///   npm:
///     path: /app/location
///     state: latest
///
/// - name: Run npm ci
///   npm:
///     path: /app/location
///     ci: true
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
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("npm".to_owned())
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
    Latest,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The executable location for npm.
    /// **[default: `"npm"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Install packages based on package-lock file, same as running `npm ci`.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    ci: Option<bool>,
    /// Use the `--force` flag when installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    force: Option<bool>,
    /// Install the node.js library globally.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    global: Option<bool>,
    /// Use the `--ignore-scripts` flag when installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    ignore_scripts: Option<bool>,
    /// The name of a node.js library to install.
    name: Option<String>,
    /// Use the `--no-bin-links` flag when installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_bin_links: Option<bool>,
    /// Use the `--no-optional` flag when installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    no_optional: Option<bool>,
    /// The base path where to install the node.js libraries.
    path: Option<String>,
    /// Install dependencies in production mode, excluding devDependencies.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    production: Option<bool>,
    /// The registry to install modules from.
    registry: Option<String>,
    /// The state of the node.js library.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Use the `--unsafe-perm` flag when installing.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    unsafe_perm: Option<bool>,
    /// The version to be installed.
    version: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("npm".to_owned()),
            ci: Some(false),
            force: Some(false),
            global: Some(false),
            ignore_scripts: Some(false),
            name: None,
            no_bin_links: Some(false),
            no_optional: Some(false),
            path: None,
            production: Some(false),
            registry: None,
            state: Some(State::Present),
            unsafe_perm: Some(false),
            version: None,
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
    path: Option<PathBuf>,
    global: bool,
    registry: Option<String>,
    check_mode: bool,
}

impl NpmClient {
    pub fn new(
        executable: &Path,
        path: Option<&str>,
        global: bool,
        registry: Option<String>,
        check_mode: bool,
    ) -> Result<Self> {
        Ok(NpmClient {
            executable: executable.to_path_buf(),
            path: path.map(PathBuf::from),
            global,
            registry,
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        cmd.arg("--json");
        if let Some(path) = &self.path {
            cmd.current_dir(path);
        }
        if let Some(registry) = &self.registry {
            cmd.arg("--registry").arg(registry);
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        trace!("command: `{cmd:?}`");
        let output = cmd
            .output()
            .map_err(|e| Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            ))?;
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(output)
    }

    pub fn list_installed(&self, name: Option<&str>) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list");
        if self.global {
            cmd.arg("--global");
        }
        cmd.arg("--depth=0");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = Self::parse_npm_list(&stdout, name);
        Ok(packages)
    }

    fn parse_npm_list(stdout: &str, filter_name: Option<&str>) -> BTreeSet<String> {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
            let mut packages = BTreeSet::new();
            if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                for (name, info) in deps {
                    if (filter_name.is_none() || filter_name == Some(name.as_str()))
                        && let Some(version) = info.get("version").and_then(|v| v.as_str())
                    {
                        packages.insert(format!("{}@{}", name, version));
                    }
                }
            }
            packages
        } else {
            BTreeSet::new()
        }
    }

    pub fn get_outdated(&self, name: Option<&str>) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("outdated");
        if self.global {
            cmd.arg("--global");
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = Self::parse_outdated(&stdout, name);
        Ok(packages)
    }

    fn parse_outdated(stdout: &str, filter_name: Option<&str>) -> BTreeSet<String> {
        if stdout.trim().is_empty() {
            return BTreeSet::new();
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(stdout) {
            let mut packages = BTreeSet::new();
            if let Some(obj) = json.as_object() {
                for (pkg_name, _) in obj {
                    if filter_name.is_none() || filter_name == Some(pkg_name.as_str()) {
                        packages.insert(pkg_name.clone());
                    }
                }
            }
            packages
        } else {
            BTreeSet::new()
        }
    }

    pub fn install(&self, params: &Params, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if self.global {
            cmd.arg("--global");
        }

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }
        if params.ignore_scripts.unwrap_or(false) {
            cmd.arg("--ignore-scripts");
        }
        if params.no_bin_links.unwrap_or(false) {
            cmd.arg("--no-bin-links");
        }
        if params.no_optional.unwrap_or(false) {
            cmd.arg("--no-optional");
        }
        if params.production.unwrap_or(false) {
            cmd.arg("--production");
        }
        if params.unsafe_perm.unwrap_or(false) {
            cmd.arg("--unsafe-perm");
        }

        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn install_from_package_json(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if self.global {
            cmd.arg("--global");
        }

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }
        if params.ignore_scripts.unwrap_or(false) {
            cmd.arg("--ignore-scripts");
        }
        if params.no_bin_links.unwrap_or(false) {
            cmd.arg("--no-bin-links");
        }
        if params.no_optional.unwrap_or(false) {
            cmd.arg("--no-optional");
        }
        if params.production.unwrap_or(false) {
            cmd.arg("--production");
        }
        if params.unsafe_perm.unwrap_or(false) {
            cmd.arg("--unsafe-perm");
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn ci(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("ci");

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }
        if params.ignore_scripts.unwrap_or(false) {
            cmd.arg("--ignore-scripts");
        }
        if params.no_bin_links.unwrap_or(false) {
            cmd.arg("--no-bin-links");
        }
        if params.no_optional.unwrap_or(false) {
            cmd.arg("--no-optional");
        }
        if params.production.unwrap_or(false) {
            cmd.arg("--production");
        }
        if params.unsafe_perm.unwrap_or(false) {
            cmd.arg("--unsafe-perm");
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn update(&self, params: &Params, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("update");

        if self.global {
            cmd.arg("--global");
        }

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }
        if params.ignore_scripts.unwrap_or(false) {
            cmd.arg("--ignore-scripts");
        }
        if params.no_bin_links.unwrap_or(false) {
            cmd.arg("--no-bin-links");
        }
        if params.no_optional.unwrap_or(false) {
            cmd.arg("--no-optional");
        }
        if params.production.unwrap_or(false) {
            cmd.arg("--production");
        }
        if params.unsafe_perm.unwrap_or(false) {
            cmd.arg("--unsafe-perm");
        }

        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove(&self, params: &Params, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall");

        if self.global {
            cmd.arg("--global");
        }

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }

        cmd.args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn is_package_installed(&self, name: &str, version: Option<&str>) -> Result<bool> {
        let installed = self.list_installed(Some(name))?;
        if let Some(ver) = version {
            Ok(installed.contains(&format!("{}@{}", name, ver)))
        } else {
            Ok(!installed.is_empty())
        }
    }
}

fn npm(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let state = params.state.clone().unwrap();
    let client = NpmClient::new(
        Path::new(&params.executable.clone().unwrap()),
        params.path.as_deref(),
        params.global.unwrap_or(false),
        params.registry.clone(),
        check_mode,
    )?;

    if params.ci.unwrap_or(false) {
        if params.path.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "path is required when using ci",
            ));
        }
        client.ci(&params)?;
        return Ok(ModuleResult {
            changed: true,
            output: None,
            extra: Some(value::to_value(json!({"ci": true}))?),
        });
    }

    let package_name = params.name.clone();
    let version = params.version.as_deref();

    if package_name.is_none() && params.path.is_some() {
        match state {
            State::Present => {
                client.install_from_package_json(&params)?;
                return Ok(ModuleResult {
                    changed: true,
                    output: None,
                    extra: Some(value::to_value(json!({"package_json": true}))?),
                });
            }
            State::Latest => {
                client.update(&params, &[])?;
                return Ok(ModuleResult {
                    changed: true,
                    output: None,
                    extra: Some(value::to_value(json!({"updated": true}))?),
                });
            }
            State::Absent => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required when state is absent",
                ));
            }
        }
    }

    let name = package_name
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "name or path is required"))?;

    let full_name = if let Some(v) = version {
        format!("{}@{}", name, v)
    } else {
        name.clone()
    };

    let is_installed = client.is_package_installed(&name, version)?;
    let is_outdated = if is_installed && matches!(state, State::Latest) {
        !client.get_outdated(Some(&name))?.is_empty()
    } else {
        false
    };

    match state {
        State::Present => {
            if is_installed {
                return Ok(ModuleResult {
                    changed: false,
                    output: None,
                    extra: Some(value::to_value(json!({
                        "package": full_name,
                        "installed": true
                    }))?),
                });
            }
            logger::add(std::slice::from_ref(&full_name));
            client.install(&params, std::slice::from_ref(&full_name))?;
            Ok(ModuleResult {
                changed: true,
                output: None,
                extra: Some(value::to_value(json!({
                    "package": full_name,
                    "installed": true
                }))?),
            })
        }
        State::Absent => {
            if !is_installed {
                return Ok(ModuleResult {
                    changed: false,
                    output: None,
                    extra: Some(value::to_value(json!({
                        "package": full_name,
                        "removed": false
                    }))?),
                });
            }
            logger::remove(std::slice::from_ref(&full_name));
            client.remove(&params, std::slice::from_ref(&full_name))?;
            Ok(ModuleResult {
                changed: true,
                output: None,
                extra: Some(value::to_value(json!({
                    "package": full_name,
                    "removed": true
                }))?),
            })
        }
        State::Latest => {
            if is_installed && !is_outdated {
                return Ok(ModuleResult {
                    changed: false,
                    output: None,
                    extra: Some(value::to_value(json!({
                        "package": full_name,
                        "updated": false
                    }))?),
                });
            }
            logger::add(std::slice::from_ref(&full_name));
            if is_installed {
                client.update(&params, std::slice::from_ref(&full_name))?;
            } else {
                client.install(&params, std::slice::from_ref(&full_name))?;
            }
            Ok(ModuleResult {
                changed: true,
                output: None,
                extra: Some(value::to_value(json!({
                    "package": full_name,
                    "updated": true
                }))?),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: coffee-script
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: Some("coffee-script".to_owned()),
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /opt/nvm/v0.10.1/bin/npm
            name: coffee-script
            version: "1.6.1"
            path: /app/location
            global: true
            state: latest
            registry: "http://registry.mysite.com"
            production: true
            ignore_scripts: true
            no_bin_links: true
            no_optional: true
            unsafe_perm: true
            force: true
            ci: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/opt/nvm/v0.10.1/bin/npm".to_owned()),
                name: Some("coffee-script".to_owned()),
                version: Some("1.6.1".to_owned()),
                path: Some("/app/location".to_owned()),
                global: Some(true),
                state: Some(State::Latest),
                registry: Some("http://registry.mysite.com".to_owned()),
                production: Some(true),
                ignore_scripts: Some(true),
                no_bin_links: Some(true),
                no_optional: Some(true),
                unsafe_perm: Some(true),
                force: Some(true),
                ci: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: coffee-script
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_npm_client_parse_npm_list() {
        let stdout = r#"{
            "dependencies": {
                "express": {"version": "4.18.2"},
                "lodash": {"version": "4.17.21"}
            }
        }"#;
        let parsed = NpmClient::parse_npm_list(stdout, None);
        assert!(parsed.contains("express@4.18.2"));
        assert!(parsed.contains("lodash@4.17.21"));
    }

    #[test]
    fn test_npm_client_parse_npm_list_with_filter() {
        let stdout = r#"{
            "dependencies": {
                "express": {"version": "4.18.2"},
                "lodash": {"version": "4.17.21"}
            }
        }"#;
        let parsed = NpmClient::parse_npm_list(stdout, Some("express"));
        assert!(parsed.contains("express@4.18.2"));
        assert!(!parsed.contains("lodash@4.17.21"));
    }

    #[test]
    fn test_npm_client_parse_outdated() {
        let stdout = r#"{
            "express": {"current": "4.18.0", "latest": "4.18.2"},
            "lodash": {"current": "4.17.20", "latest": "4.17.21"}
        }"#;
        let parsed = NpmClient::parse_outdated(stdout, None);
        assert!(parsed.contains("express"));
        assert!(parsed.contains("lodash"));
    }

    #[test]
    fn test_npm_client_parse_outdated_with_filter() {
        let stdout = r#"{
            "express": {"current": "4.18.0", "latest": "4.18.2"},
            "lodash": {"current": "4.17.20", "latest": "4.17.21"}
        }"#;
        let parsed = NpmClient::parse_outdated(stdout, Some("express"));
        assert!(parsed.contains("express"));
        assert!(!parsed.contains("lodash"));
    }
}
