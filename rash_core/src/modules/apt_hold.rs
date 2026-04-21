/// ANCHOR: module
/// # apt_hold
///
/// Manage package holds in Debian-based systems.
///
/// ## Description
///
/// This module manages package holds using `apt-mark`. Holding packages prevents
/// them from being automatically upgraded, which is critical for production systems
/// and IoT devices where specific versions must be maintained.
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
/// - name: Hold nginx package to prevent updates
///   apt_hold:
///     name: nginx
///     state: held
///
/// - name: Hold multiple packages
///   apt_hold:
///     name:
///       - nginx
///       - docker-ce
///       - linux-image-generic
///     state: held
///
/// - name: Unhold a package to allow updates
///   apt_hold:
///     name: nginx
///     state: unheld
///
/// - name: Unhold multiple packages
///   apt_hold:
///     name:
///       - nginx
///       - docker-ce
///     state: unheld
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use serde_with::{OneOrMany, serde_as};
use std::path::PathBuf;
use std::process::Command;

fn default_executable() -> Option<String> {
    Some("apt-mark".to_owned())
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Held,
    Unheld,
}

impl State {
    fn as_str(&self) -> &'static str {
        match self {
            State::Held => "hold",
            State::Unheld => "unhold",
        }
    }
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the binary to use.
    /// **[default: `"apt-mark"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Name or list of names of packages.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    pub name: Vec<String>,
    /// Whether to hold (`held`) or unhold (`unheld`) the package(s).
    /// **[default: `"held"`]**
    #[serde(default)]
    pub state: State,
}

struct AptMarkClient {
    executable: PathBuf,
    check_mode: bool,
}

impl AptMarkClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(AptMarkClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            check_mode,
        })
    }

    fn get_held_packages(&self) -> Result<Vec<String>> {
        let output = Command::new(&self.executable)
            .arg("showhold")
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to execute {} showhold: {}",
                        self.executable.display(),
                        e
                    ),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("{} showhold failed: {}", self.executable.display(), stderr),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    fn hold(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let output = Command::new(&self.executable)
            .arg("hold")
            .args(packages)
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute {} hold: {}", self.executable.display(), e),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("{} hold failed: {}", self.executable.display(), stderr),
            ));
        }

        Ok(())
    }

    fn unhold(&self, packages: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let output = Command::new(&self.executable)
            .arg("unhold")
            .args(packages)
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to execute {} unhold: {}",
                        self.executable.display(),
                        e
                    ),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("{} unhold failed: {}", self.executable.display(), stderr),
            ));
        }

        Ok(())
    }
}

fn apt_hold_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "name parameter is required",
        ));
    }

    let packages: Vec<&str> = params.name.iter().map(|s| s.trim()).collect();

    for pkg in &packages {
        if pkg.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "package name cannot be empty",
            ));
        }
    }

    let client = AptMarkClient::new(&params, check_mode)?;
    let held_packages = client.get_held_packages()?;

    let packages_owned: Vec<String> = packages.iter().map(|s| s.to_string()).collect();

    let (to_change, unchanged): (Vec<String>, Vec<String>) = packages_owned
        .iter()
        .partition(|pkg| match &params.state {
            State::Held => !held_packages.contains(pkg),
            State::Unheld => held_packages.contains(pkg),
        });

    let changed = !to_change.is_empty();

    if changed {
        match &params.state {
            State::Held => {
                logger::add(&to_change);
                client.hold(&to_change)?;
            }
            State::Unheld => {
                logger::remove(&to_change);
                client.unhold(&to_change)?;
            }
        }
    }

    let extra = Some(value::to_value(json!({
        "packages": packages_owned,
        "state": params.state.as_str(),
        "changed_packages": to_change,
        "unchanged_packages": unchanged,
    }))?);

    Ok(ModuleResult {
        changed,
        output: None,
        extra,
    })
}

#[derive(Debug)]
pub struct AptHold;

impl Module for AptHold {
    fn get_name(&self) -> &str {
        "apt_hold"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((apt_hold_impl(params, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_single_package() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: held
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["nginx".to_string()]);
        assert_eq!(params.state, State::Held);
    }

    #[test]
    fn test_parse_params_multiple_packages() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - nginx
              - docker-ce
            state: held
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.name,
            vec!["nginx".to_string(), "docker-ce".to_string()]
        );
        assert_eq!(params.state, State::Held);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["nginx".to_string()]);
        assert_eq!(params.state, State::Held);
    }

    #[test]
    fn test_parse_params_unheld() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            state: unheld
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Unheld);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_state_as_str() {
        assert_eq!(State::Held.as_str(), "hold");
        assert_eq!(State::Unheld.as_str(), "unhold");
    }

    #[test]
    fn test_parse_params_executable() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/bin/apt-mark
            name: nginx
            state: held
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.executable, Some("/usr/bin/apt-mark".to_string()));
        assert_eq!(params.name, vec!["nginx".to_string()]);
    }

    #[test]
    fn test_parse_params_empty_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: []
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.name.is_empty());
    }
}
