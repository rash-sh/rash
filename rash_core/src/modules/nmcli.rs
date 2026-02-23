/// ANCHOR: module
/// # nmcli
///
/// Manage NetworkManager connections via nmcli.
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
/// - name: Configure Ethernet connection
///   nmcli:
///     conn_name: eth0-conn
///     ifname: eth0
///     type: ethernet
///     ip4: 192.168.1.100/24
///     gw4: 192.168.1.1
///     state: present
///
/// - name: Configure connection with DNS
///   nmcli:
///     conn_name: eth0-conn
///     ifname: eth0
///     type: ethernet
///     ip4: 192.168.1.100/24
///     dns4:
///       - 8.8.8.8
///       - 8.8.4.4
///     state: present
///
/// - name: Bring connection up
///   nmcli:
///     conn_name: eth0-conn
///     state: up
///
/// - name: Bring connection down
///   nmcli:
///     conn_name: eth0-conn
///     state: down
///
/// - name: Remove connection
///   nmcli:
///     conn_name: eth0-conn
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{parse_params, Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ConnType {
    Ethernet,
    Wifi,
    Bridge,
    Bond,
    Vlan,
    Vxlan,
    Team,
    Generic,
    Tun,
    Veth,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
    Up,
    Down,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the connection profile.
    conn_name: String,
    /// Name of the network interface.
    ifname: Option<String>,
    /// Type of the connection.
    #[serde(rename = "type")]
    conn_type: Option<ConnType>,
    /// IPv4 address with CIDR prefix (e.g., 192.168.1.100/24).
    ip4: Option<String>,
    /// IPv4 gateway.
    gw4: Option<String>,
    /// List of DNS servers.
    dns4: Option<Vec<String>>,
    /// Whether the connection should be autoconnected.
    #[serde(default)]
    autoconnect: Option<bool>,
    /// State of the connection.
    #[serde(default = "default_state")]
    state: State,
}

fn default_state() -> State {
    State::Present
}

#[derive(Debug)]
pub struct Nmcli;

impl Module for Nmcli {
    fn get_name(&self) -> &str {
        "nmcli"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((nmcli(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct NmcliClient {
    check_mode: bool,
}

impl NmcliClient {
    pub fn new(check_mode: bool) -> Self {
        NmcliClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "nmcli command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn connection_exists(&self, conn_name: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .args(["-t", "-f", "NAME", "connection", "show"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == conn_name))
    }

    pub fn is_connection_active(&self, conn_name: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .args(["-t", "-f", "NAME,DEVICE", "connection", "show", "--active"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            parts.first().map(|s| *s == conn_name).unwrap_or(false)
        }))
    }

    pub fn create_connection(&self, params: &Params) -> Result<ModuleResult> {
        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would create connection '{}'", params.conn_name)),
            ));
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "add", "type"]);

        let conn_type = params
            .conn_type
            .as_ref()
            .map(|t| match t {
                ConnType::Ethernet => "ethernet",
                ConnType::Wifi => "wifi",
                ConnType::Bridge => "bridge",
                ConnType::Bond => "bond",
                ConnType::Vlan => "vlan",
                ConnType::Vxlan => "vxlan",
                ConnType::Team => "team",
                ConnType::Generic => "generic",
                ConnType::Tun => "tun",
                ConnType::Veth => "veth",
            })
            .unwrap_or("ethernet");

        cmd.arg(conn_type);
        cmd.args(["con-name", &params.conn_name]);

        if let Some(ref ifname) = params.ifname {
            cmd.args(["ifname", ifname]);
        }

        self.exec_cmd(&mut cmd)?;
        self.configure_connection(params)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Created connection '{}'", params.conn_name)),
        ))
    }

    pub fn modify_connection(&self, params: &Params) -> Result<ModuleResult> {
        let existing_settings = self.get_connection_settings(&params.conn_name)?;
        let changes = self.calculate_changes(&existing_settings, params)?;

        if changes.is_empty() {
            return Ok(ModuleResult::new(false, None, None));
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Would modify connection '{}': {}",
                    params.conn_name,
                    changes.join(", ")
                )),
            ));
        }

        self.configure_connection(params)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Modified connection '{}'", params.conn_name)),
        ))
    }

    fn get_connection_settings(&self, conn_name: &str) -> Result<ConnectionSettings> {
        let output = Command::new("nmcli")
            .args([
                "-t",
                "-f",
                "ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,connection.autoconnect",
            ])
            .args(["connection", "show", conn_name])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut settings = ConnectionSettings::default();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                match parts[0] {
                    "ipv4.addresses" => settings.ip4 = Some(parts[1].to_string()),
                    "ipv4.gateway" => settings.gw4 = Some(parts[1].to_string()),
                    "ipv4.dns" => {
                        if !parts[1].is_empty() {
                            settings.dns4 =
                                Some(parts[1].split(',').map(|s| s.trim().to_string()).collect());
                        }
                    }
                    "connection.autoconnect" => {
                        settings.autoconnect = Some(parts[1] == "yes");
                    }
                    _ => {}
                }
            }
        }

        Ok(settings)
    }

    fn calculate_changes(
        &self,
        existing: &ConnectionSettings,
        params: &Params,
    ) -> Result<Vec<String>> {
        let mut changes = Vec::new();

        if let Some(ref ip4) = params.ip4
            && existing.ip4.as_ref() != Some(ip4)
        {
            changes.push(format!("ip4: {:?} -> {}", existing.ip4, ip4));
        }

        if let Some(ref gw4) = params.gw4
            && existing.gw4.as_ref() != Some(gw4)
        {
            changes.push(format!("gw4: {:?} -> {}", existing.gw4, gw4));
        }

        if let Some(ref dns4) = params.dns4
            && existing.dns4.as_ref() != Some(dns4)
        {
            changes.push(format!("dns4: {:?} -> {:?}", existing.dns4, dns4));
        }

        Ok(changes)
    }

    fn configure_connection(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "modify", &params.conn_name]);

        if let Some(ref ip4) = params.ip4 {
            cmd.args(["ipv4.addresses", ip4]);
            cmd.args(["ipv4.method", "manual"]);
        }

        if let Some(ref gw4) = params.gw4 {
            cmd.args(["ipv4.gateway", gw4]);
        }

        if let Some(ref dns4) = params.dns4 {
            cmd.args(["ipv4.dns", &dns4.join(",")]);
        }

        if let Some(autoconnect) = params.autoconnect {
            cmd.args([
                "connection.autoconnect",
                if autoconnect { "yes" } else { "no" },
            ]);
        }

        self.exec_cmd(&mut cmd)?;
        Ok(())
    }

    pub fn delete_connection(&self, conn_name: &str) -> Result<ModuleResult> {
        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would delete connection '{}'", conn_name)),
            ));
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "delete", conn_name]);
        self.exec_cmd(&mut cmd)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Deleted connection '{}'", conn_name)),
        ))
    }

    pub fn up_connection(&self, conn_name: &str) -> Result<ModuleResult> {
        if self.is_connection_active(conn_name)? {
            return Ok(ModuleResult::new(false, None, None));
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would bring up connection '{}'", conn_name)),
            ));
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "up", conn_name]);
        self.exec_cmd(&mut cmd)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Brought up connection '{}'", conn_name)),
        ))
    }

    pub fn down_connection(&self, conn_name: &str) -> Result<ModuleResult> {
        if !self.is_connection_active(conn_name)? {
            return Ok(ModuleResult::new(false, None, None));
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would bring down connection '{}'", conn_name)),
            ));
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "down", conn_name]);
        self.exec_cmd(&mut cmd)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Brought down connection '{}'", conn_name)),
        ))
    }
}

