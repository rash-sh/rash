/// ANCHOR: module
/// # firewalld
///
/// Manage firewall rules using firewalld.
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
/// - name: Allow HTTP traffic
///   firewalld:
///     service: http
///     zone: public
///     state: enabled
///     permanent: true
///     immediate: true
///
/// - name: Allow port 8080/tcp
///   firewalld:
///     port: 8080/tcp
///     zone: public
///     state: enabled
///     permanent: true
///
/// - name: Block HTTPS traffic
///   firewalld:
///     service: https
///     zone: public
///     state: disabled
///     permanent: true
///     immediate: true
///
/// - name: Allow port range
///   firewalld:
///     port: 8000-8005/tcp
///     zone: public
///     state: enabled
///     permanent: true
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Firewall zone to operate on.
    /// **[default: `default` from system]**
    pub zone: Option<String>,
    /// Service to allow or block (e.g., http, https, ssh).
    pub service: Option<String>,
    /// Port to allow or block (e.g., 8080/tcp, 53/udp).
    pub port: Option<String>,
    /// Whether the rule should be enabled or disabled.
    pub state: State,
    /// Make the change permanent (survive reboots).
    /// **[default: `false`]**
    pub permanent: Option<bool>,
    /// Apply the change immediately without requiring a reload.
    /// **[default: `false`]**
    pub immediate: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Enabled,
    Disabled,
}

#[derive(Debug)]
pub struct Firewalld;

impl Module for Firewalld {
    fn get_name(&self) -> &str {
        "firewalld"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((firewalld(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct FirewalldClient {
    check_mode: bool,
}

impl FirewalldClient {
    pub fn new(check_mode: bool) -> Self {
        FirewalldClient { check_mode }
    }

    fn run_cmd(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("firewall-cmd")
            .args(args)
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute firewall-cmd: {e}"),
                )
            })?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "firewall-cmd failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn is_service_enabled(&self, zone: &str, service: &str, permanent: bool) -> Result<bool> {
        let mut args = vec!["--zone", zone, "--query-service", service];
        if permanent {
            args.insert(0, "--permanent");
        }

        let output = Command::new("firewall-cmd")
            .args(&args)
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute firewall-cmd: {e}"),
                )
            })?;

        Ok(output.status.success())
    }

    pub fn is_port_enabled(&self, zone: &str, port: &str, permanent: bool) -> Result<bool> {
        let mut args = vec!["--zone", zone, "--query-port", port];
        if permanent {
            args.insert(0, "--permanent");
        }

        let output = Command::new("firewall-cmd")
            .args(&args)
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute firewall-cmd: {e}"),
                )
            })?;

        Ok(output.status.success())
    }

    pub fn get_default_zone(&self) -> Result<String> {
        self.run_cmd(&["--get-default-zone"])
    }

    pub fn set_service(
        &self,
        zone: &str,
        service: &str,
        state: &State,
        permanent: bool,
        immediate: bool,
    ) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let state_arg = match state {
            State::Enabled => "--add-service",
            State::Disabled => "--remove-service",
        };

        if permanent && immediate {
            self.run_cmd(&["--permanent", "--zone", zone, state_arg, service])?;
            self.run_cmd(&["--zone", zone, state_arg, service])?;
        } else if permanent {
            self.run_cmd(&["--permanent", "--zone", zone, state_arg, service])?;
        } else {
            self.run_cmd(&["--zone", zone, state_arg, service])?;
        }

        Ok(())
    }

    pub fn set_port(
        &self,
        zone: &str,
        port: &str,
        state: &State,
        permanent: bool,
        immediate: bool,
    ) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let state_arg = match state {
            State::Enabled => "--add-port",
            State::Disabled => "--remove-port",
        };

        if permanent && immediate {
            self.run_cmd(&["--permanent", "--zone", zone, state_arg, port])?;
            self.run_cmd(&["--zone", zone, state_arg, port])?;
        } else if permanent {
            self.run_cmd(&["--permanent", "--zone", zone, state_arg, port])?;
        } else {
            self.run_cmd(&["--zone", zone, state_arg, port])?;
        }

        Ok(())
    }
}

fn validate_params(params: &Params) -> Result<()> {
    if params.service.is_none() && params.port.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Either 'service' or 'port' is required",
        ));
    }

    if params.service.is_some() && params.port.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Only one of 'service' or 'port' can be specified, not both",
        ));
    }

    if let Some(port) = &params.port
        && !port.contains('/')
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Port '{}' must include protocol (e.g., 8080/tcp or 53/udp)",
                port
            ),
        ));
    }

    Ok(())
}

