use crate::error::{Error, ErrorKind, Result};
use crate::modules::{get_param_bool, ModuleResult};
use crate::vars::Vars;

use std::process::Command;

use exec as exec_command;
use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
struct Params {
    cmd: String,
    transfer_pid_1: bool,
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("parse params: {:?}", yaml);
    let cmd = yaml
        .as_str()
        .or_else(|| yaml["cmd"].as_str())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("Not cmd param found in: {:?}", yaml),
            )
        })?;
    let transfer_pid_1 = get_param_bool(&yaml, "transfer_pid_1").or_else(|e| match e.kind() {
        ErrorKind::NotFound => Ok(false),
        _ => Err(e),
    })?;
    Ok(Params {
        cmd: cmd.to_string(),
        transfer_pid_1,
    })
}

pub fn exec(optional_params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
    let params = parse_params(optional_params)?;
    trace!("exec - params: {:?}", params);

    let mut args = params.cmd.split_whitespace();

    // safe unwrap: verify in parse_params
    let program = args.next().unwrap();

    if params.transfer_pid_1 {
        let error = exec_command::Command::new(program)
            .args(&args.clone().collect::<Vec<_>>())
            .exec();
        return Err(Error::new(ErrorKind::SubprocessFail, error));
    }

    let output = Command::new(program)
        .args(&args.collect::<Vec<_>>())
        .output()
        .or_else(|e| Err(Error::new(ErrorKind::SubprocessFail, e)))?;

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
                cmd: "ls".to_string(),
                transfer_pid_1: false,
            }
        );
    }
}
