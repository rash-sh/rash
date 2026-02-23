/// ANCHOR: module
/// # nmcli
///
/// Manage NetworkManager connections using nmcli.
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
/// - name: Create ethernet connection with static IP
///   nmcli:
///     conn_name: eth0-static
///     ifname: eth0
///     type: ethernet
///     ip4: 192.168.1.100/24
///     gw4: 192.168.1.1
///     dns4:
///       - 8.8.8.8
///       - 8.8.4.4
///     state: present
///
/// - name: Bring up a connection
///   nmcli:
///     conn_name: eth0-static
///     state: up
///
/// - name: Bring down a connection
///   nmcli:
///     conn_name: eth0-static
///     state: down
///
/// - name: Delete a connection
///   nmcli:
///     conn_name: eth0-static
///     state: absent
///
/// - name: Create WiFi connection
///   nmcli:
///     conn_name: mywifi
///     type: wifi
///     ifname: wlan0
///     ssid: MyNetwork
///     wifi_sec:
///       key-mgmt: wpa-psk
///       psk: mypassword
///     state: present
///
/// - name: Create a bridge connection
///   nmcli:
///     conn_name: br0
///     type: bridge
///     ifname: br0
///     ip4: 192.168.1.10/24
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

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

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
    Up,
    Down,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "kebab-case")]
enum ConnType {
    Ethernet,
    Wifi,
    Bridge,
    Bond,
    Team,
    Vlan,
    Vxlan,
    Dummy,
    Generic,
    Tun,
    Veth,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WifiSec {
    #[serde(rename = "key-mgmt")]
    key_mgmt: Option<String>,
    psk: Option<String>,
    #[serde(rename = "wep-key0")]
    wep_key0: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(rename = "conn_name")]
    conn_name: String,
    #[serde(default)]
    state: State,
    #[serde(rename = "type")]
    conn_type: Option<ConnType>,
    ifname: Option<String>,
    ip4: Option<String>,
    gw4: Option<String>,
    dns4: Option<Vec<String>>,
    #[serde(default = "default_true")]
    autoconnect: bool,
    ssid: Option<String>,
    wifi_sec: Option<WifiSec>,
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

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct NmcliClient {
    check_mode: bool,
}

impl NmcliClient {
    fn new(check_mode: bool) -> Self {
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

    fn connection_exists(&self, conn_name: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .args(["-t", "-f", "NAME", "connection", "show"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line == conn_name))
    }

    fn is_connection_active(&self, conn_name: &str) -> Result<bool> {
        let output = Command::new("nmcli")
            .args(["-t", "-f", "NAME,DEVICE", "connection", "show", "--active"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            !parts.is_empty() && parts[0] == conn_name
        }))
    }

    fn connection_up(&self, conn_name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_connection_active(conn_name)? {
            return Ok(false);
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "up", conn_name]);
        self.exec_cmd(&mut cmd)?;
        Ok(true)
    }

    fn connection_down(&self, conn_name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_connection_active(conn_name)? {
            return Ok(false);
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "down", conn_name]);
        self.exec_cmd(&mut cmd)?;
        Ok(true)
    }

    fn connection_delete(&self, conn_name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.connection_exists(conn_name)? {
            return Ok(false);
        }

        let mut cmd = Command::new("nmcli");
        cmd.args(["connection", "delete", conn_name]);
        self.exec_cmd(&mut cmd)?;
        Ok(true)
    }

    fn connection_create_or_modify(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            let exists = self.connection_exists(&params.conn_name)?;
            return Ok(!exists);
        }

        let exists = self.connection_exists(&params.conn_name)?;

        let mut cmd = Command::new("nmcli");
        if exists {
            cmd.args(["connection", "modify", &params.conn_name]);
        } else {
            cmd.args(["connection", "add"]);
            cmd.args(["type", &conn_type_to_string(&params.conn_type)]);
            cmd.args(["con-name", &params.conn_name]);
            if let Some(ifname) = &params.ifname {
                cmd.args(["ifname", ifname]);
            }
        }

        let mut changes = false;

        if !exists {
            changes = true;
        }

        if let Some(ifname) = &params.ifname
            && exists
        {
            cmd.args(["connection.interface-name", ifname]);
            changes = true;
        }

        if let Some(ip4) = &params.ip4 {
            if exists {
                cmd.args(["ipv4.addresses", ip4]);
                cmd.args(["ipv4.method", "manual"]);
                changes = true;
            } else {
                cmd.args(["ipv4.addresses", ip4]);
                cmd.args(["ipv4.method", "manual"]);
            }
        }

        if let Some(gw4) = &params.gw4 {
            if exists {
                cmd.args(["ipv4.gateway", gw4]);
                changes = true;
            } else {
                cmd.args(["ipv4.gateway", gw4]);
            }
        }

        if let Some(dns4) = &params.dns4 {
            let dns_str = dns4.join(",");
            if exists {
                cmd.args(["ipv4.dns", &dns_str]);
                changes = true;
            } else {
                cmd.args(["ipv4.dns", &dns_str]);
            }
        }

        let autoconnect_str = if params.autoconnect { "yes" } else { "no" };
        if exists {
            cmd.args(["connection.autoconnect", autoconnect_str]);
            changes = true;
        } else {
            cmd.args(["connection.autoconnect", autoconnect_str]);
        }

        if let Some(ssid) = &params.ssid {
            if exists {
                cmd.args(["802-11-wireless.ssid", ssid]);
                changes = true;
            } else {
                cmd.args(["802-11-wireless.ssid", ssid]);
            }
        }

        if let Some(wifi_sec) = &params.wifi_sec {
            if let Some(key_mgmt) = &wifi_sec.key_mgmt {
                if exists {
                    cmd.args(["wifi-sec.key-mgmt", key_mgmt]);
                    changes = true;
                } else {
                    cmd.args(["wifi-sec.key-mgmt", key_mgmt]);
                }
            }
            if let Some(psk) = &wifi_sec.psk {
                if exists {
                    cmd.args(["wifi-sec.psk", psk]);
                    changes = true;
                } else {
                    cmd.args(["wifi-sec.psk", psk]);
                }
            }
            if let Some(wep_key0) = &wifi_sec.wep_key0 {
                if exists {
                    cmd.args(["wifi-sec.wep-key0", wep_key0]);
                    changes = true;
                } else {
                    cmd.args(["wifi-sec.wep-key0", wep_key0]);
                }
            }
        }

        if exists && !changes {
            return Ok(false);
        }

        self.exec_cmd(&mut cmd)?;
        Ok(true)
    }
}