pub fn firewalld(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_params(&params)?;

    let client = FirewalldClient::new(check_mode);
    let permanent = params.permanent.unwrap_or(false);
    let immediate = params.immediate.unwrap_or(false);

    let zone = match &params.zone {
        Some(z) => z.clone(),
        None => client.get_default_zone()?,
    };

    let is_enabled = if let Some(service) = &params.service {
        client.is_service_enabled(&zone, service, permanent)?
    } else if let Some(port) = &params.port {
        client.is_port_enabled(&zone, port, permanent)?
    } else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Either 'service' or 'port' is required",
        ));
    };

    let desired_enabled = matches!(params.state, State::Enabled);

    if is_enabled == desired_enabled {
        let msg = if let Some(service) = &params.service {
            format!("service '{}' in zone '{}'", service, zone)
        } else {
            format!(
                "port '{}' in zone '{}'",
                params.port.as_ref().unwrap(),
                zone
            )
        };
        return Ok(ModuleResult::new(false, None, Some(msg)));
    }

    if let Some(service) = &params.service {
        client.set_service(&zone, service, &params.state, permanent, immediate)?;
    } else if let Some(port) = &params.port {
        client.set_port(&zone, port, &params.state, permanent, immediate)?;
    }

    let extra = serde_norway::to_value(serde_json::json!({
        "zone": zone,
        "service": params.service,
        "port": params.port,
        "state": if matches!(params.state, State::Enabled) { "enabled" } else { "disabled" },
        "permanent": permanent,
        "immediate": immediate,
    }))
    .ok();

    let msg = if let Some(service) = &params.service {
        format!(
            "service '{}' {} in zone '{}'",
            service,
            if matches!(params.state, State::Enabled) {
                "enabled"
            } else {
                "disabled"
            },
            zone
        )
    } else {
        format!(
            "port '{}' {} in zone '{}'",
            params.port.as_ref().unwrap(),
            if matches!(params.state, State::Enabled) {
                "enabled"
            } else {
                "disabled"
            },
            zone
        )
    };

    Ok(ModuleResult::new(true, extra, Some(msg)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_service() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            service: http
            zone: public
            state: enabled
            permanent: true
            immediate: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.service, Some("http".to_owned()));
        assert_eq!(params.zone, Some("public".to_owned()));
        assert_eq!(params.state, State::Enabled);
        assert_eq!(params.permanent, Some(true));
        assert_eq!(params.immediate, Some(true));
    }

    #[test]
    fn test_parse_params_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: 8080/tcp
            zone: public
            state: disabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.port, Some("8080/tcp".to_owned()));
        assert_eq!(params.zone, Some("public".to_owned()));
        assert_eq!(params.state, State::Disabled);
        assert_eq!(params.permanent, None);
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            service: ssh
            state: enabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.service, Some("ssh".to_owned()));
        assert_eq!(params.state, State::Enabled);
        assert_eq!(params.zone, None);
        assert_eq!(params.permanent, None);
        assert_eq!(params.immediate, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            service: http
            state: enabled
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_no_service_or_port() {
        let params = Params {
            zone: Some("public".to_owned()),
            service: None,
            port: None,
            state: State::Enabled,
            permanent: None,
            immediate: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("Either 'service' or 'port' is required")
        );
    }

    #[test]
    fn test_validate_params_both_service_and_port() {
        let params = Params {
            zone: Some("public".to_owned()),
            service: Some("http".to_owned()),
            port: Some("8080/tcp".to_owned()),
            state: State::Enabled,
            permanent: None,
            immediate: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .contains("Only one of 'service' or 'port'")
        );
    }

    #[test]
    fn test_validate_params_port_without_protocol() {
        let params = Params {
            zone: Some("public".to_owned()),
            service: None,
            port: Some("8080".to_owned()),
            state: State::Enabled,
            permanent: None,
            immediate: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("must include protocol"));
    }

    #[test]
    fn test_validate_params_valid() {
        let params = Params {
            zone: Some("public".to_owned()),
            service: Some("http".to_owned()),
            port: None,
            state: State::Enabled,
            permanent: Some(true),
            immediate: None,
        };
        assert!(validate_params(&params).is_ok());
    }
}
