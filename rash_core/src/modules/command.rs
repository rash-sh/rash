use crate::error::{Error, ErrorKind, Result};
use crate::modules::ModuleResult;

use std::process::Command;

use yaml_rust::Yaml;

#[derive(Debug, PartialEq)]
struct Params {
    cmd: String,
}

fn parse_params(yaml: Yaml) -> Result<Params> {
    trace!("command - parse params: {:?}", yaml);
    let cmd = yaml
        .as_str()
        .or_else(|| yaml["cmd"].as_str())
        .ok_or(Error::new(
            ErrorKind::NotFound,
            format!("Not cmd param found in: {:?}", yaml),
        ))?;
    Ok(Params {
        cmd: cmd.to_string(),
    })
}

pub fn exec(optional_params: Yaml) -> Result<ModuleResult> {
    let params = parse_params(optional_params)?;
    let output = Command::new(params.cmd)
        .output()
        .or_else(|e| Err(Error::new(ErrorKind::SubprocessFail, e)))?;

    Ok(ModuleResult {
        changed: true,
        extra: Some(json!({
            "rc": output.status.code(),
            "stdout": String::from_utf8(output.stdout)
                .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?,
            "stderr": String::from_utf8(output.stderr)
                .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?,
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
