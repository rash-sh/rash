/// ANCHOR: module
/// # script
///
/// Execute script files with the same process IO semantics as `command` and `shell`.
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
///     stdout: tee
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};
use crate::process::{OutputMode, ProcessSpec};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;

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
    /// Shell-like argument string. Quoting is parsed with shlex.
    pub args: Option<String>,
    /// Exact argument vector. Mutually exclusive with `args`.
    pub argv: Option<Vec<String>>,
    /// Change into this directory before running the script.
    pub chdir: Option<String>,
    /// Interpreter override. If absent, a shebang is honored; otherwise the file is executed directly.
    pub executable: Option<String>,
    /// Optional data written to stdin.
    pub stdin: Option<String>,
    /// stdout handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stdout: OutputMode,
    /// stderr handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stderr: OutputMode,
}

fn detect_shebang(path: &str) -> Result<Option<Vec<String>>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to open script file '{path}': {e}"),
        )
    })?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    if let Some(shebang) = first_line.strip_prefix("#!") {
        let parsed = shlex::split(shebang.trim()).ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, format!("Invalid shebang in {path}"))
        })?;
        if parsed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(parsed))
        }
    } else {
        Ok(None)
    }
}

fn process_spec(params: &Params) -> Result<ProcessSpec> {
    if params.args.is_some() && params.argv.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "script args and argv are mutually exclusive",
        ));
    }

    let user_args = if let Some(argv) = &params.argv {
        argv.clone()
    } else if let Some(args) = &params.args {
        shlex::split(args)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "invalid script args"))?
    } else {
        Vec::new()
    };

    let mut spec = if let Some(executable) = &params.executable {
        let parsed = shlex::split(executable)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "invalid executable"))?;
        let program = parsed
            .first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "empty executable"))?;
        let mut spec = ProcessSpec::new(program);
        spec.args.extend(parsed.iter().skip(1).cloned());
        spec.args.push(params.path.clone());
        spec
    } else if let Some(shebang) = detect_shebang(&params.path)? {
        let program = shebang
            .first()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "empty shebang"))?;
        let mut spec = ProcessSpec::new(program);
        spec.args.extend(shebang.iter().skip(1).cloned());
        spec.args.push(params.path.clone());
        spec
    } else {
        ProcessSpec::new(&params.path)
    };

    spec.args.extend(user_args);
    spec.chdir = params.chdir.clone();
    spec.stdin = params.stdin.clone();
    spec.stdout = params.stdout;
    spec.stderr = params.stderr;
    Ok(spec)
}

fn result_from_process(result: crate::process::ProcessResult) -> Result<ModuleResult> {
    let failed = !result.success();
    let extra = Some(value::to_value(json!({
        "rc": result.rc(),
        "stderr": result.stderr.clone().unwrap_or_default(),
        "failed": failed,
    }))?);
    Ok(ModuleResult::new(true, extra, result.stdout))
}

#[derive(Debug)]
pub struct Script;

impl Module for Script {
    fn get_name(&self) -> &str {
        "script"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = match optional_params.as_str() {
            Some(s) => Params {
                path: s.to_owned(),
                args: None,
                argv: None,
                chdir: None,
                executable: None,
                stdin: None,
                stdout: OutputMode::Capture,
                stderr: OutputMode::Capture,
            },
            None => parse_params(optional_params)?,
        };

        if !Path::new(&params.path).exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Script file '{}' does not exist", params.path),
            ));
        }

        if check_mode {
            return Ok((
                ModuleResult::new(
                    true,
                    None,
                    Some(format!("Would run script: {}", params.path)),
                ),
                None,
            ));
        }

        let result = process_spec(&params)?.run()?;
        Ok((result_from_process(result)?, None))
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
        assert_eq!(params.path, "./script.sh");
        assert_eq!(params.args.as_deref(), Some("--verbose"));
        assert_eq!(params.stdout, OutputMode::Capture);
    }

    #[test]
    fn test_detect_shebang_with_args() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "#!/usr/bin/env bash").unwrap();
        let shebang = detect_shebang(script_path.to_str().unwrap()).unwrap();
        assert_eq!(
            shebang,
            Some(vec!["/usr/bin/env".to_owned(), "bash".to_owned()])
        );
    }

    #[test]
    fn test_script_execution() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "echo 'hello world'").unwrap();
        let yaml: YamlValue =
            serde_norway::from_str(&format!("path: {:?}", script_path.to_str().unwrap())).unwrap();
        let (result, _) = Script
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert_eq!(result.get_output().as_deref(), Some("hello world\n"));
    }

    #[test]
    fn test_script_nonzero_is_structured() {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join("test.sh");
        let mut file = File::create(&script_path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "exit 9").unwrap();
        let yaml: YamlValue =
            serde_norway::from_str(&format!("path: {:?}", script_path.to_str().unwrap())).unwrap();
        let (result, _) = Script
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        let extra = result.get_extra().unwrap();
        assert_eq!(extra["rc"].as_i64(), Some(9));
        assert_eq!(extra["failed"].as_bool(), Some(true));
    }
}
