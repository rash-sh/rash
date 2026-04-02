/// ANCHOR: module
/// # dpkg_selections
///
/// Manage Debian package selections (hold/unhold packages).
///
/// ## Description
///
/// This module manages dpkg selections for Debian packages. It allows you to
/// set packages to be held, unheld, installed, deinstalled, or purged.
/// This is useful for preventing automatic package updates, locking package
/// versions, or managing package states during system configuration.
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
///   dpkg_selections:
///     name: nginx
///     selection: hold
///
/// - name: Hold multiple packages
///   dpkg_selections:
///     name:
///       - nginx
///       - docker-ce
///       - kernel-package
///     selection: hold
///
/// - name: Unhold a package to allow updates
///   dpkg_selections:
///     name: nginx
///     selection: install
///
/// - name: Query current package selections
///   dpkg_selections:
///     name: nginx
///   register: nginx_status
///
/// - name: Mark package for removal
///   dpkg_selections:
///     name: old-package
///     selection: deinstall
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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
use std::process::Command;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Selection {
    Install,
    Hold,
    Deinstall,
    Purge,
    #[default]
    Unhold,
}

impl Selection {
    fn as_str(&self) -> &'static str {
        match self {
            Selection::Install => "install",
            Selection::Hold => "hold",
            Selection::Deinstall => "deinstall",
            Selection::Purge => "purge",
            Selection::Unhold => "install",
        }
    }
}

#[serde_as]
#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name or list of names of packages.
    #[serde_as(deserialize_as = "OneOrMany<_>")]
    #[serde(default)]
    pub name: Vec<String>,
    /// The selection state to set. Valid values: `install`, `hold`, `deinstall`, `purge`.
    /// Using `unhold` is equivalent to `install`.
    /// **[default: `"install"`]**
    pub selection: Option<Selection>,
}

fn get_current_selection(package: &str) -> Result<Option<String>> {
    let output = Command::new("dpkg")
        .arg("--get-selections")
        .arg(package)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute dpkg --get-selections: {}", e),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() || !output.status.success() {
        return Ok(None);
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() >= 2 && parts[0] == package {
        Ok(Some(parts[1].to_string()))
    } else {
        Ok(None)
    }
}

fn set_selection(package: &str, selection: &str) -> Result<()> {
    let input = format!("{} {}", package, selection);

    let child = Command::new("dpkg")
        .arg("--set-selections")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute dpkg --set-selections: {}", e),
            )
        })?;

    if let Some(mut stdin) = child.stdin.as_ref() {
        use std::io::Write;
        writeln!(stdin, "{}", input).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write to dpkg --set-selections: {}", e),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to wait for dpkg --set-selections: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("dpkg --set-selections failed: {}", stderr),
        ));
    }

    Ok(())
}

fn dpkg_selections_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
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

    let selection = params.selection.unwrap_or_default();
    let selection_str = selection.as_str();

    let mut changed_packages: Vec<String> = Vec::new();
    let mut unchanged_packages: Vec<String> = Vec::new();
    let mut current_selections: Vec<(String, String)> = Vec::new();

    for package in &packages {
        let current = get_current_selection(package)?;

        current_selections.push((
            package.to_string(),
            current.clone().unwrap_or_else(|| "unknown".to_string()),
        ));

        if let Some(ref current_sel) = current
            && current_sel == selection_str
        {
            unchanged_packages.push(package.to_string());
            continue;
        }

        changed_packages.push(package.to_string());

        if !check_mode {
            set_selection(package, selection_str)?;
        }
    }

    let changed = !changed_packages.is_empty();
    let extra = Some(value::to_value(json!({
        "packages": packages.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
        "selection": selection_str,
        "changed_packages": changed_packages,
        "unchanged_packages": unchanged_packages,
        "current_selections": current_selections,
    }))?);

    let output = if changed {
        if check_mode {
            Some(format!(
                "Would set selection '{}' for packages: {}",
                selection_str,
                changed_packages.join(", ")
            ))
        } else {
            Some(format!(
                "Set selection '{}' for packages: {}",
                selection_str,
                changed_packages.join(", ")
            ))
        }
    } else {
        Some(format!(
            "Packages already have selection '{}': {}",
            selection_str,
            unchanged_packages.join(", ")
        ))
    };

    Ok(ModuleResult {
        changed,
        output,
        extra,
    })
}

#[derive(Debug)]
pub struct DpkgSelections;

impl Module for DpkgSelections {
    fn get_name(&self) -> &str {
        "dpkg_selections"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((dpkg_selections_impl(params, check_mode)?, None))
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
            selection: hold
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["nginx".to_string()]);
        assert_eq!(params.selection, Some(Selection::Hold));
    }

    #[test]
    fn test_parse_params_multiple_packages() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name:
              - nginx
              - docker-ce
            selection: hold
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.name,
            vec!["nginx".to_string(), "docker-ce".to_string()]
        );
        assert_eq!(params.selection, Some(Selection::Hold));
    }

    #[test]
    fn test_parse_params_no_selection() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, vec!["nginx".to_string()]);
        assert_eq!(params.selection, None);
    }

    #[test]
    fn test_parse_params_install() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            selection: install
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selection, Some(Selection::Install));
    }

    #[test]
    fn test_parse_params_deinstall() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-package
            selection: deinstall
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selection, Some(Selection::Deinstall));
    }

    #[test]
    fn test_parse_params_purge() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-package
            selection: purge
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selection, Some(Selection::Purge));
    }

    #[test]
    fn test_parse_params_unhold() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx
            selection: unhold
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selection, Some(Selection::Unhold));
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
    fn test_selection_as_str() {
        assert_eq!(Selection::Install.as_str(), "install");
        assert_eq!(Selection::Hold.as_str(), "hold");
        assert_eq!(Selection::Deinstall.as_str(), "deinstall");
        assert_eq!(Selection::Purge.as_str(), "purge");
        assert_eq!(Selection::Unhold.as_str(), "install");
    }

    #[test]
    fn test_parse_params_empty_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: []
            selection: hold
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.name.is_empty());
    }
}
