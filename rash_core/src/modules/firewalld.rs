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
/// - name: Enable HTTP service in public zone
///   firewalld:
///     service: http
///     zone: public
///     state: enabled
///
/// - name: Open port 8080/tcp in public zone
///   firewalld:
///     port: 8080/tcp
///     zone: public
///     state: enabled
///
/// - name: Disable SSH service
///   firewalld:
///     service: ssh
///     state: disabled
///
/// - name: Add interface eth0 to trusted zone
///   firewalld:
///     interface: eth0
///     zone: trusted
///     state: enabled
///
/// - name: Enable masquerading in public zone
///   firewalld:
///     masquerade: true
///     zone: public
///     state: enabled
///
/// - name: Add source network to zone
///   firewalld:
///     source: 192.168.1.0/24
///     zone: trusted
///     state: enabled
///
/// - name: Add rich rule
///   firewalld:
///     rich_rule: 'rule service name="ftp" audit limit value="1/m" accept'
///     zone: public
///     state: enabled
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Enabled,
    Disabled,
    Present,
    Absent,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Enabled => "enabled",
            State::Disabled => "disabled",
            State::Present => "present",
            State::Absent => "absent",
        }
    }

    pub fn is_add(&self) -> bool {
        matches!(self, State::Enabled | State::Present)
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The firewall zone to operate on.
    /// **[default: `public`]**
    pub zone: Option<String>,
    /// Whether the rule should be enabled, disabled, present, or absent.
    pub state: State,
    /// The name of a service to enable/disable (e.g., http, ssh, https).
    pub service: Option<String>,
    /// The port and protocol to enable/disable (e.g., '8080/tcp', '53/udp').
    pub port: Option<String>,
    /// The source address or range to enable/disable.
    pub source: Option<String>,
    /// The interface to add/remove from the zone.
    pub interface: Option<String>,
    /// Whether to enable masquerading in the zone.
    pub masquerade: Option<bool>,
    /// A rich language rule string.
    pub rich_rule: Option<String>,
    /// Enable permanent changes (survive reboots).
    /// **[default: `false`]**
    pub permanent: Option<bool>,
    /// Enable immediate changes (runtime).
    /// **[default: `true`]**
    pub immediate: Option<bool>,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            zone: Some("public".to_string()),
            state: State::Enabled,
            service: None,
            port: None,
            source: None,
            interface: None,
            masquerade: None,
            rich_rule: None,
            permanent: Some(false),
            immediate: Some(true),
        }
    }
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

struct FirewallClient {
    check_mode: bool,
    zone: String,
    permanent: bool,
    immediate: bool,
}

impl FirewallClient {
    pub fn new(zone: &str, permanent: bool, immediate: bool, check_mode: bool) -> Self {
        FirewallClient {
            check_mode,
            zone: zone.to_string(),
            permanent,
            immediate,
        }
    }

