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
/// - name: Install bottle python package
///   pip:
///     name: bottle
///
/// - name: Install bottle python package on version 0.11
///   pip:
///     name: bottle==0.11
///
/// - name: Install multiple python packages
///   pip:
///     name:
///       - django>1.11.0,<1.12.0
///       - bottle>0.10,<0.20,!=0.11
///
/// - name: Install bottle into the specified virtualenv
///   pip:
///     name: bottle
///     virtualenv: /my_app/venv
///
/// - name: Install specified python requirements
///   pip:
///     requirements: /my_app/requirements.txt
///
/// - name: Install bottle using pip3.13 executable
///   pip:
///     name: bottle
///     executable: pip3.13
///
/// - name: Install bottle with custom index URL
///   pip:
///     name: bottle
///     extra_args: -i https://example.com/pypi/simple
///
/// - name: Remove bottle python package
///   pip:
///     name: bottle
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
    Some("pip".to_owned())
}

fn default_virtualenv_command() -> Option<String> {
    Some("virtualenv".to_owned())
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Present,
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
    /// The name of a Python library to install or the URL of a remote package.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    name: Vec<String>,
    /// The state of the package.
    /// `present` will ensure the package is installed.
    /// `absent` will remove the package.
    /// `latest` will upgrade to the latest version.
    /// `forcereinstall` will force reinstallation of the package.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// The explicit executable or pathname for the pip executable.
    /// Mutually exclusive with `virtualenv`.
    /// **[default: `"pip"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Path to a pip requirements file.
    requirements: Option<String>,
    /// An optional path to a virtualenv directory to install into.
    /// If the virtualenv does not exist, it will be created.
    /// Mutually exclusive with `executable`.
    virtualenv: Option<String>,
    /// The command or pathname to create the virtual environment with.
    /// **[default: `"virtualenv"`]**
    #[serde(default = "default_virtualenv_command")]
    virtualenv_command: Option<String>,
    /// The Python executable used for creating the virtual environment.
    virtualenv_python: Option<String>,
    /// Whether the virtual environment will inherit packages from the global site-packages.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    virtualenv_site_packages: Option<bool>,
    /// Extra arguments passed to pip.
    extra_args: Option<String>,
    /// Pass the editable flag.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    editable: Option<bool>,
    /// The version number to install of the Python library.
    version: Option<String>,
    /// Change directory before running the command.
    chdir: Option<String>,
    /// The system umask to apply before installing the pip package.
    umask: Option<String>,
    /// Allow pip to modify an externally-managed Python installation (PEP 668).
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    break_system_packages: Option<bool>,
}

#[cfg(test)]
impl Default for Params {
    fn default() -> Self {
        Params {
            name: Vec::new(),
            state: Some(State::Present),
            executable: Some("pip".to_owned()),
            requirements: None,
            virtualenv: None,
            virtualenv_command: Some("virtualenv".to_owned()),
            virtualenv_python: None,
            virtualenv_site_packages: Some(false),
            extra_args: None,
            editable: Some(false),
            version: None,
            chdir: None,
            umask: None,
            break_system_packages: Some(false),
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
    chdir: Option<String>,
    umask: Option<String>,
    break_system_packages: bool,
}

impl PipClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        let executable = if let Some(ref venv) = params.virtualenv {
            let venv_path = Path::new(venv);
            let pip_path = if cfg!(windows) {
                venv_path.join("Scripts").join("pip.exe")
            } else {
                venv_path.join("bin").join("pip")
            };

            if !venv_path.exists() && !check_mode {
                Self::create_virtualenv(params)?;
            }

            pip_path
        } else {
            PathBuf::from(params.executable.as_ref().unwrap())
        };

        Ok(PipClient {
            executable,
            extra_args: params.extra_args.clone(),
            check_mode,
            chdir: params.chdir.clone(),
            umask: params.umask.clone(),
            break_system_packages: params.break_system_packages.unwrap(),
        })
    }

