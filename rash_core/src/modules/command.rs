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
    let cmd = match yaml.as_str() {
        Some(cmd) => cmd.to_string(),
        None => yaml["cmd"]
            .as_str()
            .ok_or(Error::new(
                ErrorKind::NotFound,
                format!("Not cmd param found in: {:?}", yaml),
            ))?
            .to_string(),
    };
    Ok(Params { cmd: cmd })
}

pub fn exec(optional_params: Yaml) -> Result<ModuleResult> {
    let params = parse_params(optional_params)?;
    let output = match Command::new(params.cmd).output() {
        Ok(s) => Ok(s),
        Err(e) => Err(Error::new(ErrorKind::SubprocessFail, e)),
    }?;

    Ok(ModuleResult {
        changed: true,
        extra: Some(json!({
            "rc": output.status.code(),
            "stdout": match String::from_utf8(output.stdout) {
                Ok(s) => Ok(s),
                Err(e) => Err(Error::new(ErrorKind::InvalidData, e))
            }?,
            "stderr": match String::from_utf8(output.stderr) {
                Ok(s) => Ok(s),
                Err(e) => Err(Error::new(ErrorKind::InvalidData, e))
            }?
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