fn conn_type_to_string(conn_type: &Option<ConnType>) -> String {
    match conn_type {
        Some(ConnType::Ethernet) => "ethernet".to_string(),
        Some(ConnType::Wifi) => "wifi".to_string(),
        Some(ConnType::Bridge) => "bridge".to_string(),
        Some(ConnType::Bond) => "bond".to_string(),
        Some(ConnType::Team) => "team".to_string(),
        Some(ConnType::Vlan) => "vlan".to_string(),
        Some(ConnType::Vxlan) => "vxlan".to_string(),
        Some(ConnType::Dummy) => "dummy".to_string(),
        Some(ConnType::Generic) => "generic".to_string(),
        Some(ConnType::Tun) => "tun".to_string(),
        Some(ConnType::Veth) => "veth".to_string(),
        None => "ethernet".to_string(),
    }
}

fn validate_connection_name(name: &str) -> Result<()> {
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

    if name.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Connection name contains null character",
        ));
    }

    Ok(())
}

fn validate_ip4(ip4: &str) -> Result<()> {
    let parts: Vec<&str> = ip4.split('/').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid IPv4 address format '{}'. Expected format: IP/PREFIX (e.g., 192.168.1.100/24)",
                ip4
            ),
        ));
    }

    let ip = parts[0];
    let prefix_str = parts[1];

    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() != 4 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid IPv4 address '{}'", ip),
        ));
    }

    for octet in octets {
        match octet.parse::<u8>() {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid IPv4 octet '{}'", octet),
                ));
            }
        }
    }

    match prefix_str.parse::<u8>() {
        Ok(prefix) if prefix <= 32 => {}
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid IPv4 prefix '{}'. Must be 0-32", prefix_str),
            ));
        }
    }

    Ok(())
}

fn validate_gateway(gw: &str) -> Result<()> {
    let octets: Vec<&str> = gw.split('.').collect();
    if octets.len() != 4 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid gateway address '{}'", gw),
        ));
    }

    for octet in octets {
        match octet.parse::<u8>() {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid gateway octet '{}'", octet),
                ));
            }
        }
    }

    Ok(())
}

fn validate_dns(dns: &str) -> Result<()> {
    let octets: Vec<&str> = dns.split('.').collect();
    if octets.len() != 4 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid DNS server address '{}'", dns),
        ));
    }

    for octet in octets {
        match octet.parse::<u8>() {
            Ok(_) => {}
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid DNS octet '{}'", octet),
                ));
            }
        }
    }

    Ok(())
}

