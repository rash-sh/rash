/// ANCHOR: module
/// # ufw
///
/// Manage UFW (Uncomplicated Firewall) rules.
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
/// - name: Enable UFW with default deny policy
///   ufw:
///     state: enabled
///     policy: deny
///
/// - name: Allow SSH traffic
///   ufw:
///     rule: allow
///     name: ssh
///
/// - name: Allow HTTP on port 80
///   ufw:
///     rule: allow
///     port: 80
///     proto: tcp
///
/// - name: Allow HTTPS from specific IP
///   ufw:
///     rule: allow
///     port: 443
///     proto: tcp
///     from: 192.168.1.100
///
/// - name: Deny port 8080
///   ufw:
///     rule: deny
///     port: 8080
///     proto: tcp
///
/// - name: Delete a rule
///   ufw:
///     rule: allow
///     port: 80
///     proto: tcp
///     delete: true
///
/// - name: Allow port range
///   ufw:
///     rule: allow
///     port: 8000:8005
///     proto: tcp
///
/// - name: Enable logging
///   ufw:
///     logging: on
///
/// - name: Reset UFW to defaults
///   ufw:
///     state: reset
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
    /// Firewall state: enabled, disabled, or reset.
    pub state: Option<UfwState>,
    /// Default policy for incoming traffic: allow, deny, or reject.
    /// **[default: `deny`]**
    pub policy: Option<Policy>,
    /// The rule action: allow, deny, reject, or limit.
    pub rule: Option<Rule>,
    /// Port number or range (e.g., 80, 8000:8005).
    pub port: Option<String>,
    /// Protocol: tcp or udp.
    /// **[default: `both`]**
    pub proto: Option<Protocol>,
    /// Service name to allow/deny (e.g., ssh, http).
    pub name: Option<String>,
    /// Source IP address or network.
    pub from: Option<String>,
    /// Destination IP address or network.
    pub to: Option<String>,
    /// Delete the rule instead of adding it.
    /// **[default: `false`]**
    pub delete: Option<bool>,
    /// Interface for the rule.
    pub interface: Option<String>,
    /// Interface direction: in or out.
    /// **[default: `in`]**
    pub direction: Option<Direction>,
    /// Logging level: off, on, low, medium, high, full.
    pub logging: Option<Logging>,
    /// Route traffic through the firewall.
    /// **[default: `false`]**
    pub route: Option<bool>,
    /// Comment for the rule.
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum UfwState {
    Enabled,
    Disabled,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Allow,
    Deny,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    Allow,
    Deny,
    Reject,
    Limit,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Logging {
    Off,
    On,
    Low,
    Medium,
    High,
    Full,
}

#[derive(Debug)]
pub struct Ufw;

struct UfwClient {
    check_mode: bool,
}

impl UfwClient {
    pub fn new(check_mode: bool) -> Self {
        UfwClient { check_mode }
    }

    fn run_cmd(&self, args: &[&str]) -> Result<String> {
        if self.check_mode {
            return Ok(String::new());
        }

        let output = Command::new("ufw").args(args).output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute ufw: {e}"),
            )
        })?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "ufw failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn run_cmd_vec(&self, args: &[String]) -> Result<String> {
        if self.check_mode {
            return Ok(String::new());
        }

        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run_cmd(&args_str)
    }

    fn run_cmd_with_input(&self, args: &[&str], input: &str) -> Result<String> {
        if self.check_mode {
            return Ok(String::new());
        }

        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new("ufw")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute ufw: {e}"),
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(input.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write to stdin: {e}"),
                )
            })?;
        }

        let output = child.wait_with_output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for ufw: {e}"),
            )
        })?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "ufw failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn is_enabled(&self) -> Result<bool> {
        let output = self.run_cmd(&["status"])?;
        Ok(output.contains("Status: active"))
    }

    pub fn enable(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }
        self.run_cmd_with_input(&["enable"], "y\n")?;
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        self.run_cmd(&["disable"])?;
        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }
        self.run_cmd_with_input(&["reset"], "y\n")?;
        Ok(())
    }

    pub fn set_default_policy(&self, policy: Policy) -> Result<()> {
        let policy_str = match policy {
            Policy::Allow => "allow",
            Policy::Deny => "deny",
            Policy::Reject => "reject",
        };
        self.run_cmd(&["default", policy_str, "incoming"])?;
        self.run_cmd(&["default", "allow", "outgoing"])?;
        Ok(())
    }

    pub fn set_logging(&self, logging: Logging) -> Result<()> {
        let logging_str = match logging {
            Logging::Off => "off",
            Logging::On => "on",
            Logging::Low => "low",
            Logging::Medium => "medium",
            Logging::High => "high",
            Logging::Full => "full",
        };
        self.run_cmd(&["logging", logging_str])?;
        Ok(())
    }

    pub fn rule_exists(&self, params: &Params) -> Result<bool> {
        let output = self.run_cmd(&["status", "numbered"])?;

        let rule_spec = build_rule_spec_for_check(params)?;
        for line in output.lines() {
            if line.contains(&rule_spec) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn add_rule(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let args = build_rule_args(params, false)?;
        self.run_cmd_vec(&args)?;
        Ok(())
    }

    pub fn delete_rule(&self, params: &Params) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let args = build_rule_args(params, true)?;
        self.run_cmd_vec(&args)?;
        Ok(())
    }
}

