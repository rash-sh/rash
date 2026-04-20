/// ANCHOR: module
/// # ethtool
///
/// Manage Ethernet device settings using ethtool.
///
/// Useful for IoT devices, servers, and container hosts needing
/// fine-tuned network interface configuration including link speed,
/// duplex mode, auto-negotiation, and offload features.
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
/// - name: Set interface speed and duplex
///   ethtool:
///     device: eth0
///     speed: 1000
///     duplex: full
///     autoneg: off
///
/// - name: Enable auto-negotiation
///   ethtool:
///     device: eth0
///     autoneg: on
///
/// - name: Configure offload features
///   ethtool:
///     device: eth0
///     offload:
///       rx: "on"
///       tx: "on"
///       tso: "off"
///       gso: "on"
///
/// - name: Query current interface settings
///   ethtool:
///     device: eth0
///     state: query
///   register: eth_settings
///
/// - name: Reset interface to defaults
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

use std::collections::HashMap;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Network interface name (e.g. eth0, ens33).
    pub device: String,
    /// Whether the settings should be present, absent (reset), or queried.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Link speed in Mbps. Valid values: 10, 100, 1000, 2500, 5000, 10000,
    /// 25000, 40000, 50000, 100000.
    pub speed: Option<u32>,
    /// Duplex mode.
    pub duplex: Option<Duplex>,
    /// Auto-negotiation setting.
    pub autoneg: Option<Autoneg>,
    /// Offload feature settings. Keys are feature names (rx, tx, tso, gso,
    /// gro, lro) and values are "on" or "off".
    pub offload: Option<HashMap<String, String>>,
    /// Generic feature settings. Keys are feature names and values are
    /// "on" or "off".
    pub features: Option<HashMap<String, String>>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
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
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Autoneg {
    On,
    Off,
}

fn run_ethtool(args: &[&str]) -> Result<String> {
    let output = Command::new("ethtool")
        .args(args)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute ethtool: {e}"),
            )
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "ethtool {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}

fn parse_ethtool_output(output: &str) -> HashMap<String, String> {
    let mut settings = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let value = trimmed[colon_pos + 1..].trim().to_string();
            settings.insert(key, value);
        }
    }
    settings
}

fn parse_ethtool_features_output(output: &str) -> HashMap<String, String> {
    let mut features = HashMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let value = trimmed[colon_pos + 1..].trim().to_string();
            if value == "on" || value == "off" {
                features.insert(key, value);
            }
        }
    }
    features
}

fn validate_speed_duplex(params: &Params) -> Result<()> {
    if params.speed.is_some() || params.duplex.is_some() {
        if let Some(Autoneg::On) = &params.autoneg {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Cannot set speed or duplex when autoneg is on",
            ));
        }
    }
    if params.speed.is_some() && params.duplex.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "duplex is required when speed is set",
        ));
    }
    if params.duplex.is_some() && params.speed.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "speed is required when duplex is set",
        ));
    }
    Ok(())
}

fn validate_offload_values(offload: &HashMap<String, String>) -> Result<()> {
    for (key, value) in offload {
        if value != "on" && value != "off" {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("offload feature '{}' must be 'on' or 'off', got '{}'", key, value),
            ));
        }
    }
    Ok(())
}

fn validate_feature_values(features: &HashMap<String, String>) -> Result<()> {
    for (key, value) in features {
        if value != "on" && value != "off" {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("feature '{}' must be 'on' or 'off', got '{}'", key, value),
            ));
        }
    }
    Ok(())
}

fn ethtool_query(params: Params) -> Result<ModuleResult> {
    let output = run_ethtool(&[&params.device])?;
    let settings = parse_ethtool_output(&output);

    let features_output = run_ethtool(&["-k", &params.device])?;
    let features = parse_ethtool_features_output(&features_output);

    let extra = Some(json!({
        "device": params.device,
        "settings": settings,
        "features": features,
    }).into());

    Ok(ModuleResult::new(false, extra, Some(output.trim().to_string())))
}

