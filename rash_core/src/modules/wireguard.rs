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
/// - name: Create WireGuard interface with a peer
///   wireguard:
///     interface: wg0
///     state: present
///     private_key: "PRIVATE_KEY_HERE"
///     listen_port: 51820
///     peers:
///       - public_key: "PEER_PUBLIC_KEY"
///         endpoint: "192.168.1.100:51820"
///         allowed_ips:
///           - "10.0.0.2/32"
///         persistent_keepalive: 25
///
/// - name: Configure interface with DNS and MTU
///   wireguard:
///     interface: wg0
///     state: present
///     private_key: "PRIVATE_KEY_HERE"
///     address: "10.0.0.1/24"
///     dns:
///       - "1.1.1.1"
///       - "8.8.8.8"
///     mtu: 1280
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
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const WG_CONFIG_DIR: &str = "/etc/wireguard";

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
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
    #[serde(default)]
    pub state: State,
    pub private_key: Option<String>,
    pub address: Option<String>,
    pub listen_port: Option<u16>,
    pub dns: Option<Vec<String>>,
    pub mtu: Option<u16>,
    pub peers: Option<Vec<PeerParams>>,
    pub save_config: Option<bool>,
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

fn config_path(interface: &str) -> String {
    format!("{WG_CONFIG_DIR}/{interface}.conf")
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

fn run_wg(args: &[&str]) -> Result<std::process::Output> {
    Command::new("wg")
        .args(args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
}

fn run_wg_quick(args: &[&str]) -> Result<std::process::Output> {
    Command::new("wg-quick")
        .args(args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
}

fn is_interface_up(interface: &str) -> Result<bool> {
    let output = run_wg(&["show", interface])?;
    Ok(output.status.success() && !output.stdout.is_empty())
}

fn generate_config(params: &Params) -> String {
    let mut config = String::new();

    config.push_str("[Interface]\n");

    if let Some(key) = &params.private_key {
        config.push_str(&format!("PrivateKey = {key}\n"));
    }

    if let Some(ref addr) = params.address {
        config.push_str(&format!("Address = {addr}\n"));
    }

    if let Some(port) = params.listen_port {
        config.push_str(&format!("ListenPort = {port}\n"));
    }

    if let Some(ref dns) = params.dns
        && !dns.is_empty()
    {
        config.push_str(&format!("DNS = {}\n", dns.join(", ")));
    }

    if let Some(mtu) = params.mtu {
        config.push_str(&format!("MTU = {mtu}\n"));
    }

    if let Some(save) = params.save_config
        && save
    {
        config.push_str("SaveConfig = true\n");
    }

    if let Some(ref peers) = params.peers {
        for peer in peers {
            config.push_str("\n[Peer]\n");
            config.push_str(&format!("PublicKey = {}\n", peer.public_key));

            if let Some(ref endpoint) = peer.endpoint {
                config.push_str(&format!("Endpoint = {endpoint}\n"));
            }

            if !peer.allowed_ips.is_empty() {
                config.push_str(&format!("AllowedIPs = {}\n", peer.allowed_ips.join(", ")));
            }

            if let Some(keepalive) = peer.persistent_keepalive {
                config.push_str(&format!("PersistentKeepalive = {keepalive}\n"));
            }

            if let Some(ref psk) = peer.preshared_key {
                config.push_str(&format!("PresharedKey = {psk}\n"));
            }
        }
    }

    config
}

fn write_config(interface: &str, config: &str) -> Result<()> {
    let path = config_path(interface);

    fs::create_dir_all(WG_CONFIG_DIR).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to create WireGuard config directory: {e}"),
        )
    })?;

    fs::write(&path, config).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to write WireGuard config: {e}"),
        )
    })
}

fn remove_config(interface: &str) -> Result<()> {
    let path = config_path(interface);

    if Path::new(&path).exists() {
        fs::remove_file(&path).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to remove WireGuard config: {e}"),
            )
        })?;
    }

    Ok(())
}

