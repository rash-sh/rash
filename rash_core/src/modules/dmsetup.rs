/// ANCHOR: module
/// # dmsetup
///
/// Manage Linux device mapper devices.
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
/// - name: Remove all device mapper mappings
///   dmsetup:
///     action: remove_all
///
/// - name: Remove specific device
///   dmsetup:
///     action: remove
///     name: vg0-lv_root
///     force: true
///
/// - name: Get device info
///   dmsetup:
///     action: info
///     name: vg0-lv_root
///   register: dm_info
///
/// - name: Create linear mapping
///   dmsetup:
///     action: create
///     name: my_device
///     table:
///       - "0 2097152 linear /dev/sdb1 0"
///
/// - name: List all devices
///   dmsetup:
///     action: info
///   register: all_devices
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
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
#[serde(rename_all = "snake_case")]
enum Action {
    Create,
    Remove,
    RemoveAll,
    Info,
    Table,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Action to perform on the device mapper device.
    action: Action,
    /// Device mapper device name.
    name: Option<String>,
    /// Device UUID.
    uuid: Option<String>,
    /// Table specification for device (used with create action).
    table: Option<Vec<String>>,
    /// Force operation.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Use deferred removal.
    /// **[default: `false`]**
    #[serde(default)]
    deferred: bool,
    /// Retry on failure.
    retry: Option<u32>,
}

#[derive(Debug)]
pub struct Dmsetup;

impl Module for Dmsetup {
    fn get_name(&self) -> &str {
        "dmsetup"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            dmsetup_module(parse_params(optional_params)?, check_mode)?,
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

struct DmsetupClient {
    check_mode: bool,
}

impl DmsetupClient {
    pub fn new(check_mode: bool) -> Self {
        DmsetupClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn exec_cmd_with_retry(&self, cmd: &mut Command, retries: u32) -> Result<Output> {
        let mut last_error = None;
        for attempt in 0..=retries {
            let output = cmd
                .output()
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
            trace!(
                "command: `{cmd:?}` (attempt {}/{})",
                attempt + 1,
                retries + 1
            );
            trace!("{output:?}");

            if output.status.success() {
                return Ok(output);
            }
            last_error = Some(output);
        }
        Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Command failed after {} retries: {}",
                retries,
                String::from_utf8_lossy(&last_error.unwrap().stderr)
            ),
        ))
    }

