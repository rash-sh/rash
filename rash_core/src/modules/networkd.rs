/// ANCHOR: module
/// # networkd
///
/// Manage systemd-networkd configuration files (.network, .link, .netdev).
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Configure static IP on eth0
///   networkd:
///     name: 10-eth0
///     type: network
///     state: present
///     interfaces:
///       - eth0
///     addresses:
///       - 192.168.1.100/24
///     gateway: 192.168.1.1
///     dns:
///       - 8.8.8.8
///       - 8.8.4.4
///
/// - name: Configure DHCP on eth0
///   networkd:
///     name: 20-dhcp
///     type: network
///     state: present
///     interfaces:
///       - eth0
///     dhcp: true
///
/// - name: Create bridge netdev
///   networkd:
///     name: br0
///     type: netdev
///     state: present
///     netdev_kind: bridge
///
/// - name: Create bond netdev
///   networkd:
///     name: bond0
///     type: netdev
///     state: present
///     netdev_kind: bond
///
/// - name: Configure VLAN netdev
///   networkd:
///     name: vlan10
///     type: netdev
///     state: present
///     netdev_kind: vlan
///     vlan_id: 10
///
/// - name: Configure link MTU
///   networkd:
///     name: 10-eth0
///     type: link
///     state: present
///     interfaces:
///       - eth0
///     mtu: 9000
///
/// - name: Remove network configuration
///   networkd:
///     name: 10-eth0
///     type: network
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_NETWORKD_DIR: &str = "/etc/systemd/network";

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ConfigType {
    Network,
    Link,
    Netdev,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum NetdevKind {
    Bridge,
    Bond,
    Vlan,
    Macvlan,
    Ipvlan,
    Vxlan,
    Tun,
    Tap,
    Wireguard,
    Dummy,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the configuration file (without extension).
    pub name: String,
    /// Type of configuration: network, link, or netdev.
    #[serde(rename = "type")]
    pub type_: ConfigType,
    /// Whether the configuration should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Interface names to match.
    pub interfaces: Option<Vec<String>>,
    /// IP addresses for the interface.
    pub addresses: Option<Vec<String>>,
    /// Default gateway.
    pub gateway: Option<String>,
    /// DNS servers.
    pub dns: Option<Vec<String>>,
    /// Enable DHCP (ipv4, ipv6, true, false).
    /// **[default: `false`]**
    #[serde(default)]
    pub dhcp: Option<bool>,
    /// VLAN ID (for netdev type vlan).
    pub vlan_id: Option<u16>,
    /// Netdev kind (bridge, bond, vlan, etc.).
    pub netdev_kind: Option<NetdevKind>,
    /// MTU for the link.
    pub mtu: Option<u32>,
    /// Create backup of existing config file.
    pub backup: Option<bool>,
    /// Path to the systemd-networkd configuration directory.
    /// **[default: `"/etc/systemd/network"`]**
    pub directory: Option<String>,
    /// Restart systemd-networkd after changes.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub restart: bool,
    /// Raw INI content to use instead of generated config.
    pub config: Option<String>,
}

impl ConfigType {
    fn extension(&self) -> &'static str {
        match self {
            ConfigType::Network => "network",
            ConfigType::Link => "link",
            ConfigType::Netdev => "netdev",
        }
    }
}

impl NetdevKind {
    fn as_str(&self) -> &'static str {
        match self {
            NetdevKind::Bridge => "bridge",
            NetdevKind::Bond => "bond",
            NetdevKind::Vlan => "vlan",
            NetdevKind::Macvlan => "macvlan",
            NetdevKind::Ipvlan => "ipvlan",
            NetdevKind::Vxlan => "vxlan",
            NetdevKind::Tun => "tun",
            NetdevKind::Tap => "tap",
            NetdevKind::Wireguard => "wireguard",
            NetdevKind::Dummy => "dummy",
        }
    }
}

#[derive(Debug)]
pub struct Networkd;