fn get_interface_status(interface: &str) -> Result<serde_json::Value> {
    let output = run_wg(&["show", interface, "dump"])?;

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

        if parts.len() >= 4
            && let Ok(port) = parts[1].parse::<u64>()
        {
            status.insert(
                "listen_port".to_string(),
                serde_json::Value::Number(port.into()),
            );
        }
    }

    Ok(serde_json::Value::Object(status))
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let config = generate_config(params);
    let interface = &params.interface;

    let config_file = config_path(interface);
    let existing_config = if Path::new(&config_file).exists() {
        fs::read_to_string(&config_file).ok()
    } else {
        None
    };

    if existing_config.as_ref() == Some(&config) {
        let status = get_interface_status(interface)?;
        let extra = serde_norway::to_value(serde_json::json!({
            "status": status,
        }))
        .ok();
        return Ok(ModuleResult::new(false, extra, None));
    }

    if check_mode {
        info!("Would create WireGuard config for {}", interface);
        return Ok(ModuleResult::new(true, None, None));
    }

    write_config(interface, &config)?;

    let status = get_interface_status(interface)?;
    let extra = serde_norway::to_value(serde_json::json!({
        "status": status,
    }))
    .ok();

    Ok(ModuleResult::new(true, extra, None))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let interface = &params.interface;
    let mut changed = false;

    if is_interface_up(interface)? {
        if check_mode {
            info!("Would bring down WireGuard interface {}", interface);
            return Ok(ModuleResult::new(true, None, None));
        }

        let output = run_wg_quick(&["down", interface])?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "wg-quick down failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        changed = true;
    }

    let config_file = config_path(interface);
    if Path::new(&config_file).exists() {
        if check_mode {
            info!("Would remove WireGuard config for {}", interface);
            return Ok(ModuleResult::new(true, None, None));
        }

        remove_config(interface)?;
        changed = true;
    }

    Ok(ModuleResult::new(changed, None, None))
}

fn exec_up(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let interface = &params.interface;

    if is_interface_up(interface)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some("Interface already up".to_string()),
        ));
    }

    if check_mode {
        info!("Would bring up WireGuard interface {}", interface);
        return Ok(ModuleResult::new(true, None, None));
    }

    let output = run_wg_quick(&["up", interface])?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "wg-quick up failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let status = get_interface_status(interface)?;
    let extra = serde_norway::to_value(serde_json::json!({
        "status": status,
    }))
    .ok();

    Ok(ModuleResult::new(true, extra, None))
}

fn exec_down(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let interface = &params.interface;

    if !is_interface_up(interface)? {
        return Ok(ModuleResult::new(
            false,
            None,
            Some("Interface already down".to_string()),
        ));
    }

    if check_mode {
        info!("Would bring down WireGuard interface {}", interface);
        return Ok(ModuleResult::new(true, None, None));
    }

    let output = run_wg_quick(&["down", interface])?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "wg-quick down failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Interface {} brought down", interface)),
    ))
}

