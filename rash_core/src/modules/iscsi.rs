/// ANCHOR: module
/// # iscsi
///
/// Manage iSCSI target connections using iscsiadm.
///
/// This module manages iSCSI (Internet Small Computer System Interface) storage
/// connections. It supports target discovery, login/logout, CHAP authentication,
/// and session management.
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
/// - name: Discover and login to iSCSI target
///   iscsi:
///     portal: 192.168.1.100
///     target: iqn.2024-01.com.example:storage.target01
///     state: present
///
/// - name: Login with CHAP authentication
///   iscsi:
///     portal: 192.168.1.100:3260
///     target: iqn.2024-01.com.example:storage.target01
///     state: logged_in
///     username: chapuser
///     password: chapsecret
///
/// - name: Discover targets on a portal
///   iscsi:
///     portal: 192.168.1.100
///     discover: true
///     state: present
///
/// - name: Logout from iSCSI target
///   iscsi:
///     portal: 192.168.1.100
///     target: iqn.2024-01.com.example:storage.target01
///     state: logged_out
///
/// - name: Remove iSCSI node record
///   iscsi:
///     portal: 192.168.1.100
///     target: iqn.2024-01.com.example:storage.target01
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_PORT: u16 = 3260;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Present,
    Absent,
    LoggedIn,
    LoggedOut,
}

fn default_discover() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// IQN of the iSCSI target (e.g., iqn.2024-01.com.example:storage.target01).
    /// Required unless `discover` is true without a specific target.
    target: Option<String>,
    /// Portal IP address, optionally with port (e.g., 192.168.1.100 or 192.168.1.100:3260).
    /// **[default: `3260` if no port specified]**
    portal: String,
    /// Desired state of the iSCSI target connection.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
    /// IQN of the initiator node name. When set, configures the initiator name.
    node: Option<String>,
    /// CHAP authentication username.
    username: Option<String>,
    /// CHAP authentication password.
    password: Option<String>,
    /// LUN number to reference.
    lun: Option<u32>,
    /// Whether to perform target discovery on the portal.
    /// **[default: `true`]**
    #[serde(default = "default_discover")]
    discover: bool,
}

#[derive(Debug)]
pub struct Iscsi;

impl Module for Iscsi {
    fn get_name(&self) -> &str {
        "iscsi"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            iscsi_module(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn parse_portal(portal: &str) -> (String, u16) {
    if let Some(idx) = portal.rfind(':') {
        let host = &portal[..idx];
        let port_str = &portal[idx + 1..];
        if let Ok(port) = port_str.parse::<u16>() {
            return (host.to_string(), port);
        }
    }
    (portal.to_string(), DEFAULT_PORT)
}

fn run_iscsiadm(args: &[&str]) -> Result<std::process::Output> {
    trace!("exec - iscsiadm {:?}", args);
    let output = Command::new("iscsiadm")
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::NotFound,
                    "iscsiadm command not found; install open-iscsi to use this module",
                )
            } else {
                Error::new(ErrorKind::SubprocessFail, e)
            }
        })?;
    trace!("exec - output: {output:?}");
    Ok(output)
}

fn is_logged_in(target: &str, portal_ip: &str) -> Result<bool> {
    let output = run_iscsiadm(&["-m", "session"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() || stdout.trim().is_empty() {
        return Ok(false);
    }

    for line in stdout.lines() {
        if line.contains(target) && line.contains(portal_ip) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn node_record_exists(target: &str, portal: &str) -> Result<bool> {
    let output = run_iscsiadm(&["-m", "node", "-T", target, "-p", portal])?;

    Ok(output.status.success())
}

fn discover_targets(portal: &str) -> Result<Vec<String>> {
    let output = run_iscsiadm(&["-m", "discovery", "-t", "sendtargets", "-p", portal])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("iscsiadm discovery failed: {stderr}"),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let targets: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() == 2 {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(targets)
}

fn set_chap_auth(target: &str, portal: &str, username: &str, password: &str) -> Result<()> {
    let output = run_iscsiadm(&[
        "-m",
        "node",
        "-T",
        target,
        "-p",
        portal,
        "--op=update",
        "-n",
        "node.session.auth.authmethod",
        "-v",
        "CHAP",
    ])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to set CHAP auth method: {stderr}"),
        ));
    }

    let output = run_iscsiadm(&[
        "-m",
        "node",
        "-T",
        target,
        "-p",
        portal,
        "--op=update",
        "-n",
        "node.session.auth.username",
        "-v",
        username,
    ])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to set CHAP username: {stderr}"),
        ));
    }

    let output = run_iscsiadm(&[
        "-m",
        "node",
        "-T",
        target,
        "-p",
        portal,
        "--op=update",
        "-n",
        "node.session.auth.password",
        "-v",
        password,
    ])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to set CHAP password: {stderr}"),
        ));
    }

    Ok(())
}