fn ethtool_present(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_speed_duplex(&params)?;

    if let Some(ref offload) = params.offload {
        validate_offload_values(offload)?;
    }
    if let Some(ref features) = params.features {
        validate_feature_values(features)?;
    }

    let mut changed = false;
    let mut changes: Vec<String> = Vec::new();

    if params.speed.is_some() || params.duplex.is_some() || params.autoneg.is_some() {
        let mut args = vec!["-s", &params.device];

        if let Some(speed) = params.speed {
            args.push("speed");
            args.push(&speed.to_string().leak());
        }
        if let Some(ref duplex) = params.duplex {
            let duplex_str = match duplex {
                Duplex::Half => "half",
                Duplex::Full => "full",
            };
            args.push("duplex");
            args.push(duplex_str);
        }
        if let Some(ref autoneg) = params.autoneg {
            let autoneg_str = match autoneg {
                Autoneg::On => "on",
                Autoneg::Off => "off",
            };
            args.push("autoneg");
            args.push(autoneg_str);
        }

        if !check_mode {
            run_ethtool(&args)?;
        }
        changed = true;
        changes.push(format!(
            "link settings: speed={:?} duplex={:?} autoneg={:?}",
            params.speed, params.duplex, params.autoneg
        ));
    }

    if let Some(ref offload) = params.offload {
        let mut args = vec!["-K", &params.device];
        let mut offload_changes = Vec::new();
        for (key, value) in offload {
            args.push(key.as_str());
            args.push(value.as_str());
            offload_changes.push(format!("{key}={value}"));
        }

        if !check_mode {
            run_ethtool(&args)?;
        }
        changed = true;
        changes.push(format!("offload: {}", offload_changes.join(", ")));
    }

    if let Some(ref features) = params.features {
        let mut args = vec!["-K", &params.device];
        let mut feature_changes = Vec::new();
        for (key, value) in features {
            args.push(key.as_str());
            args.push(value.as_str());
            feature_changes.push(format!("{key}={value}"));
        }

        if !check_mode {
            run_ethtool(&args)?;
        }
        changed = true;
        changes.push(format!("features: {}", feature_changes.join(", ")));
    }

    let output = if changes.is_empty() {
        format!("No changes needed for {}", params.device)
    } else if check_mode {
        format!(
            "Would apply to {}: {}",
            params.device,
            changes.join("; ")
        )
    } else {
        format!("Applied to {}: {}", params.device, changes.join("; "))
    };

    Ok(ModuleResult::new(changed, None, Some(output)))
}

fn ethtool_absent(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Would reset {} to default settings",
                params.device
            )),
        ));
    }

    let mut args = vec!["-s", &params.device, "autoneg", "on"];
    run_ethtool(&args)?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Reset {} to default settings (autoneg on)", params.device)),
    ))
}