fn build_rule_spec_for_check(params: &Params) -> Result<String> {
    let mut spec = String::new();

    if let Some(from) = &params.from {
        spec.push_str(from);
        spec.push(' ');
    }

    let rule_str = match params.rule {
        Some(Rule::Allow) => "ALLOW",
        Some(Rule::Deny) => "DENY",
        Some(Rule::Reject) => "REJECT",
        Some(Rule::Limit) => "LIMIT",
        None => "ALLOW",
    };
    spec.push_str(rule_str);

    if let Some(to) = &params.to {
        spec.push(' ');
        spec.push_str(to);
    }

    if let Some(port) = &params.port {
        spec.push(' ');
        spec.push_str(port);
    }

    if let Some(proto) = &params.proto {
        spec.push('/');
        spec.push_str(match proto {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Both => "",
        });
    }

    Ok(spec)
}

fn build_rule_args(params: &Params, delete: bool) -> Result<Vec<String>> {
    let mut args = Vec::new();

    if delete {
        args.push("delete".to_string());
    }

    let rule_str = match params.rule {
        Some(Rule::Allow) => "allow",
        Some(Rule::Deny) => "deny",
        Some(Rule::Reject) => "reject",
        Some(Rule::Limit) => "limit",
        None => "allow",
    };
    args.push(rule_str.to_string());

    if let Some(name) = &params.name {
        args.push(name.clone());
    } else if let Some(port) = &params.port {
        if let Some(proto) = &params.proto {
            args.push(format!(
                "{}/{}",
                port,
                match proto {
                    Protocol::Tcp => "tcp",
                    Protocol::Udp => "udp",
                    Protocol::Both => "any",
                }
            ));
        } else {
            args.push(port.clone());
        }
    }

    if let Some(from) = &params.from {
        args.push("from".to_string());
        args.push(from.clone());
    }

    if let Some(to) = &params.to {
        args.push("to".to_string());
        args.push(to.clone());
    }

    if let Some(interface) = &params.interface {
        let dir = match params.direction {
            Some(Direction::Out) => "out",
            _ => "in",
        };
        args.push("on".to_string());
        args.push(interface.clone());
        if params.direction.is_some() {
            args.push(dir.to_string());
        }
    }

    if let Some(comment) = &params.comment {
        args.push("comment".to_string());
        args.push(comment.clone());
    }

    if params.route.unwrap_or(false) {
        args.push("route".to_string());
    }

    Ok(args)
}