fn login_target(target: &str, portal: &str) -> Result<()> {
    let output = run_iscsiadm(&["-m", "node", "-T", target, "-p", portal, "-l"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("iscsiadm login failed: {stderr}"),
        ));
    }
    Ok(())
}

fn logout_target(target: &str, portal: &str) -> Result<()> {
    let output = run_iscsiadm(&["-m", "node", "-T", target, "-p", portal, "-u"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("iscsiadm logout failed: {stderr}"),
        ));
    }
    Ok(())
}

fn delete_node_record(target: &str, portal: &str) -> Result<()> {
    let output = run_iscsiadm(&["-m", "node", "-T", target, "-p", portal, "-o", "delete"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("iscsiadm delete node failed: {stderr}"),
        ));
    }
    Ok(())
}

fn validate_params(params: &Params) -> Result<()> {
    if params.portal.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "portal cannot be empty"));
    }

    if params.target.is_none() && !matches!(params.state, State::Present) && !params.discover {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "target is required unless discover is enabled",
        ));
    }

    if params.username.is_some() && params.password.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "password is required when username is provided",
        ));
    }

    if params.password.is_some() && params.username.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "username is required when password is provided",
        ));
    }

    if matches!(params.state, State::LoggedIn | State::LoggedOut) && params.target.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "target is required for logged_in and logged_out states",
        ));
    }

    Ok(())
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let (portal_ip, _) = parse_portal(&params.portal);
    let mut changed = false;
    let mut discovered_targets: Vec<String> = Vec::new();

    if params.discover {
        if check_mode {
            info!("Would discover iSCSI targets on portal {}", params.portal);
            changed = true;
        } else {
            discovered_targets = discover_targets(&params.portal)?;
            changed = true;
        }
    }

    if let Some(ref target) = params.target {
        let logged_in = if check_mode {
            false
        } else {
            is_logged_in(target, &portal_ip)?
        };

        if !logged_in {
            if check_mode {
                info!(
                    "Would login to iSCSI target {} on portal {}",
                    target, params.portal
                );
                changed = true;
            } else {
                if let (Some(username), Some(password)) = (&params.username, &params.password) {
                    set_chap_auth(target, &params.portal, username, password)?;
                }

                login_target(target, &params.portal)?;
                changed = true;
            }
        }
    }

    let mut extra = serde_json::Map::new();
    extra.insert(
        "portal".to_string(),
        serde_json::Value::String(params.portal.clone()),
    );

    if let Some(ref target) = params.target {
        extra.insert(
            "target".to_string(),
            serde_json::Value::String(target.clone()),
        );
    }

    if !discovered_targets.is_empty() {
        extra.insert(
            "discovered_targets".to_string(),
            serde_json::Value::Array(
                discovered_targets
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    extra.insert("changed".to_string(), serde_json::Value::Bool(changed));

    let output_msg = if check_mode {
        if let Some(ref target) = params.target {
            format!(
                "Would login to iSCSI target {target} on portal {}",
                params.portal
            )
        } else {
            format!("Would discover iSCSI targets on portal {}", params.portal)
        }
    } else if let Some(ref target) = params.target {
        if changed {
            format!(
                "Logged in to iSCSI target {target} on portal {}",
                params.portal
            )
        } else {
            format!(
                "iSCSI target {target} already logged in on portal {}",
                params.portal
            )
        }
    } else if changed {
        format!("Discovered iSCSI targets on portal {}", params.portal)
    } else {
        format!("No changes for portal {}", params.portal)
    };

    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(extra)?),
        Some(output_msg),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let target = params.target.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "target is required for absent state",
        )
    })?;

    let (portal_ip, _) = parse_portal(&params.portal);
    let mut changed = false;

    let logged_in = if check_mode {
        false
    } else {
        is_logged_in(target, &portal_ip)?
    };

    if logged_in {
        if check_mode {
            info!(
                "Would logout from iSCSI target {} on portal {}",
                target, params.portal
            );
        } else {
            logout_target(target, &params.portal)?;
        }
        changed = true;
    }

    if check_mode {
        info!(
            "Would delete iSCSI node record for {} on portal {}",
            target, params.portal
        );
        changed = true;
    } else if node_record_exists(target, &params.portal)? {
        delete_node_record(target, &params.portal)?;
        changed = true;
    }

    let extra = value::to_value(json!({
        "portal": params.portal,
        "target": target,
        "changed": changed,
    }))?;

    let output_msg = if check_mode {
        format!(
            "Would remove iSCSI target {target} from portal {}",
            params.portal
        )
    } else if changed {
        format!(
            "Removed iSCSI target {target} from portal {}",
            params.portal
        )
    } else {
        format!(
            "iSCSI target {target} not found on portal {}",
            params.portal
        )
    };

    Ok(ModuleResult::new(changed, Some(extra), Some(output_msg)))
}

