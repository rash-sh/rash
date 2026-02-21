/// ANCHOR: module
/// # seboolean
///
/// Manage SELinux boolean settings.
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
/// - name: Enable HTTPD network connect
///   seboolean:
///     name: httpd_can_network_connect
///     state: true
///
/// - name: Enable persistent HTTPD network connect
///   seboolean:
///     name: httpd_can_network_connect
///     state: true
///     persistent: true
///
/// - name: Allow containers to use NFS
///   seboolean:
///     name: virt_use_nfs
///     state: true
///     persistent: true
///
/// - name: Disable FTP home directory access
///   seboolean:
///     name: ftp_home_dir
///     state: false
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
    /// Name of the SELinux boolean to manage.
    pub name: String,
    /// Desired state of the SELinux boolean (on/off).
    pub state: bool,
    /// If true, the boolean setting will persist across reboots.
    /// **[default: `false`]**
    pub persistent: Option<bool>,
}

fn get_seboolean_value(name: &str) -> Result<bool> {
    let output = Command::new("getsebool").arg(name).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute getsebool: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to get SELinux boolean '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.split_whitespace().collect();

    if parts.len() < 3 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unexpected output from getsebool: {}", stdout.trim()),
        ));
    }

    match parts[2] {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid SELinux boolean value: {}", parts[2]),
        )),
    }
}

fn set_seboolean_value(name: &str, state: bool, persistent: bool, check_mode: bool) -> Result<()> {
    if check_mode {
        return Ok(());
    }

    let state_str = if state { "on" } else { "off" };

    let output = if persistent {
        Command::new("setsebool")
            .args(["-P", name, state_str])
            .output()
    } else {
        Command::new("setsebool").args([name, state_str]).output()
    }
    .map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute setsebool: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to set SELinux boolean '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(())
}

pub fn seboolean(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let persistent = params.persistent.unwrap_or(false);

    let current = get_seboolean_value(&params.name)?;

    if current == params.state {
        return Ok(ModuleResult::new(false, None, Some(params.name)));
    }

    set_seboolean_value(&params.name, params.state, persistent, check_mode)?;

    let extra = serde_norway::to_value(serde_json::json!({
        "name": params.name,
        "state": params.state,
        "persistent": persistent,
    }))
    .ok();

    Ok(ModuleResult::new(true, extra, Some(params.name)))
}

#[derive(Debug)]
pub struct Seboolean;

impl Module for Seboolean {
    fn get_name(&self) -> &str {
        "seboolean"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((seboolean(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: httpd_can_network_connect
            state: true
            persistent: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "httpd_can_network_connect".to_owned(),
                state: true,
                persistent: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: virt_use_nfs
            state: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "virt_use_nfs");
        assert!(!params.state);
        assert_eq!(params.persistent, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: httpd_can_network_connect
            state: true
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
