/// ANCHOR: module
/// # shell
///
/// Execute shell commands with pipe support, redirections, and environment
/// variables. This module extends the command module by providing full shell
/// features including pipes, redirections, environment variable expansion,
/// shell glob expansion, and subshell execution.
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
/// - shell:
///     cmd: cat /var/log/app.log | grep ERROR | wc -l
///     register: error_count
///
/// - shell: echo "hello world" | tr a-z A-Z
///   register: upper
///
/// - shell:
///     cmd: find . -name "*.log" -mtime +7 -delete
///     chdir: /var/log
///
/// - shell:
///     cmd: process_data.sh < input.txt > output.txt
///     executable: /bin/bash
///
/// - shell:
///     cmd: echo "file exists"
///     creates: /tmp/marker
///
/// - shell:
///     cmd: echo "file removed"
///     removes: /tmp/cleanup-target
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

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
    /// Shell to use for command execution.
    /// **[default: `"/bin/sh"`]**
    pub executable: Option<String>,
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    /// A filename, when it already exists, this step will not be run.
    pub creates: Option<String>,
    /// A filename, when it does not exist, this step will not be run.
    pub removes: Option<String>,
    /// Set stdin for the command.
    pub stdin: Option<String>,
}

fn check_creates(creates: &str) -> bool {
    Path::new(creates).exists()
}

fn check_removes(removes: &str) -> bool {
    !Path::new(removes).exists()
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

        let mut cmd = Command::new(executable);
        cmd.arg("-c").arg(&params.cmd);

        if let Some(ref chdir) = params.chdir {
            cmd.current_dir(Path::new(chdir));
        }

        let has_stdin = params.stdin.is_some();
        if has_stdin {
            cmd.stdin(Stdio::piped());
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        trace!("exec - {} -c '{}'", executable, params.cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if let Some(ref stdin_data) = params.stdin
            && let Some(ref mut stdin_handle) = child.stdin
        {
            stdin_handle
                .write_all(stdin_data.as_bytes())
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }

        let output = child
            .wait_with_output()
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

        Ok((ModuleResult::new(true, extra, module_output), None))
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
            cmd: "ls -la"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                cmd: "ls -la".to_owned(),
                executable: None,
                chdir: None,
                creates: None,
                removes: None,
                stdin: None,
            }
        );
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
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cmd, "cat file | grep pattern");
        assert_eq!(params.executable, Some("/bin/bash".to_owned()));
        assert_eq!(params.chdir, Some("/tmp".to_owned()));
        assert_eq!(params.creates, Some("/tmp/marker".to_owned()));
        assert_eq!(params.removes, Some("/tmp/cleanup".to_owned()));
        assert_eq!(params.stdin, Some("hello world".to_owned()));
    }

    #[test]
    fn test_parse_params_without_cmd() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chdir: /tmp
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
            cmd: "ls"
            yea: boo
            "#,
        )
        .unwrap();
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

        assert!(result.get_changed());
        assert_eq!(
            result.get_output(),
            Some("Would run: ls -la | head".to_string())
        );
    }

    #[test]
    fn test_check_mode_simple_string() {
        let shell = Shell;
        let yaml: YamlValue = YamlValue::String("ls".to_string());
        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("Would run: ls".to_string()));
    }

    #[test]
    fn test_creates_skips_when_file_exists() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(&format!(
            r#"
            cmd: "echo should_not_run"
            creates: "{}"
            "#,
            std::env::current_dir()
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "\\\\")
        ))
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(!result.get_changed());
        assert_eq!(result.get_output(), None);
    }

    #[test]
    fn test_removes_skips_when_file_missing() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "echo should_not_run"
            removes: "/nonexistent/path/that/does/not/exist"
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(!result.get_changed());
        assert_eq!(result.get_output(), None);
    }

    #[test]
    fn test_shell_execution() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "echo hello"
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("hello\n".to_string()));
    }

    #[test]
    fn test_shell_execution_with_pipe() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "echo 'hello world' | tr a-z A-Z"
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("HELLO WORLD\n".to_string()));
    }

    #[test]
    fn test_shell_execution_with_executable() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "echo hello from bash"
            executable: /bin/bash
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("hello from bash\n".to_string()));
    }

    #[test]
    fn test_shell_execution_with_chdir() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "pwd"
            chdir: /tmp
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("tmp"));
    }

    #[test]
    fn test_shell_execution_with_stdin() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "cat"
            stdin: "hello from stdin"
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert_eq!(result.get_output(), Some("hello from stdin".to_string()));
    }

    #[test]
    fn test_shell_execution_with_redirect() {
        let dir = tempfile::tempdir().unwrap();
        let outfile = dir.path().join("out.txt");
        let outfile_str = outfile.to_str().unwrap();

        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(&format!(
            r#"
            cmd: "echo redirected > {outfile_str}"
            "#
        ))
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        assert!(result.get_changed());
        assert!(outfile.exists());
    }

    #[test]
    fn test_shell_extra_contains_rc() {
        let shell = Shell;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            cmd: "echo ok"
            "#,
        )
        .unwrap();

        let (result, _) = shell
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap();

        let extra = result.get_extra().unwrap();
        let extra_map = extra.as_mapping().unwrap();
        assert_eq!(
            extra_map.get(serde_norway::Value::String("rc".to_string())),
            Some(&serde_norway::Value::Number(0.into()))
        );
    }
}
