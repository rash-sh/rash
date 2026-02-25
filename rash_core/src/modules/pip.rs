/// ANCHOR: module
/// # pip
///
/// Manage Python packages with pip.
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
/// - name: Install requests package
///   pip:
///     name: requests
///     state: present
///
/// - name: Install specific version
///   pip:
///     name: django
///     version: "4.2.0"
///     state: present
///
/// - name: Install multiple packages
///   pip:
///     name:
///       - requests
///       - flask
///       - gunicorn
///     state: latest
///
/// - name: Install from requirements.txt
///   pip:
///     requirements: /app/requirements.txt
///
/// - name: Install in virtualenv
///   pip:
///     name: requests
///     virtualenv: /app/venv
///
/// - name: Remove package
///   pip:
///     name: requests
///     state: absent
///
/// - name: Force reinstall package
///   pip:
///     name: requests
///     state: forcereinstall
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};

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
    Some("pip3".to_owned())
}

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
    Latest,
    Forcereinstall,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the pip binary to use.
    /// **[default: `"pip3"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional arguments to pass to pip.
    extra_args: Option<String>,
    /// The name of a Python library to install or remove.
    /// Can be a list of names.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// The state of the Python library.
    /// `present` will ensure the package is installed.
    /// `absent` will remove the package.
    /// `latest` will update to the latest version.
    /// `forcereinstall` will force reinstall the package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The version to install (e.g., "1.2.3").
    /// Used with `name` to install a specific version.
    version: Option<String>,
    /// Path to a requirements.txt file for installing packages.
    requirements: Option<String>,
    /// Path to a virtualenv directory to install packages into.
    virtualenv: Option<String>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            executable: Some("pip3".to_owned()),
            extra_args: None,
            name: Vec::new(),
            state: Some(State::Present),
            version: None,
            requirements: None,
            virtualenv: None,
        }
    }
}

#[derive(Debug)]
pub struct Pip;

impl Module for Pip {
    fn get_name(&self) -> &str {
        "pip"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((pip(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct PipClient {
    executable: PathBuf,
    extra_args: Option<String>,
    check_mode: bool,
}

impl PipClient {
    pub fn new(executable: &Path, extra_args: Option<String>, check_mode: bool) -> Result<Self> {
        Ok(PipClient {
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
    fn parse_installed(stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    return None;
                }
                line.split("==").next().map(|s| s.to_lowercase())
            })
            .collect()
    }

    pub fn get_installed(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("freeze");

        let output = self.exec_cmd(&mut cmd, true)?;
        Ok(PipClient::parse_installed(output.stdout))
    }

    pub fn get_outdated(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--outdated");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let packages: BTreeSet<String> = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_lowercase())
            })
            .collect();
        Ok(packages)
    }

    pub fn install(&self, packages: &[String], force_reinstall: bool) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("--no-cache-dir");

        if force_reinstall {
            cmd.arg("--force-reinstall");
        }

        cmd.args(packages);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn install_requirements(&self, requirements_path: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("install")
            .arg("--no-cache-dir")
            .arg("-r")
            .arg(requirements_path);
        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn uninstall(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        };

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").arg("--yes").args(packages);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn get_pip_executable(params: &Params) -> PathBuf {
    if let Some(ref executable) = params.executable {
        return PathBuf::from(executable);
    }

    if let Some(ref venv_path) = params.virtualenv {
        let venv_pip = Path::new(venv_path).join("bin").join("pip");
        if venv_pip.exists() {
            return venv_pip;
        }
    }

    PathBuf::from("pip3")
}

fn pip(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.name.is_empty() && params.requirements.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Either 'name' or 'requirements' must be provided",
        ));
    }

    let executable = get_pip_executable(&params);
    let packages: Vec<String> = if let Some(ref version) = params.version {
        if params.name.len() != 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "When using 'version', exactly one package name must be provided",
            ));
        }
        vec![format!("{}=={}", params.name[0], version)]
    } else {
        params.name.clone()
    };

    let client = PipClient::new(&executable, params.extra_args.clone(), check_mode)?;

    let mut p_to_install: Vec<String> = Vec::new();
    let mut p_to_remove: Vec<String> = Vec::new();
    let mut requirements_installed = false;
    let mut force_reinstall = false;

    match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed()?;
            p_to_install = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split("==").next().unwrap_or(p).to_lowercase();
                    !installed.contains(&pkg_name)
                })
                .cloned()
                .collect();
        }
        State::Absent => {
            let installed = client.get_installed()?;
            p_to_remove = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split("==").next().unwrap_or(p).to_lowercase();
                    installed.contains(&pkg_name)
                })
                .cloned()
                .collect();
        }
        State::Latest => {
            let installed = client.get_installed()?;
            let outdated = client.get_outdated()?;

            p_to_install = packages
                .iter()
                .filter(|p| {
                    let pkg_name = p.split("==").next().unwrap_or(p).to_lowercase();
                    !installed.contains(&pkg_name) || outdated.contains(&pkg_name)
                })
                .cloned()
                .collect();
        }
        State::Forcereinstall => {
            p_to_install = packages.clone();
            force_reinstall = true;
        }
    }

    if let Some(ref requirements_path) = params.requirements {
        if !check_mode {
            client.install_requirements(requirements_path)?;
            requirements_installed = true;
        } else {
            requirements_installed = true;
        }
    }

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install, force_reinstall)?;
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
        changed: install_changed || remove_changed || requirements_installed,
        output: None,
        extra: Some(value::to_value(
            json!({"installed_packages": p_to_install, "removed_packages": p_to_remove, "requirements_installed": requirements_installed}),
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
            name: requests
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["requests".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/pip3
            extra_args: "--index-url https://pypi.org/simple"
            name:
              - requests
              - flask
            state: latest
            version: "1.0.0"
            requirements: /app/requirements.txt
            virtualenv: /app/venv
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                executable: Some("/usr/bin/pip3".to_owned()),
                extra_args: Some("--index-url https://pypi.org/simple".to_owned()),
                name: vec!["requests".to_owned(), "flask".to_owned()],
                state: Some(State::Latest),
                version: Some("1.0.0".to_owned()),
                requirements: Some("/app/requirements.txt".to_owned()),
                virtualenv: Some("/app/venv".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_forcereinstall() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: requests
            state: forcereinstall
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["requests".to_owned()],
                state: Some(State::Forcereinstall),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: requests
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_pip_client_parse_installed() {
        let stdout = r#"requests==2.28.0
flask==2.0.1
django==4.2.0
"#
        .as_bytes();
        let parsed = PipClient::parse_installed(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "requests".to_owned(),
                "flask".to_owned(),
                "django".to_owned(),
            ])
        );
    }

    #[test]
    fn test_pip_missing_name_and_requirements() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = pip(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Either 'name' or 'requirements' must be provided")
        );
    }

    #[test]
    fn test_pip_version_with_multiple_packages() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - requests
              - flask
            version: "1.0.0"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = pip(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("exactly one package name must be provided")
        );
    }
}