pub fn ufw(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let client = UfwClient::new(check_mode);

    if let Some(state) = params.state {
        match state {
            UfwState::Enabled => {
                let is_enabled = client.is_enabled()?;
                if is_enabled {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some("UFW is already enabled".to_string()),
                    ));
                }

                if check_mode {
                    info!("Would enable UFW");
                    return Ok(ModuleResult::new(
                        true,
                        None,
                        Some("Would enable UFW".to_string()),
                    ));
                }

                if let Some(policy) = params.policy {
                    client.set_default_policy(policy)?;
                }
                client.enable()?;
                return Ok(ModuleResult::new(
                    true,
                    None,
                    Some("UFW enabled".to_string()),
                ));
            }
            UfwState::Disabled => {
                let is_enabled = client.is_enabled()?;
                if !is_enabled {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some("UFW is already disabled".to_string()),
                    ));
                }

                if check_mode {
                    info!("Would disable UFW");
                    return Ok(ModuleResult::new(
                        true,
                        None,
                        Some("Would disable UFW".to_string()),
                    ));
                }

                client.disable()?;
                return Ok(ModuleResult::new(
                    true,
                    None,
                    Some("UFW disabled".to_string()),
                ));
            }
            UfwState::Reset => {
                if check_mode {
                    info!("Would reset UFW to defaults");
                    return Ok(ModuleResult::new(
                        true,
                        None,
                        Some("Would reset UFW to defaults".to_string()),
                    ));
                }

                client.reset()?;
                return Ok(ModuleResult::new(
                    true,
                    None,
                    Some("UFW reset to defaults".to_string()),
                ));
            }
        }
    }

    if let Some(logging) = params.logging {
        if check_mode {
            info!(
                "Would set logging to {}",
                match logging {
                    Logging::Off => "off",
                    Logging::On => "on",
                    Logging::Low => "low",
                    Logging::Medium => "medium",
                    Logging::High => "high",
                    Logging::Full => "full",
                }
            );
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would set logging level to {:?}", logging)),
            ));
        }

        client.set_logging(logging)?;
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Logging set to {:?}", logging)),
        ));
    }

    if params.rule.is_some() {
        let delete = params.delete.unwrap_or(false);

        let exists = client.rule_exists(&params)?;

        if !delete && exists {
            return Ok(ModuleResult::new(
                false,
                None,
                Some("Rule already exists".to_string()),
            ));
        }

        if delete && !exists {
            return Ok(ModuleResult::new(
                false,
                None,
                Some("Rule does not exist".to_string()),
            ));
        }

        if check_mode {
            let action = if delete { "delete" } else { "add" };
            info!("Would {} rule", action);
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would {} rule", action)),
            ));
        }

        if delete {
            client.delete_rule(&params)?;
        } else {
            client.add_rule(&params)?;
        }

        let rule_str = match params.rule {
            Some(Rule::Allow) => "allow",
            Some(Rule::Deny) => "deny",
            Some(Rule::Reject) => "reject",
            Some(Rule::Limit) => "limit",
            None => "allow",
        };

        let msg = if let Some(name) = &params.name {
            format!(
                "Rule {} for service {}",
                if delete { "deleted" } else { "added" },
                name
            )
        } else if let Some(port) = &params.port {
            format!(
                "Rule {} for port {}",
                if delete { "deleted" } else { "added" },
                port
            )
        } else {
            format!(
                "Rule {} ({})",
                if delete { "deleted" } else { "added" },
                rule_str
            )
        };

        let extra = serde_norway::to_value(serde_json::json!({
            "rule": rule_str,
            "port": params.port,
            "proto": params.proto.map(|p| match p {
                Protocol::Tcp => "tcp",
                Protocol::Udp => "udp",
                Protocol::Both => "both",
            }),
            "name": params.name,
            "from": params.from,
            "to": params.to,
            "delete": delete,
        }))
        .ok();

        return Ok(ModuleResult::new(true, extra, Some(msg)));
    }

    Err(Error::new(
        ErrorKind::InvalidData,
        "One of 'state', 'logging', or 'rule' must be specified",
    ))
}

