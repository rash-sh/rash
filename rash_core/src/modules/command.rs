use crate::error::{Error, ErrorKind, Result};
use crate::modules::ModuleResult;

use std::process::Command;

use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
struct Params {
    cmd: String,
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
    Ok(Params {
        cmd: cmd.to_string(),
    })
}

pub fn exec(optional_params: Yaml) -> Result<ModuleResult> {
    let params = parse_params(optional_params)?;
    trace!("exec - params: {:?}", params);

    let mut args = params.cmd.split_whitespace();

    // safe unwrap: verify in parse_params
    let program = args.next().unwrap();
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

    Ok(ModuleResult {
        changed: true,
        output: Some(
            String::from_utf8(output.stdout)
                .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?,
        ),
        extra: Some(json!({
            "rc": output.status.code(),
            "stderr": stderr,
        })),
    })
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
                cmd: "ls".to_string()
            }
        );
    }
}
