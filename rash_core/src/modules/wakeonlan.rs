/// ANCHOR: module
/// # wakeonlan
///
/// Send Wake-on-LAN magic packets to wake up network devices.
///
/// This module sends Wake-on-LAN magic packets to wake up sleeping devices.
/// Useful for IoT device management, remote server wake-up, scheduled wake-up
/// automation, and energy-saving workflows.
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
/// - name: Wake up server
///   wakeonlan:
///     mac: 00:11:22:33:44:55
///
/// - name: Wake up device with custom broadcast
///   wakeonlan:
///     mac: 00:11:22:33:44:55
///     broadcast: 192.168.1.255
///     port: 7
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

const DEFAULT_BROADCAST: &str = "255.255.255.255";
const DEFAULT_PORT: u16 = 9;

fn default_broadcast() -> String {
    DEFAULT_BROADCAST.to_owned()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// MAC address of target device (required).
    /// Format: XX:XX:XX:XX:XX:XX (e.g., 00:11:22:33:44:55)
    mac: String,
    /// Broadcast address to send the magic packet to.
    /// **[default: `255.255.255.255`]**
    #[serde(default = "default_broadcast")]
    broadcast: String,
    /// UDP port to send the magic packet to.
    /// **[default: `9`]**
    #[serde(default = "default_port")]
    port: u16,
}

fn parse_mac_address(mac: &str) -> Result<[u8; 6]> {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() != 6 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid MAC address format: {mac}. Expected format: XX:XX:XX:XX:XX:XX"),
        ));
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid MAC address component '{part}': {e}"),
            )
        })?;
    }

    Ok(bytes)
}

fn create_magic_packet(mac: [u8; 6]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(102);

    packet.extend_from_slice(&[0xFF; 6]);
    for _ in 0..16 {
        packet.extend_from_slice(&mac);
    }

    packet
}

fn send_wol_packet(params: &Params) -> Result<()> {
    let mac = parse_mac_address(&params.mac)?;
    let packet = create_magic_packet(mac);

    let addr_str = format!("{}:{}", params.broadcast, params.port);
    let addr: SocketAddr = addr_str
        .to_socket_addrs()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?
        .next()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to resolve address: {addr_str}"),
            )
        })?;

    let socket =
        UdpSocket::bind("0.0.0.0:0").map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    socket
        .set_broadcast(true)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    socket
        .send_to(&packet, addr)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    Ok(())
}

pub fn wakeonlan(params: Params) -> Result<ModuleResult> {
    send_wol_packet(&params)?;

    let extra = Some(value::to_value(json!({
        "mac": params.mac,
        "broadcast": params.broadcast,
        "port": params.port,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Wake-on-LAN packet sent to {}", params.mac)),
    ))
}

#[derive(Debug)]
pub struct WakeOnLan;

impl Module for WakeOnLan {
    fn get_name(&self) -> &str {
        "wakeonlan"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        if check_mode {
            let extra = Some(value::to_value(json!({
                "mac": params.mac,
                "broadcast": params.broadcast,
                "port": params.port,
            }))?);

            return Ok((
                ModuleResult::new(
                    true,
                    extra,
                    Some(format!("Would send Wake-on-LAN packet to {}", params.mac)),
                ),
                None,
            ));
        }

        Ok((wakeonlan(params)?, None))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mac: "00:11:22:33:44:55"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                mac: "00:11:22:33:44:55".to_owned(),
                broadcast: DEFAULT_BROADCAST.to_owned(),
                port: DEFAULT_PORT,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mac: "aa:bb:cc:dd:ee:ff"
            broadcast: "192.168.1.255"
            port: 7
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                mac: "aa:bb:cc:dd:ee:ff".to_owned(),
                broadcast: "192.168.1.255".to_owned(),
                port: 7,
            }
        );
    }

    #[test]
    fn test_parse_params_missing_mac() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            broadcast: "192.168.1.255"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_mac_address() {
        let mac = parse_mac_address("00:11:22:33:44:55").unwrap();
        assert_eq!(mac, [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);

        let mac = parse_mac_address("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        let mac = parse_mac_address("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn test_parse_mac_address_invalid() {
        let result = parse_mac_address("invalid");
        assert!(result.is_err());

        let result = parse_mac_address("00:11:22:33:44");
        assert!(result.is_err());

        let result = parse_mac_address("00:11:22:33:44:55:66");
        assert!(result.is_err());

        let result = parse_mac_address("00:11:22:33:44:GG");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_magic_packet() {
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let packet = create_magic_packet(mac);

        assert_eq!(packet.len(), 102);

        for byte in packet.iter().take(6) {
            assert_eq!(*byte, 0xFF);
        }

        for i in 0..16 {
            let offset = 6 + (i * 6);
            assert_eq!(packet[offset], 0x00);
            assert_eq!(packet[offset + 1], 0x11);
            assert_eq!(packet[offset + 2], 0x22);
            assert_eq!(packet[offset + 3], 0x33);
            assert_eq!(packet[offset + 4], 0x44);
            assert_eq!(packet[offset + 5], 0x55);
        }
    }

    #[test]
    fn test_check_mode() {
        let wol = WakeOnLan;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mac: "00:11:22:33:44:55"
            "#,
        )
        .unwrap();
        let (result, _) = wol
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would send"));
    }
}
