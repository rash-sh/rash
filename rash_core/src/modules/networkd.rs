/// ANCHOR: module
/// # networkd
///
/// Manage systemd-networkd configuration files (.network, .link, .netdev).
///
/// systemd-networkd is a modern network configuration daemon useful for
/// IoT devices, container networking, and server network management.
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
///     config_type: network
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
///     config_type: network
///     interfaces:
///       - eth0
///     dhcp: true
///
/// - name: Create bridge netdev
///   networkd:
///     name: 10-br0
///     config_type: netdev
///     netdev_kind: bridge
///     bridge:
///       stp: false
///
/// - name: Attach interface to bridge
///   networkd:
///     name: 10-br0-slave
///     config_type: network
///     interfaces:
///       - eth0
///     bridge: br0
///
/// - name: Configure VLAN
///   networkd:
///     name: 10-vlan100
///     config_type: netdev
///     netdev_kind: vlan
///     vlan_id: 100
///
/// - name: Configure bond
///   networkd:
///     name: 10-bond0
///     config_type: netdev
///     netdev_kind: bond
///     bond:
///       mode: 802.3ad
///
/// - name: Configure link MAC address
///   networkd:
///     name: 10-eth0-link
///     config_type: link
///     interfaces:
///       - eth0
///     mac_address: "00:11:22:33:44:55"
///
/// - name: Remove network configuration
///   networkd:
///     name: 10-eth0
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
use std::process::Command;
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

