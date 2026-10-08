/// ANCHOR: module
/// # shell
///
/// Execute shell commands with pipes, redirections, expansion, and subshells. Process output can
/// be captured, inherited, discarded, or streamed and captured with `tee`.
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
/// - shell: echo "hello world" | tr a-z A-Z
///   register: upper
///
/// - shell:
///     cmd: cargo build 2>&1
///     stdout: tee
///     stderr: tee
///
/// - shell:
///     cmd: find . -name "*.log" -mtime +7 -delete
///     chdir: /var/log
///
/// - shell:
///     cmd: process_data.sh < input.txt > output.txt
///     executable: /bin/bash
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::Result;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::process::{OutputMode, ProcessPlan, ProcessSpec};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shell command to execute.
    pub cmd: String,
    /// Shell executable. Defaults to `/bin/sh`.
    pub executable: Option<String>,
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    /// Skip execution when this path already exists.
    pub creates: Option<String>,
    /// Skip execution when this path does not exist.
    pub removes: Option<String>,
    /// Data written to stdin.
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

fn check_creates(creates: &str) -> bool {
    Path::new(creates).exists()
}

fn check_removes(removes: &str) -> bool {
    !Path::new(removes).exists()
}

fn parse(optional_params: YamlValue) -> Result<Params> {
    match optional_params.as_str() {
        Some(s) => Ok(Params {
            cmd: s.to_owned(),
            executable: None,
            chdir: None,
            creates: None,
            removes: None,
            stdin: None,
            stdout: OutputMode::Capture,
            stderr: OutputMode::Capture,
        }),
        None => parse_params(optional_params),
    }
}

fn plan(params: Params, check_mode: bool) -> ProcessPlan {
    let creates_met = params.creates.as_deref().is_some_and(check_creates);
    let removes_met = params.removes.as_deref().is_some_and(check_removes);
    if creates_met || removes_met {
        return ProcessPlan::Done(ModuleResult::new(false, None, None));
    }
    if check_mode {
        return ProcessPlan::Done(ModuleResult::new(
            true,
            None,
            Some(format!("Would run: {}", params.cmd)),
        ));
    }
    let executable = params.executable.as_deref().unwrap_or("/bin/sh");
    trace!("exec - {} -c {:?}", executable, params.cmd);
    let mut spec = ProcessSpec::shell(&params.cmd, executable);
    spec.chdir = params.chdir;
    spec.stdin = params.stdin;
    spec.stdout = params.stdout;
    spec.stderr = params.stderr;
    ProcessPlan::Run(spec)
}

#[derive(Debug)]
pub struct Shell;

impl Module for Shell {
    fn get_name(&self) -> &str {
        "shell"
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
        Ok(plan(parse(params)?, check_mode))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str("cmd: ls -la").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cmd, "ls -la");
        assert_eq!(params.stdout, OutputMode::Capture);
        assert_eq!(params.stderr, OutputMode::Capture);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "cat file | grep pattern"
            executable: /bin/bash
            chdir: /tmp
            creates: /tmp/marker
            removes: /tmp/cleanup
            stdin: "hello world"
            stdout: tee
            stderr: inherit
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.executable.as_deref(), Some("/bin/bash"));
        assert_eq!(params.stdout, OutputMode::Tee);
        assert_eq!(params.stderr, OutputMode::Inherit);
    }

    #[test]
    fn test_parse_params_without_cmd() {
        let yaml: YamlValue = serde_norway::from_str("chdir: /tmp").unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str("cmd: ls\nyea: boo").unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_plan_honors_executable() {
        let yaml: YamlValue =
            serde_norway::from_str("cmd: echo hi\nexecutable: /bin/bash").unwrap();
        let ProcessPlan::Run(spec) = Shell.plan_process(yaml, false).unwrap() else {
            panic!("expected a process to run");
        };
        assert_eq!(spec.program, "/bin/bash");
        assert_eq!(spec.args, vec!["-c", "echo hi"]);
    }

    #[test]
    fn test_check_mode() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(r#"cmd: "ls -la | head""#).unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert_eq!(
            result.get_output(),
            Some("Would run: ls -la | head".to_string())
        );
    }

    #[test]
    fn test_creates_skips_when_file_exists() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(&format!(
            "cmd: echo should_not_run\ncreates: {:?}",
            std::env::current_dir().unwrap().to_str().unwrap()
        ))
        .unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_removes_skips_when_file_missing() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            "cmd: echo should_not_run\nremoves: /nonexistent/path/that/does/not/exist",
        )
        .unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_shell_execution_with_pipe() {
        let shell = Shell;
        let yaml: YamlValue =
            serde_norway::from_str(r#"cmd: "echo 'hello world' | tr a-z A-Z""#).unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert_eq!(result.get_output().as_deref(), Some("HELLO WORLD\n"));
    }

    #[test]
    fn test_shell_execution_with_stdin() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str("cmd: cat\nstdin: hello from stdin").unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        assert_eq!(result.get_output().as_deref(), Some("hello from stdin"));
    }

    #[test]
    fn test_nonzero_exit_is_structured_result() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(r#"cmd: "echo nope >&2; exit 4""#).unwrap();
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();
        let extra = result.get_extra().unwrap();
        assert_eq!(extra["rc"].as_i64(), Some(4));
        assert_eq!(extra["failed"].as_bool(), Some(true));
    }
}
