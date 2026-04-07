/// ANCHOR: module
/// # nftables
///
/// Manage nftables firewall rules.
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
/// - name: Add a table
///   nftables:
///     table: myfilter
///     family: inet
///     state: present
///
/// - name: Add a chain to the filter table
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     chain_type: filter
///     chain_hook: input
///     chain_priority: 0
///     state: present
///
/// - name: Add a rule to allow HTTP traffic
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     rule: "tcp dport 80 accept"
///     state: present
///
/// - name: Add a rule to allow established connections
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     rule: "ct state established,related accept"
///     state: present
///
/// - name: Allow traffic from specific source
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     rule: "ip saddr 192.168.1.0/24 accept"
///     state: present
///
/// - name: NAT masquerade for outgoing traffic
///   nftables:
///     table: mynat
///     chain: postrouting
///     family: ip
///     rule: "ip saddr 10.0.0.0/24 oifname eth0 masquerade"
///     state: present
///
/// - name: Delete a specific rule
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     rule: "tcp dport 8080 accept"
///     state: absent
///
/// - name: Delete a chain
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
///     state: absent
///
/// - name: Delete a table
///   nftables:
///     table: myfilter
///     family: inet
///     state: absent
///
/// - name: Flush all rules in a chain
///   nftables:
///     table: myfilter
///     chain: input
///     family: inet
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

const NFT_CMD: &str = "nft";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The nftables table name.
    pub table: String,
    /// The nftables chain name (optional for table operations).
    pub chain: Option<String>,
    /// The rule specification in nftables syntax.
    pub rule: Option<String>,
    /// Whether the rule/chain/table should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// The address family (ip, ip6, inet, arp, bridge, netdev).
    /// **[default: `"inet"`]**
    pub family: Option<Family>,
    /// The chain type (filter, nat, route for certain families).
    pub chain_type: Option<String>,
    /// The chain hook (input, output, forward, prerouting, postrouting, ingress).
    pub chain_hook: Option<String>,
    /// The chain priority (numeric value, typically 0, positive or negative).
    /// **[default: `0`]**
    pub chain_priority: Option<i32>,
    /// The policy for the chain (accept, drop).
    pub chain_policy: Option<String>,
    /// Flush all rules in the specified chain.
    /// **[default: `false`]**
    pub flush: Option<bool>,
    /// Comment for the rule (stored as a comment in nftables).
    pub comment: Option<String>,
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
    Inet,
    Ip,
    Ip6,
    Arp,
    Bridge,
    Netdev,
}

fn family_to_str(family: &Family) -> &'static str {
    match family {
        Family::Ip => "ip",
        Family::Ip6 => "ip6",
        Family::Inet => "inet",
        Family::Arp => "arp",
        Family::Bridge => "bridge",
        Family::Netdev => "netdev",
    }
}

fn table_exists(family: &Family, table: &str) -> Result<bool> {
    let output = Command::new(NFT_CMD)
        .args(["list", "table", family_to_str(family), table])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute nft: {e}"),
            )
        })?;

    Ok(output.status.success())
}

fn chain_exists(family: &Family, table: &str, chain: &str) -> Result<bool> {
    let output = Command::new(NFT_CMD)
        .args(["list", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute nft: {e}"),
            )
        })?;

    Ok(output.status.success())
}

fn rule_exists(family: &Family, table: &str, chain: &str, rule: &str) -> Result<bool> {
    let output = Command::new(NFT_CMD)
        .args(["-a", "list", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute nft: {e}"),
            )
        })?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.trim().ends_with(rule) || line.contains(rule) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn get_rule_handle(
    family: &Family,
    table: &str,
    chain: &str,
    rule: &str,
) -> Result<Option<String>> {
    let output = Command::new(NFT_CMD)
        .args(["-a", "list", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute nft: {e}"),
            )
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if (line.trim().ends_with(rule) || line.contains(rule))
            && let Some(handle_pos) = line.find("handle ")
        {
            let handle_part = &line[handle_pos + 7..];
            let handle = handle_part.split_whitespace().next();
            return Ok(handle.map(|h| h.to_string()));
        }
    }

    Ok(None)
}

