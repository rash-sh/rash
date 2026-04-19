/// ANCHOR: module
/// # ufw
///
/// Manage Ubuntu Uncomplicated Firewall (UFW).
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
/// - name: Enable UFW
///   ufw:
///     state: enabled
///
/// - name: Set default incoming policy to deny
///   ufw:
///     policy: deny
///     direction: in
///
/// - name: Allow SSH
///   ufw:
///     rule: allow
///     port: "22"
///     proto: tcp
///
/// - name: Allow HTTP
///   ufw:
///     rule: allow
///     port: "80"
///     proto: tcp
///
/// - name: Allow HTTPS
///   ufw:
///     rule: allow
///     port: "443"
///     proto: tcp
///
/// - name: Allow port from specific IP
///   ufw:
///     rule: allow
///     port: "3306"
///     proto: tcp
///     from_ip: "192.168.1.0/24"
///
/// - name: Deny port
///   ufw:
///     rule: deny
///     port: "23"
///     proto: tcp
///
/// - name: Allow service
///   ufw:
///     rule: allow
///     port: ssh
///
/// - name: Limit SSH connections
///   ufw:
///     rule: limit
///     port: "22"
///     proto: tcp
///
/// - name: Allow outgoing traffic to specific IP
///   ufw:
///     rule: allow
///     to_ip: "10.0.0.1"
///     direction: out
///
/// - name: Delete a rule
///   ufw:
///     rule: allow
///     port: "8080"
///     proto: tcp
///     state: absent
///
/// - name: Reload UFW
///   ufw:
///     state: reloaded
///
/// - name: Reset UFW to defaults
///   ufw:
///     state: reset
///
/// - name: Allow traffic on an interface
///   ufw:
///     rule: allow
///     port: "53"
///     proto: udp
///     interface: eth0
///
/// - name: Enable logging
///   ufw:
///     logging: on
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
    /// Whether the firewall should be enabled, disabled, reset, or reloaded.
    pub state: Option<State>,
    /// Set the default policy for incoming or outgoing traffic.
    pub policy: Option<Policy>,
    /// The direction for the policy (incoming or outgoing).
    /// **[default: `"incoming"`]**
    pub direction: Option<Direction>,
    /// The rule action (allow, deny, reject, limit).
    pub rule: Option<Rule>,
    /// Port number or service name.
    pub port: Option<String>,
    /// Protocol (tcp or udp).
    pub proto: Option<Proto>,
    /// Source IP address or CIDR.
    pub from_ip: Option<String>,
    /// Destination IP address or CIDR.
    pub to_ip: Option<String>,
    /// Service name to allow/deny (e.g., ssh, http).
    pub name: Option<String>,
    /// Comment for the rule.
    pub comment: Option<String>,
    /// Whether the rule should be present or absent.
    /// **[default: `"present"`]**
    pub rule_state: Option<RuleState>,
    /// Network interface for the rule.
    pub interface: Option<String>,
    /// Logging level: off, on, low, medium, high, full.
    pub logging: Option<Logging>,
    /// Route traffic through the firewall.
    /// **[default: `false`]**
    pub route: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Enabled,
    Disabled,
    Reset,
    Reloaded,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Policy {
    Allow,
    Deny,
    Reject,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    In,
    Out,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    Allow,
    Deny,
    Reject,
    Limit,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum RuleState {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Logging {
    Off,
    On,
    Low,
    Medium,
    High,
    Full,
}

fn run_ufw_cmd(args: &[&str]) -> Result<String> {
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

fn is_ufw_enabled() -> Result<bool> {
    let output = Command::new("ufw").arg("status").output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ufw: {e}"),
        )
    })?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("Status: active"))
}