    pub fn device_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(Command::new("dmsetup").args(["info", name]))?;
        Ok(output.status.success())
    }

    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = self.exec_cmd(Command::new("dmsetup").args(["ls", "--target", ""]))?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_device_list(&stdout)
    }

    pub fn get_device_info(&self, name: &str) -> Result<Option<DeviceInfo>> {
        let output = self.exec_cmd(Command::new("dmsetup").args([
            "info",
            "-C",
            "--noheadings",
            "-o",
            "name,uuid,blkdevname",
            name,
        ]))?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        if line.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        Ok(Some(DeviceInfo {
            name: parts.first().unwrap_or(&"").to_string(),
            uuid: parts.get(1).and_then(|s| {
                if !s.is_empty() {
                    Some(s.to_string())
                } else {
                    None
                }
            }),
            blkdevname: parts.get(2).map(|s| s.to_string()),
        }))
    }

    pub fn create_device(&self, params: &Params) -> Result<DmsetupResult> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "name is required for create action")
        })?;

        if self.device_exists(name)? {
            return Ok(DmsetupResult::no_change());
        }

        let table = params.table.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "table is required for create action",
            )
        })?;

        diff(
            format!("device {name}: absent"),
            format!("device {name}: present"),
        );

        if self.check_mode {
            return Ok(DmsetupResult::new(true));
        }

        let table_str = table.join("\n");

        let mut cmd = Command::new("dmsetup");
        cmd.arg("create").arg(name);

        if let Some(ref uuid) = params.uuid {
            cmd.arg("--uuid").arg(uuid);
        }

        cmd.stdin(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin
                .write_all(table_str.as_bytes())
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to create device: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(DmsetupResult::new(true))
    }

    pub fn remove_device(&self, params: &Params) -> Result<DmsetupResult> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "name is required for remove action")
        })?;

        if !self.device_exists(name)? {
            return Ok(DmsetupResult::no_change());
        }

        diff(
            format!("device {name}: present"),
            format!("device {name}: absent"),
        );

        if self.check_mode {
            return Ok(DmsetupResult::new(true));
        }

        let mut cmd = Command::new("dmsetup");
        cmd.arg("remove");

        if params.force {
            cmd.arg("--force");
        }

        if params.deferred {
            cmd.arg("--deferred");
        }

        cmd.arg(name);

        let output = if let Some(retries) = params.retry {
            self.exec_cmd_with_retry(&mut cmd, retries)?
        } else {
            self.exec_cmd(&mut cmd)?
        };

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to remove device: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(DmsetupResult::new(true))
    }

    pub fn remove_all(&self, params: &Params) -> Result<DmsetupResult> {
        let devices = self.list_devices()?;
        if devices.is_empty() {
            return Ok(DmsetupResult::no_change());
        }

        let device_names: Vec<String> = devices.iter().map(|d| d.name.clone()).collect();

        diff(
            format!("devices: {}", device_names.join(", ")),
            "devices: (none)".to_string(),
        );

        if self.check_mode {
            return Ok(DmsetupResult::new_with_devices(true, devices));
        }

        let mut cmd = Command::new("dmsetup");
        cmd.arg("remove_all");

        if params.force {
            cmd.arg("--force");
        }

        if params.deferred {
            cmd.arg("--deferred");
        }

        let output = if let Some(retries) = params.retry {
            self.exec_cmd_with_retry(&mut cmd, retries)?
        } else {
            self.exec_cmd(&mut cmd)?
        };

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to remove all devices: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(DmsetupResult::new_with_devices(true, vec![]))
    }

    pub fn get_info(&self, params: &Params) -> Result<DmsetupResult> {
        if let Some(ref name) = params.name {
            let device_info = self.get_device_info(name)?;
            Ok(DmsetupResult::new_with_devices(
                false,
                device_info.into_iter().collect(),
            ))
        } else {
            let devices = self.list_devices()?;
            Ok(DmsetupResult::new_with_devices(false, devices))
        }
    }

    pub fn get_table(&self, params: &Params) -> Result<DmsetupResult> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "name is required for table action")
        })?;

        if !self.device_exists(name)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Device {name} does not exist"),
            ));
        }

        let output = self.exec_cmd(Command::new("dmsetup").args(["table", name]))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to get table: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let table: Vec<String> = stdout.lines().map(|s| s.trim().to_string()).collect();

        Ok(DmsetupResult::new_with_table(false, table))
    }
}

#[derive(Debug, Clone)]
struct DeviceInfo {
    name: String,
    uuid: Option<String>,
    blkdevname: Option<String>,
}

fn parse_device_list(output: &str) -> Result<Vec<DeviceInfo>> {
    let mut devices = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if !parts.is_empty() {
            devices.push(DeviceInfo {
                name: parts[0].to_string(),
                uuid: parts.get(1).and_then(|s| {
                    if !s.is_empty() {
                        Some(s.to_string())
                    } else {
                        None
                    }
                }),
                blkdevname: None,
            });
        }
    }

    Ok(devices)
}

#[derive(Debug)]
struct DmsetupResult {
    changed: bool,
    devices: Option<Vec<DeviceInfo>>,
    table: Option<Vec<String>>,
}

impl DmsetupResult {
    fn new(changed: bool) -> Self {
        DmsetupResult {
            changed,
            devices: None,
            table: None,
        }
    }

    fn no_change() -> Self {
        DmsetupResult {
            changed: false,
            devices: None,
            table: None,
        }
    }

    fn new_with_devices(changed: bool, devices: Vec<DeviceInfo>) -> Self {
        DmsetupResult {
            changed,
            devices: Some(devices),
            table: None,
        }
    }

    fn new_with_table(changed: bool, table: Vec<String>) -> Self {
        DmsetupResult {
            changed,
            devices: None,
            table: Some(table),
        }
    }
}

fn validate_params(params: &Params) -> Result<()> {
    match params.action {
        Action::Create => {
            if params.name.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required for create action",
                ));
            }
            if params.table.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "table is required for create action",
                ));
            }
        }
        Action::Remove => {
            if params.name.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required for remove action",
                ));
            }
        }
        Action::Table => {
            if params.name.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required for table action",
                ));
            }
        }
        Action::RemoveAll | Action::Info => {}
    }
    Ok(())
}

