/// ANCHOR: module
/// # script
///
/// Execute script files.
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
/// - script:
///     path: ./scripts/setup.sh
///     args: --verbose --skip-tests
///     chdir: /opt/app
///
/// - script: ./deploy.sh
///
/// - script:
///     path: ./scripts/migrate.py
///     executable: python3
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
    /// Path to the script file to execute.
    pub path: String,
    /// Arguments to pass to the script.
    pub args: Option<String>,
    /// Change into this directory before running the script.
    pub chdir: Option<String>,
    /// The interpreter to use for executing the script.
    /// If not provided, the script's shebang line will be used.
    pub executable: Option<String>,
}

#[derive(Debug)]
pub struct Script;

fn detect_shebang(path: &str) -> Result<Option<String>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to open script file '{}': {}", path, e),
        )
    })?;

    let mut reader = BufReader::new(file);
    let mut first_line = String::new();

    reader
        .read_line(&mut first_line)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    if first_line.starts_with("#!") {
        let shebang = first_line.trim_start_matches("#!").trim();
        let interpreter = shebang.split_whitespace().next().unwrap_or(shebang);
        Ok(Some(interpreter.to_string()))
    } else {
        Ok(None)
    }
}

impl Module for Script {
    fn get_name(&self) -> &str {
        "script"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = match optional_params.as_str() {
            Some(s) => Params {
                path: s.to_owned(),
                args: None,
                chdir: None,
                executable: None,
            },
            None => parse_params(optional_params)?,
        };

        let script_path = Path::new(&params.path);
        if !script_path.exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Script file '{}' does not exist", params.path),
            ));
        }

        let interpreter = match params.executable {
            Some(ref exe) => Some(exe.clone()),
            None => detect_shebang(&params.path)?,
        };

        let mut cmd = match interpreter {
            Some(ref exe) => {
                trace!("exec - '{}' '{}'", exe, params.path);
                Command::new(exe)
            }
            None => {
                trace!("exec - directly '{}'", params.path);
                Command::new(&params.path)
            }
        };

        if let Some(ref _exe) = interpreter {
            cmd.arg(&params.path);
        }

        if let Some(ref args) = params.args {
            cmd.args(args.split_whitespace());
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
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "./script.sh"
            args: "--verbose"
            chdir: "/tmp"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "./script.sh".to_owned(),
                args: Some("--verbose".to_owned()),
                chdir: Some("/tmp".to_owned()),
                executable: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "./script.sh"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "./script.sh".to_owned(),
                args: None,
                chdir: None,
                executable: None,
            }
        );
    }

    #[test]
    fn test_detect_shebang() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/bash").unwrap();
        writeln!(file, "echo hello").unwrap();

        let shebang = detect_shebang(script_path.to_str().unwrap()).unwrap();
        assert_eq!(shebang, Some("/bin/bash".to_string()));
    }

    #[test]
    fn test_detect_shebang_none() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "echo hello").unwrap();

        let shebang = detect_shebang(script_path.to_str().unwrap()).unwrap();
        assert_eq!(shebang, None);
    }

    #[test]
    fn test_script_execution() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "echo 'hello world'").unwrap();

        let yaml: YamlValue = serde_norway::from_str(&format!(
            r#"
            path: "{}"
            "#,
            script_path.to_str().unwrap().replace('\\', "\\\\")
        ))
        .unwrap();

        let script = Script;
        let (result, _) = script
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("hello world\n".to_string()));
    }
}
