/// ANCHOR: module
/// # conntrack
///
/// Manage Linux connection tracking table entries. Essential for container
/// networking, firewall troubleshooting, and IoT network management.
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
/// - name: Flush all connection tracking entries
///   conntrack:
///     flush: true
///
/// - name: Drop connections from specific IP
///   conntrack:
///     source: 10.0.0.1
///     state: absent
///
/// - name: Drop connections to specific IP and port
///   conntrack:
///     destination: 192.168.1.100
///     protocol: tcp
///     port: 443
///     state: absent
///
/// - name: Drop UDP connections from a subnet
///   conntrack:
///     source: 10.0.0.0/24
///     protocol: udp
///     state: absent
///
/// - name: List connections from specific IP
///   conntrack:
///     source: 10.0.0.1
///     state: list
///
/// - name: Drop connections from source to destination
///   conntrack:
///     source: 10.0.0.1
///     destination: 192.168.1.100
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Flush all connection tracking entries.
    /// **[default: `false`]**
    pub flush: Option<bool>,
    /// Source IP address or CIDR to filter connections.
    pub source: Option<String>,
    /// Destination IP address or CIDR to filter connections.
    pub destination: Option<String>,
    /// Network protocol to filter (tcp, udp, icmp, sctp, dccp, gre).
    pub protocol: Option<String>,
    /// Port number to filter (used with protocol).
    pub port: Option<u16>,
    /// Source port number to filter.
    pub source_port: Option<u16>,
    /// Whether to list entries or delete matching entries.
    /// **[default: `"absent"`]**
    pub state: Option<State>,
    /// Connection state to filter (e.g., ESTABLISHED, TIME_WAIT, CLOSE, SYN_SENT).
    pub conn_state: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    List,
}

fn run_conntrack_cmd(args: &[&str]) -> Result<String> {
    let output = Command::new("conntrack").args(args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute conntrack: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "conntrack failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_filter_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(source) = &params.source {
        args.push("-s".to_string());
        args.push(source.clone());
    }

    if let Some(destination) = &params.destination {
        args.push("-d".to_string());
        args.push(destination.clone());
    }

    if let Some(protocol) = &params.protocol {
        args.push("-p".to_string());
        args.push(protocol.clone());
    }

    if let Some(port) = &params.port {
        args.push("--dport".to_string());
        args.push(port.to_string());
    }

    if let Some(source_port) = &params.source_port {
        args.push("--sport".to_string());
        args.push(source_port.to_string());
    }

    if let Some(conn_state) = &params.conn_state {
        args.push("-e".to_string());
        args.push(conn_state.clone());
    }

    args
}

fn build_description(params: &Params) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(source) = &params.source {
        parts.push(format!("src={}", source));
    }

    if let Some(destination) = &params.destination {
        parts.push(format!("dst={}", destination));
    }

    if let Some(protocol) = &params.protocol {
        parts.push(format!("proto={}", protocol));
    }

    if let Some(port) = &params.port {
        parts.push(format!("dport={}", port));
    }

    if let Some(source_port) = &params.source_port {
        parts.push(format!("sport={}", source_port));
    }

    if let Some(conn_state) = &params.conn_state {
        parts.push(format!("state={}", conn_state));
    }

    if parts.is_empty() {
        "all entries".to_string()
    } else {
        parts.join(", ")
    }
}

fn flush_entries(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        info!("Would flush all connection tracking entries");
        return Ok(ModuleResult::new(
            true,
            None,
            Some("Would flush all connection tracking entries".to_string()),
        ));
    }

    run_conntrack_cmd(&["-F"])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some("Flushed all connection tracking entries".to_string()),
    ))
}

fn list_entries(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let desc = build_description(params);

    if check_mode {
        info!("Would list connection tracking entries: {}", desc);
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Would list entries: {}", desc)),
        ));
    }

    let mut args = vec!["-L"];
    let filter_args = build_filter_args(params);
    let filter_str: Vec<&str> = filter_args.iter().map(|s| s.as_str()).collect();
    args.extend(filter_str);

    let output = run_conntrack_cmd(&args)?;

    let entries: Vec<serde_json::Value> = if output.is_empty() {
        Vec::new()
    } else {
        output
            .lines()
            .map(|line| serde_json::Value::String(line.to_string()))
            .collect()
    };

    let entry_count = entries.len();

    let extra = serde_norway::to_value(serde_json::json!({
        "entries": entries,
        "count": entry_count,
    }))
    .ok();

    Ok(ModuleResult::new(
        false,
        extra,
        Some(format!("Found {} entries: {}", entry_count, desc)),
    ))
}

fn delete_entries(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let desc = build_description(params);

    let filter_args = build_filter_args(params);
    if filter_args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "At least one filter parameter (source, destination, protocol, port, source_port, or conn_state) is required when state is 'absent'",
        ));
    }

    if check_mode {
        info!("Would delete connection tracking entries: {}", desc);
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete entries: {}", desc)),
        ));
    }

    let mut args = vec!["-D"];
    let filter_str: Vec<&str> = filter_args.iter().map(|s| s.as_str()).collect();
    args.extend(filter_str);

    let output = run_conntrack_cmd(&args)?;

    let msg = if output.is_empty() {
        format!("No matching entries found: {}", desc)
    } else {
        format!("Deleted entries: {}", desc)
    };

    let changed = !output.is_empty() && !output.contains("0 entries");

    Ok(ModuleResult::new(changed, None, Some(msg)))
}