fn dmsetup_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = DmsetupClient::new(check_mode);

    let result = match params.action {
        Action::Create => client.create_device(&params)?,
        Action::Remove => client.remove_device(&params)?,
        Action::RemoveAll => client.remove_all(&params)?,
        Action::Info => client.get_info(&params)?,
        Action::Table => client.get_table(&params)?,
    };

    let mut extra = serde_json::Map::new();

    if let Some(ref name) = params.name {
        extra.insert("name".to_string(), serde_json::Value::String(name.clone()));
    }

    if let Some(devices) = &result.devices {
        let devices_json: Vec<serde_json::Value> = devices
            .iter()
            .map(|d| {
                let mut map = serde_json::Map::new();
                map.insert(
                    "name".to_string(),
                    serde_json::Value::String(d.name.clone()),
                );
                if let Some(ref uuid) = d.uuid {
                    map.insert("uuid".to_string(), serde_json::Value::String(uuid.clone()));
                }
                if let Some(ref blkdevname) = d.blkdevname {
                    map.insert(
                        "blkdevname".to_string(),
                        serde_json::Value::String(blkdevname.clone()),
                    );
                }
                serde_json::Value::Object(map)
            })
            .collect();
        extra.insert(
            "devices".to_string(),
            serde_json::Value::Array(devices_json),
        );
    }

    if let Some(table) = &result.table {
        let table_json: Vec<serde_json::Value> = table
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect();
        extra.insert("table".to_string(), serde_json::Value::Array(table_json));
    }

    Ok(ModuleResult {
        changed: result.changed,
        output: None,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_create() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            name: my_device
            table:
              - "0 2097152 linear /dev/sdb1 0"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Create);
        assert_eq!(params.name, Some("my_device".to_owned()));
        assert_eq!(
            params.table,
            Some(vec!["0 2097152 linear /dev/sdb1 0".to_owned()])
        );
    }

    #[test]
    fn test_parse_params_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: remove
            name: vg0-lv_root
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Remove);
        assert_eq!(params.name, Some("vg0-lv_root".to_owned()));
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_remove_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: remove_all
            deferred: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::RemoveAll);
        assert!(params.deferred);
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: info
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Info);
    }

    #[test]
    fn test_parse_params_info_with_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: info
            name: vg0-lv_root
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Info);
        assert_eq!(params.name, Some("vg0-lv_root".to_owned()));
    }

    #[test]
    fn test_parse_params_table() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: table
            name: my_device
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Table);
        assert_eq!(params.name, Some("my_device".to_owned()));
    }

    #[test]
    fn test_parse_params_with_uuid() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            name: my_device
            uuid: "some-uuid-value"
            table:
              - "0 2097152 linear /dev/sdb1 0"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.uuid, Some("some-uuid-value".to_owned()));
    }

    #[test]
    fn test_parse_params_with_retry() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: remove
            name: my_device
            retry: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.retry, Some(3));
    }

    #[test]
    fn test_validate_params_create_missing_name() {
        let params = Params {
            action: Action::Create,
            name: None,
            uuid: None,
            table: Some(vec!["0 2097152 linear /dev/sdb1 0".to_string()]),
            force: false,
            deferred: false,
            retry: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_create_missing_table() {
        let params = Params {
            action: Action::Create,
            name: Some("my_device".to_string()),
            uuid: None,
            table: None,
            force: false,
            deferred: false,
            retry: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_remove_missing_name() {
        let params = Params {
            action: Action::Remove,
            name: None,
            uuid: None,
            table: None,
            force: false,
            deferred: false,
            retry: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_table_missing_name() {
        let params = Params {
            action: Action::Table,
            name: None,
            uuid: None,
            table: None,
            force: false,
            deferred: false,
            retry: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: info
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_device_list() {
        let output = "vg0-lv_root\t\tLVM-abc123\nvg0-lv_swap\t\tLVM-def456\n";
        let devices = parse_device_list(output).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "vg0-lv_root");
        assert_eq!(devices[1].name, "vg0-lv_swap");
    }

    #[test]
    fn test_parse_device_list_empty() {
        let output = "";
        let devices = parse_device_list(output).unwrap();
        assert!(devices.is_empty());
    }
}