#[derive(Debug, Default)]
struct ConnectionSettings {
    ip4: Option<String>,
    gw4: Option<String>,
    dns4: Option<Vec<String>>,
    autoconnect: Option<bool>,
}

fn validate_conn_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Connection name cannot be empty",
        ));
    }

    if name.len() > 255 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Connection name too long (max 255 characters)",
        ));
    }

    Ok(())
}

fn validate_ip4(ip: &str) -> Result<()> {
    let parts: Vec<&str> = ip.split('/').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid IPv4 address format '{}'. Expected format: 192.168.1.100/24",
                ip
            ),
        ));
    }

    let addr_parts: Vec<&str> = parts[0].split('.').collect();
    if addr_parts.len() != 4 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid IPv4 address '{}'", parts[0]),
        ));
    }

    for part in addr_parts {
        match part.parse::<u8>() {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid IPv4 octet '{}'", part),
                ));
            }
        }
    }

    match parts[1].parse::<u8>() {
        Ok(prefix) if prefix <= 32 => {}
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid CIDR prefix '{}'. Must be 0-32", parts[1]),
            ));
        }
    }

    Ok(())
}

fn nmcli(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_conn_name(&params.conn_name)?;

    if let Some(ref ip4) = params.ip4 {
        validate_ip4(ip4)?;
    }

    if let Some(ref gw4) = params.gw4 {
        let addr_parts: Vec<&str> = gw4.split('.').collect();
        if addr_parts.len() != 4 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid gateway IPv4 address '{}'", gw4),
            ));
        }
        for part in addr_parts {
            if part.parse::<u8>().is_err() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid gateway IPv4 octet '{}'", part),
                ));
            }
        }
    }

    let client = NmcliClient::new(check_mode);

    match params.state {
        State::Present => {
            if client.connection_exists(&params.conn_name)? {
                let result = client.modify_connection(&params)?;
                if result.get_changed() {
                    diff(
                        "connection: unchanged".to_string(),
                        "connection: modified".to_string(),
                    );
                }
                Ok(result)
            } else {
                diff(
                    "connection: absent".to_string(),
                    "connection: present".to_string(),
                );
                client.create_connection(&params)
            }
        }
        State::Absent => {
            if !client.connection_exists(&params.conn_name)? {
                Ok(ModuleResult::new(false, None, None))
            } else {
                diff(
                    "connection: present".to_string(),
                    "connection: absent".to_string(),
                );
                client.delete_connection(&params.conn_name)
            }
        }
        State::Up => {
            diff("connection: down".to_string(), "connection: up".to_string());
            client.up_connection(&params.conn_name)
        }
        State::Down => {
            diff("connection: up".to_string(), "connection: down".to_string());
            client.down_connection(&params.conn_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            ifname: eth0
            type: ethernet
            ip4: 192.168.1.100/24
            gw4: 192.168.1.1
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.conn_name, "eth0-conn");
        assert_eq!(params.ifname, Some("eth0".to_string()));
        assert_eq!(params.conn_type, Some(ConnType::Ethernet));
        assert_eq!(params.ip4, Some("192.168.1.100/24".to_string()));
        assert_eq!(params.gw4, Some("192.168.1.1".to_string()));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_with_dns() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            dns4:
              - 8.8.8.8
              - 8.8.4.4
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.dns4,
            Some(vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()])
        );
    }

    #[test]
    fn test_parse_params_state_up() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            state: up
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);
    }

    #[test]
    fn test_parse_params_state_down() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            state: down
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Down);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_connection_types() {
        let types = vec![
            ("ethernet", ConnType::Ethernet),
            ("wifi", ConnType::Wifi),
            ("bridge", ConnType::Bridge),
            ("bond", ConnType::Bond),
            ("vlan", ConnType::Vlan),
        ];

        for (type_str, expected_type) in types {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                conn_name: test-conn
                type: {}
                "#,
                type_str
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.conn_type, Some(expected_type));
        }
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_conn_name() {
        assert!(validate_conn_name("eth0-conn").is_ok());
        assert!(validate_conn_name("my connection").is_ok());

        assert!(validate_conn_name("").is_err());
        assert!(validate_conn_name(&"a".repeat(256)).is_err());
    }

    #[test]
    fn test_validate_ip4() {
        assert!(validate_ip4("192.168.1.100/24").is_ok());
        assert!(validate_ip4("10.0.0.1/8").is_ok());
        assert!(validate_ip4("172.16.0.1/16").is_ok());
        assert!(validate_ip4("0.0.0.0/0").is_ok());
        assert!(validate_ip4("255.255.255.255/32").is_ok());

        assert!(validate_ip4("192.168.1.100").is_err());
        assert!(validate_ip4("192.168.1.100/33").is_err());
        assert!(validate_ip4("256.1.1.1/24").is_err());
        assert!(validate_ip4("192.168.1/24").is_err());
        assert!(validate_ip4("192.168.1.1.1/24").is_err());
        assert!(validate_ip4("192.168.1.abc/24").is_err());
    }

    #[test]
    fn test_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-conn
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }
}