fn exec_logged_in(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let target = params.target.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "target is required for logged_in state",
        )
    })?;

    let (portal_ip, _) = parse_portal(&params.portal);

    let logged_in = if check_mode {
        false
    } else {
        is_logged_in(target, &portal_ip)?
    };

    if logged_in {
        let extra = value::to_value(json!({
            "portal": params.portal,
            "target": target,
            "logged_in": true,
        }))?;
        return Ok(ModuleResult::new(
            false,
            Some(extra),
            Some(format!("Already logged in to {target}")),
        ));
    }

    if check_mode {
        info!(
            "Would login to iSCSI target {} on portal {}",
            target, params.portal
        );
        let extra = value::to_value(json!({
            "portal": params.portal,
            "target": target,
            "logged_in": false,
        }))?;
        return Ok(ModuleResult::new(
            true,
            Some(extra),
            Some(format!("Would login to {target}")),
        ));
    }

    if let (Some(username), Some(password)) = (&params.username, &params.password) {
        set_chap_auth(target, &params.portal, username, password)?;
    }

    login_target(target, &params.portal)?;

    let extra = value::to_value(json!({
        "portal": params.portal,
        "target": target,
        "logged_in": true,
    }))?;

    Ok(ModuleResult::new(
        true,
        Some(extra),
        Some(format!("Logged in to iSCSI target {target}")),
    ))
}

fn exec_logged_out(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let target = params.target.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "target is required for logged_out state",
        )
    })?;

    let (portal_ip, _) = parse_portal(&params.portal);

    let logged_in = if check_mode {
        true
    } else {
        is_logged_in(target, &portal_ip)?
    };

    if !logged_in {
        let extra = value::to_value(json!({
            "portal": params.portal,
            "target": target,
            "logged_in": false,
        }))?;
        return Ok(ModuleResult::new(
            false,
            Some(extra),
            Some(format!("Already logged out from {target}")),
        ));
    }

    if check_mode {
        info!(
            "Would logout from iSCSI target {} on portal {}",
            target, params.portal
        );
        let extra = value::to_value(json!({
            "portal": params.portal,
            "target": target,
            "logged_in": true,
        }))?;
        return Ok(ModuleResult::new(
            true,
            Some(extra),
            Some(format!("Would logout from {target}")),
        ));
    }

    logout_target(target, &params.portal)?;

    let extra = value::to_value(json!({
        "portal": params.portal,
        "target": target,
        "logged_in": false,
    }))?;

    Ok(ModuleResult::new(
        true,
        Some(extra),
        Some(format!("Logged out from iSCSI target {target}")),
    ))
}