fn validate_params(params: &Params) -> Result<()> {
    if params.flush.unwrap_or(false)
        && (params.source.is_some()
            || params.destination.is_some()
            || params.protocol.is_some()
            || params.port.is_some()
            || params.source_port.is_some()
            || params.conn_state.is_some())
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Cannot specify filter parameters when 'flush' is true",
        ));
    }

    if params.port.is_some() && params.protocol.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "'protocol' is required when 'port' is specified",
        ));
    }

    if params.source_port.is_some() && params.protocol.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "'protocol' is required when 'source_port' is specified",
        ));
    }

    Ok(())
}

pub fn conntrack(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_params(&params)?;

    if params.flush.unwrap_or(false) {
        return flush_entries(check_mode);
    }

    match params.state.unwrap_or(State::Absent) {
        State::List => list_entries(&params, check_mode),
        State::Absent => delete_entries(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Conntrack;

impl Module for Conntrack {
    fn get_name(&self) -> &str {
        "conntrack"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((conntrack(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_flush() {
        let yaml: YamlValue = serde_norway::from_str(r#"flush: true"#).unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.flush, Some(true));
    }

    #[test]
    fn test_parse_params_source_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.source, Some("10.0.0.1".to_string()));
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            destination: 192.168.1.100
            protocol: tcp
            port: 443
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.source, Some("10.0.0.1".to_string()));
        assert_eq!(params.destination, Some("192.168.1.100".to_string()));
        assert_eq!(params.protocol, Some("tcp".to_string()));
        assert_eq!(params.port, Some(443));
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            state: list
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::List));
    }

    #[test]
    fn test_parse_params_with_conn_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            conn_state: ESTABLISHED
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.conn_state, Some("ESTABLISHED".to_string()));
    }

    #[test]
    fn test_parse_params_with_source_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            protocol: udp
            source_port: 12345
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.source_port, Some(12345));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 10.0.0.1
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_filter_args_source() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: None,
            conn_state: None,
        };
        let args = build_filter_args(&params);
        assert_eq!(args, vec!["-s", "10.0.0.1"]);
    }

    #[test]
    fn test_build_filter_args_full() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: Some("192.168.1.100".to_string()),
            protocol: Some("tcp".to_string()),
            port: Some(443),
            source_port: None,
            state: None,
            conn_state: None,
        };
        let args = build_filter_args(&params);
        assert!(args.contains(&"-s".to_string()));
        assert!(args.contains(&"10.0.0.1".to_string()));
        assert!(args.contains(&"-d".to_string()));
        assert!(args.contains(&"192.168.1.100".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"tcp".to_string()));
        assert!(args.contains(&"--dport".to_string()));
        assert!(args.contains(&"443".to_string()));
    }

    #[test]
    fn test_build_description_all() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: Some("192.168.1.100".to_string()),
            protocol: Some("tcp".to_string()),
            port: Some(443),
            source_port: None,
            state: None,
            conn_state: Some("ESTABLISHED".to_string()),
        };
        let desc = build_description(&params);
        assert!(desc.contains("src=10.0.0.1"));
        assert!(desc.contains("dst=192.168.1.100"));
        assert!(desc.contains("proto=tcp"));
        assert!(desc.contains("dport=443"));
        assert!(desc.contains("state=ESTABLISHED"));
    }

    #[test]
    fn test_build_description_empty() {
        let params = Params {
            flush: None,
            source: None,
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: None,
            conn_state: None,
        };
        let desc = build_description(&params);
        assert_eq!(desc, "all entries");
    }

    #[test]
    fn test_validate_params_flush_with_filters() {
        let params = Params {
            flush: Some(true),
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: None,
            conn_state: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("flush"));
    }

    #[test]
    fn test_validate_params_port_without_protocol() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: Some(443),
            source_port: None,
            state: None,
            conn_state: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("protocol"));
    }

    #[test]
    fn test_validate_params_source_port_without_protocol() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: None,
            source_port: Some(12345),
            state: None,
            conn_state: None,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("protocol"));
    }

    #[test]
    fn test_validate_params_valid() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: Some("tcp".to_string()),
            port: Some(443),
            source_port: None,
            state: Some(State::Absent),
            conn_state: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_flush_only() {
        let params = Params {
            flush: Some(true),
            source: None,
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: None,
            conn_state: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_conntrack_flush_check_mode() {
        let params = Params {
            flush: Some(true),
            source: None,
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: None,
            conn_state: None,
        };
        let result = conntrack(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would flush"));
    }

    #[test]
    fn test_conntrack_delete_no_filters() {
        let params = Params {
            flush: None,
            source: None,
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: Some(State::Absent),
            conn_state: None,
        };
        let error = conntrack(params, true).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("filter parameter"));
    }

    #[test]
    fn test_conntrack_delete_check_mode() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: Some(State::Absent),
            conn_state: None,
        };
        let result = conntrack(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would delete"));
    }

    #[test]
    fn test_conntrack_list_check_mode() {
        let params = Params {
            flush: None,
            source: Some("10.0.0.1".to_string()),
            destination: None,
            protocol: None,
            port: None,
            source_port: None,
            state: Some(State::List),
            conn_state: None,
        };
        let result = conntrack(params, true).unwrap();
        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("Would list"));
    }
}
