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
    /// stdout handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stdout: OutputMode,
    /// stderr handling: capture (default), inherit, null, or tee.
    #[serde(default)]
    pub stderr: OutputMode,
}

fn check_creates(creates: &str) -> bool {
    Path::new(creates).exists()
}

fn check_removes(removes: &str) -> bool {
    !Path::new(removes).exists()
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
        let params: Params = match optional_params.as_str() {
            Some(s) => Params {
                cmd: s.to_owned(),
                executable: None,
                chdir: None,
                creates: None,
                removes: None,
                stdin: None,
                stdout: OutputMode::Capture,
                stderr: OutputMode::Capture,
            },
            None => parse_params(optional_params)?,
        };

        if let Some(creates) = &params.creates
            && check_creates(creates)
        {
            return Ok((ModuleResult::new(false, None, None), None));
        }

        if let Some(removes) = &params.removes
            && check_removes(removes)
        {
            return Ok((ModuleResult::new(false, None, None), None));
        }

        if check_mode {
            return Ok((
                ModuleResult::new(true, None, Some(format!("Would run: {}", params.cmd))),
                None,
            ));
        }

        let executable = params.executable.as_deref().unwrap_or("/bin/sh");
        let mut spec = ProcessSpec::shell(&params.cmd, executable);
        spec.chdir = params.chdir.clone();
        spec.stdin = params.stdin.clone();
        spec.stdout = params.stdout;
        spec.stderr = params.stderr;

        trace!("exec - {} -c {:?}", executable, params.cmd);
        let result = spec.run()?;
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