fn iscsi_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");
    validate_params(&params)?;

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
        State::LoggedIn => exec_logged_in(&params, check_mode),
        State::LoggedOut => exec_logged_out(&params, check_mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.portal, "192.168.1.100");
        assert_eq!(
            params.target,
            Some("iqn.2024-01.com.example:storage.target01".to_string())
        );
        assert_eq!(params.state, State::Present);
        assert!(params.discover);
        assert_eq!(params.username, None);
        assert_eq!(params.password, None);
        assert_eq!(params.node, None);
        assert_eq!(params.lun, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100:3260"
            target: "iqn.2024-01.com.example:storage.target01"
            state: logged_in
            node: "iqn.2024-01.com.example:initiator"
            username: "chapuser"
            password: "chapsecret"
            lun: 0
            discover: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.portal, "192.168.1.100:3260");
        assert_eq!(
            params.target,
            Some("iqn.2024-01.com.example:storage.target01".to_string())
        );
        assert_eq!(params.state, State::LoggedIn);
        assert_eq!(
            params.node,
            Some("iqn.2024-01.com.example:initiator".to_string())
        );
        assert_eq!(params.username, Some("chapuser".to_string()));
        assert_eq!(params.password, Some("chapsecret".to_string()));
        assert_eq!(params.lun, Some(0));
        assert!(!params.discover);
    }

    #[test]
    fn test_parse_params_states() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: logged_in
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::LoggedIn);

        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: logged_out
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::LoggedOut);
    }

    #[test]
    fn test_parse_params_discover_only() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            discover: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.portal, "192.168.1.100");
        assert_eq!(params.target, None);
        assert!(params.discover);
    }

    #[test]
    fn test_parse_params_missing_portal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            target: "iqn.2024-01.com.example:storage.target01"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_deny_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
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
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_portal() {
        assert_eq!(
            parse_portal("192.168.1.100"),
            ("192.168.1.100".to_string(), 3260)
        );
        assert_eq!(
            parse_portal("192.168.1.100:3260"),
            ("192.168.1.100".to_string(), 3260)
        );
        assert_eq!(
            parse_portal("192.168.1.100:3261"),
            ("192.168.1.100".to_string(), 3261)
        );
        assert_eq!(parse_portal("[::1]"), ("[::1]".to_string(), 3260));
        assert_eq!(parse_portal("[::1]:3260"), ("[::1]".to_string(), 3260));
        assert_eq!(
            parse_portal("10.0.0.1:abc"),
            ("10.0.0.1:abc".to_string(), 3260)
        );
    }

    #[test]
    fn test_validate_params_empty_portal() {
        let params = Params {
            target: Some("iqn.2024-01.com.example:storage.target01".to_string()),
            portal: "".to_string(),
            state: State::Present,
            node: None,
            username: None,
            password: None,
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_chap_username_only() {
        let params = Params {
            target: Some("iqn.2024-01.com.example:storage.target01".to_string()),
            portal: "192.168.1.100".to_string(),
            state: State::Present,
            node: None,
            username: Some("chapuser".to_string()),
            password: None,
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_chap_password_only() {
        let params = Params {
            target: Some("iqn.2024-01.com.example:storage.target01".to_string()),
            portal: "192.168.1.100".to_string(),
            state: State::Present,
            node: None,
            username: None,
            password: Some("chapsecret".to_string()),
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_logged_in_requires_target() {
        let params = Params {
            target: None,
            portal: "192.168.1.100".to_string(),
            state: State::LoggedIn,
            node: None,
            username: None,
            password: None,
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_logged_out_requires_target() {
        let params = Params {
            target: None,
            portal: "192.168.1.100".to_string(),
            state: State::LoggedOut,
            node: None,
            username: None,
            password: None,
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_valid() {
        let params = Params {
            target: Some("iqn.2024-01.com.example:storage.target01".to_string()),
            portal: "192.168.1.100".to_string(),
            state: State::Present,
            node: None,
            username: None,
            password: None,
            lun: None,
            discover: true,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_with_chap() {
        let params = Params {
            target: Some("iqn.2024-01.com.example:storage.target01".to_string()),
            portal: "192.168.1.100".to_string(),
            state: State::LoggedIn,
            node: None,
            username: Some("chapuser".to_string()),
            password: Some("chapsecret".to_string()),
            lun: None,
            discover: false,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_check_mode_present() {
        let iscsi = Iscsi;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            "#,
        )
        .unwrap();
        let (result, _) = iscsi
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_logged_out() {
        let iscsi = Iscsi;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: logged_out
            "#,
        )
        .unwrap();
        let (result, _) = iscsi
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_absent() {
        let iscsi = Iscsi;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            target: "iqn.2024-01.com.example:storage.target01"
            state: absent
            "#,
        )
        .unwrap();
        let (result, _) = iscsi
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_discover_only() {
        let iscsi = Iscsi;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            portal: "192.168.1.100"
            discover: true
            "#,
        )
        .unwrap();
        let (result, _) = iscsi
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
    }
}
