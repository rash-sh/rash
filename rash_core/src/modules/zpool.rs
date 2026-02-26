/// ANCHOR: module
/// # zpool
///
/// Manage ZFS storage pools.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Create mirrored ZFS pool
///   zpool:
///     name: rpool
///     state: present
///     type: mirror
///     devices:
///       - /dev/nvme0n1p3
///       - /dev/nvme1n1p3
///     properties:
///       ashift: 12
///       autoexpand: on
///     features:
///       encryption: enabled
///
/// - name: Create single device pool
///   zpool:
///     name: datapool
///     state: present
///     devices:
///       - /dev/sda1
///
/// - name: Set pool property
///   zpool:
///     name: rpool
///     state: present
///     properties:
///       cachefile: none
///
/// - name: Export pool
///   zpool:
///     name: rpool
///     state: exported
///
/// - name: Import pool by name
///   zpool:
///     name: rpool
///     state: imported
///
/// - name: Import pool by GUID
///   zpool:
///     guid: 1234567890abcdef
///     state: imported
///     name: rpool
///
/// - name: Destroy pool
///   zpool:
///     name: rpool
///     state: absent
///     force: true
///
/// - name: Start scrub
///   zpool:
///     name: rpool
///     state: scrubbed
///
/// - name: Get pool info
///   zpool:
///     name: rpool
///     state: info
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::collections::HashMap;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum State {
    #[default]
    Info,
    Present,
    Absent,
    Imported,
    Exported,
    Scrubbed,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum PoolType {
    #[default]
    Single,
    Mirror,
    Raidz,
    Raidz2,
    Raidz3,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Pool name.
    name: String,
    /// Pool state.
    /// **[default: `"info"`]**
    #[serde(default)]
    state: State,
    /// List of devices for pool creation.
    devices: Option<Vec<String>>,
    /// Pool type (single, mirror, raidz, raidz2, raidz3).
    /// **[default: `"single"`]**
    #[serde(default)]
    pool_type: PoolType,
    /// Pool properties (ashift, autoexpand, etc.).
    properties: Option<HashMap<String, String>>,
    /// Feature flags to enable.
    features: Option<HashMap<String, String>>,
    /// Alternate root mount point.
    altroot: Option<String>,
    /// Mount host for pools.
    mounthost: Option<String>,
    /// Force operation.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Pool GUID for import by GUID.
    guid: Option<String>,
}

#[derive(Debug)]
pub struct Zpool;

impl Module for Zpool {
    fn get_name(&self) -> &str {
        "zpool"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            zpool_module(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct ZpoolClient {
    check_mode: bool,
}

impl ZpoolClient {
    pub fn new(check_mode: bool) -> Self {
        ZpoolClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing zpool command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn pool_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("zpool").args(["list", "-o", "name", "-H", name]),
            false,
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(output.status.success() && !stdout.is_empty() && stdout == name)
    }

    pub fn get_pool_info(&self, name: &str) -> Result<PoolInfo> {
        let output = self.exec_cmd(
            Command::new("zpool")
                .args(["list", "-H", "-o", "guid,state,status,size,allocated,free"])
                .arg(name),
            false,
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Pool {} not found", name),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split_whitespace().collect();

        Ok(PoolInfo {
            guid: parts.first().map(|s| s.to_string()),
            state: parts.get(1).map(|s| s.to_string()).unwrap_or_default(),
            status: parts.get(2).map(|s| s.to_string()),
            size: parts.get(3).map(|s| s.to_string()),
            allocated: parts.get(4).map(|s| s.to_string()),
            free: parts.get(5).map(|s| s.to_string()),
        })
    }

    pub fn get_pool_properties(&self, name: &str) -> Result<HashMap<String, String>> {
        let output = self.exec_cmd(
            Command::new("zpool")
                .args(["get", "-H", "-o", "property,value", "all"])
                .arg(name),
            false,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut props = HashMap::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                props.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        Ok(props)
    }

    pub fn get_pool_devices(&self, name: &str) -> Result<Vec<String>> {
        let output = self.exec_cmd(
            Command::new("zpool").args(["status", "-P", "-L"]).arg(name),
            false,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("/dev/") {
                devices.push(trimmed.to_string());
            }
        }

        Ok(devices)
    }

    pub fn create_pool(&self, params: &Params) -> Result<ZpoolResult> {
        diff(
            format!("state: absent (pool {})", params.name),
            format!("state: present (pool {})", params.name),
        );

        if self.check_mode {
            return Ok(ZpoolResult::new(true, None));
        }

        let devices = params.devices.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "devices is required when state is present and pool doesn't exist",
            )
        })?;

        let mut cmd = Command::new("zpool");
        cmd.arg("create");

        if let Some(ref altroot) = params.altroot {
            cmd.args(["-o", &format!("altroot={}", altroot)]);
        }

        if let Some(ref mounthost) = params.mounthost {
            cmd.args(["-o", &format!("mounthost={}", mounthost)]);
        }

        if let Some(ref properties) = params.properties {
            for (key, value) in properties {
                cmd.args(["-o", &format!("{}={}", key, value)]);
            }
        }

        if let Some(ref features) = params.features {
            for (key, value) in features {
                cmd.args(["-O", &format!("feature@{}={}", key, value)]);
            }
        }

        cmd.arg(&params.name);

        match params.pool_type {
            PoolType::Single => {}
            PoolType::Mirror => {
                cmd.arg("mirror");
            }
            PoolType::Raidz => {
                cmd.arg("raidz");
            }
            PoolType::Raidz2 => {
                cmd.arg("raidz2");
            }
            PoolType::Raidz3 => {
                cmd.arg("raidz3");
            }
        }

        for device in devices {
            cmd.arg(device);
        }

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZpoolResult::new(true, output_str))
    }

    pub fn set_pool_properties(&self, params: &Params) -> Result<ZpoolResult> {
        let properties = params.properties.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "properties is required to set pool properties",
            )
        })?;

        if self.check_mode {
            for (key, value) in properties {
                diff(
                    format!("property {} on pool {}", key, params.name),
                    format!("property {}={} on pool {}", key, value, params.name),
                );
            }
            return Ok(ZpoolResult::new(true, None));
        }

        let mut changed = false;
        for (key, value) in properties {
            let output = self.exec_cmd(
                Command::new("zpool").args(["set", &format!("{}={}", key, value), &params.name]),
                true,
            )?;
            if output.status.success() {
                changed = true;
            }
        }

        Ok(ZpoolResult::new(changed, None))
    }

    pub fn destroy_pool(&self, params: &Params) -> Result<ZpoolResult> {
        diff(
            format!("state: present (pool {})", params.name),
            format!("state: absent (pool {})", params.name),
        );

        if self.check_mode {
            return Ok(ZpoolResult::new(true, None));
        }

        let mut cmd = Command::new("zpool");
        cmd.arg("destroy");

        if params.force {
            cmd.arg("-f");
        }

        cmd.arg(&params.name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZpoolResult::new(true, output_str))
    }

    pub fn import_pool(&self, params: &Params) -> Result<ZpoolResult> {
        diff(
            format!("state: exported (pool {})", params.name),
            format!("state: imported (pool {})", params.name),
        );

        if self.check_mode {
            return Ok(ZpoolResult::new(true, None));
        }

        let mut cmd = Command::new("zpool");
        cmd.arg("import");

        if let Some(ref guid) = params.guid {
            cmd.arg(guid);
        } else {
            cmd.arg(&params.name);
        }

        if let Some(ref altroot) = params.altroot {
            cmd.args(["-o", &format!("altroot={}", altroot)]);
        }

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZpoolResult::new(true, output_str))
    }

    pub fn export_pool(&self, params: &Params) -> Result<ZpoolResult> {
        diff(
            format!("state: imported (pool {})", params.name),
            format!("state: exported (pool {})", params.name),
        );

        if self.check_mode {
            return Ok(ZpoolResult::new(true, None));
        }

        let mut cmd = Command::new("zpool");
        cmd.arg("export");

        if params.force {
            cmd.arg("-f");
        }

        cmd.arg(&params.name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZpoolResult::new(true, output_str))
    }

    pub fn scrub_pool(&self, params: &Params) -> Result<ZpoolResult> {
        diff(
            format!("scrub: not running (pool {})", params.name),
            format!("scrub: started (pool {})", params.name),
        );

        if self.check_mode {
            return Ok(ZpoolResult::new(true, None));
        }

        let output = self.exec_cmd(Command::new("zpool").args(["scrub", &params.name]), true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZpoolResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct PoolInfo {
    guid: Option<String>,
    state: String,
    status: Option<String>,
    size: Option<String>,
    allocated: Option<String>,
    free: Option<String>,
}

#[derive(Debug)]
struct ZpoolResult {
    changed: bool,
    output: Option<String>,
}

impl ZpoolResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        ZpoolResult { changed, output }
    }

    fn no_change() -> Self {
        ZpoolResult {
            changed: false,
            output: None,
        }
    }
}

