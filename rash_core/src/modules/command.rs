/// ANCHOR: module
/// # command
///
/// Execute commands. `argv` executes directly without shell parsing; `cmd` keeps the historical
/// `/bin/sh -c` behavior. With `transfer_pid`, `cmd` is split with shell-like quoting rules and
/// the program replaces Rash directly (no intermediate shell), so it keeps Rash's PID and receives
/// signals itself, as required for container entrypoints. Process output can be captured,
/// inherited, discarded, or streamed and captured with `tee`.
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
use crate::process::{OutputMode, ProcessPlan, ProcessSpec};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    #[serde(flatten)]
    pub required: Required,
    /// Replace the Rash process with this command, keeping its PID (e.g. PID 1 in containers).
    /// `cmd` is split with shell-like quoting and executed directly, without `/bin/sh`.
    /// No later Rash task is executed.
    pub transfer_pid: Option<bool>,
    /// Optional data written to the child stdin.
    pub stdin: Option<String>,
    /// stdout handling: `capture` (default, registered), `tee` (streamed live and registered),
    /// `inherit` (streamed live, not registered) or `null` (discarded; quote it in YAML).
    #[serde(default)]
    pub stdout: OutputMode,
    /// stderr handling: `capture` (default, registered), `tee` (streamed live and registered),
    /// `inherit` (streamed live, not registered) or `null` (discarded; quote it in YAML).
    #[serde(default)]
    pub stderr: OutputMode,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Required {
    /// Execute using `/bin/sh -c`, preserving command's historical string behavior.
    /// With `transfer_pid`, it is split into arguments and executed directly instead.
    Cmd(String),
    /// Execute the program directly and pass each argument exactly as provided.
    Argv(Vec<String>),
}

fn argv_spec(argv: &[String]) -> Result<ProcessSpec> {
    let program = argv
        .first()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("{argv:?} invalid argv")))?;
    let mut spec = ProcessSpec::new(program);
    spec.args = argv.iter().skip(1).cloned().collect();
    Ok(spec)
}

/// Split `cmd` for direct execution: replacing Rash with `/bin/sh -c` would leave the shell,
/// not the program, as the PID that receives signals and reaps zombies.
fn split_cmd(command: &str) -> Result<Vec<String>> {
    shlex::split(command).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("cmd {command:?} has invalid quoting"),
        )
    })
}

fn process_spec(params: &Params) -> Result<ProcessSpec> {
    let transfer_pid = params.transfer_pid.unwrap_or(false);
    let mut spec = match &params.required {
        Required::Cmd(command) if transfer_pid => argv_spec(&split_cmd(command)?)?,
        Required::Cmd(command) => ProcessSpec::shell(command, "/bin/sh"),
        Required::Argv(argv) => argv_spec(argv)?,
    };
    spec.chdir = params.chdir.clone();
    spec.stdin = params.stdin.clone();
    spec.stdout = params.stdout;
    spec.stderr = params.stderr;
    Ok(spec)
}

fn parse(optional_params: YamlValue) -> Result<Params> {
    match optional_params.as_str() {
        Some(s) => Ok(Params {
            chdir: None,
            required: Required::Cmd(s.to_owned()),
            transfer_pid: None,
            stdin: None,
            stdout: OutputMode::Capture,
            stderr: OutputMode::Capture,
        }),
        None => parse_params(optional_params),
    }
}

fn plan(params: &Params, check_mode: bool) -> Result<ProcessPlan> {
    if check_mode {
        let display = match &params.required {
            Required::Cmd(s) => s.clone(),
            Required::Argv(argv) => argv.join(" "),
        };
        return Ok(ProcessPlan::Done(ModuleResult::new(
            true,
            None,
            Some(format!("Would run: {display}")),
        )));
    }
    let spec = process_spec(params)?;
    Ok(if params.transfer_pid.unwrap_or(false) {
        ProcessPlan::Replace(spec)
    } else {
        ProcessPlan::Run(spec)
    })
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
        Ok((
            self.plan_process(optional_params, check_mode)?.execute()?,
            None,
        ))
    }

    fn plan_process(&self, params: YamlValue, check_mode: bool) -> Result<ProcessPlan> {
        plan(&parse(params)?, check_mode)
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

    fn spec_for(yaml: &str) -> ProcessSpec {
        process_spec(&parse_params(serde_norway::from_str(yaml).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn test_cmd_without_transfer_pid_uses_shell() {
        let spec = spec_for("cmd: echo $HOME");
        assert_eq!(spec.program, "/bin/sh");
        assert_eq!(spec.args, vec!["-c", "echo $HOME"]);
    }

    #[test]
    fn test_cmd_with_transfer_pid_is_split_and_direct() {
        let spec = spec_for("cmd: sh -c 'echo $$' \"two words\"\ntransfer_pid: true");
        assert_eq!(spec.program, "sh");
        assert_eq!(spec.args, vec!["-c", "echo $$", "two words"]);
    }

    #[test]
    fn test_cmd_with_transfer_pid_rejects_bad_quoting_or_empty() {
        for cmd in ["cmd: \"'unterminated\"", "cmd: \"\""] {
            let yaml: YamlValue =
                serde_norway::from_str(&format!("{cmd}\ntransfer_pid: true")).unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(
                process_spec(&params).unwrap_err().kind(),
                ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn test_plan_rejects_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str("cmd: ls\nbogus: 1").unwrap();
        let error = Command.plan_process(yaml, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_plan_argv_with_tee_and_transfer_pid() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            argv: [printf, "%s", "hello world"]
            stdout: tee
            "#,
        )
        .unwrap();
        let ProcessPlan::Run(spec) = Command.plan_process(yaml, false).unwrap() else {
            panic!("expected a process to run");
        };
        assert_eq!(spec.program, "printf");
        assert_eq!(spec.args, vec!["%s", "hello world"]);
        assert_eq!(spec.stdout, OutputMode::Tee);

        let yaml: YamlValue = serde_norway::from_str("cmd: ls\ntransfer_pid: true").unwrap();
        let plan = Command.plan_process(yaml, false).unwrap();
        assert!(matches!(plan, ProcessPlan::Replace(_)));
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
