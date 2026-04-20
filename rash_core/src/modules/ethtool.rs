/// ANCHOR: module
/// # ethtool
///
/// Manage Ethernet device settings using ethtool.
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
/// - name: Set link speed and duplex
///   ethtool:
///     device: eth0
///     speed: 1000
///     duplex: full
///
/// - name: Enable auto-negotiation
///   ethtool:
///     device: eth0
///     autoneg: true
///
/// - name: Disable auto-negotiation with specific speed
///   ethtool:
///     device: eth0
///     autoneg: false
///     speed: 10000
///     duplex: full
///
/// - name: Configure offload features
///   ethtool:
///     device: eth0
///     offload:
///       rx: true
///       tx: true
///       tso: false
///       gso: true
///
/// - name: Query current device settings
///   ethtool:
///     device: eth0
///     state: query
///   register: eth_settings
///
/// - name: Reset device to default settings
///   ethtool:
///     device: eth0
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Duplex {
    Half,
    Full,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Query,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
pub struct Offload {
    pub rx: Option<bool>,
    pub tx: Option<bool>,
    pub tso: Option<bool>,
    pub gso: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Network interface name (e.g., eth0, ens33).
    pub device: String,
    /// Whether the settings should be present, absent (reset), or query current state.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Link speed in Mbps (10, 100, 1000, 2500, 5000, 10000, 25000, 40000, 50000, 100000).
    pub speed: Option<u32>,
    /// Duplex mode (half or full).
    pub duplex: Option<Duplex>,
    /// Enable or disable auto-negotiation.
    pub autoneg: Option<bool>,
    /// Offload feature settings.
    pub offload: Option<Offload>,
}

fn validate_device_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device name cannot be empty",
        ));
    }
    if name.len() > 15 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Device name '{}' too long (max 15 characters)", name),
        ));
    }
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.' && c != '@' {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid character '{}' in device name", c),
            ));
        }
    }
    Ok(())
}

fn validate_params(params: &Params) -> Result<()> {
    validate_device_name(&params.device)?;

    let valid_speeds = [
        10, 100, 1000, 2500, 5000, 10000, 25000, 40000, 50000, 100000,
    ];
    if let Some(speed) = params.speed
        && !valid_speeds.contains(&speed)
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid speed '{}'. Must be one of: {}",
                speed,
                valid_speeds
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }

    if params.speed.is_some() && params.duplex.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "duplex is required when speed is specified",
        ));
    }
    if params.speed.is_none() && params.duplex.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "speed is required when duplex is specified",
        ));
    }

    Ok(())
}

fn run_ethtool(args: &[&str]) -> Result<String> {
    let output = Command::new("ethtool").args(args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ethtool: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "ethtool {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_current_settings(device: &str) -> Result<String> {
    run_ethtool(&[device])
}

fn parse_setting_value(output: &str, key: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with(key) {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                return Some(parts[1].trim().to_string());
            }
        }
    }
    None
}