fn get_default_policy(direction: Direction) -> Result<Option<Policy>> {
    let output = Command::new("ufw")
        .arg("status")
        .arg("verbose")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute ufw: {e}"),
            )
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        if line.contains("Default:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                for part in &parts {
                    if part.contains("(incoming)") && direction == Direction::In {
                        let policy_str = parts[1].replace(',', "");
                        return Ok(parse_policy_str(&policy_str));
                    }
                    if part.contains("(outgoing)") && direction == Direction::Out {
                        for p in &parts {
                            if !p.contains("Default")
                                && !p.contains("(incoming)")
                                && !p.contains("(outgoing)")
                                && !p.contains("(routed)")
                            {
                                return Ok(parse_policy_str(p.replace(',', "").as_str()));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

fn parse_policy_str(s: &str) -> Option<Policy> {
    match s.to_lowercase().as_str() {
        "allow" => Some(Policy::Allow),
        "deny" => Some(Policy::Deny),
        "reject" => Some(Policy::Reject),
        _ => None,
    }
}

fn rule_exists(params: &Params) -> Result<bool> {
    let args = vec!["status", "numbered"];

    let output = Command::new("ufw").args(&args).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute ufw: {e}"),
        )
    })?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let rule_str = build_rule_search_string(params);

    for line in stdout.lines() {
        if line.contains(&rule_str) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn build_rule_search_string(params: &Params) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(rule) = &params.rule {
        parts.push(rule_to_str(rule).to_string());
    }

    if let Some(from_ip) = &params.from_ip {
        parts.push(format!("from {}", from_ip));
    }

    if let Some(to_ip) = &params.to_ip {
        parts.push(format!("to {}", to_ip));
    }

    if let Some(name) = &params.name {
        parts.push(name.clone());
    } else if let Some(port) = &params.port {
        parts.push(port.clone());
    }

    if let Some(proto) = &params.proto {
        parts.push(proto_to_str(proto).to_string());
    }

    parts.join(" ")
}

fn rule_to_str(rule: &Rule) -> &'static str {
    match rule {
        Rule::Allow => "ALLOW",
        Rule::Deny => "DENY",
        Rule::Reject => "REJECT",
        Rule::Limit => "LIMIT",
    }
}

fn proto_to_str(proto: &Proto) -> &'static str {
    match proto {
        Proto::Tcp => "tcp",
        Proto::Udp => "udp",
    }
}

fn build_rule_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(rule_state) = &params.rule_state {
        match rule_state {
            RuleState::Absent => args.push("delete".to_string()),
            RuleState::Present => {}
        }
    }

    if params.route.unwrap_or(false) {
        args.push("route".to_string());
    }

    if let Some(rule) = &params.rule {
        args.push(rule_to_str(rule).to_lowercase());
    }

    if let Some(from_ip) = &params.from_ip {
        args.push("from".to_string());
        args.push(from_ip.clone());
    }

    if let Some(to_ip) = &params.to_ip {
        args.push("to".to_string());
        args.push(to_ip.clone());
    }

    if let Some(name) = &params.name {
        args.push(name.clone());
    } else if let Some(port) = &params.port {
        args.push(port.clone());
    }

    if let Some(proto) = &params.proto {
        args.push(proto_to_str(proto).to_string());
    }

    if let Some(interface) = &params.interface {
        args.push("on".to_string());
        args.push(interface.clone());
    }

    if let Some(comment) = &params.comment {
        args.push("comment".to_string());
        args.push(format!("\"{comment}\""));
    }

    args
}

fn enable_ufw(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        info!("Would enable UFW");
        return Ok(ModuleResult::new(
            true,
            None,
            Some("UFW would be enabled".to_string()),
        ));
    }

    run_ufw_cmd(&["enable"])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some("UFW enabled".to_string()),
    ))
}

fn disable_ufw(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        info!("Would disable UFW");
        return Ok(ModuleResult::new(
            true,
            None,
            Some("UFW would be disabled".to_string()),
        ));
    }

    run_ufw_cmd(&["disable"])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some("UFW disabled".to_string()),
    ))
}

fn reset_ufw(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        info!("Would reset UFW");
        return Ok(ModuleResult::new(
            true,
            None,
            Some("UFW would be reset".to_string()),
        ));
    }

    run_ufw_cmd(&["reset"])?;
    Ok(ModuleResult::new(true, None, Some("UFW reset".to_string())))
}

fn reload_ufw(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        info!("Would reload UFW");
        return Ok(ModuleResult::new(
            true,
            None,
            Some("UFW would be reloaded".to_string()),
        ));
    }

    run_ufw_cmd(&["reload"])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some("UFW reloaded".to_string()),
    ))
}

