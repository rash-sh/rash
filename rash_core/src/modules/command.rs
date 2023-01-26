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
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command as StdCommand;

use exec as exec_command;
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use serde_yaml::value;
use serde_yaml::Value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(flatten)]
    pub required: Required,
    /// [DEPRECATED] Execute command as PID 1.
    /// Note: from this point on, your rash script execution is transferred to the command
    pub transfer_pid_1: Option<bool>,
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

fn exec_transferring_pid(params: Params) -> Result<(ModuleResult, Vars)> {
    let args_vec = match params.required {
        Required::Cmd(s) => s
            .split_whitespace()
            .map(String::from)
            .collect::<Vec<String>>(),
        Required::Argv(x) => x,
    };
    let mut args = args_vec.iter();

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
        optional_params: Value,
        vars: Vars,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Vars)> {
        let params: Params = match optional_params.as_str() {
            Some(s) => Params {
                required: Required::Cmd(s.to_string()),
                transfer_pid_1: None,
                transfer_pid: None,
            },
            None => parse_params(optional_params)?,
        };

        match params.transfer_pid {
            Some(true) => {
                warn!("transfer_pid_1 option will be removed in 1.9.0");
                exec_transferring_pid(params)
            }
            None | Some(false) => {
                match params.transfer_pid {
                    Some(true) => exec_transferring_pid(params),
                    None | Some(false) => {
                        let output = match params.required {
                            Required::Cmd(cmd) => {
                                trace!("exec - /bin/sh -c '{cmd:?}'");
                                StdCommand::new("/bin/sh")
                                    // safe unwrap: verified
                                    .args(vec!["-c", &cmd])
                                    .output()
                                    .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
                            }
                            Required::Argv(argv) => {
                                // safe unwrap: verify in parse_params
                                let mut args = argv.iter();
                                let program = args.next().ok_or_else(|| {
                                    Error::new(
                                        ErrorKind::InvalidData,
                                        format!("{args:?} invalid cmd"),
                                    )
                                })?;
                                trace!("exec - '{argv:?}'");
                                StdCommand::new(program)
                                    .args(args)
                                    .output()
                                    .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
                            }
                        };

                        trace!("exec - output: {:?}", output);
                        let stderr = String::from_utf8(output.stderr)
                            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

                        if !output.status.success() {
                            return Err(Error::new(ErrorKind::InvalidData, stderr));
                        }
                        let output_string = String::from_utf8(output.stdout)
                            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

                        let module_output = if output_string.is_empty() {
                            None
                        } else {
                            Some(output_string)
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
                }
            }
        }
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: Value = serde_yaml::from_str(
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
                required: Required::Cmd("ls".to_string()),
                transfer_pid_1: None,
                transfer_pid: Some(false),
            }
        );
    }

    #[test]
    fn test_parse_params_without_cmd_or_argv() {
        let yaml: Value = serde_yaml::from_str(
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
        let yaml: Value = serde_yaml::from_str(
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
