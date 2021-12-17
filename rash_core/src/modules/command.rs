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
///       - "Hellow World"'
///     transfer_pid_1: true
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, ModuleResult};
use crate::vars::Vars;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use exec as exec_command;
#[cfg(feature = "docs")]
use schemars::JsonSchema;
use serde::Deserialize;
use yaml_rust::Yaml;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
pub struct Params {
    /// The command to run.
    cmd: Option<String>,
    /// Passes the command arguments as a list rather than a string.
    /// Only the string or the list form can be provided, not both.
    argv: Option<Vec<String>>,
    /// Execute command as PID 1.
    /// Note: from this point on, your rash script execution is transferred to the command
    transfer_pid_1: Option<bool>,
}

pub fn exec(optional_params: Yaml, vars: Vars, _check_mode: bool) -> Result<(ModuleResult, Vars)> {
    let params: Params = match optional_params.as_str() {
        Some(s) => Params {
            cmd: Some(s.to_string()),
            argv: None,
            transfer_pid_1: None,
        },
        None => parse_params(optional_params)?,
    };

    if params.cmd.is_none() & params.argv.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "argv or cmd must be defined",
        ));
    }

    match params.transfer_pid_1 {
        Some(true) => {
            let args_vec = match params.cmd {
                Some(s) => s
                    .split_whitespace()
                    .map(String::from)
                    .collect::<Vec<String>>(),
                // safe unwrap: verify in parse_params
                None => params.argv.unwrap(),
            };
            let mut args = args_vec.iter();

            let program = args.next().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, format!("{:?} invalid cmd", args))
            })?;
            let error = exec_command::Command::new(program)
                .args(&args.clone().collect::<Vec<_>>())
                .exec();
            Err(Error::new(ErrorKind::SubprocessFail, error))
        }
        None | Some(false) => {
            let output = if params.cmd.is_some() {
                Command::new("/bin/sh")
                    // safe unwrap: verify in parse_params
                    .args(vec!["-c", &params.cmd.unwrap()])
                    .output()
                    .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
            } else {
                // safe unwrap: verify in parse_params
                let argv = params.argv.unwrap();
                let mut args = argv.iter();
                let program = args.next().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, format!("{:?} invalid cmd", args))
                })?;

                Command::new(program)
                    .args(args)
                    .output()
                    .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
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

            Ok((
                ModuleResult {
                    changed: true,
                    output: module_output,
                    extra: Some(json!({
                        "rc": output.status.code(),
                        "stderr": stderr,
                    })),
                },
                vars,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str(
            r#"
        cmd: "ls"
        transfer_pid_1: false
        "#,
        )
        .unwrap()
        .first()
        .unwrap()
        .clone();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                cmd: Some("ls".to_string()),
                argv: None,
                transfer_pid_1: Some(false),
            }
        );
    }
}
