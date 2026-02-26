/// ANCHOR: module
/// # make
///
/// Run make commands for build automation.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Build project
///   make:
///     chdir: /opt/project
///     target: all
///     jobs: 4
///
/// - name: Clean build artifacts
///   make:
///     chdir: /opt/project
///     target: clean
///
/// - name: Install with custom Makefile
///   make:
///     chdir: /opt/project
///     file: Makefile.local
///     target: install
///
/// - name: Build with additional parameters
///   make:
///     chdir: /opt/project
///     target: release
///     params:
///       PREFIX: /usr/local
///       DEBUG: 0
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Change into this directory before running make.
    pub chdir: Option<String>,
    /// The makefile to use.
    pub file: Option<String>,
    /// Set the number of jobs to run simultaneously.
    pub jobs: Option<u32>,
    /// Additional parameters to pass to make as key=value pairs.
    pub params: Option<HashMap<String, String>>,
    /// The make target to run.
    pub target: Option<String>,
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
        let params: Params = parse_params(optional_params)?;

        let mut cmd = Command::new("make");

        if let Some(ref file) = params.file {
            cmd.args(["-f", file]);
        }

        if let Some(jobs) = params.jobs {
            cmd.arg(format!("-j{}", jobs));
        }

        if let Some(ref target) = params.target {
            cmd.arg(target);
        }

        if let Some(ref extra_params) = params.params {
            for (key, value) in extra_params {
                cmd.arg(format!("{}={}", key, value));
            }
        }

        if let Some(ref chdir) = params.chdir {
            cmd.current_dir(Path::new(chdir));
        }

        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("exec - output: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            return Err(Error::new(ErrorKind::InvalidData, stderr));
        }
        let output_string = String::from_utf8_lossy(&output.stdout);

        let module_output = if output_string.is_empty() {
            None
        } else {
            Some(output_string.into_owned())
        };

        let extra = Some(value::to_value(json!({
            "rc": output.status.code(),
            "stderr": stderr,
        }))?);

        Ok((
            ModuleResult {
                changed: true,
                output: module_output,
                extra,
            },
            None,
        ))
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
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /opt/project
            target: all
            file: Makefile.local
            jobs: 4
            params:
              PREFIX: /usr/local
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chdir, Some("/opt/project".to_owned()));
        assert_eq!(params.target, Some("all".to_owned()));
        assert_eq!(params.file, Some("Makefile.local".to_owned()));
        assert_eq!(params.jobs, Some(4));
        assert_eq!(
            params.params,
            Some(HashMap::from([(
                "PREFIX".to_owned(),
                "/usr/local".to_owned()
            )]))
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: clean
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.target, Some("clean".to_owned()));
        assert_eq!(params.chdir, None);
        assert_eq!(params.file, None);
        assert_eq!(params.jobs, None);
        assert_eq!(params.params, None);
    }

    #[test]
    fn test_parse_params_empty() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.target, None);
        assert_eq!(params.chdir, None);
        assert_eq!(params.file, None);
        assert_eq!(params.jobs, None);
        assert_eq!(params.params, None);
    }
}
