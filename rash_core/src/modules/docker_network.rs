/// ANCHOR: module
/// # docker_network
///
/// Manage Docker networks for container orchestration.
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
/// - name: Create a bridge network
///   docker_network:
///     name: mynetwork
///     state: present
///
/// - name: Create a network with custom subnet
///   docker_network:
///     name: app_network
///     driver: bridge
///     subnet: "172.20.0.0/16"
///     gateway: "172.20.0.1"
///     state: present
///
/// - name: Create an overlay network for swarm
///   docker_network:
///     name: swarm_network
///     driver: overlay
///     scope: swarm
///     attachable: true
///     state: present
///
/// - name: Create an isolated internal network
///   docker_network:
///     name: internal_network
///     driver: bridge
///     internal: true
///     state: present
///
/// - name: Create a network with IP range
///   docker_network:
///     name: limited_network
///     subnet: "172.30.0.0/16"
///     ip_range: "172.30.0.0/24"
///     state: present
///
/// - name: Create an IPv6 enabled network
///   docker_network:
///     name: ipv6_network
///     enable_ipv6: true
///     subnet: "fd00:dead:beef::/48"
///     state: present
///
/// - name: Remove a network
///   docker_network:
///     name: old_network
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
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Driver {
    #[default]
    Bridge,
    Overlay,
    Macvlan,
    Null,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Scope {
    #[default]
    Local,
    Swarm,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct IpamConfig {
    /// IPv4 subnet CIDR.
    subnet: Option<String>,
    /// IPv4 gateway.
    gateway: Option<String>,
    /// IPv4 address range.
    ip_range: Option<String>,
    /// IPv6 subnet CIDR.
    subnet_ipv6: Option<String>,
    /// IPv6 gateway.
    gateway_ipv6: Option<String>,
}

fn default_driver() -> Driver {
    Driver::Bridge
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Network name (required).
    name: String,
    /// Network driver (bridge, overlay, macvlan, null).
    #[serde(default = "default_driver")]
    driver: Driver,
    /// IPv4 subnet CIDR.
    #[serde(default)]
    subnet: Option<String>,
    /// IPv4 gateway.
    #[serde(default)]
    gateway: Option<String>,
    /// IPv4 address range.
    #[serde(default)]
    ip_range: Option<String>,
    /// Restrict external access to the network.
    #[serde(default)]
    internal: bool,
    /// Enable IPv6 networking.
    #[serde(default)]
    enable_ipv6: bool,
    /// Allow manual container attachment to network.
    #[serde(default)]
    attachable: bool,
    /// Network scope (local, swarm).
    #[serde(default)]
    scope: Scope,
    /// Desired state of the network.
    #[serde(default)]
    state: State,
    /// Force removal of the network.
    #[serde(default)]
    force: bool,
    /// IPAM configuration.
    #[serde(default)]
    ipam_config: Option<Vec<IpamConfig>>,
}

#[derive(Debug)]
pub struct DockerNetwork;

#[derive(Debug, Clone)]
struct NetworkInfo {
    id: String,
    name: String,
    driver: String,
    scope: String,
    internal: bool,
    enable_ipv6: bool,
    attachable: bool,
}

impl Module for DockerNetwork {
    fn get_name(&self) -> &str {
        "docker_network"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_network(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct DockerClient {
    check_mode: bool,
}

impl DockerClient {
    fn new(check_mode: bool) -> Self {
        DockerClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `docker {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing docker: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn network_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            &[
                "network",
                "ls",
                "--filter",
                &format!("name={}", name),
                "--format",
                "{{.Name}}",
            ],
            false,
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    fn get_network_info(&self, name: &str) -> Result<Option<NetworkInfo>> {
        let output = self.exec_cmd(
            &[
                "network",
                "inspect",
                "--format",
                "{{.Id}}|{{.Name}}|{{.Driver}}|{{.Scope}}|{{.Internal}}|{{.EnableIPv6}}|{{.Attachable}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 7 {
            Ok(Some(NetworkInfo {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                driver: parts[2].to_string(),
                scope: parts[3].to_string(),
                internal: parts[4] == "true",
                enable_ipv6: parts[5] == "true",
                attachable: parts[6] == "true",
            }))
        } else {
            Ok(None)
        }
    }

    fn create_network(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["network".to_string(), "create".to_string()];

        args.push("--driver".to_string());
        args.push(driver_to_string(&params.driver));

        if let Some(ref subnet) = params.subnet {
            args.push("--subnet".to_string());
            args.push(subnet.clone());
        }

        if let Some(ref gateway) = params.gateway {
            args.push("--gateway".to_string());
            args.push(gateway.clone());
        }

        if let Some(ref ip_range) = params.ip_range {
            args.push("--ip-range".to_string());
            args.push(ip_range.clone());
        }

        if params.internal {
            args.push("--internal".to_string());
        }

        if params.enable_ipv6 {
            args.push("--ipv6".to_string());
        }

        if params.attachable {
            args.push("--attachable".to_string());
        }

        match params.scope {
            Scope::Swarm => {
                args.push("--scope".to_string());
                args.push("swarm".to_string());
            }
            Scope::Local => {}
        }

        if let Some(ref ipam_configs) = params.ipam_config {
            for config in ipam_configs {
                if let Some(ref subnet) = config.subnet {
                    args.push("--subnet".to_string());
                    args.push(subnet.clone());
                }
                if let Some(ref gateway) = config.gateway {
                    args.push("--gateway".to_string());
                    args.push(gateway.clone());
                }
                if let Some(ref ip_range) = config.ip_range {
                    args.push("--ip-range".to_string());
                    args.push(ip_range.clone());
                }
                if let Some(ref subnet_ipv6) = config.subnet_ipv6 {
                    args.push("--subnet".to_string());
                    args.push(subnet_ipv6.clone());
                }
                if let Some(ref gateway_ipv6) = config.gateway_ipv6 {
                    args.push("--gateway".to_string());
                    args.push(gateway_ipv6.clone());
                }
            }
        }

        args.push(params.name.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn remove_network(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.network_exists(name)? {
            return Ok(false);
        }

        let mut args = vec!["network", "rm"];
        if force {
            args.push("-f");
        }
        args.push(name);

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn get_network_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_network_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("driver".to_string(), serde_json::Value::String(info.driver));
            result.insert("scope".to_string(), serde_json::Value::String(info.scope));
            result.insert(
                "internal".to_string(),
                serde_json::Value::Bool(info.internal),
            );
            result.insert(
                "enable_ipv6".to_string(),
                serde_json::Value::Bool(info.enable_ipv6),
            );
            result.insert(
                "attachable".to_string(),
                serde_json::Value::Bool(info.attachable),
            );
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn driver_to_string(driver: &Driver) -> String {
    match driver {
        Driver::Bridge => "bridge",
        Driver::Overlay => "overlay",
        Driver::Macvlan => "macvlan",
        Driver::Null => "null",
    }
    .to_string()
}

fn validate_network_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Network name cannot be empty",
        ));
    }

    if name.len() > 63 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Network name too long (max 63 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Network name contains invalid characters (only [a-zA-Z0-9.-_] allowed)",
        ));
    }

    if name.starts_with('-') || name.starts_with('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Network name cannot start with '-' or '.'",
        ));
    }

    Ok(())
}

fn validate_subnet(subnet: &str) -> Result<()> {
    if subnet.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Subnet cannot be empty"));
    }

    if !subnet.contains('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Subnet must include CIDR notation (e.g., 172.20.0.0/16)",
        ));
    }

    Ok(())
}

fn docker_network(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_network_name(&params.name)?;

    if let Some(ref subnet) = params.subnet {
        validate_subnet(subnet)?;
    }

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            let exists = client.network_exists(&params.name)?;

            if !exists {
                client.create_network(&params)?;
                diff(
                    format!("network: {} (absent)", params.name),
                    format!("network: {} (present)", params.name),
                );
                output_messages.push(format!(
                    "Network '{}' created with driver '{}'",
                    params.name,
                    driver_to_string(&params.driver)
                ));
                changed = true;
            } else {
                output_messages.push(format!("Network '{}' already exists", params.name));
            }
        }
        State::Absent => {
            if client.remove_network(&params.name, params.force)? {
                diff(
                    format!("network: {} (present)", params.name),
                    format!("network: {} (absent)", params.name),
                );
                output_messages.push(format!("Network '{}' removed", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Network '{}' not found", params.name));
            }
        }
    }

    let extra = client.get_network_state(&params.name)?;

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mynetwork
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "mynetwork");
        assert_eq!(params.driver, Driver::Bridge);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app_network
            driver: bridge
            subnet: "172.20.0.0/16"
            gateway: "172.20.0.1"
            ip_range: "172.20.0.0/24"
            internal: true
            enable_ipv6: true
            attachable: true
            scope: local
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "app_network");
        assert_eq!(params.driver, Driver::Bridge);
        assert_eq!(params.subnet, Some("172.20.0.0/16".to_string()));
        assert_eq!(params.gateway, Some("172.20.0.1".to_string()));
        assert_eq!(params.ip_range, Some("172.20.0.0/24".to_string()));
        assert!(params.internal);
        assert!(params.enable_ipv6);
        assert!(params.attachable);
        assert_eq!(params.scope, Scope::Local);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_overlay() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: swarm_network
            driver: overlay
            scope: swarm
            attachable: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "swarm_network");
        assert_eq!(params.driver, Driver::Overlay);
        assert_eq!(params.scope, Scope::Swarm);
        assert!(params.attachable);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_network
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_network");
        assert_eq!(params.state, State::Absent);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_macvlan() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: macvlan_net
            driver: macvlan
            subnet: "192.168.1.0/24"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "macvlan_net");
        assert_eq!(params.driver, Driver::Macvlan);
        assert_eq!(params.subnet, Some("192.168.1.0/24".to_string()));
    }

    #[test]
    fn test_parse_params_ipam_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: multi_subnet_network
            driver: bridge
            ipam_config:
              - subnet: "172.20.0.0/16"
                gateway: "172.20.0.1"
                ip_range: "172.20.0.0/24"
              - subnet_ipv6: "fd00:dead:beef::/48"
                gateway_ipv6: "fd00:dead:beef::1"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let ipam_configs = params.ipam_config.unwrap();
        assert_eq!(ipam_configs.len(), 2);
        assert_eq!(ipam_configs[0].subnet, Some("172.20.0.0/16".to_string()));
        assert_eq!(ipam_configs[0].gateway, Some("172.20.0.1".to_string()));
        assert_eq!(
            ipam_configs[1].subnet_ipv6,
            Some("fd00:dead:beef::/48".to_string())
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mynetwork
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_network_name() {
        assert!(validate_network_name("mynetwork").is_ok());
        assert!(validate_network_name("my-network").is_ok());
        assert!(validate_network_name("my_network").is_ok());
        assert!(validate_network_name("my.network").is_ok());
        assert!(validate_network_name("mynetwork123").is_ok());
        assert!(validate_network_name("MyNetwork").is_ok());

        assert!(validate_network_name("").is_err());
        assert!(validate_network_name(&"a".repeat(64)).is_err());
        assert!(validate_network_name("-mynetwork").is_err());
        assert!(validate_network_name(".mynetwork").is_err());
        assert!(validate_network_name("my network").is_err());
        assert!(validate_network_name("my/network").is_err());
    }

    #[test]
    fn test_validate_subnet() {
        assert!(validate_subnet("172.20.0.0/16").is_ok());
        assert!(validate_subnet("192.168.1.0/24").is_ok());
        assert!(validate_subnet("10.0.0.0/8").is_ok());
        assert!(validate_subnet("fd00:dead:beef::/48").is_ok());

        assert!(validate_subnet("").is_err());
        assert!(validate_subnet("172.20.0.0").is_err());
    }

    #[test]
    fn test_driver_to_string() {
        assert_eq!(driver_to_string(&Driver::Bridge), "bridge");
        assert_eq!(driver_to_string(&Driver::Overlay), "overlay");
        assert_eq!(driver_to_string(&Driver::Macvlan), "macvlan");
        assert_eq!(driver_to_string(&Driver::Null), "null");
    }
}