fn settings_match(current: &str, params: &Params) -> bool {
    if let Some(speed) = params.speed {
        if let Some(val) = parse_setting_value(current, "Speed") {
            let current_speed: u32 = val.trim_end_matches("Mb/s").trim().parse().unwrap_or(0);
            if current_speed != speed {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(ref duplex) = params.duplex {
        if let Some(val) = parse_setting_value(current, "Duplex") {
            let current_duplex = val.to_lowercase();
            let desired = match duplex {
                Duplex::Half => "half",
                Duplex::Full => "full",
            };
            if current_duplex != desired {
                return false;
            }
        } else {
            return false;
        }
    }

    if let Some(autoneg) = params.autoneg {
        if let Some(val) = parse_setting_value(current, "Auto-negotiation") {
            let is_on = val.eq_ignore_ascii_case("on");
            if is_on != autoneg {
                return false;
            }
        } else {
            return false;
        }
    }

    true
}

fn apply_link_settings(device: &str, params: &Params) -> Result<()> {
    let mut args = vec![device];

    if let Some(autoneg) = params.autoneg {
        args.push("autoneg");
        args.push(if autoneg { "on" } else { "off" });
    }

    if let Some(speed) = params.speed {
        args.push("speed");
        args.push(Box::leak(speed.to_string().into_boxed_str()) as &str);
    }

    if let Some(ref duplex) = params.duplex {
        args.push("duplex");
        args.push(match duplex {
            Duplex::Half => "half",
            Duplex::Full => "full",
        });
    }

    run_ethtool(&args)?;
    Ok(())
}

fn apply_offload_settings(device: &str, offload: &Offload) -> Result<()> {
    let features = [
        ("rx", offload.rx),
        ("tx", offload.tx),
        ("tso", offload.tso),
        ("gso", offload.gso),
    ];

    for (name, value) in features {
        if let Some(v) = value {
            run_ethtool(&["-K", device, name, if v { "on" } else { "off" }])?;
        }
    }

    Ok(())
}

fn reset_device(device: &str) -> Result<()> {
    run_ethtool(&["-r", device])?;
    Ok(())
}

fn exec_ethtool(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_params(&params)?;

    let state = params.state.unwrap_or_default();

    match state {
        State::Query => {
            if check_mode {
                info!("Would query settings for {}", params.device);
                return Ok(ModuleResult::new(false, None, None));
            }
            let current = get_current_settings(&params.device)?;
            let extra = serde_norway::to_value(serde_json::json!({
                "settings": current,
                "device": params.device,
            }))
            .ok();
            return Ok(ModuleResult::new(false, extra, Some(current)));
        }
        State::Absent => {
            if check_mode {
                info!("Would reset device {}", params.device);
                return Ok(ModuleResult::new(true, None, None));
            }
            reset_device(&params.device)?;
            let output = format!("Reset device {}", params.device);
            return Ok(ModuleResult::new(true, None, Some(output)));
        }
        State::Present => {}
    }

    let current = get_current_settings(&params.device).unwrap_or_default();

    let mut changed = false;

    if (params.speed.is_some() || params.duplex.is_some() || params.autoneg.is_some())
        && !settings_match(&current, &params)
    {
        if check_mode {
            info!(
                "Would apply link settings to {}: speed={:?}, duplex={:?}, autoneg={:?}",
                params.device, params.speed, params.duplex, params.autoneg
            );
            changed = true;
        } else {
            apply_link_settings(&params.device, &params)?;
            changed = true;
        }
    }

    if let Some(ref offload) = params.offload {
        if check_mode {
            if offload.rx.is_some()
                || offload.tx.is_some()
                || offload.tso.is_some()
                || offload.gso.is_some()
            {
                info!("Would apply offload settings to {}", params.device);
                changed = true;
            }
        } else {
            apply_offload_settings(&params.device, offload)?;
            changed = true;
        }
    }

    if changed {
        let output = format!("Updated settings for {}", params.device);
        let extra = serde_norway::to_value(serde_json::json!({
            "device": params.device,
        }))
        .ok();
        Ok(ModuleResult::new(true, extra, Some(output)))
    } else {
        let extra = serde_norway::to_value(serde_json::json!({
            "settings": current,
            "device": params.device,
        }))
        .ok();
        Ok(ModuleResult::new(false, extra, None))
    }
}

#[derive(Debug)]
pub struct Ethtool;

impl Module for Ethtool {
    fn get_name(&self) -> &str {
        "ethtool"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            exec_ethtool(parse_params(optional_params)?, check_mode)?,
            None,
        ))
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
    fn test_parse_params_basic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.state, None);
        assert_eq!(params.speed, None);
        assert_eq!(params.duplex, None);
        assert_eq!(params.autoneg, None);
        assert_eq!(params.offload, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            speed: 1000
            duplex: full
            autoneg: false
            state: present
            offload:
              rx: true
              tx: true
              tso: false
              gso: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.speed, Some(1000));
        assert_eq!(params.duplex, Some(Duplex::Full));
        assert_eq!(params.autoneg, Some(false));
        assert_eq!(params.state, Some(State::Present));
        assert!(params.offload.is_some());
        let offload = params.offload.unwrap();
        assert_eq!(offload.rx, Some(true));
        assert_eq!(offload.tx, Some(true));
        assert_eq!(offload.tso, Some(false));
        assert_eq!(offload.gso, Some(true));
    }

    #[test]
    fn test_parse_params_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: ens33
            state: query
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Query));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_duplex_half() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            speed: 100
            duplex: half
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.duplex, Some(Duplex::Half));
    }

    #[test]
    fn test_parse_params_deny_unknown() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            unknown_field: value
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_device_name_valid() {
        assert!(validate_device_name("eth0").is_ok());
        assert!(validate_device_name("ens33").is_ok());
        assert!(validate_device_name("enp0s3").is_ok());
        assert!(validate_device_name("wlan0").is_ok());
        assert!(validate_device_name("br-123").is_ok());
        assert!(validate_device_name("veth.test").is_ok());
    }

    #[test]
    fn test_validate_device_name_invalid() {
        assert!(validate_device_name("").is_err());
        assert!(validate_device_name("invalid device name").is_err());
        assert!(validate_device_name(&"a".repeat(16)).is_err());
    }

    #[test]
    fn test_validate_params_speed_without_duplex() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: None,
            autoneg: None,
            offload: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_duplex_without_speed() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: None,
            duplex: Some(Duplex::Full),
            autoneg: None,
            offload: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_invalid_speed() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(42),
            duplex: Some(Duplex::Full),
            autoneg: None,
            offload: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_valid_speed_duplex() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: Some(Duplex::Full),
            autoneg: None,
            offload: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_parse_setting_value() {
        let output = "Settings for eth0:\n\tSupported ports: [ TP ]\n\tSpeed: 1000Mb/s\n\tDuplex: Full\n\tAuto-negotiation: on\n";
        assert_eq!(
            parse_setting_value(output, "Speed"),
            Some("1000Mb/s".to_string())
        );
        assert_eq!(
            parse_setting_value(output, "Duplex"),
            Some("Full".to_string())
        );
        assert_eq!(
            parse_setting_value(output, "Auto-negotiation"),
            Some("on".to_string())
        );
        assert_eq!(parse_setting_value(output, "Link detected"), None);
    }

    #[test]
    fn test_settings_match() {
        let current =
            "Settings for eth0:\n\tSpeed: 1000Mb/s\n\tDuplex: Full\n\tAuto-negotiation: on\n";

        let params_match = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: Some(Duplex::Full),
            autoneg: Some(true),
            offload: None,
        };
        assert!(settings_match(current, &params_match));

        let params_no_match_speed = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(100),
            duplex: Some(Duplex::Full),
            autoneg: None,
            offload: None,
        };
        assert!(!settings_match(current, &params_no_match_speed));

        let params_no_match_duplex = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: Some(Duplex::Half),
            autoneg: None,
            offload: None,
        };
        assert!(!settings_match(current, &params_no_match_duplex));

        let params_no_match_autoneg = Params {
            device: "eth0".to_string(),
            state: None,
            speed: None,
            duplex: None,
            autoneg: Some(false),
            offload: None,
        };
        assert!(!settings_match(current, &params_no_match_autoneg));
    }

    #[test]
    fn test_state_default() {
        assert_eq!(State::default(), State::Present);
    }
}