    fn get_base_cmd(&self) -> Command {
        Command::new("firewall-cmd")
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<(bool, String)> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("firewall-cmd failed: {}", stderr.trim()),
            ));
        }

        Ok((output.status.success(), stdout))
    }

    fn exec_cmd_check_mode(&self, cmd: &mut Command) -> Result<(bool, String)> {
        if self.check_mode {
            return Ok((true, "check mode - skipped".to_string()));
        }
        self.exec_cmd(cmd)
    }

    fn build_zone_args(&self, cmd: &mut Command, include_zone: bool) {
        if include_zone {
            cmd.args(["--zone", &self.zone]);
        }
        if self.permanent {
            cmd.arg("--permanent");
        }
    }

    pub fn is_service_enabled(&self, service: &str) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.args(["--query-service", service]);

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_service(&self, service: &str, add: bool) -> Result<(bool, Option<String>)> {
        let is_enabled = self.is_service_enabled(service)?;

        if add && is_enabled || !add && !is_enabled {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if add {
                    "--add-service"
                } else {
                    "--remove-service"
                });
                cmd.arg(service);
                if !permanent {
                    cmd.arg("--timeout=0");
                }
            },
            add,
        )?;

        Ok(results)
    }

    pub fn is_port_enabled(&self, port: &str) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.args(["--query-port", port]);

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_port(&self, port: &str, add: bool) -> Result<(bool, Option<String>)> {
        let is_enabled = self.is_port_enabled(port)?;

        if add && is_enabled || !add && !is_enabled {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, _permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if add { "--add-port" } else { "--remove-port" });
                cmd.arg(port);
            },
            add,
        )?;

        Ok(results)
    }

    pub fn is_source_enabled(&self, source: &str) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.args(["--query-source", source]);

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_source(&self, source: &str, add: bool) -> Result<(bool, Option<String>)> {
        let is_enabled = self.is_source_enabled(source)?;

        if add && is_enabled || !add && !is_enabled {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, _permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if add {
                    "--add-source"
                } else {
                    "--remove-source"
                });
                cmd.arg(source);
            },
            add,
        )?;

        Ok(results)
    }

    pub fn is_interface_in_zone(&self, interface: &str) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.args(["--query-interface", interface]);

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_interface(&self, interface: &str, add: bool) -> Result<(bool, Option<String>)> {
        let is_in_zone = self.is_interface_in_zone(interface)?;

        if add && is_in_zone || !add && !is_in_zone {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, _permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if add {
                    "--add-interface"
                } else {
                    "--remove-interface"
                });
                cmd.arg(interface);
            },
            add,
        )?;

        Ok(results)
    }

    pub fn is_masquerade_enabled(&self) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.arg("--query-masquerade");

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_masquerade(&self, enable: bool) -> Result<(bool, Option<String>)> {
        let is_enabled = self.is_masquerade_enabled()?;

        if enable && is_enabled || !enable && !is_enabled {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, _permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if enable {
                    "--add-masquerade"
                } else {
                    "--remove-masquerade"
                });
            },
            enable,
        )?;

        Ok(results)
    }

    pub fn is_rich_rule_enabled(&self, rich_rule: &str) -> Result<bool> {
        let mut cmd = self.get_base_cmd();
        self.build_zone_args(&mut cmd, true);
        cmd.args(["--query-rich-rule", rich_rule]);

        let (success, _) = self.exec_cmd(&mut cmd)?;
        Ok(success)
    }

    pub fn manage_rich_rule(&self, rich_rule: &str, add: bool) -> Result<(bool, Option<String>)> {
        let is_enabled = self.is_rich_rule_enabled(rich_rule)?;

        if add && is_enabled || !add && !is_enabled {
            return Ok((false, None));
        }

        if self.check_mode {
            return Ok((true, None));
        }

        let results = self.execute_dual_commands(
            |cmd, _permanent| {
                self.build_zone_args(cmd, true);
                cmd.arg(if add {
                    "--add-rich-rule"
                } else {
                    "--remove-rich-rule"
                });
                cmd.arg(rich_rule);
            },
            add,
        )?;

        Ok(results)
    }

    fn execute_dual_commands<F>(&self, build_cmd: F, add: bool) -> Result<(bool, Option<String>)>
    where
        F: Fn(&mut Command, bool),
    {
        let mut outputs = Vec::new();
        let mut changed = false;

        if self.immediate {
            let mut cmd = self.get_base_cmd();
            build_cmd(&mut cmd, false);
            let (_, stdout) = self.exec_cmd_check_mode(&mut cmd)?;
            if !stdout.trim().is_empty() && stdout.trim() != "success" {
                outputs.push(stdout.trim().to_string());
            }
            changed = true;
        }

        if self.permanent {
            let mut cmd = self.get_base_cmd();
            build_cmd(&mut cmd, true);
            let (_, stdout) = self.exec_cmd_check_mode(&mut cmd)?;
            if !stdout.trim().is_empty() && stdout.trim() != "success" {
                outputs.push(stdout.trim().to_string());
            }
            changed = true;
        }

        let output = if outputs.is_empty() {
            if add {
                Some("added".to_string())
            } else {
                Some("removed".to_string())
            }
        } else {
            Some(outputs.join("\n"))
        };

        Ok((changed, output))
    }
}

