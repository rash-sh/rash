/// ANCHOR: module
/// # hostname
///
/// Manage system hostname.
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
/// - name: Set hostname
///   hostname:
///     name: web01
///
/// - name: Set hostname using systemd
///   hostname:
///     name: web01
///     use: systemd
///
/// - name: Set hostname from inventory
///   hostname:
///     name: "{{ inventory_hostname }}"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const ETC_HOSTNAME: &str = "/etc/hostname";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// Use hostnamectl (systemd)
    Systemd,
    /// Write directly to /etc/hostname
    Generic,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the host.
    name: String,
    /// Which strategy to use to update the hostname.
    /// If not set, auto-detects based on system capabilities.
    #[serde(rename = "use")]
    use_: Option<Strategy>,
}

#[derive(Debug)]
pub struct Hostname;

impl Module for Hostname {
    fn get_name(&self) -> &str {
        "hostname"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            set_hostname(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn validate_hostname(hostname: &str) -> Result<()> {
    if hostname.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Hostname cannot be empty",
        ));
    }

    if hostname.len() > 253 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Hostname too long (max 253 characters)",
        ));
    }

    for label in hostname.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid hostname label: {}", label),
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Hostname label cannot start or end with hyphen: {}", label),
            ));
        }
        for c in label.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid character '{}' in hostname", c),
                ));
            }
        }
    }

    Ok(())
}

fn get_current_hostname() -> Result<String> {
    let output = Command::new("hostname")
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to get hostname: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn has_systemd() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn detect_strategy() -> Strategy {
    if has_systemd() {
        Strategy::Systemd
    } else {
        Strategy::Generic
    }
}

fn set_hostname_systemd(hostname: &str, check_mode: bool) -> Result<()> {
    if check_mode {
        return Ok(());
    }

    let output = Command::new("hostnamectl")
        .args(["set-hostname", hostname])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to set hostname: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn set_hostname_generic(hostname: &str, check_mode: bool) -> Result<()> {
    if check_mode {
        return Ok(());
    }

    fs::write(ETC_HOSTNAME, hostname).map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let output = Command::new("hostname")
        .args(["-F", ETC_HOSTNAME])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to set hostname: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn set_hostname(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_hostname(&params.name)?;

    let current = get_current_hostname()?;

    if current == params.name {
        return Ok(ModuleResult::new(false, None, None));
    }

    let strategy = params.use_.unwrap_or_else(detect_strategy);

    diff(current.clone(), params.name.clone());

    match strategy {
        Strategy::Systemd => set_hostname_systemd(&params.name, check_mode)?,
        Strategy::Generic => set_hostname_generic(&params.name, check_mode)?,
    }

    let output = format!("Set hostname to {}", params.name);

    Ok(ModuleResult::new(true, None, Some(output)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web01
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "web01".to_owned(),
                use_: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_strategy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web01
            use: systemd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "web01".to_owned(),
                use_: Some(Strategy::Systemd),
            }
        );
    }

    #[test]
    fn test_parse_params_generic_strategy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web01
            use: generic
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "web01".to_owned(),
                use_: Some(Strategy::Generic),
            }
        );
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web01
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_hostname() {
        assert!(validate_hostname("web01").is_ok());
        assert!(validate_hostname("web01.example.com").is_ok());
        assert!(validate_hostname("my-host").is_ok());
        assert!(validate_hostname("my-host.example.com").is_ok());

        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-invalid").is_err());
        assert!(validate_hostname("invalid-").is_err());
        assert!(validate_hostname("invalid host").is_err());
        assert!(validate_hostname(&"a".repeat(254)).is_err());
    }

    #[test]
    fn test_validate_hostname_labels() {
        assert!(validate_hostname("a").is_ok());
        assert!(validate_hostname(&"a".repeat(63)).is_ok());
        assert!(validate_hostname(&"a".repeat(64)).is_err());
    }
}