pub fn ethtool(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();

    match state {
        State::Query => ethtool_query(params),
        State::Present => ethtool_present(params, check_mode),
        State::Absent => ethtool_absent(params, check_mode),
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
        Ok((ethtool(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            speed: 1000
            duplex: full
            autoneg: off
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        assert_eq!(params.speed, Some(1000));
        assert_eq!(params.duplex, Some(Duplex::Full));
        assert_eq!(params.autoneg, Some(Autoneg::Off));
        assert_eq!(params.state, None);
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
        assert_eq!(params.device, "eth0");
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_offload() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            offload:
              rx: "on"
              tx: "on"
              tso: "off"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        let offload = params.offload.unwrap();
        assert_eq!(offload.get("rx").unwrap(), "on");
        assert_eq!(offload.get("tx").unwrap(), "on");
        assert_eq!(offload.get("tso").unwrap(), "off");
    }

    #[test]
    fn test_parse_params_with_features() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            features:
              rxvlan: "on"
              txvlan: "off"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "eth0");
        let features = params.features.unwrap();
        assert_eq!(features.get("rxvlan").unwrap(), "on");
        assert_eq!(features.get("txvlan").unwrap(), "off");
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: ens33
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "ens33");
        assert_eq!(params.state, None);
        assert_eq!(params.speed, None);
        assert_eq!(params.duplex, None);
        assert_eq!(params.autoneg, None);
        assert_eq!(params.offload, None);
        assert_eq!(params.features, None);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: eth0
            unknown: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_ethtool_output() {
        let output = "Settings for eth0:\n\tSupported ports: [ TP ]\n\tSupported link modes:   10baseT/Half 10baseT/Full\n\tSpeed: 1000Mb/s\n\tDuplex: Full\n\tAuto-negotiation: on\n\tPort: Twisted Pair\n";
        let settings = parse_ethtool_output(output);
        assert_eq!(settings.get("Speed").unwrap(), "1000Mb/s");
        assert_eq!(settings.get("Duplex").unwrap(), "Full");
        assert_eq!(settings.get("Auto-negotiation").unwrap(), "on");
    }

    #[test]
    fn test_parse_ethtool_features_output() {
        let output = "Features for eth0:\nrx-checksumming: on\ntx-checksumming: on\nscatter-gather: on\ntcp-segmentation-offload: on\ngeneric-segmentation-offload: on\ngeneric-receive-offload: on\n";
        let features = parse_ethtool_features_output(output);
        assert_eq!(features.get("rx-checksumming").unwrap(), "on");
        assert_eq!(features.get("tcp-segmentation-offload").unwrap(), "on");
    }

    #[test]
    fn test_validate_speed_duplex_ok() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: Some(Duplex::Full),
            autoneg: Some(Autoneg::Off),
            offload: None,
            features: None,
        };
        assert!(validate_speed_duplex(&params).is_ok());
    }

    #[test]
    fn test_validate_speed_duplex_with_autoneg_on() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: Some(Duplex::Full),
            autoneg: Some(Autoneg::On),
            offload: None,
            features: None,
        };
        let result = validate_speed_duplex(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("autoneg"));
    }

    #[test]
    fn test_validate_speed_without_duplex() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: Some(1000),
            duplex: None,
            autoneg: Some(Autoneg::Off),
            offload: None,
            features: None,
        };
        let result = validate_speed_duplex(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplex"));
    }

    #[test]
    fn test_validate_duplex_without_speed() {
        let params = Params {
            device: "eth0".to_string(),
            state: None,
            speed: None,
            duplex: Some(Duplex::Full),
            autoneg: Some(Autoneg::Off),
            offload: None,
            features: None,
        };
        let result = validate_speed_duplex(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("speed"));
    }

    #[test]
    fn test_validate_offload_values_ok() {
        let offload: HashMap<String, String> = [
            ("rx".to_string(), "on".to_string()),
            ("tx".to_string(), "off".to_string()),
        ]
        .into_iter()
        .collect();
        assert!(validate_offload_values(&offload).is_ok());
    }

    #[test]
    fn test_validate_offload_values_invalid() {
        let offload: HashMap<String, String> =
            [("rx".to_string(), "yes".to_string())].into_iter().collect();
        let result = validate_offload_values(&offload);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be 'on' or 'off'"));
    }

    #[test]
    fn test_validate_feature_values_ok() {
        let features: HashMap<String, String> =
            [("rxvlan".to_string(), "on".to_string())].into_iter().collect();
        assert!(validate_feature_values(&features).is_ok());
    }

    #[test]
    fn test_validate_feature_values_invalid() {
        let features: HashMap<String, String> =
            [("rxvlan".to_string(), "enabled".to_string())].into_iter().collect();
        let result = validate_feature_values(&features);
        assert!(result.is_err());
    }

    #[test]
    fn test_state_default() {
        assert_eq!(State::default(), State::Present);
    }

    #[test]
    fn test_ethtool_present_check_mode_no_changes() {
        let params = Params {
            device: "eth0".to_string(),
            state: Some(State::Present),
            speed: None,
            duplex: None,
            autoneg: None,
            offload: None,
            features: None,
        };
        let result = ethtool_present(params, true).unwrap();
        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("No changes needed"));
    }

    #[test]
    fn test_ethtool_present_check_mode_with_speed() {
        let params = Params {
            device: "eth0".to_string(),
            state: Some(State::Present),
            speed: Some(1000),
            duplex: Some(Duplex::Full),
            autoneg: Some(Autoneg::Off),
            offload: None,
            features: None,
        };
        let result = ethtool_present(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would apply"));
    }

    #[test]
    fn test_ethtool_present_check_mode_with_offload() {
        let params = Params {
            device: "eth0".to_string(),
            state: Some(State::Present),
            speed: None,
            duplex: None,
            autoneg: None,
            offload: Some(
                [("rx".to_string(), "on".to_string()), ("tx".to_string(), "off".to_string())]
                    .into_iter()
                    .collect(),
            ),
            features: None,
        };
        let result = ethtool_present(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("offload"));
    }

    #[test]
    fn test_ethtool_absent_check_mode() {
        let params = Params {
            device: "eth0".to_string(),
            state: Some(State::Absent),
            speed: None,
            duplex: None,
            autoneg: None,
            offload: None,
            features: None,
        };
        let result = ethtool_absent(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would reset"));
    }
}