fn validate_port_format(port: &str) -> Result<()> {
    if !port.contains('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Port must include protocol (e.g., '8080/tcp'): {}", port),
        ));
    }

    let parts: Vec<&str> = port.split('/').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid port format: {}", port),
        ));
    }

    let port_num = parts[0];
    if port_num.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Port number cannot be empty: {}", port),
        ));
    }

    let protocol = parts[1].to_lowercase();
    if protocol != "tcp" && protocol != "udp" && protocol != "sctp" && protocol != "dccp" {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid protocol '{}'. Must be tcp, udp, sctp, or dccp",
                protocol
            ),
        ));
    }

    Ok(())
}

fn validate_zone(zone: &str) -> Result<()> {
    if zone.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Zone cannot be empty"));
    }

    if zone.len() > 64 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Zone name too long (max 64 characters)",
        ));
    }

    if zone.contains(char::is_control) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Zone name contains invalid characters",
        ));
    }

    Ok(())
}

fn validate_identifier(name: &str, field: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} cannot be empty", field),
        ));
    }

    if name.len() > 256 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} too long (max 256 characters)", field),
        ));
    }

    if name.contains(char::is_control) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("{} contains invalid characters", field),
        ));
    }

    Ok(())
}

fn firewalld(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let zone = params.zone.clone().unwrap_or_else(|| "public".to_string());
    let permanent = params.permanent.unwrap_or(false);
    let immediate = params.immediate.unwrap_or(true);
    let add = params.state.is_add();

    validate_zone(&zone)?;

    let client = FirewallClient::new(&zone, permanent, immediate, check_mode);

    let mut changed = false;
    let mut messages = Vec::new();

    if let Some(service) = &params.service {
        validate_identifier(service, "Service")?;
        let (service_changed, msg) = client.manage_service(service, add)?;
        if service_changed {
            let action = if add { "enabled" } else { "disabled" };
            diff(
                format!("{}: {}", service, if add { "disabled" } else { "enabled" }),
                format!("{}: {}", service, action),
            );
            if let Some(m) = msg {
                messages.push(format!("Service {}: {}", service, m));
            } else {
                messages.push(format!("Service {}: {}", service, action));
            }
        }
        changed |= service_changed;
    }

    if let Some(port) = &params.port {
        validate_port_format(port)?;
        let (port_changed, msg) = client.manage_port(port, add)?;
        if port_changed {
            let action = if add { "enabled" } else { "disabled" };
            diff(
                format!(
                    "port {}: {}",
                    port,
                    if add { "disabled" } else { "enabled" }
                ),
                format!("port {}: {}", port, action),
            );
            if let Some(m) = msg {
                messages.push(format!("Port {}: {}", port, m));
            } else {
                messages.push(format!("Port {}: {}", port, action));
            }
        }
        changed |= port_changed;
    }

    if let Some(source) = &params.source {
        validate_identifier(source, "Source")?;
        let (source_changed, msg) = client.manage_source(source, add)?;
        if source_changed {
            let action = if add { "added" } else { "removed" };
            diff(
                format!(
                    "source {}: {}",
                    source,
                    if add { "absent" } else { "present" }
                ),
                format!("source {}: {}", source, action),
            );
            if let Some(m) = msg {
                messages.push(format!("Source {}: {}", source, m));
            } else {
                messages.push(format!("Source {}: {}", source, action));
            }
        }
        changed |= source_changed;
    }

    if let Some(interface) = &params.interface {
        validate_identifier(interface, "Interface")?;
        let (interface_changed, msg) = client.manage_interface(interface, add)?;
        if interface_changed {
            let action = if add { "added" } else { "removed" };
            diff(
                format!(
                    "interface {}: {}",
                    interface,
                    if add { "absent" } else { "present" }
                ),
                format!("interface {}: {}", interface, action),
            );
            if let Some(m) = msg {
                messages.push(format!("Interface {}: {}", interface, m));
            } else {
                messages.push(format!("Interface {}: {}", interface, action));
            }
        }
        changed |= interface_changed;
    }

    if let Some(enable_masq) = params.masquerade {
        let (masq_changed, msg) = client.manage_masquerade(enable_masq)?;
        if masq_changed {
            let action = if enable_masq { "enabled" } else { "disabled" };
            diff(
                format!(
                    "masquerade: {}",
                    if enable_masq { "disabled" } else { "enabled" }
                ),
                format!("masquerade: {}", action),
            );
            if let Some(m) = msg {
                messages.push(format!("Masquerade: {}", m));
            } else {
                messages.push(format!("Masquerade: {}", action));
            }
        }
        changed |= masq_changed;
    }

    if let Some(rich_rule) = &params.rich_rule {
        validate_identifier(rich_rule, "Rich rule")?;
        let (rule_changed, msg) = client.manage_rich_rule(rich_rule, add)?;
        if rule_changed {
            let action = if add { "added" } else { "removed" };
            diff(
                format!("rich_rule: {}", if add { "absent" } else { "present" }),
                format!("rich_rule: {}", action),
            );
            if let Some(m) = msg {
                messages.push(format!("Rich rule: {}", m));
            } else {
                messages.push(format!("Rich rule: {}", action));
            }
        }
        changed |= rule_changed;
    }

    let extra = serde_json::json!({
        "zone": zone,
        "state": params.state.as_str(),
        "permanent": permanent,
        "immediate": immediate,
    });

    let output = if messages.is_empty() {
        None
    } else {
        Some(messages.join("\n"))
    };

    Ok(ModuleResult::new(
        changed,
        Some(serde_norway::to_value(extra)?),
        output,
    ))
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
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.service, Some("http".to_string()));
        assert_eq!(params.zone, Some("public".to_string()));
        assert_eq!(params.state, State::Enabled);
    }

    #[test]
    fn test_parse_params_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            port: 8080/tcp
            zone: trusted
            state: disabled
            permanent: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.port, Some("8080/tcp".to_string()));
        assert_eq!(params.zone, Some("trusted".to_string()));
        assert_eq!(params.state, State::Disabled);
        assert_eq!(params.permanent, Some(true));
    }

    #[test]
    fn test_parse_params_interface() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            interface: eth0
            zone: internal
            state: enabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.interface, Some("eth0".to_string()));
        assert_eq!(params.zone, Some("internal".to_string()));
    }

    #[test]
    fn test_parse_params_masquerade() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            masquerade: true
            zone: public
            state: enabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.masquerade, Some(true));
    }

    #[test]
    fn test_parse_params_source() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            source: 192.168.1.0/24
            zone: trusted
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.source, Some("192.168.1.0/24".to_string()));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_rich_rule() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rich_rule: 'rule service name="ftp" accept'
            zone: public
            state: enabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.rich_rule,
            Some("rule service name=\"ftp\" accept".to_string())
        );
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            service: http
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_state_is_add() {
        assert!(State::Enabled.is_add());
        assert!(State::Present.is_add());
        assert!(!State::Disabled.is_add());
        assert!(!State::Absent.is_add());
    }

    #[test]
    fn test_validate_port_format_valid() {
        assert!(validate_port_format("80/tcp").is_ok());
        assert!(validate_port_format("53/udp").is_ok());
        assert!(validate_port_format("8080-8090/tcp").is_ok());
        assert!(validate_port_format("1234/sctp").is_ok());
        assert!(validate_port_format("5678/dccp").is_ok());
    }

    #[test]
    fn test_validate_port_format_invalid() {
        assert!(validate_port_format("80").is_err());
        assert!(validate_port_format("80/").is_err());
        assert!(validate_port_format("80/invalid").is_err());
        assert!(validate_port_format("/tcp").is_err());
    }

    #[test]
    fn test_validate_zone_valid() {
        assert!(validate_zone("public").is_ok());
        assert!(validate_zone("trusted").is_ok());
        assert!(validate_zone("internal").is_ok());
        assert!(validate_zone("dmz").is_ok());
        assert!(validate_zone("work").is_ok());
    }

    #[test]
    fn test_validate_zone_invalid() {
        assert!(validate_zone("").is_err());
        assert!(validate_zone(&"a".repeat(65)).is_err());
        assert!(validate_zone("zone\nwith\nnewlines").is_err());
    }

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("http", "Service").is_ok());
        assert!(validate_identifier("eth0", "Interface").is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        assert!(validate_identifier("", "Service").is_err());
        assert!(validate_identifier(&"a".repeat(257), "Service").is_err());
    }
}
