/// ANCHOR: module
/// # route
///
/// Manage network routing tables using ip route commands.
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
/// - name: Add default gateway
///   route:
///     destination: "0.0.0.0/0"
///     gateway: "192.168.1.1"
///
/// - name: Add static route via specific interface
///   route:
///     destination: "10.0.0.0/24"
///     gateway: "192.168.1.1"
///     interface: eth0
///
/// - name: Add route with metric
///   route:
///     destination: "172.16.0.0/16"
///     gateway: "10.0.0.1"
///     metric: 100
///
/// - name: Add route to specific routing table
///   route:
///     destination: "192.168.2.0/24"
///     gateway: "192.168.1.2"
///     table: 100
///
/// - name: Remove a route
///   route:
///     destination: "10.0.0.0/24"
///     gateway: "192.168.1.1"
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The destination network address (e.g., "192.168.0.0/24" or "default").
    pub destination: String,
    /// The gateway IP address for the route.
    pub gateway: Option<String>,
    /// Whether the route should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// The network interface for the route (e.g., eth0).
    pub interface: Option<String>,
    /// Route metric value (lower is preferred).
    pub metric: Option<u32>,
    /// Routing table ID or name.
    pub table: Option<String>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn build_route_spec(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("table".to_string());
        args.push(table.clone());
    }

    args.push(params.destination.clone());

    if let Some(gateway) = &params.gateway {
        args.push("via".to_string());
        args.push(gateway.clone());
    }

    if let Some(interface) = &params.interface {
        args.push("dev".to_string());
        args.push(interface.clone());
    }

    if let Some(metric) = params.metric {
        args.push("metric".to_string());
        args.push(metric.to_string());
    }

    args
}

fn route_exists(params: &Params) -> Result<bool> {
    let mut args = vec!["route".to_string(), "show".to_string()];

    if let Some(table) = &params.table {
        args.push("table".to_string());
        args.push(table.clone());
    }

    args.push("to".to_string());
    args.push(params.destination.clone());

    if let Some(gateway) = &params.gateway {
        args.push("via".to_string());
        args.push(gateway.clone());
    }

    let output = Command::new("ip").args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ip route show: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "ip route show failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut matching_lines = stdout.lines().filter(|line| !line.is_empty());

    if let Some(interface) = &params.interface {
        matching_lines = matching_lines
            .filter(|line| line.contains(&format!("dev {interface}")))
            .collect::<Vec<_>>()
            .into_iter();
    }

    if let Some(metric) = params.metric {
        matching_lines = matching_lines
            .filter(|line| line.contains(&format!("metric {metric}")))
            .collect::<Vec<_>>()
            .into_iter();
    }

    Ok(matching_lines.count() > 0)
}

