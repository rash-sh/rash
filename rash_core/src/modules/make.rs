/// ANCHOR: module
/// # make
///
/// Run make targets.
///
/// This module allows you to run make targets in a directory.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: never
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Build a project
///   make:
///     chdir: /path/to/project
///     target: build
///
/// - name: Clean and build
///   make:
///     chdir: /path/to/project
///     target: all
///     params:
///       - clean
///       - build
///
/// - name: Run make with specific file
///   make:
///     chdir: /path/to/project
///     file: MyMakefile
///     target: install
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The directory where make should be executed.
    /// **[default: current directory]**
    chdir: Option<String>,
    /// The makefile to use.
    file: Option<String>,
    /// The target to run.
    target: Option<String>,
    /// Additional parameters to pass to make.
    #[serde(default)]
    params: Vec<String>,
}

pub fn make(params: Params) -> Result<ModuleResult> {
    let mut cmd = Command::new("make");

    if let Some(ref chdir) = params.chdir {
        cmd.current_dir(chdir);
    }

    if let Some(ref file) = params.file {
        cmd.arg("-f").arg(file);
    }

    if let Some(ref target) = params.target {
        cmd.arg(target);
    }

    for param in &params.params {
        cmd.arg(param);
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute make: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("make failed: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let output_str = if stdout.is_empty() {
        None
    } else {
        Some(stdout.to_string())
    };

    Ok(ModuleResult {
        changed: true,
        output: output_str,
        extra: None,
    })
}

#[derive(Debug)]
pub struct Make;

impl Module for Make {
    fn get_name(&self) -> &str {
        "make"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((make(parse_params(optional_params)?)?, None))
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
            chdir: /path/to/project
            target: build
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chdir, Some("/path/to/project".to_string()));
        assert_eq!(params.target, Some("build".to_string()));
        assert!(params.params.is_empty());
    }

    #[test]
    fn test_parse_params_with_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /path/to/project
            file: MyMakefile
            target: install
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.file, Some("MyMakefile".to_string()));
    }

    #[test]
    fn test_parse_params_with_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            params:
              - clean
              - build
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.params, vec!["clean", "build"]);
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.chdir.is_none());
        assert!(params.target.is_none());
        assert!(params.params.is_empty());
    }
}