fn add_table(family: &Family, table: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["add", "table", family_to_str(family), table])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to add table: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add table {}: {}",
                table,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn delete_table(family: &Family, table: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["delete", "table", family_to_str(family), table])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to delete table: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete table {}: {}",
                table,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn add_chain(params: &Params, family: &Family) -> Result<()> {
    let mut chain_spec = String::new();

    if let Some(chain_type) = &params.chain_type {
        chain_spec.push_str(&format!("{{ type {} ", chain_type));
    } else {
        chain_spec.push_str("{ ");
    }

    if let Some(chain_hook) = &params.chain_hook {
        chain_spec.push_str(&format!("hook {} ", chain_hook));
    }

    let priority = params.chain_priority.unwrap_or(0);
    chain_spec.push_str(&format!("priority {} ", priority));

    if let Some(policy) = &params.chain_policy {
        chain_spec.push_str(&format!("policy {} ", policy));
    }

    chain_spec.push('}');

    let chain_name = params.chain.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "chain name is required for chain operations",
        )
    })?;

    let output = Command::new(NFT_CMD)
        .args([
            "add",
            "chain",
            family_to_str(family),
            &params.table,
            chain_name,
            &chain_spec,
        ])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to add chain: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add chain {}: {}",
                chain_name,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn add_chain_simple(family: &Family, table: &str, chain: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["add", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to add chain: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add chain {}: {}",
                chain,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn delete_chain(family: &Family, table: &str, chain: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["delete", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to delete chain: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete chain {}: {}",
                chain,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn flush_chain(family: &Family, table: &str, chain: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["flush", "chain", family_to_str(family), table, chain])
        .output()
        .map_err(|e| {
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
                chain,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn add_rule(family: &Family, table: &str, chain: &str, rule: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args(["add", "rule", family_to_str(family), table, chain, rule])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to add rule: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to add rule '{}': {}",
                rule,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn delete_rule(family: &Family, table: &str, chain: &str, handle: &str) -> Result<()> {
    let output = Command::new(NFT_CMD)
        .args([
            "delete",
            "rule",
            family_to_str(family),
            table,
            chain,
            "handle",
            handle,
        ])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to delete rule: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete rule handle {}: {}",
                handle,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

pub fn nftables(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let family = params.family.unwrap_or_default();
    let flush = params.flush.unwrap_or(false);

    if flush {
        let chain = params.chain.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "chain is required for flush operation",
            )
        })?;

        if !chain_exists(&family, &params.table, chain)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Chain '{}' does not exist in table '{}'",
                    chain, params.table
                ),
            ));
        }

        if check_mode {
            info!("Would flush chain {} in table {}", chain, params.table);
            return Ok(ModuleResult::new(true, None, None));
        }

        flush_chain(&family, &params.table, chain)?;
        return Ok(ModuleResult::new(true, None, None));
    }

    if let Some(chain) = &params.chain {
        if let Some(rule) = &params.rule {
            match state {
                State::Present => {
                    if !chain_exists(&family, &params.table, chain)? {
                        if check_mode {
                            info!(
                                "Would create chain {} in table {} before adding rule",
                                chain, params.table
                            );
                            return Ok(ModuleResult::new(true, None, None));
                        }

                        if params.chain_type.is_some() || params.chain_hook.is_some() {
                            add_chain(&params, &family)?;
                        } else {
                            add_chain_simple(&family, &params.table, chain)?;
                        }
                    }

                    if rule_exists(&family, &params.table, chain, rule)? {
                        return Ok(ModuleResult::new(false, None, None));
                    }

                    if check_mode {
                        info!("Would add rule '{}' to chain {}", rule, chain);
                        return Ok(ModuleResult::new(true, None, None));
                    }

                    add_rule(&family, &params.table, chain, rule)?;
                    Ok(ModuleResult::new(true, None, None))
                }
                State::Absent => {
                    if !chain_exists(&family, &params.table, chain)? {
                        return Ok(ModuleResult::new(false, None, None));
                    }

                    let handle = get_rule_handle(&family, &params.table, chain, rule)?;
                    if handle.is_none() {
                        return Ok(ModuleResult::new(false, None, None));
                    }

                    if check_mode {
                        info!("Would delete rule '{}' from chain {}", rule, chain);
                        return Ok(ModuleResult::new(true, None, None));
                    }

                    delete_rule(&family, &params.table, chain, handle.as_ref().unwrap())?;
                    Ok(ModuleResult::new(true, None, None))
                }
            }
        } else {
            match state {
                State::Present => {
                    if chain_exists(&family, &params.table, chain)? {
                        return Ok(ModuleResult::new(false, None, None));
                    }

                    if !table_exists(&family, &params.table)? {
                        if check_mode {
                            info!("Would create table {}", params.table);
                            return Ok(ModuleResult::new(true, None, None));
                        }
                        add_table(&family, &params.table)?;
                    }

                    if check_mode {
                        info!("Would create chain {} in table {}", chain, params.table);
                        return Ok(ModuleResult::new(true, None, None));
                    }

                    if params.chain_type.is_some() || params.chain_hook.is_some() {
                        add_chain(&params, &family)?;
                    } else {
                        add_chain_simple(&family, &params.table, chain)?;
                    }
                    Ok(ModuleResult::new(true, None, None))
                }
                State::Absent => {
                    if !chain_exists(&family, &params.table, chain)? {
                        return Ok(ModuleResult::new(false, None, None));
                    }

                    if check_mode {
                        info!("Would delete chain {} from table {}", chain, params.table);
                        return Ok(ModuleResult::new(true, None, None));
                    }

                    delete_chain(&family, &params.table, chain)?;
                    Ok(ModuleResult::new(true, None, None))
                }
            }
        }
    } else {
        match state {
            State::Present => {
                if table_exists(&family, &params.table)? {
                    return Ok(ModuleResult::new(false, None, None));
                }

                if check_mode {
                    info!("Would create table {}", params.table);
                    return Ok(ModuleResult::new(true, None, None));
                }

                add_table(&family, &params.table)?;
                Ok(ModuleResult::new(true, None, None))
            }
            State::Absent => {
                if !table_exists(&family, &params.table)? {
                    return Ok(ModuleResult::new(false, None, None));
                }

                if check_mode {
                    info!("Would delete table {}", params.table);
                    return Ok(ModuleResult::new(true, None, None));
                }

                delete_table(&family, &params.table)?;
                Ok(ModuleResult::new(true, None, None))
            }
        }
    }
}

#[derive(Debug)]
pub struct Nftables;

impl Module for Nftables {
    fn get_name(&self) -> &str {
        "nftables"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((nftables(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_table_only() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            family: inet
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.table, "myfilter");
        assert_eq!(params.family, Some(Family::Inet));
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.chain, None);
    }

    #[test]
    fn test_parse_params_chain() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            chain: input
            family: inet
            chain_type: filter
            chain_hook: input
            chain_priority: 0
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.table, "myfilter");
        assert_eq!(params.chain, Some("input".to_string()));
        assert_eq!(params.chain_type, Some("filter".to_string()));
        assert_eq!(params.chain_hook, Some("input".to_string()));
        assert_eq!(params.chain_priority, Some(0));
    }

    #[test]
    fn test_parse_params_rule() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            chain: input
            rule: "tcp dport 80 accept"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.table, "myfilter");
        assert_eq!(params.chain, Some("input".to_string()));
        assert_eq!(params.rule, Some("tcp dport 80 accept".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            chain: input
            rule: "tcp dport 8080 accept"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_flush() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            chain: input
            family: inet
            flush: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.flush, Some(true));
    }

    #[test]
    fn test_parse_params_nat_example() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: mynat
            chain: postrouting
            family: ip
            rule: "ip saddr 10.0.0.0/24 oifname eth0 masquerade"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.table, "mynat");
        assert_eq!(params.chain, Some("postrouting".to_string()));
        assert_eq!(params.family, Some(Family::Ip));
    }

    #[test]
    fn test_parse_params_chain_policy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            chain: input
            family: inet
            chain_type: filter
            chain_hook: input
            chain_priority: 0
            chain_policy: drop
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chain_policy, Some("drop".to_string()));
    }

    #[test]
    fn test_family_to_str() {
        assert_eq!(family_to_str(&Family::Ip), "ip");
        assert_eq!(family_to_str(&Family::Ip6), "ip6");
        assert_eq!(family_to_str(&Family::Inet), "inet");
        assert_eq!(family_to_str(&Family::Arp), "arp");
        assert_eq!(family_to_str(&Family::Bridge), "bridge");
        assert_eq!(family_to_str(&Family::Netdev), "netdev");
    }

    #[test]
    fn test_parse_params_default_family() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.family, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            table: myfilter
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
