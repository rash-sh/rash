/// ANCHOR: module
/// # command
///
/// Execute commands.
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
/// - command:
///     argv:
///       - echo
///       - "Hellow World"
///     transfer_pid: true
///
/// - command: ls examples
///   register: ls_result
///
/// - command:
///     cmd: ls .
///     chdir: examples
///   register: ls_result
///
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::env::set_current_dir;
use std::path::Path;
use std::process::Command as StdCommand;

use exec as exec_command;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use serde_yaml::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Change into this directory before running the command.
    pub chdir: Option<String>,
    #[serde(flatten)]
    pub required: Required,
    /// Execute command as PID 1.
    /// Note: from this point on, your rash script execution is transferred to the command
    pub transfer_pid: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Required {
    /// The command to run.
    Cmd(String),
    /// Passes the command arguments as a list rather than a string.
    /// Only the string or the list form can be provided, not both.
    Argv(Vec<String>),
}

fn exec_transferring_pid(params: Params) -> Result<(ModuleResult, Value)> {
    let args_vec = match params.required {
        Required::Cmd(s) => s
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<String>>(),
        Required::Argv(x) => x,
    };
    let mut args = args_vec.iter();

    if let Some(s) = params.chdir {
        set_current_dir(Path::new(&s)).map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
    };

    let program = args
        .next()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("{args:?} invalid cmd")))?;
    let error = exec_command::Command::new(program)
        .args(&args.clone().collect::<Vec<_>>())
        .exec();
    Err(Error::new(ErrorKind::SubprocessFail, error))
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
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        let params: Params = match optional_params.as_str() {
            Some(s) => Params {
                chdir: None,
                required: Required::Cmd(s.to_owned()),
                transfer_pid: None,
            },
            None => parse_params(optional_params)?,
        };

        match params.transfer_pid {
            Some(true) => exec_transferring_pid(params),
            None | Some(false) => match params.transfer_pid {
                Some(true) => exec_transferring_pid(params),
                None | Some(false) => {
                    let mut cmd = match params.required.clone() {
                        Required::Cmd(cmd) => {
                            trace!("exec - /bin/sh -c '{cmd:?}'");
                            StdCommand::new("/bin/sh")
                        }
                        Required::Argv(argv) => {
                            let program = argv.first().ok_or_else(|| {
                                Error::new(ErrorKind::InvalidData, format!("{argv:?} invalid cmd"))
                            })?;
                            trace!("exec - '{argv:?}'");
                            StdCommand::new(program)
                        }
                    };

                    let cmd_args = match params.required {
                        Required::Cmd(s) => cmd.args(vec!["-c", &s]),
                        Required::Argv(argv) => {
                            let args = argv.iter().skip(1);
                            cmd.args(args)
                        }
                    };

                    let cmd_chdir = match params.chdir {
                        Some(s) => cmd_args.current_dir(Path::new(&s)),
                        None => cmd_args,
                    };

                    let output = cmd_chdir
                        .output()
                        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

                    trace!("exec - output: {:?}", output);
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
                        vars,
                    ))
                }
            },
        }
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
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            cmd: "ls"
            transfer_pid: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                chdir: None,
                required: Required::Cmd("ls".to_owned()),
                transfer_pid: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_without_cmd_or_argv() {
        let yaml: YamlValue = serde_yaml::from_str(
            r#"
            transfer_pid: false
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_yaml::from_str(
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
}
