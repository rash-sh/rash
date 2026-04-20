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
/// - name: Set link speed and duplex with auto-negotiation
///   ethtool:
///     device: eth0
///     speed: 1000
///     duplex: full
///     autoneg: true
///
/// - name: Disable auto-negotiation and set fixed speed
///   ethtool:
///     device: eth0
///     speed: 100
///     duplex: half
///     autoneg: false
///
/// - name: Query current device settings
///   ethtool:
///     device: eth0
///     state: query
///
/// - name: Enable RX/TX offload features
///   ethtool:
///     device: eth0
///     offload:
///       rx: true
///       tx: true
///       tso: true
///       gso: true
///
/// - name: Disable specific offload features
///   ethtool:
///     device: eth0
///     offload:
///       tso: false
///       gso: false
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

#[derive(Debug, Default, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Query,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Duplex {
    Half,
    Full,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
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
    /// Desired state of the settings.
    /// `present` applies settings, `absent` resets to defaults, `query` returns current settings.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Link speed in Mbps (10, 100, 1000, 10000, 25000, 40000, 100000).
    pub speed: Option<u32>,
    /// Duplex mode.
    pub duplex: Option<Duplex>,
    /// Enable or disable auto-negotiation.
    pub autoneg: Option<bool>,
    /// Offload feature settings.
    pub offload: Option<Offload>,
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
            ethtool_exec(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn validate_device(device: &str) -> Result<()> {
    if device.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "device cannot be empty",
        ));
    }

    if device.len() > 15 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("device name '{}' too long (max 15 characters)", device),
        ));
    }

    for c in device.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.' && c != '@' {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("invalid character '{}' in device name", c),
            ));
        }
    }

    Ok(())
}

fn validate_speed(speed: u32) -> Result<()> {
    let valid_speeds = [10, 100, 1000, 2500, 5000, 10000, 25000, 40000, 50000, 100000];
    if !valid_speeds.contains(&speed) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "invalid speed '{}'. Valid speeds: {:?}",
                speed, valid_speeds
            ),
        ));
    }
    Ok(())
}

fn run_ethtool(args: &[&str]) -> Result<String> {
    let output = Command::new("ethtool")
        .args(args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

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

fn get_current_settings(device: &str) -> Result<EthtoolSettings> {
    let output = run_ethtool(&[device])?;

    let mut speed: Option<u32> = None;
    let mut duplex: Option<String> = None;
    let mut autoneg: Option<bool> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Speed:") {
            let val = rest.trim();
            if let Some(num) = val.strip_suffix("Mb/s") {
                speed = num.trim().parse::<u32>().ok();
            }
        } else if let Some(rest) = trimmed.strip_prefix("Duplex:") {
            duplex = Some(rest.trim().to_lowercase());
        } else if let Some(rest) = trimmed.strip_prefix("Auto-negotiation:") {
            let val = rest.trim().to_lowercase();
            autoneg = Some(val == "on");
        }
    }

    Ok(EthtoolSettings {
        speed,
        duplex,
        autoneg,
    })
}

#[derive(Debug)]
struct EthtoolSettings {
    speed: Option<u32>,
    duplex: Option<String>,
    autoneg: Option<bool>,
}

fn get_offload_state(device: &str, feature: &str) -> Result<bool> {
    let output = run_ethtool(&["-k", device])?;

    for line in output.lines() {
        let trimmed = line.trim();
        let prefix = format!("{}: ", feature);
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            return Ok(rest.trim() == "on");
        }
    }

    Err(Error::new(
        ErrorKind::SubprocessFail,
        format!("feature '{}' not found for device '{}'", feature, device),
    ))
}

