/// ANCHOR: module
/// # make
///
/// Run targets in a Makefile.
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
/// - make:
///     chdir: /home/ubuntu/cool-project
///
/// - make:
///     chdir: /home/ubuntu/cool-project
///     target: install
///
/// - make:
///     chdir: /home/ubuntu/cool-project
///     target: all
///     params:
///       NUM_THREADS: 4
///       BACKEND: lapack
///
/// - make:
///     chdir: /home/ubuntu/cool-project
///     target: all
///     file: /some-project/Makefile
///     jobs: 4
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
    pub chdir: String,
    /// The target to run (e.g., `install`, `test`, `all`).
    pub target: Option<String>,
    /// Use a custom Makefile path.
    pub file: Option<String>,
    /// Set the number of make jobs to run concurrently.
    pub jobs: Option<u32>,
    /// Use a specific make binary (default: "make").
    pub make: Option<String>,
    /// Extra parameters to pass to make as KEY=VALUE pairs.
    pub params: Option<HashMap<String, String>>,
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

        let make_binary = params.make.as_deref().unwrap_or("make");
        let mut cmd = Command::new(make_binary);

        cmd.current_dir(Path::new(&params.chdir));

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
                if value.is_empty() {
                    cmd.arg(key);
                } else {
                    cmd.arg(format!("{}={}", key, value));
                }
            }
        }

        trace!("exec - {:?}", cmd);

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
            "chdir": params.chdir,
            "target": params.target,
            "file": params.file,
            "jobs": params.jobs,
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /home/ubuntu/cool-project
            target: install
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                chdir: "/home/ubuntu/cool-project".to_owned(),
                target: Some("install".to_owned()),
                file: None,
                jobs: None,
                make: None,
                params: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /home/ubuntu/cool-project
            target: all
            file: /some-project/Makefile
            jobs: 4
            make: gmake
            params:
              NUM_THREADS: 4
              BACKEND: lapack
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chdir, "/home/ubuntu/cool-project");
        assert_eq!(params.target, Some("all".to_owned()));
        assert_eq!(params.file, Some("/some-project/Makefile".to_owned()));
        assert_eq!(params.jobs, Some(4));
        assert_eq!(params.make, Some("gmake".to_owned()));
        assert!(params.params.is_some());
        let p = params.params.unwrap();
        assert_eq!(p.get("NUM_THREADS"), Some(&"4".to_owned()));
        assert_eq!(p.get("BACKEND"), Some(&"lapack".to_owned()));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /tmp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                chdir: "/tmp".to_owned(),
                target: None,
                file: None,
                jobs: None,
                make: None,
                params: None,
            }
        );
    }

    #[test]
    fn test_parse_params_missing_chdir() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: all
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
