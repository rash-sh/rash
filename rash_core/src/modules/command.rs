/// ANCHOR: module
/// # command
///
/// Execute commands.
///
/// ## Parameters
///
/// ```yaml
/// argv:
///   type: list
///   description: |
///     Passes the command as a list rather than a string.
///     Only the string or the list form can be provided, not both.
///     One or the other must be provided.
/// cmd:
///   type: string
///   description: The command to run.
/// transfer_pid_1:
///   type: bool
///   description: |
///     Execute command as PID 1.
///     Note: from this point your rash script execution is transferer to command.
/// ```
/// ## Example
///
/// ```yaml
/// - command: 'echo "Hellow World"'
/// ```
/// ANCHOR_END: module
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{get_param_bool, get_param_list, ModuleResult};
use crate::vars::Vars;

use std::process::Command;

use exec as exec_command;
use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
struct Params {
    cmd: Option<String>,
    argv: Option<Vec<String>>,
    transfer_pid_1: bool,
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("parse params: {:?}", yaml);
    let cmd = yaml
        .as_str()
        .or_else(|| yaml["cmd"].as_str())
        .map(String::from);

    let argv = get_param_list(&yaml, "argv").ok();
    let transfer_pid_1 = get_param_bool(&yaml, "transfer_pid_1").or_else(|e| match e.kind() {
        ErrorKind::NotFound => Ok(false),
        _ => Err(e),
    })?;

    if cmd.is_none() & argv.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "argv or cmd must be defined",
        ));
    }

    Ok(Params {
        cmd,
        argv,
        transfer_pid_1,
    })
}

pub fn exec(optional_params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
    let params = parse_params(optional_params)?;
    trace!("exec - params: {:?}", params);

    if params.transfer_pid_1 {
        let args_vec = match params.cmd {
            Some(s) => s
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<String>>(),
            // safe unwrap: verify in parse_params
            None => params.argv.unwrap(),
        };
        let mut args = args_vec.iter();

        // safe unwrap: verify in parse_params
        let program = args.next().unwrap();
        let error = exec_command::Command::new(program)
            .args(&args.clone().collect::<Vec<_>>())
            .exec();
        return Err(Error::new(ErrorKind::SubprocessFail, error));
    }

    let output = if params.cmd.is_some() {
        Command::new("/bin/sh")
            .args(vec!["-c", &params.cmd.unwrap()])
            .output()
            .or_else(|e| Err(Error::new(ErrorKind::SubprocessFail, e)))?
    } else {
        let argv = params.argv.unwrap();
        let mut args = argv.iter();
        // safe unwrap: verify in parse_params
        let program = args.next().unwrap();
        Command::new(program)
            .args(args)
            .output()
            .or_else(|e| Err(Error::new(ErrorKind::SubprocessFail, e)))?
    };

    trace!("exec - output: {:?}", output);
    let stderr =
        String::from_utf8(output.stderr).or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;

    if !output.status.success() {
        return Err(Error::new(ErrorKind::InvalidData, stderr));
    }
    let output_string =
        String::from_utf8(output.stdout).or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_parse_params() {
        let yaml = YamlLoader::load_from_str("ls")
            .unwrap()
            .first()
            .unwrap()
            .clone();
        let params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                cmd: Some("ls".to_string()),
                argv: None,
                transfer_pid_1: false,
            }
        );
    }
}