impl Module for Ufw {
    fn get_name(&self) -> &str {
        "ufw"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((ufw(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: enabled
            policy: deny
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(UfwState::Enabled));
        assert_eq!(params.policy, Some(Policy::Deny));
    }

    #[test]
    fn test_parse_params_rule() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "80"
            proto: tcp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Allow));
        assert_eq!(params.port, Some("80".to_string()));
        assert_eq!(params.proto, Some(Protocol::Tcp));
    }

    #[test]
    fn test_parse_params_rule_with_from() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "443"
            proto: tcp
            from: 192.168.1.100
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Allow));
        assert_eq!(params.from, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_parse_params_service() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            name: ssh
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Allow));
        assert_eq!(params.name, Some("ssh".to_string()));
    }

    #[test]
    fn test_parse_params_delete() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "80"
            proto: tcp
            delete: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.delete, Some(true));
    }

    #[test]
    fn test_parse_params_logging() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            logging: on
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.logging, Some(Logging::On));
    }

    #[test]
    fn test_parse_params_port_range() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: 8000:8005
            proto: tcp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.port, Some("8000:8005".to_string()));
    }

    #[test]
    fn test_parse_params_interface() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "80"
            interface: eth0
            direction: in
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.interface, Some("eth0".to_string()));
        assert_eq!(params.direction, Some(Direction::In));
    }

    #[test]
    fn test_parse_params_comment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "22"
            proto: tcp
            comment: "Allow SSH access"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.comment, Some("Allow SSH access".to_string()));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "80"
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_no_required_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: "80"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.state.is_none());
        assert!(params.rule.is_none());
        assert!(params.logging.is_none());
    }

    #[test]
    fn test_build_rule_args_basic() {
        let params = Params {
            state: None,
            policy: None,
            rule: Some(Rule::Allow),
            port: Some("80".to_string()),
            proto: Some(Protocol::Tcp),
            name: None,
            from: None,
            to: None,
            delete: Some(false),
            interface: None,
            direction: None,
            logging: None,
            route: None,
            comment: None,
        };
        let args = build_rule_args(&params, false).unwrap();
        assert!(args.contains(&"allow".to_string()));
        assert!(args.contains(&"80/tcp".to_string()));
    }

    #[test]
    fn test_build_rule_args_delete() {
        let params = Params {
            state: None,
            policy: None,
            rule: Some(Rule::Allow),
            port: Some("80".to_string()),
            proto: Some(Protocol::Tcp),
            name: None,
            from: None,
            to: None,
            delete: Some(true),
            interface: None,
            direction: None,
            logging: None,
            route: None,
            comment: None,
        };
        let args = build_rule_args(&params, true).unwrap();
        assert!(args.contains(&"delete".to_string()));
        assert!(args.contains(&"allow".to_string()));
    }

    #[test]
    fn test_build_rule_args_with_from() {
        let params = Params {
            state: None,
            policy: None,
            rule: Some(Rule::Allow),
            port: Some("443".to_string()),
            proto: Some(Protocol::Tcp),
            name: None,
            from: Some("192.168.1.100".to_string()),
            to: None,
            delete: Some(false),
            interface: None,
            direction: None,
            logging: None,
            route: None,
            comment: None,
        };
        let args = build_rule_args(&params, false).unwrap();
        assert!(args.contains(&"from".to_string()));
        assert!(args.contains(&"192.168.1.100".to_string()));
    }

    #[test]
    fn test_build_rule_args_service() {
        let params = Params {
            state: None,
            policy: None,
            rule: Some(Rule::Allow),
            port: None,
            proto: None,
            name: Some("ssh".to_string()),
            from: None,
            to: None,
            delete: Some(false),
            interface: None,
            direction: None,
            logging: None,
            route: None,
            comment: None,
        };
        let args = build_rule_args(&params, false).unwrap();
        assert!(args.contains(&"allow".to_string()));
        assert!(args.contains(&"ssh".to_string()));
    }

    #[test]
    fn test_ufw_no_required_field() {
        let params = Params {
            state: None,
            policy: None,
            rule: None,
            port: Some("80".to_string()),
            proto: None,
            name: None,
            from: None,
            to: None,
            delete: None,
            interface: None,
            direction: None,
            logging: None,
            route: None,
            comment: None,
        };
        let error = ufw(params, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