    fn create_virtualenv(params: &Params) -> Result<()> {
        let venv_path = params
            .virtualenv
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "virtualenv path is required"))?;

        let mut cmd = Command::new(params.virtualenv_command.as_ref().unwrap());

        if let Some(ref python) = params.virtualenv_python {
            cmd.arg("-p").arg(python);
        }

        if params.virtualenv_site_packages.unwrap() {
            cmd.arg("--system-site-packages");
        }

        cmd.arg(venv_path);

        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create virtualenv: {e}"),
            )
        })?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(())
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        if let Some(ref chdir) = self.chdir {
            cmd.current_dir(chdir);
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

        if self.break_system_packages {
            cmd.arg("--break-system-packages");
        }

        let _old_umask = if let Some(ref umask_str) = self.umask {
            let mode = u32::from_str_radix(umask_str, 8).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid umask '{umask_str}': {e}"),
                )
            })?;
            Some(unsafe { nix::libc::umask(mode) })
        } else {
            None
        };

        let output = cmd.output().map_err(|e| {
            if let Some(old_mode) = _old_umask {
                unsafe { nix::libc::umask(old_mode) };
            }
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            )
        })?;

        if let Some(old_mode) = _old_umask {
            unsafe { nix::libc::umask(old_mode) };
        }

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

    fn parse_installed_packages(stdout: Vec<u8>) -> BTreeSet<String> {
        let output_string = String::from_utf8_lossy(&stdout);
        output_string
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.first().map(|s| s.to_string())
            })
            .collect()
    }

    pub fn get_installed_packages(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--format=freeze");

        let output = self.exec_cmd(&mut cmd, true)?;
        let packages: BTreeSet<String> = Self::parse_installed_packages(output.stdout)
            .into_iter()
            .map(|p| {
                p.split("==")
                    .next()
                    .map(|s| s.to_lowercase())
                    .unwrap_or(p.to_lowercase())
            })
            .collect();
        Ok(packages)
    }

    pub fn install(&self, packages: &[String], editable: bool) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install");

        if editable {
            cmd.arg("-e");
        }

        for pkg in packages {
            cmd.arg(pkg);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn install_requirements(&self, requirements: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("-r").arg(requirements);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn uninstall(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").arg("-y");

        for pkg in packages {
            cmd.arg(pkg);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn upgrade(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("--upgrade");

        for pkg in packages {
            cmd.arg(pkg);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn force_reinstall(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("install").arg("--force-reinstall");

        for pkg in packages {
            cmd.arg(pkg);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn extract_package_name(spec: &str) -> String {
    if spec.starts_with("git+")
        || spec.starts_with("hg+")
        || spec.starts_with("svn+")
        || spec.starts_with("bzr+")
    {
        if spec.contains("#egg=")
            && let Some(egg_part) = spec.split("#egg=").last()
        {
            return egg_part.to_lowercase();
        }
        if let Some(last_part) = spec.split('/').next_back() {
            return last_part.trim_end_matches(".git").to_lowercase();
        }
    }

    if spec.starts_with("file://")
        && let Some(filename) = spec.split('/').next_back()
    {
        return filename
            .trim_end_matches(".tar.gz")
            .trim_end_matches(".whl")
            .to_lowercase();
    }

    spec.split(['=', '>', '<', '!', '[', ';'])
        .next()
        .unwrap_or(spec)
        .to_lowercase()
}

fn pip(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.name.is_empty() && params.requirements.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "one of 'name' or 'requirements' is required",
        ));
    }

    if params.executable.is_some() && params.virtualenv.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "'executable' and 'virtualenv' are mutually exclusive",
        ));
    }

    let packages: Vec<String> = if let Some(ref version) = params.version {
        if params.name.len() != 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "'version' can only be used with a single 'name'",
            ));
        }
        vec![format!("{}=={}", params.name[0], version)]
    } else {
        params.name.clone()
    };

    let client = PipClient::new(&params, check_mode)?;

    if let Some(ref requirements) = params.requirements {
        logger::add(std::slice::from_ref(requirements));
        client.install_requirements(requirements)?;
        return Ok(ModuleResult {
            changed: true,
            output: None,
            extra: Some(value::to_value(json!({
                "requirements": requirements,
            }))?),
        });
    }

    let package_names: BTreeSet<String> =
        packages.iter().map(|p| extract_package_name(p)).collect();

    let (p_to_install, p_to_remove, p_to_upgrade, p_to_reinstall) = match params.state.unwrap() {
        State::Present => {
            let installed = client.get_installed_packages()?;
            let to_install: Vec<String> = package_names
                .difference(&installed)
                .map(|p| {
                    packages
                        .iter()
                        .find(|pkg| extract_package_name(pkg) == *p)
                        .cloned()
                        .unwrap_or_else(|| p.clone())
                })
                .collect();
            (to_install, Vec::new(), Vec::new(), Vec::new())
        }
        State::Absent => {
            let installed = client.get_installed_packages()?;
            let to_remove: Vec<String> = package_names.intersection(&installed).cloned().collect();
            (Vec::new(), to_remove, Vec::new(), Vec::new())
        }
        State::Latest => (packages.clone(), Vec::new(), Vec::new(), Vec::new()),
        State::Forcereinstall => (Vec::new(), Vec::new(), Vec::new(), packages.clone()),
    };

    let install_changed = if !p_to_install.is_empty() {
        logger::add(&p_to_install);
        client.install(&p_to_install, params.editable.unwrap())?;
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

    let upgrade_changed = if !p_to_upgrade.is_empty() {
        logger::add(&p_to_upgrade);
        client.upgrade(&p_to_upgrade)?;
        true
    } else {
        false
    };

    let reinstall_changed = if !p_to_reinstall.is_empty() {
        logger::add(&p_to_reinstall);
        client.force_reinstall(&p_to_reinstall)?;
        true
    } else {
        false
    };

    Ok(ModuleResult {
        changed: install_changed || remove_changed || upgrade_changed || reinstall_changed,
        output: None,
        extra: Some(value::to_value(json!({
            "name": packages,
            "installed_packages": p_to_install,
            "removed_packages": p_to_remove,
            "upgraded_packages": p_to_upgrade,
        }))?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bottle
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec!["bottle".to_owned()],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_multiple_packages() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - django>1.11.0,<1.12.0
              - bottle>0.10,<0.20,!=0.11
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: vec![
                    "django>1.11.0,<1.12.0".to_owned(),
                    "bottle>0.10,<0.20,!=0.11".to_owned()
                ],
                state: Some(State::Present),
                ..Default::default()
            }
        );
    }

    #[test]
    fn test_parse_params_with_virtualenv() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bottle
            virtualenv: /my_app/venv
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.virtualenv, Some("/my_app/venv".to_owned()));
    }

    #[test]
    fn test_parse_params_with_requirements() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            requirements: /my_app/requirements.txt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.requirements,
            Some("/my_app/requirements.txt".to_owned())
        );
    }

    #[test]
    fn test_parse_params_all_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bottle
            state: latest
            executable: pip3.13
            virtualenv: /my_app/venv
            virtualenv_command: python -m venv
            virtualenv_python: python3.13
            virtualenv_site_packages: true
            extra_args: -i https://example.com/pypi/simple
            editable: true
            version: "0.11"
            chdir: /my_app
            umask: "0022"
            break_system_packages: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["bottle".to_owned()]);
        assert_eq!(params.state, Some(State::Latest));
        assert_eq!(params.executable, Some("pip3.13".to_owned()));
        assert_eq!(params.virtualenv, Some("/my_app/venv".to_owned()));
        assert_eq!(params.virtualenv_command, Some("python -m venv".to_owned()));
        assert_eq!(params.virtualenv_python, Some("python3.13".to_owned()));
        assert_eq!(params.virtualenv_site_packages, Some(true));
        assert_eq!(
            params.extra_args,
            Some("-i https://example.com/pypi/simple".to_owned())
        );
        assert_eq!(params.editable, Some(true));
        assert_eq!(params.version, Some("0.11".to_owned()));
        assert_eq!(params.chdir, Some("/my_app".to_owned()));
        assert_eq!(params.umask, Some("0022".to_owned()));
        assert_eq!(params.break_system_packages, Some(true));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bottle
            foo: yea
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_pip_client_parse_installed_packages() {
        let stdout = r#"bottle==0.12.25
django==4.2.7
requests==2.31.0
"#
        .as_bytes();
        let parsed = PipClient::parse_installed_packages(stdout.to_vec());

        assert_eq!(
            parsed,
            BTreeSet::from([
                "bottle==0.12.25".to_owned(),
                "django==4.2.7".to_owned(),
                "requests==2.31.0".to_owned(),
            ])
        );
    }

    #[test]
    fn test_extract_package_name_simple() {
        assert_eq!(extract_package_name("bottle"), "bottle");
        assert_eq!(extract_package_name("BOTTLE"), "bottle");
    }

    #[test]
    fn test_extract_package_name_with_version() {
        assert_eq!(extract_package_name("bottle==0.11"), "bottle");
        assert_eq!(extract_package_name("bottle>=0.10"), "bottle");
        assert_eq!(extract_package_name("bottle>0.10,<0.20"), "bottle");
        assert_eq!(extract_package_name("bottle!=0.11"), "bottle");
    }

    #[test]
    fn test_extract_package_name_with_extras() {
        assert_eq!(extract_package_name("django[bcrypt]"), "django");
        assert_eq!(
            extract_package_name("requests[security]==2.31.0"),
            "requests"
        );
    }

    #[test]
    fn test_extract_package_name_git_url() {
        assert_eq!(
            extract_package_name("git+https://github.com/user/repo.git#egg=mypackage"),
            "mypackage"
        );
        assert_eq!(
            extract_package_name("git+https://github.com/user/mypackage.git"),
            "mypackage"
        );
    }

    #[test]
    fn test_extract_package_name_file_url() {
        assert_eq!(
            extract_package_name("file:///path/to/mypackage.tar.gz"),
            "mypackage"
        );
        assert_eq!(
            extract_package_name("file:///path/to/mypackage.whl"),
            "mypackage"
        );
    }

    #[test]
    fn test_pip_params_missing_name_and_requirements() {
        let yaml: YamlValue = serde_norway::from_str(r#"{}"#).unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = pip(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("one of 'name' or 'requirements' is required")
        );
    }

    #[test]
    fn test_pip_params_executable_and_virtualenv_mutually_exclusive() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: bottle
            executable: pip3
            virtualenv: /my_app/venv
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = pip(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("'executable' and 'virtualenv' are mutually exclusive")
        );
    }
}
