/// ANCHOR: module
/// # iptables
///
/// Manage iptables firewall rules.
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
///   iptables:
///     chain: INPUT
///     protocol: tcp
///     destination_port: "80"
///     jump: ACCEPT
///
/// - name: Allow established connections
///   iptables:
///     chain: INPUT
///     ctstate: ESTABLISHED,RELATED
///     jump: ACCEPT
///
/// - name: Allow traffic from specific source
///   iptables:
///     chain: INPUT
///     source: "192.168.1.0/24"
///     jump: ACCEPT
///
/// - name: NAT masquerade for outgoing traffic
///   iptables:
///     table: nat
///     chain: POSTROUTING
///     source: "10.0.0.0/24"
///     out_interface: eth0
///     jump: MASQUERADE
///
/// - name: Forward port 8080 to 80
///   iptables:
///     table: nat
///     chain: PREROUTING
///     in_interface: eth0
///     protocol: tcp
///     destination_port: "8080"
///     jump: DNAT
///     to_destination: "127.0.0.1:80"
///
/// - name: Remove a specific rule
///   iptables:
///     chain: INPUT
///     protocol: tcp
///     destination_port: "8080"
///     jump: ACCEPT
///     state: absent
///
/// - name: Set the policy for the INPUT chain
///   iptables:
///     chain: INPUT
///     policy: DROP
///
/// - name: Flush all rules in INPUT chain
///   iptables:
///     chain: INPUT
///     flush: true
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

const DEFAULT_IPTABLES_CMD: &str = "iptables";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The iptables chain to modify.
    pub chain: String,
    /// The iptables table to modify.
    /// **[default: `"filter"`]**
    pub table: Option<String>,
    /// Whether the rule should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Set the policy for the chain (ACCEPT, DROP, REJECT, etc.).
    pub policy: Option<String>,
    /// The protocol of the rule (tcp, udp, icmp, all).
    pub protocol: Option<String>,
    /// The source address/network.
    pub source: Option<String>,
    /// The destination address/network.
    pub destination: Option<String>,
    /// The source port.
    pub source_port: Option<String>,
    /// The destination port.
    pub destination_port: Option<String>,
    /// The jump target (ACCEPT, DROP, REJECT, LOG, etc.).
    pub jump: Option<String>,
    /// The target for DNAT/SNAT (e.g., "192.168.1.1:80").
    pub to_destination: Option<String>,
    /// The source for SNAT (e.g., "192.168.1.1").
    pub to_source: Option<String>,
    /// The ports for DNAT/SNAT (e.g., "8080-8090").
    pub to_ports: Option<String>,
    /// The input interface.
    pub in_interface: Option<String>,
    /// The output interface.
    pub out_interface: Option<String>,
    /// Connection tracking states (ESTABLISHED, RELATED, NEW, INVALID).
    pub ctstate: Option<String>,
    /// Match extensions (state, conntrack, etc.).
    #[serde(rename = "match")]
    pub match_ext: Option<String>,
    /// Append rule as a specific rule number (1-based).
    pub rule_num: Option<String>,
    /// Flush all rules in the chain.
    /// **[default: `false`]**
    pub flush: Option<bool>,
    /// Comment for the rule (requires iptables comment module).
    pub comment: Option<String>,
    /// The iptables command to use (iptables, ip6tables).
    /// **[default: `"iptables"`]**
    pub ip_version: Option<IpVersion>,
    /// Perform a flush before adding rules.
    /// **[default: `false`]**
    pub flush_all: Option<bool>,
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
pub enum IpVersion {
    #[default]
    Ipv4,
    Ipv6,
}

fn get_iptables_cmd(ip_version: &Option<IpVersion>) -> &'static str {
    match ip_version {
        Some(IpVersion::Ipv6) => "ip6tables",
        _ => DEFAULT_IPTABLES_CMD,
    }
}