fn get_current_routes(params: &Params) -> Result<String> {
    let mut args = vec!["route".to_string(), "show".to_string()];

    if let Some(table) = &params.table {
        args.push("table".to_string());
        args.push(table.clone());
    }

    args.push("to".to_string());
    args.push(params.destination.clone());

    let output = Command::new("ip").args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ip route show: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "ip route show failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn add_route(params: &Params) -> Result<()> {
    let mut args = vec!["route".to_string(), "add".to_string()];
    args.extend(build_route_spec(params));

    let output = Command::new("ip").args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ip route add: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add route: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn delete_route(params: &Params) -> Result<()> {
    let mut args = vec!["route".to_string(), "del".to_string()];
    args.extend(build_route_spec(params));

    let output = Command::new("ip").args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ip route del: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete route: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

pub fn route(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();

    match state {
        State::Present => {
            let exists = route_exists(&params)?;
            if exists {
                let current = get_current_routes(&params)?;
                return Ok(ModuleResult::new(
                    false,
                    Some(serde_norway::to_value(&serde_json::json!({
                        "routes": current,
                    }))
                    .unwrap_or(None)),
                    None,
                ));
            }

            if check_mode {
                info!(
                    "Would add route: {} via {:?}",
                    params.destination, params.gateway
                );
                return Ok(ModuleResult::new(true, None, None));
            }

            add_route(&params)?;
            let current = get_current_routes(&params)?;
            Ok(ModuleResult::new(
                true,
                Some(serde_norway::to_value(&serde_json::json!({
                    "routes": current,
                }))
                .unwrap_or(None)),
                None,
            ))
        }
        State::Absent => {
            let exists = route_exists(&params)?;
            if !exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if check_mode {
                info!(
                    "Would delete route: {} via {:?}",
                    params.destination, params.gateway
                );
                return Ok(ModuleResult::new(true, None, None));
            }

            delete_route(&params)?;
            Ok(ModuleResult::new(true, None, None))
        }
    }
}

#[derive(Debug)]
pub struct Route;

impl Module for Route {
    fn get_name(&self) -> &str {
        "route"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((route(parse_params(optional_params)?, check_mode)?, None))
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
            destination: "0.0.0.0/0"
            gateway: "192.168.1.1"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.destination, "0.0.0.0/0");
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(params.state, None);
        assert_eq!(params.interface, None);
        assert_eq!(params.metric, None);
        assert_eq!(params.table, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "10.0.0.0/24"
            gateway: "192.168.1.1"
            state: present
            interface: eth0
            metric: 100
            table: "100"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.destination, "10.0.0.0/24");
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.interface, Some("eth0".to_string()));
        assert_eq!(params.metric, Some(100));
        assert_eq!(params.table, Some("100".to_string()));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "10.0.0.0/24"
            gateway: "192.168.1.1"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_default_route() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "default"
            gateway: "10.0.0.1"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.destination, "default");
        assert_eq!(params.gateway, Some("10.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_params_no_gateway() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "169.254.0.0/16"
            interface: eth0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.destination, "169.254.0.0/16");
        assert_eq!(params.gateway, None);
        assert_eq!(params.interface, Some("eth0".to_string()));
    }

    #[test]
    fn test_parse_params_state_default() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "192.168.1.0/24"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
        assert_eq!(State::default(), State::Present);
    }

    #[test]
    fn test_build_route_spec_basic() {
        let params = Params {
            destination: "10.0.0.0/24".to_string(),
            gateway: Some("192.168.1.1".to_string()),
            state: None,
            interface: None,
            metric: None,
            table: None,
        };
        let args = build_route_spec(&params);
        assert_eq!(args, vec!["10.0.0.0/24", "via", "192.168.1.1"]);
    }

    #[test]
    fn test_build_route_spec_full() {
        let params = Params {
            destination: "172.16.0.0/16".to_string(),
            gateway: Some("10.0.0.1".to_string()),
            state: None,
            interface: Some("eth0".to_string()),
            metric: Some(100),
            table: Some("100".to_string()),
        };
        let args = build_route_spec(&params);
        assert_eq!(
            args,
            vec![
                "table", "100", "172.16.0.0/16", "via", "10.0.0.1", "dev",
                "eth0", "metric", "100",
            ]
        );
    }

    #[test]
    fn test_build_route_spec_no_gateway() {
        let params = Params {
            destination: "169.254.0.0/16".to_string(),
            gateway: None,
            state: None,
            interface: Some("eth0".to_string()),
            metric: None,
            table: None,
        };
        let args = build_route_spec(&params);
        assert_eq!(args, vec!["169.254.0.0/16", "dev", "eth0"]);
    }

    #[test]
    fn test_build_route_spec_default() {
        let params = Params {
            destination: "default".to_string(),
            gateway: Some("192.168.1.1".to_string()),
            state: None,
            interface: None,
            metric: None,
            table: None,
        };
        let args = build_route_spec(&params);
        assert_eq!(args, vec!["default", "via", "192.168.1.1"]);
    }

    #[test]
    fn test_parse_params_deny_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            destination: "10.0.0.0/24"
            unknown_field: "value"
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }
}