fn wireguard(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_interface_name(&params.interface)?;

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
        State::Up => exec_up(&params, check_mode),
        State::Down => exec_down(&params, check_mode),
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
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str("interface: wg0").unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.interface, "wg0");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.private_key, None);
        assert_eq!(params.address, None);
        assert_eq!(params.listen_port, None);
        assert_eq!(params.dns, None);
        assert_eq!(params.mtu, None);
        assert_eq!(params.peers, None);
        assert_eq!(params.save_config, None);
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
                allowed_ips:
                  - "10.0.0.2/32"
                  - "192.168.2.0/24"
                persistent_keepalive: 25
                preshared_key: "psk123"
              - public_key: "peer456"
                allowed_ips:
                  - "10.0.0.3/32"
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        let peers = params.peers.unwrap();
        assert_eq!(peers.len(), 2);

        assert_eq!(peers[0].public_key, "peer123");
        assert_eq!(peers[0].endpoint, Some("192.168.1.100:51820".to_string()));
        assert_eq!(
            peers[0].allowed_ips,
            vec!["10.0.0.2/32".to_string(), "192.168.2.0/24".to_string()]
        );
        assert_eq!(peers[0].persistent_keepalive, Some(25));
        assert_eq!(peers[0].preshared_key, Some("psk123".to_string()));

        assert_eq!(peers[1].public_key, "peer456");
        assert_eq!(peers[1].endpoint, None);
        assert_eq!(peers[1].allowed_ips, vec!["10.0.0.3/32".to_string()]);
        assert_eq!(peers[1].persistent_keepalive, None);
        assert_eq!(peers[1].preshared_key, None);
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
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: absent
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: present
            private_key: "abc123"
            address: "10.0.0.1/24"
            listen_port: 51820
            dns:
              - "1.1.1.1"
              - "8.8.8.8"
            mtu: 1280
            save_config: true
            "#,
        )
        .unwrap();

        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.private_key, Some("abc123".to_string()));
        assert_eq!(params.address, Some("10.0.0.1/24".to_string()));
        assert_eq!(params.listen_port, Some(51820));
        assert_eq!(
            params.dns,
            Some(vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()])
        );
        assert_eq!(params.mtu, Some(1280));
        assert_eq!(params.save_config, Some(true));
    }

    #[test]
    fn test_parse_params_deny_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: wg0
            state: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_interface() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
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
    fn test_generate_config_interface_only() {
        let params = Params {
            interface: "wg0".to_string(),
            state: State::Present,
            private_key: Some("private123".to_string()),
            address: None,
            listen_port: Some(51820),
            dns: None,
            mtu: None,
            peers: None,
            save_config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[Interface]"));
        assert!(config.contains("PrivateKey = private123"));
        assert!(config.contains("ListenPort = 51820"));
        assert!(!config.contains("[Peer]"));
    }

    #[test]
    fn test_generate_config_full() {
        let params = Params {
            interface: "wg0".to_string(),
            state: State::Present,
            private_key: Some("private123".to_string()),
            address: Some("10.0.0.1/24".to_string()),
            listen_port: Some(51820),
            dns: Some(vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()]),
            mtu: Some(1280),
            peers: Some(vec![PeerParams {
                public_key: "peer123".to_string(),
                endpoint: Some("192.168.1.100:51820".to_string()),
                allowed_ips: vec!["10.0.0.2/32".to_string()],
                persistent_keepalive: Some(25),
                preshared_key: Some("psk123".to_string()),
            }]),
            save_config: Some(true),
        };

        let config = generate_config(&params);
        assert!(config.contains("[Interface]"));
        assert!(config.contains("PrivateKey = private123"));
        assert!(config.contains("Address = 10.0.0.1/24"));
        assert!(config.contains("ListenPort = 51820"));
        assert!(config.contains("DNS = 1.1.1.1, 8.8.8.8"));
        assert!(config.contains("MTU = 1280"));
        assert!(config.contains("SaveConfig = true"));
        assert!(config.contains("[Peer]"));
        assert!(config.contains("PublicKey = peer123"));
        assert!(config.contains("Endpoint = 192.168.1.100:51820"));
        assert!(config.contains("AllowedIPs = 10.0.0.2/32"));
        assert!(config.contains("PersistentKeepalive = 25"));
        assert!(config.contains("PresharedKey = psk123"));
    }

    #[test]
    fn test_generate_config_no_private_key() {
        let params = Params {
            interface: "wg0".to_string(),
            state: State::Present,
            private_key: None,
            address: None,
            listen_port: None,
            dns: None,
            mtu: None,
            peers: None,
            save_config: None,
        };

        let config = generate_config(&params);
        assert!(config.contains("[Interface]"));
        assert!(!config.contains("PrivateKey"));
    }

    #[test]
    fn test_generate_config_multiple_peers() {
        let params = Params {
            interface: "wg0".to_string(),
            state: State::Present,
            private_key: Some("key".to_string()),
            address: None,
            listen_port: None,
            dns: None,
            mtu: None,
            peers: Some(vec![
                PeerParams {
                    public_key: "peer1".to_string(),
                    endpoint: Some("1.1.1.1:51820".to_string()),
                    allowed_ips: vec!["10.0.0.2/32".to_string()],
                    persistent_keepalive: None,
                    preshared_key: None,
                },
                PeerParams {
                    public_key: "peer2".to_string(),
                    endpoint: None,
                    allowed_ips: vec!["10.0.0.3/32".to_string(), "10.0.0.4/32".to_string()],
                    persistent_keepalive: Some(30),
                    preshared_key: None,
                },
            ]),
            save_config: None,
        };

        let config = generate_config(&params);
        assert_eq!(config.matches("[Peer]").count(), 2);
        assert!(config.contains("PublicKey = peer1"));
        assert!(config.contains("PublicKey = peer2"));
        assert!(config.contains("AllowedIPs = 10.0.0.3/32, 10.0.0.4/32"));
        assert!(config.contains("PersistentKeepalive = 30"));
    }

    #[test]
    fn test_config_path() {
        assert_eq!(config_path("wg0"), "/etc/wireguard/wg0.conf");
        assert_eq!(config_path("wg1"), "/etc/wireguard/wg1.conf");
    }
}