fn nmcli(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_connection_name(&params.conn_name)?;

    if let Some(ip4) = &params.ip4 {
        validate_ip4(ip4)?;
    }

    if let Some(gw4) = &params.gw4 {
        validate_gateway(gw4)?;
    }

    if let Some(dns4) = &params.dns4 {
        for dns in dns4 {
            validate_dns(dns)?;
        }
    }

    if params.conn_type == Some(ConnType::Wifi)
        && params.ssid.is_none()
        && params.state == State::Present
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "ssid is required for wifi connections",
        ));
    }

    let client = NmcliClient::new(check_mode);

    match params.state {
        State::Present => {
            let changed = client.connection_create_or_modify(&params)?;
            if changed {
                diff(
                    format!("connection {} absent", params.conn_name),
                    format!("connection {} present", params.conn_name),
                );
            }
            Ok(ModuleResult::new(
                changed,
                None,
                Some(format!("Connection {} ensured", params.conn_name)),
            ))
        }
        State::Absent => {
            let changed = client.connection_delete(&params.conn_name)?;
            if changed {
                diff(
                    format!("connection {} present", params.conn_name),
                    format!("connection {} absent", params.conn_name),
                );
            }
            Ok(ModuleResult::new(
                changed,
                None,
                Some(format!("Connection {} removed", params.conn_name)),
            ))
        }
        State::Up => {
            let changed = client.connection_up(&params.conn_name)?;
            if changed {
                diff(
                    format!("connection {} down", params.conn_name),
                    format!("connection {} up", params.conn_name),
                );
            }
            Ok(ModuleResult::new(
                changed,
                None,
                Some(format!("Connection {} activated", params.conn_name)),
            ))
        }
        State::Down => {
            let changed = client.connection_down(&params.conn_name)?;
            if changed {
                diff(
                    format!("connection {} up", params.conn_name),
                    format!("connection {} down", params.conn_name),
                );
            }
            Ok(ModuleResult::new(
                changed,
                None,
                Some(format!("Connection {} deactivated", params.conn_name)),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.conn_name, "eth0-static");
        assert_eq!(params.state, State::Present);
        assert!(params.autoconnect);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            ifname: eth0
            type: ethernet
            ip4: 192.168.1.100/24
            gw4: 192.168.1.1
            dns4:
              - 8.8.8.8
              - 8.8.4.4
            autoconnect: true
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.conn_name, "eth0-static");
        assert_eq!(params.ifname, Some("eth0".to_string()));
        assert_eq!(params.conn_type, Some(ConnType::Ethernet));
        assert_eq!(params.ip4, Some("192.168.1.100/24".to_string()));
        assert_eq!(params.gw4, Some("192.168.1.1".to_string()));
        assert_eq!(
            params.dns4,
            Some(vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()])
        );
        assert!(params.autoconnect);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_wifi() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: mywifi
            type: wifi
            ifname: wlan0
            ssid: MyNetwork
            wifi_sec:
              key-mgmt: wpa-psk
              psk: mypassword
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.conn_name, "mywifi");
        assert_eq!(params.conn_type, Some(ConnType::Wifi));
        assert_eq!(params.ifname, Some("wlan0".to_string()));
        assert_eq!(params.ssid, Some("MyNetwork".to_string()));
        assert!(params.wifi_sec.is_some());
        let wifi_sec = params.wifi_sec.unwrap();
        assert_eq!(wifi_sec.key_mgmt, Some("wpa-psk".to_string()));
        assert_eq!(wifi_sec.psk, Some("mypassword".to_string()));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_state_up_down() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            state: up
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            state: down
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Down);
    }

    #[test]
    fn test_validate_connection_name() {
        assert!(validate_connection_name("eth0").is_ok());
        assert!(validate_connection_name("my-connection").is_ok());
        assert!(validate_connection_name("my_connection").is_ok());

        assert!(validate_connection_name("").is_err());
        assert!(validate_connection_name(&"a".repeat(256)).is_err());
        assert!(validate_connection_name("conn\0name").is_err());
    }

    #[test]
    fn test_validate_ip4() {
        assert!(validate_ip4("192.168.1.100/24").is_ok());
        assert!(validate_ip4("10.0.0.1/8").is_ok());
        assert!(validate_ip4("172.16.0.1/16").is_ok());
        assert!(validate_ip4("0.0.0.0/0").is_ok());

        assert!(validate_ip4("192.168.1.100").is_err());
        assert!(validate_ip4("192.168.1.100/").is_err());
        assert!(validate_ip4("192.168.1.100/33").is_err());
        assert!(validate_ip4("192.168.1.300/24").is_err());
        assert!(validate_ip4("192.168.1/24").is_err());
    }

    #[test]
    fn test_validate_gateway() {
        assert!(validate_gateway("192.168.1.1").is_ok());
        assert!(validate_gateway("10.0.0.1").is_ok());
        assert!(validate_gateway("172.16.0.1").is_ok());

        assert!(validate_gateway("192.168.1.300").is_err());
        assert!(validate_gateway("192.168.1").is_err());
        assert!(validate_gateway("192.168.1.1.1").is_err());
    }

    #[test]
    fn test_validate_dns() {
        assert!(validate_dns("8.8.8.8").is_ok());
        assert!(validate_dns("1.1.1.1").is_ok());

        assert!(validate_dns("8.8.8.300").is_err());
        assert!(validate_dns("8.8.8").is_err());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            conn_name: eth0-static
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