impl Module for Networkd {
    fn get_name(&self) -> &str {
        "networkd"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((networkd(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn get_config_path(params: &Params) -> PathBuf {
    let dir = params.directory.as_deref().unwrap_or(DEFAULT_NETWORKD_DIR);
    PathBuf::from(format!(
        "{dir}/{}.{}",
        params.name,
        params.type_.extension()
    ))
}

fn generate_network_config(params: &Params) -> String {
    let mut sections: Vec<String> = Vec::new();

    let mut match_section = String::from("[Match]\n");
    if let Some(ref interfaces) = params.interfaces {
        for iface in interfaces {
            match_section.push_str(&format!("Name={iface}\n"));
        }
    }
    sections.push(match_section);

    let mut network_section = String::from("\n[Network]\n");

    if let Some(dhcp) = params.dhcp
        && dhcp
    {
        network_section.push_str("DHCP=yes\n");
    }

    if let Some(ref addresses) = params.addresses {
        for addr in addresses {
            network_section.push_str(&format!("Address={addr}\n"));
        }
    }

    if let Some(ref gateway) = params.gateway {
        network_section.push_str(&format!("Gateway={gateway}\n"));
    }

    if let Some(ref dns) = params.dns {
        for server in dns {
            network_section.push_str(&format!("DNS={server}\n"));
        }
    }

    sections.push(network_section);

    sections.join("")
}

fn generate_link_config(params: &Params) -> String {
    let mut sections: Vec<String> = Vec::new();

    let mut match_section = String::from("[Match]\n");
    if let Some(ref interfaces) = params.interfaces {
        for iface in interfaces {
            match_section.push_str(&format!("OriginalName={iface}\n"));
        }
    }
    sections.push(match_section);

    let mut link_section = String::from("\n[Link]\n");
    if let Some(mtu) = params.mtu {
        link_section.push_str(&format!("MTUBytes={mtu}\n"));
    }
    sections.push(link_section);

    sections.join("")
}

fn generate_netdev_config(params: &Params) -> String {
    let mut sections: Vec<String> = Vec::new();

    let mut netdev_section = String::from("[NetDev]\n");
    netdev_section.push_str(&format!("Name={}\n", params.name));

    if let Some(ref kind) = params.netdev_kind {
        let kind_str = kind.as_str();
        netdev_section.push_str(&format!("Kind={kind_str}\n"));
    }
    sections.push(netdev_section);

    if let Some(ref kind) = params.netdev_kind {
        match kind {
            NetdevKind::Vlan => {
                if let Some(vlan_id) = params.vlan_id {
                    let mut vlan_section = String::from("\n[VLAN]\n");
                    vlan_section.push_str(&format!("Id={vlan_id}\n"));
                    sections.push(vlan_section);
                }
            }
            NetdevKind::Bridge => {
                sections.push(String::from("\n[Bridge]\n"));
            }
            NetdevKind::Bond => {
                sections.push(String::from("\n[Bond]\n"));
            }
            _ => {}
        }
    }

    sections.join("")
}

fn generate_config(params: &Params) -> String {
    match params.type_ {
        ConfigType::Network => generate_network_config(params),
        ConfigType::Link => generate_link_config(params),
        ConfigType::Netdev => generate_netdev_config(params),
    }
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();
    let backup_path = path.with_extension(format!("{ext}.bak.{timestamp}"));
    fs::copy(path, &backup_path)
        .map_err(|e| Error::new(ErrorKind::IOError, format!("Failed to create backup: {e}")))?;
    Ok(backup_path)
}

fn restart_networkd(check_mode: bool) -> Result<bool> {
    if check_mode {
        return Ok(true);
    }

    let output = std::process::Command::new("systemctl")
        .args(["restart", "systemd-networkd"])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to restart systemd-networkd: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "systemctl restart systemd-networkd failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn networkd(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let config_path = get_config_path(&params);
    let mut extra: HashMap<String, YamlValue> = HashMap::new();

    extra.insert(
        "config_file".to_string(),
        YamlValue::String(config_path.to_string_lossy().to_string()),
    );
    extra.insert(
        "type".to_string(),
        YamlValue::String(params.type_.extension().to_string()),
    );

    match params.state {
        State::Absent => {
            let changed = if config_path.exists() {
                if !check_mode {
                    if params.backup.unwrap_or(false) {
                        create_backup(&config_path)?;
                    }
                    fs::remove_file(&config_path)?;
                }
                true
            } else {
                false
            };

            if changed {
                diff(
                    format!("config file {} exists", config_path.display()),
                    format!("config file {} removed", config_path.display()),
                );
            }

            Ok(ModuleResult::new(
                changed,
                Some(YamlValue::Mapping(
                    extra
                        .into_iter()
                        .map(|(k, v)| (YamlValue::String(k), v))
                        .collect(),
                )),
                Some(format!(
                    "networkd configuration removed: {}",
                    config_path.display()
                )),
            ))
        }
        State::Present => {
            let new_config = if let Some(ref config) = params.config {
                config.clone()
            } else {
                generate_config(&params)
            };

            let existing_config = if config_path.exists() {
                Some(fs::read_to_string(&config_path).map_err(|e| {
                    Error::new(
                        ErrorKind::IOError,
                        format!("Failed to read existing config: {e}"),
                    )
                })?)
            } else {
                None
            };

            let changed = !matches!(existing_config, Some(ref existing) if *existing == new_config);

            if changed {
                let old_content = existing_config.unwrap_or_default();
                diff(&old_content, &new_config);

                if !check_mode {
                    if let Some(parent) = config_path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }

                    if config_path.exists() && params.backup.unwrap_or(false) {
                        create_backup(&config_path)?;
                    }

                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&config_path)
                        .map_err(|e| {
                            Error::new(
                                ErrorKind::IOError,
                                format!("Failed to write networkd config: {e}"),
                            )
                        })?;
                    file.write_all(new_config.as_bytes())?;

                    if params.restart {
                        restart_networkd(false)?;
                    }
                } else if params.restart {
                    restart_networkd(true)?;
                }
            }

            extra.insert("config".to_string(), YamlValue::String(new_config));

            Ok(ModuleResult::new(
                changed,
                Some(YamlValue::Mapping(
                    extra
                        .into_iter()
                        .map(|(k, v)| (YamlValue::String(k), v))
                        .collect(),
                )),
                Some(format!(
                    "networkd configuration written to {}",
                    config_path.display()
                )),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_network_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            type: network
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "10-eth0");
        assert_eq!(params.type_, ConfigType::Network);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_network_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            type: network
            state: present
            interfaces:
              - eth0
            addresses:
              - 192.168.1.100/24
            gateway: 192.168.1.1
            dns:
              - 8.8.8.8
              - 8.8.4.4
            dhcp: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "10-eth0");
        assert_eq!(params.type_, ConfigType::Network);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.interfaces, Some(vec!["eth0".to_string()]));
        assert_eq!(params.addresses, Some(vec!["192.168.1.100/24".to_string()]));
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(
            params.dns,
            Some(vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()])
        );
        assert_eq!(params.dhcp, Some(false));
    }

    #[test]
    fn test_parse_params_netdev_bridge() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: br0
            type: netdev
            netdev_kind: bridge
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "br0");
        assert_eq!(params.type_, ConfigType::Netdev);
        assert_eq!(params.netdev_kind, Some(NetdevKind::Bridge));
    }

