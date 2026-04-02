/// ANCHOR: module
/// # docker_volume
///
/// Manage Docker volumes.
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
/// - name: Create a volume
///   docker_volume:
///     name: myvolume
///     state: present
///
/// - name: Create a volume with driver
///   docker_volume:
///     name: myvolume
///     driver: local
///     state: present
///
/// - name: Create a volume with driver options
///   docker_volume:
///     name: nfs_volume
///     driver: local
///     driver_options:
///       type: nfs
///       o: addr=192.168.1.1,rw
///       device: ":/export/data"
///     state: present
///
/// - name: Create a volume with labels
///   docker_volume:
///     name: myvolume
///     labels:
///       environment: production
///       app: myapp
///     state: present
///
/// - name: Remove a volume
///   docker_volume:
///     name: myvolume
///     state: absent
///
/// - name: Remove a volume forcefully
///   docker_volume:
///     name: myvolume
///     state: absent
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

fn default_state() -> State {
    State::default()
}

fn default_driver() -> String {
    "local".to_owned()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
struct Params {
    #[serde(default = "default_state")]
    state: State,
    name: String,
    #[serde(default = "default_driver")]
    driver: String,
    driver_options: Option<std::collections::HashMap<String, String>>,
    labels: Option<std::collections::HashMap<String, String>>,
    force: Option<bool>,
}

fn run_command(cmd: &mut Command) -> Result<Output> {
    trace!("running command: {:?}", cmd);
    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    trace!("command output: {:?}", output);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("docker command failed: {}", stderr),
        ));
    }
    Ok(output)
}

fn volume_exists(name: &str) -> Result<bool> {
    let output =
        run_command(Command::new("docker").args(["volume", "ls", "--format", "{{.Name}}"]))?;
    let volumes = String::from_utf8_lossy(&output.stdout);
    Ok(volumes.lines().any(|v| v.trim() == name))
}

fn create_volume(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let mut changed = false;
    let exists = volume_exists(&params.name)?;

    if !exists {
        if check_mode {
            diff(
                format!("volume {} absent", params.name),
                format!("volume {} present (driver: {})", params.name, params.driver),
            );
            return Ok((true, None));
        }

        let mut cmd = Command::new("docker");
        cmd.args(["volume", "create"]);

        cmd.arg("--driver").arg(&params.driver);

        if let Some(driver_options) = &params.driver_options {
            for (key, value) in driver_options {
                cmd.arg("-o").arg(format!("{}={}", key, value));
            }
        }

        if let Some(labels) = &params.labels {
            for (key, value) in labels {
                cmd.arg("--label").arg(format!("{}={}", key, value));
            }
        }

        cmd.arg(&params.name);

        diff(
            format!("volume {} absent", params.name),
            format!("volume {} present (driver: {})", params.name, params.driver),
        );
        run_command(&mut cmd)?;
        changed = true;
    }

    Ok((changed, None))
}

fn remove_volume(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let exists = volume_exists(&params.name)?;

    if exists {
        if check_mode {
            diff(
                format!("volume {} present", params.name),
                format!("volume {} absent", params.name),
            );
            return Ok((true, None));
        }

        let mut cmd = Command::new("docker");
        cmd.args(["volume", "rm"]);

        if params.force.unwrap_or(false) {
            cmd.arg("-f");
        }

        cmd.arg(&params.name);

        run_command(&mut cmd)?;
        diff(
            format!("volume {} present", params.name),
            format!("volume {} absent", params.name),
        );
        return Ok((true, None));
    }

    Ok((false, None))
}

#[derive(Debug)]
pub struct DockerVolume;

impl Module for DockerVolume {
    fn get_name(&self) -> &str {
        "docker_volume"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        trace!("exec docker_volume module");
        let params: Params = parse_params(params)?;

        let (changed, extra) = match params.state {
            State::Present => create_volume(&params, check_mode)?,
            State::Absent => remove_volume(&params, check_mode)?,
        };

        Ok((ModuleResult::new(changed, extra, None), None))
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
    fn test_parse_params_present() {
        let yaml = serde_norway::from_str(
            r#"
name: test_volume
state: present
driver: local
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "test_volume");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.driver, "local");
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = serde_norway::from_str(
            r#"
name: test_volume
state: absent
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "test_volume");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_driver_options() {
        let yaml = serde_norway::from_str(
            r#"
name: nfs_volume
driver: local
driver_options:
  type: nfs
  o: addr=192.168.1.1,rw
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.driver_options.is_some());
        let opts = params.driver_options.unwrap();
        assert_eq!(opts.get("type"), Some(&"nfs".to_owned()));
    }

    #[test]
    fn test_parse_params_with_labels() {
        let yaml = serde_norway::from_str(
            r#"
name: test_volume
labels:
  environment: production
  app: myapp
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.labels.is_some());
        let labels = params.labels.unwrap();
        assert_eq!(labels.get("environment"), Some(&"production".to_owned()));
        assert_eq!(labels.get("app"), Some(&"myapp".to_owned()));
    }

    #[test]
    fn test_parse_params_force() {
        let yaml = serde_norway::from_str(
            r#"
name: test_volume
state: absent
force: true
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.force, Some(true));
    }

    #[test]
    fn test_default_driver() {
        let yaml = serde_norway::from_str(
            r#"
name: test_volume
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.driver, "local");
    }
}
