/// ANCHOR: module
/// # ipaddr
///
/// Manage IP addresses on network interfaces.
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
/// - name: Add IP address to interface
///   ipaddr:
///     interface: eth0
///     address: 192.168.1.10/24
///
/// - name: Add IPv6 address
///   ipaddr:
///     interface: eth0
///     address: 2001:db8::1/64
///     family: ipv6
///
/// - name: Remove IP address from interface
///   ipaddr:
///     interface: eth0
///     address: 192.168.1.10/24
///     state: absent
///
/// - name: Add secondary IP address
///   ipaddr:
///     interface: eth0
///     address: 192.168.2.10/24
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Network interface name.
    pub interface: String,
    /// IP address with CIDR (e.g., 192.168.1.10/24).
    pub address: String,
    /// Whether the address should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// IP address family (ipv4 or ipv6).
    /// Auto-detected from address format if not specified.
    /// **[default: `"ipv4"`]**
    pub family: Option<Family>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Family {
    #[default]
    Ipv4,
    Ipv6,
}

fn detect_family(address: &str) -> Family {
    if address.contains(':') {
        Family::Ipv6
    } else {
        Family::Ipv4
    }
}

fn get_ip_command() -> &'static str {
    "ip"
}

fn interface_exists(interface: &str) -> Result<bool> {
    let output = match Command::new(get_ip_command())
        .args(["link", "show", interface])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Ok(false),
    };

    Ok(output.status.success())
}

fn address_exists(interface: &str, address: &str, family: Family) -> Result<bool> {
    let family_arg = match family {
        Family::Ipv4 => "-4",
        Family::Ipv6 => "-6",
    };

    let output = match Command::new(get_ip_command())
        .args([family_arg, "addr", "show", interface])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Ok(false),
    };

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("inet ") || line.contains("inet6 ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let addr = parts[1];
                if addr == address {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

fn add_address(interface: &str, address: &str, family: Family) -> Result<()> {
    let family_arg = match family {
        Family::Ipv4 => "-4",
        Family::Ipv6 => "-6",
    };

    let output = Command::new(get_ip_command())
        .args([family_arg, "addr", "add", address, "dev", interface])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute ip addr add: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add address {} to {}: {}",
                address,
                interface,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn remove_address(interface: &str, address: &str, family: Family) -> Result<()> {
    let family_arg = match family {
        Family::Ipv4 => "-4",
        Family::Ipv6 => "-6",
    };

    let output = Command::new(get_ip_command())
        .args([family_arg, "addr", "del", address, "dev", interface])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute ip addr del: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to remove address {} from {}: {}",
                address,
                interface,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn validate_address(address: &str) -> Result<()> {
    if address.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Address cannot be empty",
        ));
    }

    let parts: Vec<&str> = address.split('/').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Address must include CIDR notation (e.g., 192.168.1.10/24)",
        ));
    }

    let ip = parts[0];
    let cidr: u8 = parts[1]
        .parse()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Invalid CIDR notation"))?;

    if ip.contains(':') {
        if cidr > 128 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "IPv6 CIDR must be between 0 and 128",
            ));
        }
    } else if cidr > 32 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "IPv4 CIDR must be between 0 and 32",
        ));
    }

    Ok(())
}

fn validate_interface(interface: &str) -> Result<()> {
    if interface.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Interface cannot be empty",
        ));
    }

    Ok(())
}

pub fn ipaddr(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_interface(&params.interface)?;
    validate_address(&params.address)?;

    let family = params
        .family
        .unwrap_or_else(|| detect_family(&params.address));
    let state = params.state.unwrap_or_default();

    if !check_mode && !interface_exists(&params.interface)? {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Interface {} does not exist", params.interface),
        ));
    }

    match state {
        State::Present => {
            let exists = address_exists(&params.interface, &params.address, family)?;
            if exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if check_mode {
                diff(
                    format!("Interface {} without {}", params.interface, params.address),
                    format!("Interface {} with {}", params.interface, params.address),
                );
                return Ok(ModuleResult::new(true, None, None));
            }

            add_address(&params.interface, &params.address, family)?;
            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Added {} to {}", params.address, params.interface)),
            ))
        }
        State::Absent => {
            let exists = address_exists(&params.interface, &params.address, family)?;
            if !exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if check_mode {
                diff(
                    format!("Interface {} with {}", params.interface, params.address),
                    format!("Interface {} without {}", params.interface, params.address),
                );
                return Ok(ModuleResult::new(true, None, None));
            }

            remove_address(&params.interface, &params.address, family)?;
            Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Removed {} from {}",
                    params.address, params.interface
                )),
            ))
        }
    }
}

#[derive(Debug)]
pub struct Ipaddr;

impl Module for Ipaddr {
    fn get_name(&self) -> &str {
        "ipaddr"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((ipaddr(parse_params(optional_params)?, check_mode)?, None))
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
            interface: eth0
            address: 192.168.1.10/24
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                interface: "eth0".to_owned(),
                address: "192.168.1.10/24".to_owned(),
                state: None,
                family: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: eth0
            address: 192.168.1.10/24
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_family() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: eth0
            address: 2001:db8::1/64
            family: ipv6
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.family, Some(Family::Ipv6));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: eth0
            address: 192.168.1.10/24
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_detect_family_ipv4() {
        assert_eq!(detect_family("192.168.1.10/24"), Family::Ipv4);
        assert_eq!(detect_family("10.0.0.1/8"), Family::Ipv4);
    }

    #[test]
    fn test_detect_family_ipv6() {
        assert_eq!(detect_family("2001:db8::1/64"), Family::Ipv6);
        assert_eq!(detect_family("::1/128"), Family::Ipv6);
    }

    #[test]
    fn test_validate_address_valid() {
        assert!(validate_address("192.168.1.10/24").is_ok());
        assert!(validate_address("10.0.0.1/8").is_ok());
        assert!(validate_address("2001:db8::1/64").is_ok());
        assert!(validate_address("::1/128").is_ok());
    }

    #[test]
    fn test_validate_address_empty() {
        assert!(validate_address("").is_err());
    }

    #[test]
    fn test_validate_address_no_cidr() {
        assert!(validate_address("192.168.1.10").is_err());
    }

    #[test]
    fn test_validate_address_invalid_cidr_ipv4() {
        assert!(validate_address("192.168.1.10/33").is_err());
    }

    #[test]
    fn test_validate_address_invalid_cidr_ipv6() {
        assert!(validate_address("2001:db8::1/129").is_err());
    }

    #[test]
    fn test_validate_address_invalid_cidr_format() {
        assert!(validate_address("192.168.1.10/abc").is_err());
    }

    #[test]
    fn test_validate_interface_valid() {
        assert!(validate_interface("eth0").is_ok());
        assert!(validate_interface("wlan0").is_ok());
        assert!(validate_interface("lo").is_ok());
    }

    #[test]
    fn test_validate_interface_empty() {
        assert!(validate_interface("").is_err());
    }
}
