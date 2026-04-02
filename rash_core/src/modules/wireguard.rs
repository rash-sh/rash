/// ANCHOR: module
/// # wireguard
///
/// Manage WireGuard VPN interfaces and peers.
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
/// - name: Create WireGuard interface
///   wireguard:
///     interface: wg0
///     state: present
///     private_key: "PRIVATE_KEY_HERE"
///     listen_port: 51820
///
/// - name: Configure WireGuard peer
///   wireguard:
///     interface: wg0
///     state: present
///     peers:
///       - public_key: "PEER_PUBLIC_KEY"
///         endpoint: "192.168.1.100:51820"
///         allowed_ips: ["10.0.0.2/32"]
///         persistent_keepalive: 25
///
/// - name: Start WireGuard interface
///   wireguard:
///     interface: wg0
///     state: up
///
/// - name: Stop WireGuard interface
///   wireguard:
///     interface: wg0
///     state: down
///
/// - name: Remove WireGuard interface
///   wireguard:
///     interface: wg0
///     state: absent
///
/// - name: Complete WireGuard setup
///   wireguard:
///     interface: wg0
///     state: up
///     private_key: "PRIVATE_KEY_HERE"
///     listen_port: 51820
///     peers:
///       - public_key: "PEER_PUBLIC_KEY"
///         endpoint: "peer.example.com:51820"
///         allowed_ips: ["10.0.0.0/24"]
///         persistent_keepalive: 25
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const WG_CONFIG_DIR: &str = "/etc/wireguard";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PeerParams {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub preshared_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub interface: String,
    pub state: State,
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub listen_port: Option<u16>,
    pub peers: Option<Vec<PeerParams>>,
    pub endpoint: Option<String>,
    pub allowed_ips: Option<Vec<String>>,
    pub persistent_keepalive: Option<u16>,
    pub dns: Option<String>,
    pub mtu: Option<u16>,
    #[serde(default = "default_false")]
    pub save_config: bool,
}

#[derive(Debug)]
pub struct Wireguard;

impl Module for Wireguard {
    fn get_name(&self) -> &str {
        "wireguard"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((wireguard(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct WireGuardClient {
    check_mode: bool,
}

impl WireGuardClient {
    fn new(check_mode: bool) -> Self {
        WireGuardClient { check_mode }
    }

    fn config_path(interface: &str) -> String {
        format!("{WG_CONFIG_DIR}/{interface}.conf")
    }

    fn interface_exists(interface: &str) -> Result<bool> {
        let output = Command::new("ip")
            .args(["link", "show", interface])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        Ok(output.status.success())
    }

    fn is_interface_up(interface: &str) -> Result<bool> {
        let output = Command::new("wg")
            .args(["show", interface])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        Ok(output.status.success() && !output.stdout.is_empty())
    }

    fn generate_config(params: &Params) -> String {
        let mut config = String::new();

        config.push_str("[Interface]\n");

        if let Some(key) = &params.private_key {
            config.push_str(&format!("PrivateKey = {key}\n"));
        }

        if let Some(port) = params.listen_port {
            config.push_str(&format!("ListenPort = {port}\n"));
        }

        if let Some(dns) = &params.dns {
            config.push_str(&format!("DNS = {dns}\n"));
        }

        if let Some(mtu) = params.mtu {
            config.push_str(&format!("MTU = {mtu}\n"));
        }

        if let Some(peers) = &params.peers {
            for peer in peers {
                config.push_str("\n[Peer]\n");
                config.push_str(&format!("PublicKey = {}\n", peer.public_key));

                if let Some(endpoint) = &peer.endpoint {
                    config.push_str(&format!("Endpoint = {endpoint}\n"));
                }

                if !peer.allowed_ips.is_empty() {
                    config.push_str(&format!("AllowedIPs = {}\n", peer.allowed_ips.join(", ")));
                }

                if let Some(keepalive) = peer.persistent_keepalive {
                    config.push_str(&format!("PersistentKeepalive = {keepalive}\n"));
                }

                if let Some(psk) = &peer.preshared_key {
                    config.push_str(&format!("PresharedKey = {psk}\n"));
                }
            }
        }

        config
    }

    fn write_config(interface: &str, config: &str) -> Result<()> {
        let path = Self::config_path(interface);

        fs::create_dir_all(WG_CONFIG_DIR).map_err(|e| {
            Error::new(
                ErrorKind::FileSystemError,
                format!("Failed to create WireGuard config directory: {e}"),
            )
        })?;

        fs::write(&path, config).map_err(|e| {
            Error::new(
                ErrorKind::FileSystemError,
                format!("Failed to write WireGuard config: {e}"),
            )
        })?;

        Ok(())
    }

    fn remove_config(interface: &str) -> Result<()> {
        let path = Self::config_path(interface);

        if Path::new(&path).exists() {
            fs::remove_file(&path).map_err(|e| {
                Error::new(
                    ErrorKind::FileSystemError,
                    format!("Failed to remove WireGuard config: {e}"),
                )
            })?;
        }

        Ok(())
    }

    fn bring_up(interface: &str) -> Result<()> {
        let output = Command::new("wg-quick")
            .args(["up", interface])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to bring up WireGuard interface: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(())
    }

    fn bring_down(interface: &str) -> Result<()> {
        let output = Command::new("wg-quick")
            .args(["down", interface])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to bring down WireGuard interface: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(())
    }

    fn get_interface_status(interface: &str) -> Result<serde_json::Value> {
        let output = Command::new("wg")
            .args(["show", interface, "dump"])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut status = serde_json::Map::new();
        status.insert(
            "interface".to_string(),
            serde_json::Value::String(interface.to_string()),
        );

        if !output.status.success() || stdout.is_empty() {
            status.insert("up".to_string(), serde_json::Value::Bool(false));
            return Ok(serde_json::Value::Object(status));
        }

        status.insert("up".to_string(), serde_json::Value::Bool(true));

        let lines: Vec<&str> = stdout.lines().collect();
        if !lines.is_empty() {
            let interface_line = lines[0];
            let parts: Vec<&str> = interface_line.split('\t').collect();

            if parts.len() >= 4 {
                status.insert(
                    "listen_port".to_string(),
                    serde_json::Value::Number(parts[1].parse().unwrap_or(0)),
                );
            }
        }

        Ok(serde_json::Value::Object(status))
    }
}

fn validate_interface_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Interface name cannot be empty",
        ));
    }

    if name.len() > 15 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Interface name too long (max 15 characters)",
        ));
    }

    if !name.starts_with("wg") {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "WireGuard interface names should start with 'wg'",
        ));
    }

    Ok(())
}