    #[test]
    fn test_parse_params_netdev_vlan() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vlan10
            type: netdev
            netdev_kind: vlan
            vlan_id: 10
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "vlan10");
        assert_eq!(params.netdev_kind, Some(NetdevKind::Vlan));
        assert_eq!(params.vlan_id, Some(10));
    }

    #[test]
    fn test_parse_params_link() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            type: link
            interfaces:
              - eth0
            mtu: 9000
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.type_, ConfigType::Link);
        assert_eq!(params.mtu, Some(9000));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            type: network
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_deny_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            type: network
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_with_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: custom
            type: network
            config: |
              [Match]
              Name=eth0

              [Network]
              DHCP=yes
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.config.is_some());
        assert!(params.config.unwrap().contains("DHCP=yes"));
    }

    #[test]
    fn test_generate_network_config_static() {
        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: Some(vec!["8.8.8.8".to_string()]),
            dhcp: Some(false),
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[Match]\n"));
        assert!(config.contains("Name=eth0\n"));
        assert!(config.contains("[Network]\n"));
        assert!(config.contains("Address=192.168.1.100/24\n"));
        assert!(config.contains("Gateway=192.168.1.1\n"));
        assert!(config.contains("DNS=8.8.8.8\n"));
    }

    #[test]
    fn test_generate_network_config_dhcp() {
        let params = Params {
            name: "20-dhcp".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: Some(true),
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("DHCP=yes\n"));
    }

    #[test]
    fn test_generate_link_config() {
        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Link,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: Some(9000),
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[Match]\n"));
        assert!(config.contains("OriginalName=eth0\n"));
        assert!(config.contains("[Link]\n"));
        assert!(config.contains("MTUBytes=9000\n"));
    }

    #[test]
    fn test_generate_netdev_bridge() {
        let params = Params {
            name: "br0".to_string(),
            type_: ConfigType::Netdev,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: Some(NetdevKind::Bridge),
            mtu: None,
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[NetDev]\n"));
        assert!(config.contains("Name=br0\n"));
        assert!(config.contains("Kind=bridge\n"));
        assert!(config.contains("[Bridge]\n"));
    }

    #[test]
    fn test_generate_netdev_vlan() {
        let params = Params {
            name: "vlan10".to_string(),
            type_: ConfigType::Netdev,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: Some(10),
            netdev_kind: Some(NetdevKind::Vlan),
            mtu: None,
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[NetDev]\n"));
        assert!(config.contains("Name=vlan10\n"));
        assert!(config.contains("Kind=vlan\n"));
        assert!(config.contains("[VLAN]\n"));
        assert!(config.contains("Id=10\n"));
    }

    #[test]
    fn test_config_type_extension() {
        assert_eq!(ConfigType::Network.extension(), "network");
        assert_eq!(ConfigType::Link.extension(), "link");
        assert_eq!(ConfigType::Netdev.extension(), "netdev");
    }

    #[test]
    fn test_netdev_kind_as_str() {
        assert_eq!(NetdevKind::Bridge.as_str(), "bridge");
        assert_eq!(NetdevKind::Bond.as_str(), "bond");
        assert_eq!(NetdevKind::Vlan.as_str(), "vlan");
        assert_eq!(NetdevKind::Macvlan.as_str(), "macvlan");
        assert_eq!(NetdevKind::Ipvlan.as_str(), "ipvlan");
        assert_eq!(NetdevKind::Vxlan.as_str(), "vxlan");
        assert_eq!(NetdevKind::Tun.as_str(), "tun");
        assert_eq!(NetdevKind::Tap.as_str(), "tap");
        assert_eq!(NetdevKind::Wireguard.as_str(), "wireguard");
        assert_eq!(NetdevKind::Dummy.as_str(), "dummy");
    }

    #[test]
    fn test_get_config_path() {
        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: None,
            restart: true,
            config: None,
        };

        assert_eq!(
            get_config_path(&params),
            PathBuf::from("/etc/systemd/network/10-eth0.network")
        );
    }

    #[test]
    fn test_get_config_path_custom_dir() {
        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Link,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some("/tmp/networkd".to_string()),
            restart: true,
            config: None,
        };

        assert_eq!(
            get_config_path(&params),
            PathBuf::from("/tmp/networkd/10-eth0.link")
        );
    }

    #[test]
    fn test_networkd_create_network_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: Some(vec!["8.8.8.8".to_string()]),
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path.clone()),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let config_path = format!("{dir_path}/10-eth0.network");
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Name=eth0"));
        assert!(content.contains("Address=192.168.1.100/24"));
        assert!(content.contains("Gateway=192.168.1.1"));
        assert!(content.contains("DNS=8.8.8.8"));
    }

    #[test]
    fn test_networkd_no_change() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/10-eth0.network");

        let initial_content = "[Match]\nName=eth0\n\n[Network]\nAddress=192.168.1.100/24\n";
        fs::write(&config_path, initial_content).unwrap();

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_networkd_remove_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/10-eth0.network");

        fs::write(&config_path, "[Match]\nName=eth0\n").unwrap();
        assert!(Path::new(&config_path).exists());

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Absent,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);
        assert!(!Path::new(&config_path).exists());
    }

    #[test]
    fn test_networkd_check_mode() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/10-eth0.network");

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, true).unwrap();
        assert!(result.changed);
        assert!(!Path::new(&config_path).exists());
    }

    #[test]
    fn test_networkd_with_backup() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/10-eth0.network");

        fs::write(&config_path, "[Match]\nName=eth0\n").unwrap();

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["10.0.0.1/24".to_string()]),
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: Some(true),
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("10-eth0.network.bak")
            })
            .collect();
        assert_eq!(backup_files.len(), 1);
    }

    #[test]
    fn test_networkd_with_raw_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/custom.network");

        let raw_config = "[Match]\nName=eth0\n\n[Network]\nDHCP=yes\n";

        let params = Params {
            name: "custom".to_string(),
            type_: ConfigType::Network,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: Some(raw_config.to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, raw_config);
    }

    #[test]
    fn test_networkd_create_netdev_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/br0.netdev");

        let params = Params {
            name: "br0".to_string(),
            type_: ConfigType::Netdev,
            state: State::Present,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: Some(NetdevKind::Bridge),
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[NetDev]"));
        assert!(content.contains("Name=br0"));
        assert!(content.contains("Kind=bridge"));
        assert!(content.contains("[Bridge]"));
    }

    #[test]
    fn test_networkd_create_link_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let config_path = format!("{dir_path}/10-eth0.link");

        let params = Params {
            name: "10-eth0".to_string(),
            type_: ConfigType::Link,
            state: State::Present,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: Some(9000),
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("OriginalName=eth0"));
        assert!(content.contains("MTUBytes=9000"));
    }

    #[test]
    fn test_networkd_remove_absent_config() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();

        let params = Params {
            name: "nonexistent".to_string(),
            type_: ConfigType::Network,
            state: State::Absent,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            mtu: None,
            backup: None,
            directory: Some(dir_path),
            restart: false,
            config: None,
        };

        let result = networkd(params, false).unwrap();
        assert!(!result.changed);
    }
}