fn set_policy(policy: Policy, direction: Direction, check_mode: bool) -> Result<ModuleResult> {
    let policy_str = match policy {
        Policy::Allow => "allow",
        Policy::Deny => "deny",
        Policy::Reject => "reject",
    };

    let dir_str = match direction {
        Direction::In => "incoming",
        Direction::Out => "outgoing",
    };

    let current = get_default_policy(direction)?;
    if current == Some(policy) {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Policy {} for {} already set", policy_str, dir_str)),
        ));
    }

    if check_mode {
        info!("Would set {} policy to {}", dir_str, policy_str);
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would set {} policy to {}", dir_str, policy_str)),
        ));
    }

    run_ufw_cmd(&["default", policy_str, dir_str])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Set {} policy to {}", dir_str, policy_str)),
    ))
}

fn build_rule_description(params: &Params) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(rule) = &params.rule {
        parts.push(rule_to_str(rule).to_lowercase());
    }

    if let Some(from_ip) = &params.from_ip {
        parts.push(format!("from {}", from_ip));
    }

    if let Some(to_ip) = &params.to_ip {
        parts.push(format!("to {}", to_ip));
    }

    if let Some(name) = &params.name {
        parts.push(format!("service {}", name));
    } else if let Some(port) = &params.port {
        parts.push(port.clone());
    }

    if let Some(proto) = &params.proto {
        parts.push(proto_to_str(proto).to_string());
    }

    if let Some(interface) = &params.interface {
        parts.push(format!("on {}", interface));
    }

    if let Some(comment) = &params.comment {
        parts.push(format!("comment '{}'", comment));
    }

    parts.join(" ")
}

fn manage_rule(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let exists = rule_exists(params)?;
    let rule_state = params.rule_state.unwrap_or_default();
    let rule_desc = build_rule_description(params);

    match rule_state {
        RuleState::Present => {
            if exists {
                return Ok(ModuleResult::new(
                    false,
                    None,
                    Some(format!("Rule already exists: {}", rule_desc)),
                ));
            }

            if check_mode {
                info!("Would add rule: {}", rule_desc);
                return Ok(ModuleResult::new(
                    true,
                    None,
                    Some(format!("Would add rule: {}", rule_desc)),
                ));
            }

            let args = build_rule_args(params);
            let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ufw_cmd(&args_str)?;
            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Rule added: {}", rule_desc)),
            ))
        }
        RuleState::Absent => {
            if !exists {
                return Ok(ModuleResult::new(
                    false,
                    None,
                    Some(format!("Rule does not exist: {}", rule_desc)),
                ));
            }

            if check_mode {
                info!("Would delete rule: {}", rule_desc);
                return Ok(ModuleResult::new(
                    true,
                    None,
                    Some(format!("Would delete rule: {}", rule_desc)),
                ));
            }

            let args = build_rule_args(params);
            let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ufw_cmd(&args_str)?;
            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Rule deleted: {}", rule_desc)),
            ))
        }
    }
}

fn set_logging(logging: Logging, check_mode: bool) -> Result<ModuleResult> {
    let logging_str = match logging {
        Logging::Off => "off",
        Logging::On => "on",
        Logging::Low => "low",
        Logging::Medium => "medium",
        Logging::High => "high",
        Logging::Full => "full",
    };

    if check_mode {
        info!("Would set UFW logging to {}", logging_str);
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would set UFW logging to {}", logging_str)),
        ));
    }

    run_ufw_cmd(&["logging", logging_str])?;
    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("UFW logging set to {}", logging_str)),
    ))
}