impl ConfigType {
    fn extension(&self) -> &'static str {
        match self {
            ConfigType::Network => "network",
            ConfigType::Link => "link",
            ConfigType::Netdev => "netdev",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the configuration file (without extension).
    /// Will be used as filename: `<name>.<config_type>`.
    pub name: String,
    /// Whether the configuration should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Type of systemd-networkd configuration file.
    /// **[default: `"network"`]**
    #[serde(default = "default_config_type")]
    pub config_type: ConfigType,
    /// Interface names to match (used in [Match] section).
    pub interfaces: Option<Vec<String>>,
    /// IP addresses with CIDR notation.
    pub addresses: Option<Vec<String>>,
    /// Default gateway.
    pub gateway: Option<String>,
    /// DNS server addresses.
    pub dns: Option<Vec<String>>,
    /// Enable DHCP (IPv4).
    pub dhcp: Option<bool>,
    /// VLAN ID (for netdev type with vlan kind).
    pub vlan_id: Option<u32>,
    /// NetDev kind (bridge, bond, vlan, vxlan, etc.).
    pub netdev_kind: Option<String>,
    /// Bridge name to attach interface to (for network type).
    pub bridge: Option<String>,
    /// Bridge parameters (for netdev type with bridge kind).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub bridge_params: Option<YamlValue>,
    /// Bond parameters (for netdev type with bond kind).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub bond_params: Option<YamlValue>,
    /// MAC address (for link type).
    pub mac_address: Option<String>,
    /// MTU for the interface.
    pub mtu: Option<u32>,
    /// Raw INI-style configuration sections (overrides individual parameters).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub config: Option<YamlValue>,
    /// Restart systemd-networkd after changes.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub restart: bool,
    /// Create backup of existing config file.
    pub backup: Option<bool>,
    /// Path to the systemd-networkd configuration directory.
    /// **[default: `"/etc/systemd/network"`]**
    pub directory: Option<String>,
}

fn default_config_type() -> ConfigType {
    ConfigType::Network
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
    let ext = params.config_type.extension();
    PathBuf::from(dir).join(format!("{}.{}", params.name, ext))
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = path.with_extension(format!(
        "{}.bak.{}",
        path.extension().unwrap_or_default().to_string_lossy(),
        timestamp
    ));
    fs::copy(path, &backup_path)
        .map_err(|e| Error::new(ErrorKind::IOError, format!("Failed to create backup: {e}")))?;
    Ok(backup_path)
}

fn build_ini_section(header: &str, entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut section = format!("[{header}]\n");
    for (key, value) in entries {
        section.push_str(&format!("{key}={value}\n"));
    }
    section
}

fn build_network_config(params: &Params) -> String {
    let mut sections = String::new();

    let mut match_entries: Vec<(String, String)> = Vec::new();
    if let Some(ref interfaces) = params.interfaces {
        for iface in interfaces {
            match_entries.push(("Name".to_string(), iface.clone()));
        }
    }

    sections.push_str(&build_ini_section("Match", &match_entries));

    let mut network_entries: Vec<(String, String)> = Vec::new();

    if let Some(dhcp) = params.dhcp {
        let val = if dhcp { "yes" } else { "no" };
        network_entries.push(("DHCP".to_string(), val.to_string()));
    }

    if let Some(ref addresses) = params.addresses {
        for addr in addresses {
            network_entries.push(("Address".to_string(), addr.clone()));
        }
    }

    if let Some(ref gateway) = params.gateway {
        network_entries.push(("Gateway".to_string(), gateway.clone()));
    }

    if let Some(ref dns) = params.dns {
        for ns in dns {
            network_entries.push(("DNS".to_string(), ns.clone()));
        }
    }

    if let Some(ref bridge) = params.bridge {
        network_entries.push(("Bridge".to_string(), bridge.clone()));
    }

    if let Some(mtu) = params.mtu {
        network_entries.push(("MTU".to_string(), mtu.to_string()));
    }

    sections.push_str(&build_ini_section("Network", &network_entries));

    sections.trim_end().to_string()
}

fn build_link_config(params: &Params) -> String {
    let mut sections = String::new();

    let mut match_entries: Vec<(String, String)> = Vec::new();
    if let Some(ref interfaces) = params.interfaces {
        for iface in interfaces {
            match_entries.push(("OriginalName".to_string(), iface.clone()));
        }
    }
    sections.push_str(&build_ini_section("Match", &match_entries));

    let mut link_entries: Vec<(String, String)> = Vec::new();
    if let Some(ref mac) = params.mac_address {
        link_entries.push(("MACAddress".to_string(), mac.clone()));
    }
    if let Some(mtu) = params.mtu {
        link_entries.push(("MTU".to_string(), mtu.to_string()));
    }
    sections.push_str(&build_ini_section("Link", &link_entries));

    sections.trim_end().to_string()
}

fn build_netdev_config(params: &Params) -> String {
    let mut sections = String::new();

    let mut netdev_entries: Vec<(String, String)> = Vec::new();
    netdev_entries.push(("Name".to_string(), params.name.clone()));
    if let Some(ref kind) = params.netdev_kind {
        netdev_entries.push(("Kind".to_string(), kind.clone()));
    }
    if let Some(mtu) = params.mtu {
        netdev_entries.push(("MTU".to_string(), mtu.to_string()));
    }
    sections.push_str(&build_ini_section("NetDev", &netdev_entries));

    if let Some(vlan_id) = params.vlan_id {
        let vlan_entries: Vec<(String, String)> = vec![("Id".to_string(), vlan_id.to_string())];
        sections.push_str(&build_ini_section("VLAN", &vlan_entries));
    }

    if let Some(ref bond_params) = params.bond_params
        && let Some(entries) = yaml_value_to_ini_entries(bond_params)
    {
        sections.push_str(&build_ini_section("Bond", &entries));
    }

    if let Some(ref bridge_params) = params.bridge_params
        && let Some(entries) = yaml_value_to_ini_entries(bridge_params)
    {
        sections.push_str(&build_ini_section("Bridge", &entries));
    }

    sections.trim_end().to_string()
}

fn yaml_value_to_ini_entries(value: &YamlValue) -> Option<Vec<(String, String)>> {
    match value {
        YamlValue::Mapping(map) => {
            let mut entries = Vec::new();
            for (k, v) in map {
                let key = match k {
                    YamlValue::String(s) => s.clone(),
                    YamlValue::Number(n) => n.to_string(),
                    other => format!("{other:?}"),
                };
                let val = match v {
                    YamlValue::String(s) => s.clone(),
                    YamlValue::Number(n) => n.to_string(),
                    YamlValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
                    other => format!("{other:?}"),
                };
                entries.push((key, val));
            }
            if entries.is_empty() {
                None
            } else {
                Some(entries)
            }
        }
        _ => None,
    }
}

fn build_config(params: &Params) -> String {
    match params.config_type {
        ConfigType::Network => build_network_config(params),
        ConfigType::Link => build_link_config(params),
        ConfigType::Netdev => build_netdev_config(params),
    }
}

fn restart_networkd() -> Result<bool> {
    let output = Command::new("systemctl")
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

fn configs_are_equal(config1: &str, config2: &str) -> bool {
    let normalize = |c: &str| {
        c.lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    normalize(config1) == normalize(config2)
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
        "config_type".to_string(),
        YamlValue::String(params.config_type.extension().to_string()),
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

            extra.insert("restarted".to_string(), YamlValue::Bool(false));

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
                match config {
                    YamlValue::String(s) => s.clone(),
                    _ => serde_norway::to_string(config).map_err(|e| {
                        Error::new(
                            ErrorKind::InvalidData,
                            format!("Failed to serialize config: {e}"),
                        )
                    })?,
                }
            } else {
                build_config(&params)
            };

            let existing_config = if config_path.exists() {
                Some(fs::read_to_string(&config_path)?)
            } else {
                None
            };

            let (final_config, changed) = if let Some(ref existing) = existing_config {
                if configs_are_equal(existing, &new_config) {
                    (existing.clone(), false)
                } else {
                    (new_config.clone(), true)
                }
            } else {
                (new_config.clone(), true)
            };

            let mut restarted = false;

            if changed && !check_mode {
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
                file.write_all(final_config.as_bytes())?;
                file.write_all(b"\n")?;

                if params.restart {
                    restarted = restart_networkd()?;
                }

                let old_content = existing_config.unwrap_or_default();
                diff(&old_content, &final_config);
            } else if changed && check_mode && params.restart {
                restarted = true;
            }

            extra.insert("restarted".to_string(), YamlValue::Bool(restarted));
            extra.insert(
                "config".to_string(),
                YamlValue::String(final_config.clone()),
            );

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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "10-eth0");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.config_type, ConfigType::Network);
        assert!(params.restart);
    }

    #[test]
    fn test_parse_params_full_network() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            config_type: network
            interfaces:
              - eth0
            addresses:
              - 192.168.1.100/24
            gateway: 192.168.1.1
            dns:
              - 8.8.8.8
              - 8.8.4.4
            restart: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "10-eth0");
        assert_eq!(params.config_type, ConfigType::Network);
        assert_eq!(params.interfaces, Some(vec!["eth0".to_string()]));
        assert_eq!(params.addresses, Some(vec!["192.168.1.100/24".to_string()]));
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(
            params.dns,
            Some(vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()])
        );
    }

    #[test]
    fn test_parse_params_netdev() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-br0
            config_type: netdev
            netdev_kind: bridge
            bridge_params:
              stp: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.config_type, ConfigType::Netdev);
        assert_eq!(params.netdev_kind, Some("bridge".to_string()));
        assert!(params.bridge_params.is_some());
    }

    #[test]
    fn test_parse_params_link() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0-link
            config_type: link
            interfaces:
              - eth0
            mac_address: "00:11:22:33:44:55"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.config_type, ConfigType::Link);
        assert_eq!(params.mac_address, Some("00:11:22:33:44:55".to_string()));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: 10-eth0
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_network_config_static() {
        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: Some(vec!["8.8.8.8".to_string()]),
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_network_config(&params);
        assert!(config.contains("[Match]"));
        assert!(config.contains("Name=eth0"));
        assert!(config.contains("[Network]"));
        assert!(config.contains("Address=192.168.1.100/24"));
        assert!(config.contains("Gateway=192.168.1.1"));
        assert!(config.contains("DNS=8.8.8.8"));
    }

    #[test]
    fn test_build_network_config_dhcp() {
        let params = Params {
            name: "20-dhcp".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: Some(true),
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_network_config(&params);
        assert!(config.contains("DHCP=yes"));
    }

    #[test]
    fn test_build_link_config() {
        let params = Params {
            name: "10-eth0-link".to_string(),
            state: State::Present,
            config_type: ConfigType::Link,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: Some("00:11:22:33:44:55".to_string()),
            mtu: Some(9000),
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_link_config(&params);

        let match_section = &config[..config.find("[Link]").unwrap_or(0)];
        let link_section = &config[config.find("[Link]").unwrap_or(0)..];

        assert!(match_section.contains("[Match]"));
        assert!(match_section.contains("OriginalName=eth0"));
        assert!(!match_section.contains("MACAddress="));
        assert!(link_section.contains("MACAddress=00:11:22:33:44:55"));
        assert!(link_section.contains("MTU=9000"));
    }

    #[test]
    fn test_build_netdev_config_bridge() {
        let params = Params {
            name: "10-br0".to_string(),
            state: State::Present,
            config_type: ConfigType::Netdev,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: Some("bridge".to_string()),
            bridge: None,
            bridge_params: Some(serde_norway::from_str("stp: false").unwrap()),
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_netdev_config(&params);
        assert!(config.contains("[NetDev]"));
        assert!(config.contains("Name=10-br0"));
        assert!(config.contains("Kind=bridge"));
        assert!(config.contains("[Bridge]"));
        assert!(config.contains("stp=false"));
    }

    #[test]
    fn test_build_netdev_config_vlan() {
        let params = Params {
            name: "10-vlan100".to_string(),
            state: State::Present,
            config_type: ConfigType::Netdev,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: Some(100),
            netdev_kind: Some("vlan".to_string()),
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_netdev_config(&params);
        assert!(config.contains("[NetDev]"));
        assert!(config.contains("Name=10-vlan100"));
        assert!(config.contains("Kind=vlan"));
        assert!(config.contains("[VLAN]"));
        assert!(config.contains("Id=100"));
    }

    #[test]
    fn test_networkd_create_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("10-eth0.network");

        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: Some(vec!["8.8.8.8".to_string()]),
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);
        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[Match]"));
        assert!(content.contains("Name=eth0"));
        assert!(content.contains("[Network]"));
        assert!(content.contains("Address=192.168.1.100/24"));
        assert!(content.contains("Gateway=192.168.1.1"));
    }

    #[test]
    fn test_networkd_no_change() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("10-eth0.network");

        let initial_content =
            "[Match]\nName=eth0\n\n[Network]\nAddress=192.168.1.100/24\nGateway=192.168.1.1\n";
        fs::write(&config_path, initial_content).unwrap();

        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_networkd_remove_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("10-eth0.network");

        fs::write(&config_path, "[Match]\nName=eth0\n").unwrap();
        assert!(config_path.exists());

        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Absent,
            config_type: ConfigType::Network,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_networkd_remove_nonexistent() {
        let dir = tempdir().unwrap();

        let params = Params {
            name: "10-missing".to_string(),
            state: State::Absent,
            config_type: ConfigType::Network,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_networkd_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("10-eth0.network");

        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["192.168.1.100/24".to_string()]),
            gateway: Some("192.168.1.1".to_string()),
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, true).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_networkd_with_backup() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("10-eth0.network");

        fs::write(&config_path, "[Match]\nName=eth0\n").unwrap();

        let params = Params {
            name: "10-eth0".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: Some(vec!["10.0.0.1/24".to_string()]),
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: Some(true),
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("10-eth0.network.")
            })
            .collect();
        assert_eq!(backup_files.len(), 1);
    }

    #[test]
    fn test_networkd_with_raw_config() {
        let dir = tempdir().unwrap();

        let params = Params {
            name: "10-custom".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: Some(YamlValue::String(
                "[Match]\nName=eth0\n\n[Network]\nDHCP=yes\n".to_string(),
            )),
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let config_path = dir.path().join("10-custom.network");
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("DHCP=yes"));
    }

    #[test]
    fn test_config_type_extension() {
        assert_eq!(ConfigType::Network.extension(), "network");
        assert_eq!(ConfigType::Link.extension(), "link");
        assert_eq!(ConfigType::Netdev.extension(), "netdev");
    }

    #[test]
    fn test_configs_are_equal() {
        let config1 = "[Match]\nName=eth0\n\n[Network]\nAddress=192.168.1.100/24\n";
        let config2 = "[Match]\nName=eth0\n\n[Network]\nAddress=192.168.1.100/24\n";
        assert!(configs_are_equal(config1, config2));

        let config3 = "[Match]\nName=eth1\n\n[Network]\nAddress=192.168.1.100/24\n";
        assert!(!configs_are_equal(config1, config3));
    }

    #[test]
    fn test_networkd_bridge_network() {
        let dir = tempdir().unwrap();

        let params = Params {
            name: "10-br0-slave".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: Some("br0".to_string()),
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let config_path = dir.path().join("10-br0-slave.network");
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Bridge=br0"));
    }

    #[test]
    fn test_networkd_mtu() {
        let dir = tempdir().unwrap();

        let params = Params {
            name: "10-jumbo".to_string(),
            state: State::Present,
            config_type: ConfigType::Network,
            interfaces: Some(vec!["eth0".to_string()]),
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: None,
            bridge: None,
            bridge_params: None,
            bond_params: None,
            mac_address: None,
            mtu: Some(9000),
            config: None,
            restart: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
        };

        let result = networkd(params, false).unwrap();
        assert!(result.changed);

        let config_path = dir.path().join("10-jumbo.network");
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("MTU=9000"));
    }

    #[test]
    fn test_build_netdev_config_bond() {
        let params = Params {
            name: "10-bond0".to_string(),
            state: State::Present,
            config_type: ConfigType::Netdev,
            interfaces: None,
            addresses: None,
            gateway: None,
            dns: None,
            dhcp: None,
            vlan_id: None,
            netdev_kind: Some("bond".to_string()),
            bridge: None,
            bridge_params: None,
            bond_params: Some(serde_norway::from_str("mode: 802.3ad").unwrap()),
            mac_address: None,
            mtu: None,
            config: None,
            restart: false,
            backup: None,
            directory: None,
        };

        let config = build_netdev_config(&params);
        assert!(config.contains("[NetDev]"));
        assert!(config.contains("Name=10-bond0"));
        assert!(config.contains("Kind=bond"));
        assert!(config.contains("[Bond]"));
        assert!(config.contains("mode=802.3ad"));
    }
}