fn apply_link_settings(
    device: &str,
    speed: Option<u32>,
    duplex: Option<&Duplex>,
    autoneg: Option<bool>,
    check_mode: bool,
) -> Result<bool> {
    if speed.is_none() && duplex.is_none() && autoneg.is_none() {
        return Ok(false);
    }

    let current = get_current_settings(device)?;

    let speed_matches = match (speed, current.speed) {
        (Some(s), Some(cs)) => s == cs,
        (None, _) => true,
        _ => false,
    };

    let duplex_matches = match duplex {
        Some(Duplex::Full) => current.duplex.as_deref() == Some("full"),
        Some(Duplex::Half) => current.duplex.as_deref() == Some("half"),
        None => true,
    };

    let autoneg_matches = match (autoneg, current.autoneg) {
        (Some(a), Some(ca)) => a == ca,
        (None, _) => true,
        _ => false,
    };

    if speed_matches && duplex_matches && autoneg_matches {
        return Ok(false);
    }

    let old_desc = format!(
        "speed={:?} duplex={:?} autoneg={:?}",
        current.speed, current.duplex, current.autoneg
    );

    if check_mode {
        let new_desc = format!(
            "speed={:?} duplex={} autoneg={:?}",
            speed,
            duplex
                .map(|d| match d {
                    Duplex::Full => "full",
                    Duplex::Half => "half",
                })
                .unwrap_or("unchanged"),
            autoneg
        );
        diff(old_desc, new_desc);
        return Ok(true);
    }

    let mut args: Vec<String> = vec![device.to_string()];

    if let Some(s) = speed {
        args.push("speed".to_string());
        args.push(s.to_string());
    }

    if let Some(d) = duplex {
        args.push("duplex".to_string());
        args.push(
            match d {
                Duplex::Full => "full",
                Duplex::Half => "half",
            }
            .to_string(),
        );
    }

    if let Some(a) = autoneg {
        args.push("autoneg".to_string());
        args.push(if a { "on" } else { "off" }.to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ethtool(&arg_refs)?;

    Ok(true)
}

fn apply_offload(
    device: &str,
    offload: &Offload,
    check_mode: bool,
) -> Result<bool> {
    let mut changed = false;

    let features = [
        ("rx", offload.rx, "rx-checksumming"),
        ("tx", offload.tx, "tx-checksumming"),
        ("tso", offload.tso, "tcp-segmentation-offload"),
        ("gso", offload.gso, "generic-segmentation-offload"),
    ];

    for (key, desired, feature_name) in &features {
        if let &Some(want) = desired {
            let current = get_offload_state(device, feature_name)?;
            if current != want {
                if !check_mode {
                    let val = if want { "on" } else { "off" };
                    run_ethtool(&["-K", device, *key, val])?;
                }
                changed = true;
            }
        }
    }

    if changed && check_mode {
        diff("offload settings unchanged", "offload settings changed");
    }

    Ok(changed)
}

fn reset_device(device: &str, check_mode: bool) -> Result<bool> {
    if check_mode {
        return Ok(true);
    }

    let output = Command::new("ethtool")
        .args(["-r", device])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "ethtool -r {} failed: {}",
                device,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn ethtool_exec(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.device)?;

    match params.state {
        State::Query => {
            let output = run_ethtool(&[&params.device])?;
            let offload_output = run_ethtool(&["-k", &params.device])
                .unwrap_or_else(|_| "Offload info unavailable".to_string());

            let combined = format!("{}\n\nOffload features:\n{}", output, offload_output);

            Ok(ModuleResult::new(
                false,
                Some(serde_norway::to_value(&combined).unwrap_or_default()),
                Some(combined),
            ))
        }
        State::Present => {
            if let Some(speed) = params.speed {
                validate_speed(speed)?;
            }

            if params.speed.is_some() && params.duplex.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "duplex is required when speed is specified",
                ));
            }

            if params.duplex.is_some() && params.speed.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "speed is required when duplex is specified",
                ));
            }

            let mut changed = false;

            changed |= apply_link_settings(
                &params.device,
                params.speed,
                params.duplex.as_ref(),
                params.autoneg,
                check_mode,
            )?;

            if let Some(ref offload) = params.offload {
                changed |= apply_offload(&params.device, offload, check_mode)?;
            }

            let msg = format!("ethtool settings applied to {}", params.device);
            Ok(ModuleResult::new(changed, None, Some(msg)))
        }
        State::Absent => {
            let changed = reset_device(&params.device, check_mode)?;

            if changed {
                diff(
                    format!("{} custom settings", params.device),
                    format!("{} reset to defaults", params.device),
                );
            }

            let msg = format!("ethtool settings reset for {}", params.device);
            Ok(ModuleResult::new(changed, None, Some(msg)))
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
            device: eth0
            speed: 1000
            duplex: full
            autoneg: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.speed, Some(1000));
        assert_eq!(params.duplex, Some(Duplex::Full));
        assert_eq!(params.autoneg, Some(true));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            state: query
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.state, State::Query);
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
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_offload() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
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
        let offload = params.offload.unwrap();
        assert_eq!(offload.rx, Some(true));
        assert_eq!(offload.tx, Some(true));
        assert_eq!(offload.tso, Some(false));
        assert_eq!(offload.gso, Some(true));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.state, State::Present);
        assert!(params.speed.is_none());
        assert!(params.duplex.is_none());
        assert!(params.autoneg.is_none());
        assert!(params.offload.is_none());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device() {
        assert!(validate_device("eth0").is_ok());
        assert!(validate_device("ens33").is_ok());
        assert!(validate_device("enp0s3").is_ok());
        assert!(validate_device("wlan0").is_ok());

        assert!(validate_device("").is_err());
        assert!(validate_device(&"a".repeat(16)).is_err());
        assert!(validate_device("eth 0").is_err());
        assert!(validate_device("eth/0").is_err());
    }

    #[test]
    fn test_validate_speed() {
        assert!(validate_speed(10).is_ok());
        assert!(validate_speed(100).is_ok());
        assert!(validate_speed(1000).is_ok());
        assert!(validate_speed(10000).is_ok());
        assert!(validate_speed(25000).is_ok());
        assert!(validate_speed(40000).is_ok());
        assert!(validate_speed(100000).is_ok());

        assert!(validate_speed(42).is_err());
        assert!(validate_speed(999).is_err());
    }

    #[test]
    fn test_parse_params_speed_without_duplex() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            speed: 1000
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = ethtool_exec(params, true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("duplex is required"));
    }

    #[test]
    fn test_parse_params_duplex_without_speed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            duplex: full
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = ethtool_exec(params, true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("speed is required"));
    }
}