pub fn ufw(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    if let Some(state) = &params.state {
        match state {
            State::Enabled => {
                let enabled = is_ufw_enabled()?;
                if enabled {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some("UFW already enabled".to_string()),
                    ));
                }
                return enable_ufw(check_mode);
            }
            State::Disabled => {
                let enabled = is_ufw_enabled()?;
                if !enabled {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some("UFW already disabled".to_string()),
                    ));
                }
                return disable_ufw(check_mode);
            }
            State::Reset => return reset_ufw(check_mode),
            State::Reloaded => return reload_ufw(check_mode),
        }
    }

    if let Some(policy) = &params.policy {
        let direction = params.direction.unwrap_or_default();
        return set_policy(*policy, direction, check_mode);
    }

    if let Some(logging) = &params.logging {
        return set_logging(*logging, check_mode);
    }

    if params.rule.is_some() {
        return manage_rule(&params, check_mode);
    }

    Err(Error::new(
        ErrorKind::InvalidData,
        "Either 'state', 'policy', 'rule', or 'logging' must be specified",
    ))
}

#[derive(Debug)]
pub struct Ufw;

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
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Enabled));
    }

    #[test]
    fn test_parse_params_policy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policy: deny
            direction: in
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.policy, Some(Policy::Deny));
        assert_eq!(params.direction, Some(Direction::In));
    }

    #[test]
    fn test_parse_params_rule() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "22"
            proto: tcp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Allow));
        assert_eq!(params.port, Some("22".to_string()));
        assert_eq!(params.proto, Some(Proto::Tcp));
    }

    #[test]
    fn test_parse_params_rule_with_from_ip() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "3306"
            proto: tcp
            from_ip: "192.168.1.0/24"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Allow));
        assert_eq!(params.from_ip, Some("192.168.1.0/24".to_string()));
    }

    #[test]
    fn test_parse_params_rule_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "8080"
            proto: tcp
            rule_state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule_state, Some(RuleState::Absent));
    }

    #[test]
    fn test_parse_params_with_comment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: allow
            port: "22"
            proto: tcp
            comment: "Allow SSH"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.comment, Some("Allow SSH".to_string()));
    }

    #[test]
    fn test_parse_params_limit_rule() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rule: limit
            port: "22"
            proto: tcp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.rule, Some(Rule::Limit));
    }

    #[test]
    fn test_build_rule_args_basic() {
        let params = Params {
            state: None,
            policy: None,
            direction: None,
            rule: Some(Rule::Allow),
            port: Some("22".to_string()),
            proto: Some(Proto::Tcp),
            from_ip: None,
            to_ip: None,
            name: None,
            comment: None,
            rule_state: None,
            interface: None,
            logging: None,
            route: None,
        };
        let args = build_rule_args(&params);
        assert!(args.contains(&"allow".to_string()));
        assert!(args.contains(&"22".to_string()));
        assert!(args.contains(&"tcp".to_string()));
    }

    #[test]
    fn test_build_rule_args_with_from_ip() {
        let params = Params {
            state: None,
            policy: None,
            direction: None,
            rule: Some(Rule::Allow),
            port: Some("3306".to_string()),
            proto: Some(Proto::Tcp),
            from_ip: Some("192.168.1.0/24".to_string()),
            to_ip: None,
            name: None,
            comment: None,
            rule_state: None,
            interface: None,
            logging: None,
            route: None,
        };
        let args = build_rule_args(&params);
        assert!(args.contains(&"from".to_string()));
        assert!(args.contains(&"192.168.1.0/24".to_string()));
    }

    #[test]
    fn test_build_rule_args_delete() {
        let params = Params {
            state: None,
            policy: None,
            direction: None,
            rule: Some(Rule::Allow),
            port: Some("8080".to_string()),
            proto: Some(Proto::Tcp),
            from_ip: None,
            to_ip: None,
            name: None,
            comment: None,
            rule_state: Some(RuleState::Absent),
            interface: None,
            logging: None,
            route: None,
        };
        let args = build_rule_args(&params);
        assert!(args.contains(&"delete".to_string()));
    }

    #[test]
    fn test_rule_to_str() {
        assert_eq!(rule_to_str(&Rule::Allow), "ALLOW");
        assert_eq!(rule_to_str(&Rule::Deny), "DENY");
        assert_eq!(rule_to_str(&Rule::Reject), "REJECT");
        assert_eq!(rule_to_str(&Rule::Limit), "LIMIT");
    }

    #[test]
    fn test_proto_to_str() {
        assert_eq!(proto_to_str(&Proto::Tcp), "tcp");
        assert_eq!(proto_to_str(&Proto::Udp), "udp");
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: enabled
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
