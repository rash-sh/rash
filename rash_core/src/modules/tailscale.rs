/// ANCHOR: module
/// # tailscale
///
/// Manage Tailscale mesh VPN networking.
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
/// - name: Connect to Tailscale network
///   tailscale:
///     state: up
///     auth_key: "{{ tailscale_auth_key }}"
///
/// - name: Connect with custom hostname and advertise routes
///   tailscale:
///     state: up
///     auth_key: "{{ tailscale_auth_key }}"
///     hostname: my-device
///     advertise_routes:
///       - 10.0.0.0/24
///       - 192.168.1.0/24
///
/// - name: Use an exit node
///   tailscale:
///     state: up
///     exit_node: 100.64.0.1
///
/// - name: Disconnect from Tailscale
///   tailscale:
///     state: down
///
/// - name: Logout from Tailscale
///   tailscale:
///     state: logout
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

const TAILSCALE_BIN: &str = "tailscale";

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Up,
    Down,
    Logout,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Desired state of the Tailscale connection.
    #[serde(default)]
    state: State,
    /// Authentication key for login. Required when state is 'up'.
    auth_key: Option<String>,
    /// Subnet routes to advertise (e.g. ["10.0.0.0/24"]).
    advertise_routes: Option<Vec<String>>,
    /// IP address of the exit node to use.
    exit_node: Option<String>,
    /// Custom hostname for this node.
    hostname: Option<String>,
}

#[derive(Debug)]
pub struct Tailscale;

impl Module for Tailscale {
    fn get_name(&self) -> &str {
        "tailscale"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            exec_tailscale(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn run_tailscale(args: &[&str]) -> Result<std::process::Output> {
    Command::new(TAILSCALE_BIN)
        .args(args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
}

fn is_connected() -> bool {
    run_tailscale(&["status"])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn exec_tailscale(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Up => exec_up(&params, check_mode),
        State::Down => exec_down(check_mode),
        State::Logout => exec_logout(check_mode),
    }
}

fn exec_up(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if is_connected() {
        return Ok(ModuleResult::new(
            false,
            None,
            Some("Already connected".to_string()),
        ));
    }

    if params.auth_key.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "auth_key is required when state is 'up'",
        ));
    }

    let mut args: Vec<String> = vec!["up".to_string()];

    if let Some(ref key) = params.auth_key {
        args.push("--authkey".to_string());
        args.push(key.clone());
    }

    if let Some(ref hostname) = params.hostname {
        args.push("--hostname".to_string());
        args.push(hostname.clone());
    }

    if let Some(ref exit_node) = params.exit_node {
        args.push("--exit-node".to_string());
        args.push(exit_node.clone());
    }

    if let Some(ref routes) = params.advertise_routes
        && !routes.is_empty()
    {
        args.push("--advertise-routes".to_string());
        args.push(routes.join(","));
    }

    if check_mode {
        let cmd_str = format!("{} {}", TAILSCALE_BIN, args.join(" "));
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would run: {}", cmd_str)),
        ));
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = run_tailscale(&arg_refs)?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "tailscale up failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(ModuleResult::new(
        true,
        None,
        Some("Connected to Tailscale".to_string()),
    ))
}

fn exec_down(check_mode: bool) -> Result<ModuleResult> {
    if !is_connected() {
        return Ok(ModuleResult::new(
            false,
            None,
            Some("Already disconnected".to_string()),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("Would run: tailscale down".to_string()),
        ));
    }

    let output = run_tailscale(&["down"])?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "tailscale down failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(ModuleResult::new(
        true,
        None,
        Some("Disconnected from Tailscale".to_string()),
    ))
}

fn exec_logout(check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some("Would run: tailscale logout".to_string()),
        ));
    }

    let output = run_tailscale(&["logout"])?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "tailscale logout failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(ModuleResult::new(
        true,
        None,
        Some("Logged out from Tailscale".to_string()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);
        assert_eq!(params.auth_key, None);
        assert_eq!(params.advertise_routes, None);
        assert_eq!(params.exit_node, None);
        assert_eq!(params.hostname, None);
    }

    #[test]
    fn test_parse_params_up_with_auth_key() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: up
            auth_key: tskey-abc123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);
        assert_eq!(params.auth_key, Some("tskey-abc123".to_string()));
    }

    #[test]
    fn test_parse_params_down() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: down
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Down);
    }

    #[test]
    fn test_parse_params_logout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: logout
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Logout);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: up
            auth_key: tskey-abc123
            hostname: my-device
            exit_node: 100.64.0.1
            advertise_routes:
              - 10.0.0.0/24
              - 192.168.1.0/24
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Up);
        assert_eq!(params.auth_key, Some("tskey-abc123".to_string()));
        assert_eq!(params.hostname, Some("my-device".to_string()));
        assert_eq!(params.exit_node, Some("100.64.0.1".to_string()));
        assert_eq!(
            params.advertise_routes,
            Some(vec![
                "10.0.0.0/24".to_string(),
                "192.168.1.0/24".to_string(),
            ])
        );
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: up
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_exec_up_no_auth_key() {
        let params = Params {
            state: State::Up,
            auth_key: None,
            advertise_routes: None,
            exit_node: None,
            hostname: None,
        };
        let result = exec_up(&params, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }
}