fn build_rule_spec(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("-t".to_string());
        args.push(table.clone());
    }

    args.push("-A".to_string());
    args.push(params.chain.clone());

    if let Some(protocol) = &params.protocol {
        args.push("-p".to_string());
        args.push(protocol.clone());
    }

    if let Some(source) = &params.source {
        args.push("-s".to_string());
        args.push(source.clone());
    }

    if let Some(destination) = &params.destination {
        args.push("-d".to_string());
        args.push(destination.clone());
    }

    if let Some(source_port) = &params.source_port {
        args.push("--sport".to_string());
        args.push(source_port.clone());
    }

    if let Some(destination_port) = &params.destination_port {
        args.push("--dport".to_string());
        args.push(destination_port.clone());
    }

    if let Some(in_interface) = &params.in_interface {
        args.push("-i".to_string());
        args.push(in_interface.clone());
    }

    if let Some(out_interface) = &params.out_interface {
        args.push("-o".to_string());
        args.push(out_interface.clone());
    }

    if let Some(match_ext) = &params.match_ext {
        args.push("-m".to_string());
        args.push(match_ext.clone());
    }

    if let Some(ctstate) = &params.ctstate {
        if params.match_ext.is_none() {
            args.push("-m".to_string());
            args.push("conntrack".to_string());
        }
        args.push("--ctstate".to_string());
        args.push(ctstate.clone());
    }

    if let Some(comment) = &params.comment {
        if params.match_ext.is_none() && params.ctstate.is_none() {
            args.push("-m".to_string());
            args.push("comment".to_string());
        }
        args.push("--comment".to_string());
        args.push(format!("\"{comment}\""));
    }

    if let Some(jump) = &params.jump {
        args.push("-j".to_string());
        args.push(jump.clone());
    }

    if let Some(to_destination) = &params.to_destination {
        args.push("--to-destination".to_string());
        args.push(to_destination.clone());
    }

    if let Some(to_source) = &params.to_source {
        args.push("--to-source".to_string());
        args.push(to_source.clone());
    }

    if let Some(to_ports) = &params.to_ports {
        args.push("--to-ports".to_string());
        args.push(to_ports.clone());
    }

    args
}

fn build_check_spec(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("-t".to_string());
        args.push(table.clone());
    }

    args.push("-C".to_string());
    args.push(params.chain.clone());

    if let Some(protocol) = &params.protocol {
        args.push("-p".to_string());
        args.push(protocol.clone());
    }

    if let Some(source) = &params.source {
        args.push("-s".to_string());
        args.push(source.clone());
    }

    if let Some(destination) = &params.destination {
        args.push("-d".to_string());
        args.push(destination.clone());
    }

    if let Some(source_port) = &params.source_port {
        args.push("--sport".to_string());
        args.push(source_port.clone());
    }

    if let Some(destination_port) = &params.destination_port {
        args.push("--dport".to_string());
        args.push(destination_port.clone());
    }

    if let Some(in_interface) = &params.in_interface {
        args.push("-i".to_string());
        args.push(in_interface.clone());
    }

    if let Some(out_interface) = &params.out_interface {
        args.push("-o".to_string());
        args.push(out_interface.clone());
    }

    if let Some(match_ext) = &params.match_ext {
        args.push("-m".to_string());
        args.push(match_ext.clone());
    }

    if let Some(ctstate) = &params.ctstate {
        if params.match_ext.is_none() {
            args.push("-m".to_string());
            args.push("conntrack".to_string());
        }
        args.push("--ctstate".to_string());
        args.push(ctstate.clone());
    }

    if let Some(comment) = &params.comment {
        if params.match_ext.is_none() && params.ctstate.is_none() {
            args.push("-m".to_string());
            args.push("comment".to_string());
        }
        args.push("--comment".to_string());
        args.push(format!("\"{comment}\""));
    }

    if let Some(jump) = &params.jump {
        args.push("-j".to_string());
        args.push(jump.clone());
    }

    if let Some(to_destination) = &params.to_destination {
        args.push("--to-destination".to_string());
        args.push(to_destination.clone());
    }

    if let Some(to_source) = &params.to_source {
        args.push("--to-source".to_string());
        args.push(to_source.clone());
    }

    if let Some(to_ports) = &params.to_ports {
        args.push("--to-ports".to_string());
        args.push(to_ports.clone());
    }

    args
}

fn rule_exists(cmd: &str, params: &Params) -> Result<bool> {
    let args = build_check_spec(params);
    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute {cmd}: {e}"),
        )
    })?;

    Ok(output.status.success())
}

