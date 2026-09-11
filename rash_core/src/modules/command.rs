/// ANCHOR: module
/// # command
///
/// Execute commands. `argv` executes directly without shell parsing; `cmd` keeps the historical
/// `/bin/sh -c` behavior. Process output can be captured, inherited, discarded, or streamed and
/// captured with `tee`.
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
/// - command:
///     argv:
///       - echo
///       - "Hello World"
///     transfer_pid: true
///
/// - command: ls examples
///   register: ls_result
///
/// - command:
///     argv: [cargo, build]
///     stdout: tee
///     stderr: tee
///
/// - command:
///     cmd: ls .
///     chdir: examples
///   register: ls_result
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};
use crate::process::{OutputMode, ProcessSpec};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

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
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    #[serde(flatten)]
    pub required: Required,
    /// Replace the Rash process with this command. No later Rash task is executed.
    pub transfer_pid: Option<bool>,
    /// Optional data written to the child stdin.
    pub stdin: Option<String>,
    /// stdout handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stdout: OutputMode,
    /// stderr handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stderr: OutputMode,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Required {
    /// Execute using `/bin/sh -c`, preserving command's historical string behavior.
    Cmd(String),
    /// Execute the program directly and pass each argument exactly as provided.
    Argv(Vec<String>),
}

fn process_spec(params: &Params) -> Result<ProcessSpec> {
    let mut spec = match &params.required {
        Required::Cmd(command) => ProcessSpec::shell(command, "/bin/sh"),
        Required::Argv(argv) => {
            let program = argv.first().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, format!("{argv:?} invalid argv"))
            })?;
            let mut spec = ProcessSpec::new(program);
            spec.args = argv.iter().skip(1).cloned().collect();
            spec
        }
    };
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
pub struct Command;

impl Module for Command {
    fn get_name(&self) -> &str {
        "command"
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
                chdir: None,
                required: Required::Cmd(s.to_owned()),
                transfer_pid: None,
                stdin: None,
                stdout: OutputMode::Capture,
                stderr: OutputMode::Capture,
            },
            None => parse_params(optional_params)?,
        };

        let display = match &params.required {
            Required::Cmd(s) => s.clone(),
            Required::Argv(argv) => argv.join(" "),
        };

        if check_mode {
            return Ok((
                ModuleResult::new(true, None, Some(format!("Would run: {display}"))),
                None,
            ));
        }

        let mut spec = process_spec(&params)?;
        if params.transfer_pid.unwrap_or(false) {
            spec.process_group = false;
            return Err(spec.replace());
        }

        let result = spec.run()?;
        trace!("exec - process result: {result:?}");
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

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "ls"
            transfer_pid: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.required, Required::Cmd("ls".to_owned()));
        assert_eq!(params.transfer_pid, Some(false));
        assert_eq!(params.stdout, OutputMode::Capture);
        assert_eq!(params.stderr, OutputMode::Capture);
    }

    #[test]
    fn test_parse_params_without_cmd_or_argv() {
        let yaml: YamlValue = serde_norway::from_str("transfer_pid: false").unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "ls"
            yea: boo
            transfer_pid: false
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_check_mode_cmd() {
        let command = Command;
        let yaml: YamlValue = serde_norway::from_str(r#"cmd: "ls -la""#).unwrap();
        let (result, _) = command
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("Would run: ls -la".to_string()));
    }

    #[test]
    fn test_check_mode_argv() {
        let command = Command;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            argv:
              - echo
              - "hello world"
            "#,
        )
        .unwrap();
        let (result, _) = command
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert_eq!(
            result.get_output(),
            Some("Would run: echo hello world".to_string())
        );
    }

    #[test]
    fn test_check_mode_simple_string() {
        let command = Command;
        let yaml = YamlValue::String("ls".to_string());
        let (result, _) = command
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert_eq!(result.get_output(), Some("Would run: ls".to_string()));
    }

    #[test]
    fn test_nonzero_exit_is_structured_result() {
        let command = Command;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            argv: [sh, -c, "echo boom >&2; exit 7"]
            "#,
        )
        .unwrap();
        let (result, _) = command
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        let extra = result.get_extra().unwrap();
        assert_eq!(extra["rc"].as_i64(), Some(7));
        assert_eq!(extra["failed"].as_bool(), Some(true));
        assert!(extra["stderr"].as_str().unwrap().contains("boom"));
    }

    #[test]
    fn test_argv_preserves_spaces() {
        let command = Command;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            argv: [printf, "%s", "hello world"]
            "#,
        )
        .unwrap();
        let (result, _) = command
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert_eq!(result.get_output().as_deref(), Some("hello world"));
    }
}
