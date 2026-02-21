/// ANCHOR: module
/// # alternatives
///
/// Manage symbolic links determining default commands.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Set Java 11 as default java
///   alternatives:
///     name: java
///     path: /usr/lib/jvm/java-11-openjdk/bin/java
///
/// - name: Set vim as default editor
///   alternatives:
///     name: editor
///     path: /usr/bin/vim.basic
///
/// - name: Set python to python3 with custom link
///   alternatives:
///     name: python
///     path: /usr/bin/python3
///     link: /usr/bin/python
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

const UPDATE_ALTERNATIVES: &str = "update-alternatives";

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The generic name of the link group (e.g., java, editor, python).
    pub name: String,
    /// The path to the real executable that should be linked to.
    pub path: String,
    /// The path to the symbolic link (default is auto-detected).
    pub link: Option<String>,
    /// The priority of the alternative (higher values have higher priority).
    pub priority: Option<i32>,
}

#[derive(Debug, Clone)]
struct AlternativeInfo {
    link: String,
    current: Option<String>,
}

fn get_alternative_info(name: &str) -> Result<AlternativeInfo> {
    let output = Command::new(UPDATE_ALTERNATIVES)
        .args(["--display", name])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && stderr.contains("no alternatives") {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("No alternatives for '{name}'"),
        ));
    }

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to query alternatives: {stderr}"),
        ));
    }

    let link = stdout
        .lines()
        .find_map(|line| {
            if line.starts_with("link ") {
                Some(line.trim_start_matches("link ").trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| format!("/usr/bin/{name}"));

    let current = stdout.lines().find_map(|line| {
        if line.contains(" - status is ") {
            let status = line.split(" - status is ").nth(1)?;
            Some(status.trim().trim_end_matches('.').to_string())
        } else if line.contains("currently points to ") {
            let path = line.split("currently points to ").nth(1)?;
            Some(path.trim().to_string())
        } else {
            None
        }
    });

    Ok(AlternativeInfo { link, current })
}

fn is_alternative_installed(name: &str, path: &str) -> Result<bool> {
    let output = Command::new(UPDATE_ALTERNATIVES)
        .args(["--list", name])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().any(|line| line.trim() == path))
        }
        Err(_) => Ok(false),
    }
}

fn install_alternative(params: &Params, link: &str, check_mode: bool) -> Result<()> {
    if check_mode {
        return Ok(());
    }

    let priority = params.priority.unwrap_or(50);

    let status = Command::new(UPDATE_ALTERNATIVES)
        .args([
            "--install",
            link,
            &params.name,
            &params.path,
            &priority.to_string(),
        ])
        .status()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to install alternative {} -> {}",
                params.name, params.path
            ),
        ));
    }

    Ok(())
}

fn set_alternative(name: &str, path: &str, check_mode: bool) -> Result<()> {
    if check_mode {
        return Ok(());
    }

    let status = Command::new(UPDATE_ALTERNATIVES)
        .args(["--set", name, path])
        .status()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to set alternative {} to {}", name, path),
        ));
    }

    Ok(())
}

fn run_alternatives(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path_exists = Path::new(&params.path).exists();
    if !path_exists {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Path '{}' does not exist", params.path),
        ));
    }

    let alt_info = match get_alternative_info(&params.name) {
        Ok(info) => Some(info),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };

    let link = params.link.clone().unwrap_or_else(|| {
        alt_info
            .as_ref()
            .map(|i| i.link.clone())
            .unwrap_or_else(|| format!("/usr/bin/{}", params.name))
    });

    let installed = is_alternative_installed(&params.name, &params.path)?;

    match alt_info {
        None => {
            install_alternative(&params, &link, check_mode)?;
            let msg = format!(
                "Installed and set alternative {} -> {}",
                params.name, params.path
            );
            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({"path": params.path, "link": link}))?),
                Some(msg),
            ))
        }
        Some(info) => {
            let current = info.current.unwrap_or_default();

            if !installed {
                install_alternative(&params, &link, check_mode)?;
            }

            if current == params.path {
                let msg = format!(
                    "Alternative {} is already set to {}",
                    params.name, params.path
                );
                return Ok(ModuleResult::new(
                    false,
                    Some(value::to_value(json!({"path": params.path, "link": link}))?),
                    Some(msg),
                ));
            }

            if !installed && check_mode {
                let msg = format!(
                    "Would install and set alternative {} -> {}",
                    params.name, params.path
                );
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({"path": params.path, "link": link}))?),
                    Some(msg),
                ));
            }

            if !installed {
                install_alternative(&params, &link, false)?;
            }

            set_alternative(&params.name, &params.path, check_mode)?;

            let msg = if check_mode {
                format!(
                    "Would change alternative {} from {} to {}",
                    params.name, current, params.path
                )
            } else {
                format!(
                    "Changed alternative {} from {} to {}",
                    params.name, current, params.path
                )
            };

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({"path": params.path, "link": link}))?),
                Some(msg),
            ))
        }
    }
}

#[derive(Debug)]
pub struct Alternatives;

impl Module for Alternatives {
    fn get_name(&self) -> &str {
        "alternatives"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((run_alternatives(params, check_mode)?, None))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: java
            path: /usr/lib/jvm/java-11-openjdk/bin/java
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "java".to_owned(),
                path: "/usr/lib/jvm/java-11-openjdk/bin/java".to_owned(),
                link: None,
                priority: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: editor
            path: /usr/bin/vim.basic
            link: /usr/bin/editor
            priority: 100
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "editor".to_owned(),
                path: "/usr/bin/vim.basic".to_owned(),
                link: Some("/usr/bin/editor".to_owned()),
                priority: Some(100),
            }
        );
    }

    #[test]
    fn test_parse_params_missing_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /usr/bin/vim
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: editor
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_priority() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: editor
            path: /usr/bin/vim
            priority: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
