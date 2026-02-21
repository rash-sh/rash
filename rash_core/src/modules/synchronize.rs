/// ANCHOR: module
/// # synchronize
///
/// Wrap rsync to synchronize files.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - synchronize:
///     src: ./dist/
///     dest: /opt/app/
///
/// - synchronize:
///     src: ./src/
///     dest: /var/www/html/
///     delete: true
///     rsync_opts:
///       - --exclude=.git
///       - --chmod=D755,F644
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path on the source host that will be synchronized.
    pub src: String,
    /// Path on the destination host that will be synchronized.
    pub dest: String,
    /// Delete files in dest that don't exist in src.
    /// [default: false]
    #[serde(default)]
    pub delete: bool,
    /// Additional rsync options.
    pub rsync_opts: Option<Vec<String>>,
}

fn check_rsync_available() -> Result<()> {
    let output = Command::new("rsync")
        .arg("--version")
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, format!("rsync not found: {}", e)))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "rsync --version failed",
        ));
    }
    Ok(())
}

fn build_rsync_args(params: &Params) -> Vec<String> {
    let mut args = vec!["-a".to_string()];

    if params.delete {
        args.push("--delete".to_string());
    }

    if let Some(ref opts) = params.rsync_opts {
        for opt in opts {
            args.push(opt.clone());
        }
    }

    let src = if params.src.ends_with('/') {
        params.src.clone()
    } else {
        format!("{}/", params.src)
    };

    args.push(src);
    args.push(params.dest.clone());

    args
}

pub fn run_rsync(params: Params) -> Result<(ModuleResult, Option<Value>)> {
    trace!("params: {params:?}");

    check_rsync_available()?;

    let src_path = Path::new(&params.src);
    if !src_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("src path does not exist: {}", params.src),
        ));
    }

    let args = build_rsync_args(&params);
    trace!("rsync args: {:?}", args);

    let output = Command::new("rsync")
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    trace!("rsync output: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("rsync failed: {}", stderr),
        ));
    }

    let module_output = if stdout.is_empty() {
        None
    } else {
        Some(stdout.into_owned())
    };

    let extra = Some(value::to_value(json!({
        "rc": output.status.code(),
        "stdout": module_output,
        "stderr": stderr,
        "cmd": format!("rsync {}", args.join(" ")),
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

#[derive(Debug)]
pub struct Synchronize;

impl Module for Synchronize {
    fn get_name(&self) -> &str {
        "synchronize"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        run_rsync(params)
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
            src: ./dist/
            dest: /opt/app/
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "./dist/".to_owned(),
                dest: "/opt/app/".to_owned(),
                delete: false,
                rsync_opts: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: ./dist/
            dest: /opt/app/
            delete: true
            rsync_opts:
              - --exclude=.git
              - --chmod=D755,F644
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                src: "./dist/".to_owned(),
                dest: "/opt/app/".to_owned(),
                delete: true,
                rsync_opts: Some(vec![
                    "--exclude=.git".to_owned(),
                    "--chmod=D755,F644".to_owned()
                ]),
            }
        );
    }

    #[test]
    fn test_parse_params_missing_src() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dest: /opt/app/
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_dest() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: ./dist/
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: ./dist/
            dest: /opt/app/
            random: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_rsync_args_basic() {
        let params = Params {
            src: "./dist/".to_owned(),
            dest: "/opt/app/".to_owned(),
            delete: false,
            rsync_opts: None,
        };
        let args = build_rsync_args(&params);
        assert_eq!(args, vec!["-a", "./dist/", "/opt/app/"]);
    }

    #[test]
    fn test_build_rsync_args_with_delete() {
        let params = Params {
            src: "./dist/".to_owned(),
            dest: "/opt/app/".to_owned(),
            delete: true,
            rsync_opts: None,
        };
        let args = build_rsync_args(&params);
        assert_eq!(args, vec!["-a", "--delete", "./dist/", "/opt/app/"]);
    }

    #[test]
    fn test_build_rsync_args_with_opts() {
        let params = Params {
            src: "./dist/".to_owned(),
            dest: "/opt/app/".to_owned(),
            delete: false,
            rsync_opts: Some(vec!["--exclude=.git".to_owned(), "-v".to_owned()]),
        };
        let args = build_rsync_args(&params);
        assert_eq!(
            args,
            vec!["-a", "--exclude=.git", "-v", "./dist/", "/opt/app/"]
        );
    }

    #[test]
    fn test_build_rsync_args_src_without_trailing_slash() {
        let params = Params {
            src: "./dist".to_owned(),
            dest: "/opt/app/".to_owned(),
            delete: false,
            rsync_opts: None,
        };
        let args = build_rsync_args(&params);
        assert_eq!(args, vec!["-a", "./dist/", "/opt/app/"]);
    }

    fn rsync_available() -> bool {
        Command::new("rsync")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_run_rsync_nonexistent_src() {
        if !rsync_available() {
            return;
        }
        let params = Params {
            src: "/nonexistent/path/".to_owned(),
            dest: "/tmp/dest/".to_owned(),
            delete: false,
            rsync_opts: None,
        };
        let result = run_rsync(params);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
