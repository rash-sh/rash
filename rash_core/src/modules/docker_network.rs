/// ANCHOR: module
/// # docker_network
///
/// Manage Docker networks.
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
/// - name: Create a network
///   docker_network:
///     name: mynetwork
///     state: present
///
/// - name: Create a network with specific driver
///   docker_network:
///     name: mynetwork
///     driver: bridge
///     state: present
///
/// - name: Create a network with subnet
///   docker_network:
///     name: mynetwork
///     driver: bridge
///     subnet: 172.20.0.0/16
///     gateway: 172.20.0.1
///     state: present
///
/// - name: Create an overlay network for Swarm
///   docker_network:
///     name: myoverlay
///     driver: overlay
///     state: present
///
/// - name: Create a network with options
///   docker_network:
///     name: mynetwork
///     driver: bridge
///     options:
///       com.docker.network.bridge.enable_icc: "true"
///     state: present
///
/// - name: Remove a network
///   docker_network:
///     name: mynetwork
///     state: absent
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
    "bridge".to_owned()
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
    subnet: Option<String>,
    gateway: Option<String>,
    ip_range: Option<String>,
    options: Option<std::collections::HashMap<String, String>>,
    attachable: Option<bool>,
    scope: Option<String>,
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

fn network_exists(name: &str) -> Result<bool> {
    let output =
        run_command(Command::new("docker").args(["network", "ls", "--format", "{{.Name}}"]))?;
    let networks = String::from_utf8_lossy(&output.stdout);
    Ok(networks.lines().any(|n| n.trim() == name))
}

fn create_network(params: &Params, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let mut changed = false;
    let exists = network_exists(&params.name)?;

    if !exists {
        if check_mode {
            diff(
                format!("network {} absent", params.name),
                format!(
                    "network {} present (driver: {})",
                    params.name, params.driver
                ),
            );
            return Ok((true, None));
        }

        let mut cmd = Command::new("docker");
        cmd.args(["network", "create"]);

        cmd.arg("--driver").arg(&params.driver);

        if let Some(subnet) = &params.subnet {
            cmd.arg("--subnet").arg(subnet);
        }

        if let Some(gateway) = &params.gateway {
            cmd.arg("--gateway").arg(gateway);
        }

        if let Some(ip_range) = &params.ip_range {
            cmd.arg("--ip-range").arg(ip_range);
        }

        if let Some(options) = &params.options {
            for (key, value) in options {
                cmd.arg("-o").arg(format!("{}={}", key, value));
            }
        }

        if let Some(attachable) = params.attachable
            && attachable
        {
            cmd.arg("--attachable");
        }

        if let Some(scope) = &params.scope {
            cmd.arg("--scope").arg(scope);
        }

        cmd.arg(&params.name);

        diff(
            format!("network {} absent", params.name),
            format!(
                "network {} present (driver: {})",
                params.name, params.driver
            ),
        );
        run_command(&mut cmd)?;
        changed = true;
    }

    Ok((changed, None))
}

fn remove_network(name: &str, check_mode: bool) -> Result<(bool, Option<YamlValue>)> {
    let exists = network_exists(name)?;

    if exists {
        if check_mode {
            diff(
                format!("network {} present", name),
                format!("network {} absent", name),
            );
            return Ok((true, None));
        }

        run_command(Command::new("docker").args(["network", "rm", name]))?;
        diff(
            format!("network {} present", name),
            format!("network {} absent", name),
        );
        return Ok((true, None));
    }

    Ok((false, None))
}

#[derive(Debug)]
pub struct DockerNetwork;

impl Module for DockerNetwork {
    fn get_name(&self) -> &str {
        "docker_network"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        trace!("exec docker_network module");
        let params: Params = parse_params(params)?;

        let (changed, extra) = match params.state {
            State::Present => create_network(&params, check_mode)?,
            State::Absent => remove_network(&params.name, check_mode)?,
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
name: test_network
state: present
driver: bridge
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "test_network");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.driver, "bridge");
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = serde_norway::from_str(
            r#"
name: test_network
state: absent
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "test_network");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_subnet() {
        let yaml = serde_norway::from_str(
            r#"
name: test_network
subnet: 172.20.0.0/16
gateway: 172.20.0.1
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.subnet, Some("172.20.0.0/16".to_owned()));
        assert_eq!(params.gateway, Some("172.20.0.1".to_owned()));
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml = serde_norway::from_str(
            r#"
name: test_network
options:
  com.docker.network.bridge.enable_icc: "true"
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.options.is_some());
        let opts = params.options.unwrap();
        assert_eq!(
            opts.get("com.docker.network.bridge.enable_icc"),
            Some(&"true".to_owned())
        );
    }

    #[test]
    fn test_default_driver() {
        let yaml = serde_norway::from_str(
            r#"
name: test_network
"#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.driver, "bridge");
    }
}