fn flush_chain(cmd: &str, params: &Params) -> Result<()> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("-t".to_string());
        args.push(table.clone());
    }

    args.push("-F".to_string());
    args.push(params.chain.clone());

    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to flush chain: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to flush chain {}: {}",
                params.chain,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn set_policy(cmd: &str, params: &Params, policy: &str) -> Result<()> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("-t".to_string());
        args.push(table.clone());
    }

    args.push("-P".to_string());
    args.push(params.chain.clone());
    args.push(policy.to_string());

    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to set policy: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to set policy for chain {}: {}",
                params.chain,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn get_current_policy(cmd: &str, params: &Params) -> Result<Option<String>> {
    let mut args = Vec::new();

    if let Some(table) = &params.table {
        args.push("-t".to_string());
        args.push(table.clone());
    }

    args.push("-L".to_string());
    args.push(params.chain.clone());

    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to list chain: {e}"),
        )
    })?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("Chain ")
            && line.contains("policy")
            && let Some(policy_start) = line.find("policy ")
        {
            let policy_part = &line[policy_start + 7..];
            if let Some(end) = policy_part.find(')') {
                return Ok(Some(policy_part[..end].to_string()));
            }
        }
    }

    Ok(None)
}

fn add_rule(cmd: &str, params: &Params) -> Result<()> {
    let mut args = build_rule_spec(params);

    if let Some(rule_num) = &params.rule_num {
        args[1] = "-I".to_string();
        args.insert(2, rule_num.clone());
    }

    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to add rule: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add iptables rule: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn delete_rule(cmd: &str, params: &Params) -> Result<()> {
    let args = build_check_spec(params);

    let output = Command::new(cmd).args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to delete rule: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete iptables rule: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

pub fn iptables(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let cmd = get_iptables_cmd(&params.ip_version);
    let flush = params.flush.unwrap_or(false);

    if flush {
        if check_mode {
            info!("Would flush chain {}", params.chain);
            return Ok(ModuleResult::new(true, None, None));
        }
        flush_chain(cmd, &params)?;
        return Ok(ModuleResult::new(true, None, None));
    }

    if let Some(policy) = &params.policy {
        if check_mode {
            let current = get_current_policy(cmd, &params)?;
            if current.as_deref() == Some(policy.as_str()) {
                return Ok(ModuleResult::new(false, None, None));
            }
            info!("Would set policy {} for chain {}", policy, params.chain);
            return Ok(ModuleResult::new(true, None, None));
        }

        let current = get_current_policy(cmd, &params)?;
        if current.as_deref() == Some(policy.as_str()) {
            return Ok(ModuleResult::new(false, None, None));
        }

        set_policy(cmd, &params, policy)?;
        return Ok(ModuleResult::new(true, None, None));
    }

    match state {
        State::Present => {
            let exists = rule_exists(cmd, &params)?;
            if exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if check_mode {
                info!("Would add rule to chain {}", params.chain);
                return Ok(ModuleResult::new(true, None, None));
            }

            add_rule(cmd, &params)?;
            Ok(ModuleResult::new(true, None, None))
        }
        State::Absent => {
            let exists = rule_exists(cmd, &params)?;
            if !exists {
                return Ok(ModuleResult::new(false, None, None));
            }

            if check_mode {
                info!("Would remove rule from chain {}", params.chain);
                return Ok(ModuleResult::new(true, None, None));
            }

            delete_rule(cmd, &params)?;
            Ok(ModuleResult::new(true, None, None))
        }
    }
}

#[derive(Debug)]
pub struct Iptables;

impl Module for Iptables {
    fn get_name(&self) -> &str {
        "iptables"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((iptables(parse_params(optional_params)?, check_mode)?, None))
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
            chain: INPUT
            protocol: tcp
            destination_port: "80"
            jump: ACCEPT
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chain, "INPUT");
        assert_eq!(params.protocol, Some("tcp".to_string()));
        assert_eq!(params.destination_port, Some("80".to_string()));
        assert_eq!(params.jump, Some("ACCEPT".to_string()));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_with_table() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: nat
            chain: POSTROUTING
            source: "10.0.0.0/24"
            jump: MASQUERADE
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.table, Some("nat".to_string()));
        assert_eq!(params.chain, "POSTROUTING");
        assert_eq!(params.source, Some("10.0.0.0/24".to_string()));
    }

    #[test]
    fn test_parse_params_with_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            protocol: tcp
            destination_port: "8080"
            jump: ACCEPT
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_policy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            policy: DROP
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, Some("DROP".to_string()));
    }

    #[test]
    fn test_parse_params_with_flush() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            flush: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.flush, Some(true));
    }

    #[test]
    fn test_parse_params_with_ctstate() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            ctstate: ESTABLISHED,RELATED
            jump: ACCEPT
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.ctstate, Some("ESTABLISHED,RELATED".to_string()));
    }

    #[test]
    fn test_parse_params_with_comment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            protocol: tcp
            destination_port: "22"
            jump: ACCEPT
            comment: "Allow SSH"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.comment, Some("Allow SSH".to_string()));
    }

    #[test]
    fn test_parse_params_ipv6() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chain: INPUT
            protocol: tcp
            destination_port: "80"
            jump: ACCEPT
            ip_version: ipv6
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.ip_version, Some(IpVersion::Ipv6));
    }

    #[test]
    fn test_build_rule_spec_basic() {
        let params = Params {
            chain: "INPUT".to_string(),
            table: None,
            state: None,
            policy: None,
            protocol: Some("tcp".to_string()),
            source: None,
            destination: None,
            source_port: None,
            destination_port: Some("80".to_string()),
            jump: Some("ACCEPT".to_string()),
            to_destination: None,
            to_source: None,
            to_ports: None,
            in_interface: None,
            out_interface: None,
            ctstate: None,
            match_ext: None,
            rule_num: None,
            flush: None,
            comment: None,
            ip_version: None,
            flush_all: None,
        };
        let args = build_rule_spec(&params);
        assert!(args.contains(&"-A".to_string()));
        assert!(args.contains(&"INPUT".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"tcp".to_string()));
        assert!(args.contains(&"--dport".to_string()));
        assert!(args.contains(&"80".to_string()));
        assert!(args.contains(&"-j".to_string()));
        assert!(args.contains(&"ACCEPT".to_string()));
    }

    #[test]
    fn test_build_rule_spec_with_table() {
        let params = Params {
            chain: "POSTROUTING".to_string(),
            table: Some("nat".to_string()),
            state: None,
            policy: None,
            protocol: None,
            source: Some("10.0.0.0/24".to_string()),
            destination: None,
            source_port: None,
            destination_port: None,
            jump: Some("MASQUERADE".to_string()),
            to_destination: None,
            to_source: None,
            to_ports: None,
            in_interface: None,
            out_interface: Some("eth0".to_string()),
            ctstate: None,
            match_ext: None,
            rule_num: None,
            flush: None,
            comment: None,
            ip_version: None,
            flush_all: None,
        };
        let args = build_rule_spec(&params);
        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"nat".to_string()));
        assert!(args.contains(&"-o".to_string()));
        assert!(args.contains(&"eth0".to_string()));
    }

    #[test]
    fn test_build_rule_spec_with_dnat() {
        let params = Params {
            chain: "PREROUTING".to_string(),
            table: Some("nat".to_string()),
            state: None,
            policy: None,
            protocol: Some("tcp".to_string()),
            source: None,
            destination: None,
            source_port: None,
            destination_port: Some("8080".to_string()),
            jump: Some("DNAT".to_string()),
            to_destination: Some("127.0.0.1:80".to_string()),
            to_source: None,
            to_ports: None,
            in_interface: Some("eth0".to_string()),
            out_interface: None,
            ctstate: None,
            match_ext: None,
            rule_num: None,
            flush: None,
            comment: None,
            ip_version: None,
            flush_all: None,
        };
        let args = build_rule_spec(&params);
        assert!(args.contains(&"--to-destination".to_string()));
        assert!(args.contains(&"127.0.0.1:80".to_string()));
    }

    #[test]
    fn test_get_iptables_cmd() {
        assert_eq!(get_iptables_cmd(&None), "iptables");
        assert_eq!(get_iptables_cmd(&Some(IpVersion::Ipv4)), "iptables");
        assert_eq!(get_iptables_cmd(&Some(IpVersion::Ipv6)), "ip6tables");
    }
}