fn wireguard(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_interface_name(&params.interface)?;

    let client = WireGuardClient::new(check_mode);
    let interface = &params.interface;
    let mut changed = false;

    match params.state {
        State::Present => {
            let config = WireGuardClient::generate_config(&params);

            if check_mode {
                info!("Would create WireGuard config for {}", interface);
                return Ok(ModuleResult::new(true, None, None));
            }

            let config_path = WireGuardClient::config_path(interface);
            let existing_config = if Path::new(&config_path).exists() {
                fs::read_to_string(&config_path).ok()
            } else {
                None
            };

            if existing_config != Some(config.clone()) {
                WireGuardClient::write_config(interface, &config)?;
                changed = true;
            }

            let status = WireGuardClient::get_interface_status(interface)?;
            Ok(ModuleResult::new(
                changed,
                None,
                Some(serde_norway::value::to_value(status)?),
            ))
        }
        State::Absent => {
            if check_mode {
                info!("Would remove WireGuard interface {}", interface);
                return Ok(ModuleResult::new(true, None, None));
            }

            let is_up = WireGuardClient::is_interface_up(interface)?;
            if is_up {
                WireGuardClient::bring_down(interface)?;
                changed = true;
            }

            let config_exists = Path::new(&WireGuardClient::config_path(interface)).exists();
            if config_exists {
                WireGuardClient::remove_config(interface)?;
                changed = true;
            }

            Ok(ModuleResult::new(changed, None, None))
        }
        State::Up => {
            if check_mode {
                info!("Would bring up WireGuard interface {}", interface);
                return Ok(ModuleResult::new(true, None, None));
            }

            let is_up = WireGuardClient::is_interface_up(interface)?;
            if is_up {
                return Ok(ModuleResult::new(false, None, None));
            }

            WireGuardClient::bring_up(interface)?;
            let status = WireGuardClient::get_interface_status(interface)?;

            Ok(ModuleResult::new(
                true,
                None,
                Some(serde_norway::value::to_value(status)?),
            ))
        }
        State::Down => {
            if check_mode {
                info!("Would bring down WireGuard interface {}", interface);
                return Ok(ModuleResult::new(true, None, None));
            }

            let is_up = WireGuardClient::is_interface_up(interface)?;
            if !is_up {
                return Ok(ModuleResult::new(false, None, None));
            }

            WireGuardClient::bring_down(interface)?;
            Ok(ModuleResult::new(true, None, None))
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
            interface: wg0
            state: present
            private_key: "abc123"
            listen_port: 51820
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.interface, "wg0");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.private_key, Some("abc123".to_string()));
        assert_eq!(params.listen_port, Some(51820));
    }

    #[test]
    fn test_parse_params_with_peers() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: present
            private_key: "abc123"
            peers:
              - public_key: "peer123"
                endpoint: "192.168.1.100:51820"
                allowed_ips: ["10.0.0.2/32"]
                persistent_keepalive: 25
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.peers.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_params_up() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: up
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);
    }

    #[test]
    fn test_parse_params_down() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: down
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Down);
    }

    #[test]
    fn test_validate_interface_name() {
        assert!(validate_interface_name("wg0").is_ok());
        assert!(validate_interface_name("wg1").is_ok());
        assert!(validate_interface_name("wgtest").is_ok());
        assert!(validate_interface_name("").is_err());
        assert!(validate_interface_name("eth0").is_err());
        assert!(validate_interface_name("wg12345678901234").is_err());
    }

    #[test]
    fn test_generate_config() {
        let params = Params {
            interface: "wg0".to_string(),
            state: State::Present,
            private_key: Some("private123".to_string()),
            public_key: None,
            listen_port: Some(51820),
            peers: Some(vec![PeerParams {
                public_key: "peer123".to_string(),
                endpoint: Some("192.168.1.100:51820".to_string()),
                allowed_ips: vec!["10.0.0.2/32".to_string()],
                persistent_keepalive: Some(25),
                preshared_key: None,
            }]),
            endpoint: None,
            allowed_ips: None,
            persistent_keepalive: None,
            dns: None,
            mtu: None,
            save_config: false,
        };

        let config = WireGuardClient::generate_config(&params);
        assert!(config.contains("PrivateKey = private123"));
        assert!(config.contains("ListenPort = 51820"));
        assert!(config.contains("PublicKey = peer123"));
        assert!(config.contains("Endpoint = 192.168.1.100:51820"));
        assert!(config.contains("AllowedIPs = 10.0.0.2/32"));
        assert!(config.contains("PersistentKeepalive = 25"));
    }
}