fn zpool_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ZpoolClient::new(check_mode);

    let pool_exists = if !params.name.is_empty() {
        client.pool_exists(&params.name)?
    } else {
        false
    };

    let result = match params.state {
        State::Info => {
            if pool_exists {
                ZpoolResult::no_change()
            } else {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Pool {} not found", params.name),
                ));
            }
        }
        State::Present => {
            if pool_exists {
                if params.properties.is_some() {
                    client.set_pool_properties(&params)?
                } else {
                    ZpoolResult::no_change()
                }
            } else {
                client.create_pool(&params)?
            }
        }
        State::Absent => {
            if pool_exists {
                client.destroy_pool(&params)?
            } else {
                ZpoolResult::no_change()
            }
        }
        State::Imported => {
            if pool_exists {
                ZpoolResult::no_change()
            } else {
                client.import_pool(&params)?
            }
        }
        State::Exported => {
            if pool_exists {
                client.export_pool(&params)?
            } else {
                ZpoolResult::no_change()
            }
        }
        State::Scrubbed => {
            if pool_exists {
                client.scrub_pool(&params)?
            } else {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("Pool {} not found", params.name),
                ));
            }
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );

    if pool_exists || (result.changed && matches!(params.state, State::Present | State::Imported)) {
        if let Ok(info) = client.get_pool_info(&params.name) {
            if let Some(guid) = info.guid {
                extra.insert("guid".to_string(), serde_json::Value::String(guid));
            }
            extra.insert("state".to_string(), serde_json::Value::String(info.state));
            if let Some(status) = info.status {
                extra.insert("status".to_string(), serde_json::Value::String(status));
            }
            if let Some(size) = info.size {
                extra.insert("size".to_string(), serde_json::Value::String(size));
            }
            if let Some(allocated) = info.allocated {
                extra.insert(
                    "allocated".to_string(),
                    serde_json::Value::String(allocated),
                );
            }
            if let Some(free) = info.free {
                extra.insert("free".to_string(), serde_json::Value::String(free));
            }
        }

        if let Ok(props) = client.get_pool_properties(&params.name) {
            extra.insert(
                "properties".to_string(),
                serde_json::to_value(props).unwrap_or(serde_json::Value::Null),
            );
        }

        if let Ok(devices) = client.get_pool_devices(&params.name) {
            extra.insert(
                "devices".to_string(),
                serde_json::to_value(devices).unwrap_or(serde_json::Value::Null),
            );
        }
    }

    Ok(ModuleResult {
        changed: result.changed,
        output: result.output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: present
            pool_type: mirror
            devices:
              - /dev/nvme0n1p3
              - /dev/nvme1n1p3
            properties:
              ashift: "12"
              autoexpand: on
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "rpool");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.pool_type, PoolType::Mirror);
        assert_eq!(
            params.devices,
            Some(vec![
                "/dev/nvme0n1p3".to_owned(),
                "/dev/nvme1n1p3".to_owned()
            ])
        );
        assert!(params.properties.is_some());
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "rpool");
        assert_eq!(params.state, State::Absent);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_imported() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: imported
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Imported);
    }

    #[test]
    fn test_parse_params_exported() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: exported
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Exported);
    }

    #[test]
    fn test_parse_params_scrubbed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: scrubbed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Scrubbed);
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: info
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Info);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Info);
    }

    #[test]
    fn test_parse_params_pool_types() {
        for (pool_type_str, expected) in [
            ("single", PoolType::Single),
            ("mirror", PoolType::Mirror),
            ("raidz", PoolType::Raidz),
            ("raidz2", PoolType::Raidz2),
            ("raidz3", PoolType::Raidz3),
        ] {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                name: rpool
                state: present
                pool_type: {}
                devices:
                  - /dev/sda1
                "#,
                pool_type_str
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.pool_type, expected);
        }
    }

    #[test]
    fn test_parse_params_with_guid() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            guid: 1234567890abcdef
            state: imported
            name: rpool
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.guid, Some("1234567890abcdef".to_owned()));
    }

    #[test]
    fn test_parse_params_with_features() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: present
            devices:
              - /dev/sda1
            features:
              encryption: enabled
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.features.is_some());
        let features = params.features.unwrap();
        assert_eq!(features.get("encryption"), Some(&"enabled".to_owned()));
    }

    #[test]
    fn test_parse_params_with_altroot() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: present
            devices:
              - /dev/sda1
            altroot: /mnt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.altroot, Some("/mnt".to_owned()));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            invalid_field: value
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
            name: rpool
            state: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_pool_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool
            state: present
            type: invalid
            devices:
              - /dev/sda1
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
