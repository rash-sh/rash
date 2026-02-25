/// ANCHOR: module
/// # mdadm
///
/// Manage Linux software RAID arrays using mdadm.
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
/// - name: Create RAID1 array
///   mdadm:
///     action: create
///     device: /dev/md0
///     name: data
///     level: 1
///     devices:
///       - /dev/sdb1
///       - /dev/sdc1
///     metadata: 1.2
///
/// - name: Assemble existing array
///   mdadm:
///     action: assemble
///     device: /dev/md0
///     devices:
///       - /dev/sdb1
///       - /dev/sdc1
///
/// - name: Stop RAID array
///   mdadm:
///     action: stop
///     device: /dev/md0
///
/// - name: Destroy RAID array (wipe superblocks)
///   mdadm:
///     action: destroy
///     devices:
///       - /dev/sdb1
///       - /dev/sdc1
///     force: true
///
/// - name: Get array info
///   mdadm:
///     action: info
///     device: /dev/md0
///   register: raid_info
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
use std::process::{Command, Output};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Action {
    Create,
    Assemble,
    Stop,
    Destroy,
    Info,
}

fn default_action() -> Action {
    Action::Info
}

fn default_metadata() -> String {
    "1.2".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Action to perform on the RAID array.
    /// **[default: `"info"`]**
    #[serde(default = "default_action")]
    action: Action,
    /// RAID device path (e.g., /dev/md0).
    device: Option<String>,
    /// Array name.
    name: Option<String>,
    /// RAID level (0, 1, 5, 6, 10).
    level: Option<u8>,
    /// List of component devices.
    devices: Option<Vec<String>>,
    /// List of spare devices.
    spare_devices: Option<Vec<String>>,
    /// Number of active devices in the array.
    raid_devices: Option<u32>,
    /// Metadata format.
    /// **[default: `"1.2"`]**
    #[serde(default = "default_metadata")]
    metadata: String,
    /// Chunk size (e.g., 64K, 512K).
    chunk: Option<String>,
    /// Force operation.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Enable write-intent bitmap.
    /// **[default: `false`]**
    #[serde(default)]
    bitmap: bool,
}

#[derive(Debug)]
pub struct Mdadm;

impl Module for Mdadm {
    fn get_name(&self) -> &str {
        "mdadm"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            mdadm_module(parse_params(optional_params)?, check_mode)?,
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

struct MdadmClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct ArrayInfo {
    device: String,
    name: Option<String>,
    level: Option<String>,
    devices: Vec<String>,
    state: Option<String>,
    size: Option<String>,
    uuid: Option<String>,
}

impl MdadmClient {
    pub fn new(check_mode: bool) -> Self {
        MdadmClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn array_exists(&self, device: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("mdadm")
                .args(["--detail", "--brief", device])
                .env("LC_ALL", "C"),
        )?;

        Ok(output.status.success())
    }

    fn get_array_info(&self, device: &str) -> Result<Option<ArrayInfo>> {
        let output = self.exec_cmd(
            Command::new("mdadm")
                .args(["--detail", device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Some(parse_mdadm_detail(&stdout, device)?))
    }

    fn create_array(&self, params: &Params) -> Result<bool> {
        let device = params.device.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "device is required for create action",
            )
        })?;

        let devices = params.devices.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "devices is required for create action",
            )
        })?;

        let level = params.level.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "level is required for create action",
            )
        })?;

        if self.array_exists(device)? {
            return Ok(false);
        }

        diff(
            format!("RAID array {}: absent", device),
            format!(
                "RAID array {}: present (RAID{}, {} devices)",
                device,
                level,
                devices.len()
            ),
        );

        if self.check_mode {
            return Ok(true);
        }

        let raid_devices = params.raid_devices.unwrap_or(devices.len() as u32);

        let mut cmd = Command::new("mdadm");
        cmd.args(["--create", device, "--run"])
            .args(["--level", &level.to_string()])
            .args(["--raid-devices", &raid_devices.to_string()])
            .args(["--metadata", &params.metadata]);

        if params.force {
            cmd.arg("--force");
        }

        if let Some(chunk) = &params.chunk {
            cmd.args(["--chunk", chunk]);
        }

        if let Some(name) = &params.name {
            cmd.args(["--name", name]);
        }

        for dev in devices {
            cmd.arg(dev);
        }

        if let Some(spares) = &params.spare_devices {
            for spare in spares {
                cmd.arg(spare);
            }
        }

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to create RAID array: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(true)
    }

    fn assemble_array(&self, params: &Params) -> Result<bool> {
        let device = params.device.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "device is required for assemble action",
            )
        })?;

        let devices = params.devices.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "devices is required for assemble action",
            )
        })?;

        if self.array_exists(device)? {
            return Ok(false);
        }

        diff(
            format!("RAID array {}: stopped", device),
            format!("RAID array {}: assembled", device),
        );

        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = Command::new("mdadm");
        cmd.args(["--assemble", device]);

        if let Some(name) = &params.name {
            cmd.args(["--name", name]);
        }

        for dev in devices {
            cmd.arg(dev);
        }

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to assemble RAID array: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(true)
    }

    fn stop_array(&self, params: &Params) -> Result<bool> {
        let device = params.device.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "device is required for stop action")
        })?;

        if !self.array_exists(device)? {
            return Ok(false);
        }

        diff(
            format!("RAID array {}: active", device),
            format!("RAID array {}: stopped", device),
        );

        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(
            Command::new("mdadm")
                .args(["--stop", device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if params.force && stderr.contains("does not appear to be active") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to stop RAID array: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn destroy_array(&self, params: &Params) -> Result<bool> {
        let devices = params.devices.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "devices is required for destroy action",
            )
        })?;

        let mut changed = false;
        let mut devices_to_wipe: Vec<&String> = Vec::new();

        for device in devices {
            let output = self.exec_cmd(
                Command::new("mdadm")
                    .args(["--examine", device])
                    .env("LC_ALL", "C"),
            )?;

            if output.status.success() {
                devices_to_wipe.push(device);
                changed = true;
            }
        }

        if !changed {
            return Ok(false);
        }

        diff(
            format!("RAID superblocks on {:?}: present", devices_to_wipe),
            format!("RAID superblocks on {:?}: wiped", devices_to_wipe),
        );

        if self.check_mode {
            return Ok(true);
        }

        for device in devices_to_wipe {
            let mut cmd = Command::new("mdadm");
            cmd.args(["--zero-superblock", device]);

            if params.force {
                cmd.arg("--force");
            }

            let output = self.exec_cmd(&mut cmd)?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to destroy RAID superblock on {}: {}",
                        device,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        }

        Ok(true)
    }

    fn get_info(&self, params: &Params) -> Result<Option<ArrayInfo>> {
        let device = params.device.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "device is required for info action")
        })?;

        self.get_array_info(device)
    }
}

fn parse_mdadm_detail(output: &str, device: &str) -> Result<ArrayInfo> {
    let mut name = None;
    let mut level = None;
    let mut state = None;
    let mut size = None;
    let mut uuid = None;
    let mut devices = Vec::new();

    for line in output.lines() {
        let line = line.trim();

        if line.starts_with("Name") {
            if let Some(value) = line.split(':').nth(1) {
                name = Some(value.trim().to_string());
            }
        } else if line.starts_with("Raid Level") {
            if let Some(value) = line.split(':').nth(1) {
                level = Some(value.trim().to_string());
            }
        } else if line.starts_with("State") && !line.starts_with("State Time") {
            if let Some(value) = line.split(':').nth(1) {
                state = Some(value.trim().to_string());
            }
        } else if line.starts_with("Array Size") {
            if let Some(value) = line.split(':').nth(1) {
                let size_str = value.trim();
                size = Some(size_str.split_whitespace().next().unwrap_or("").to_string());
            }
        } else if line.starts_with("UUID") {
            if let Some(pos) = line.find(':') {
                uuid = Some(line[pos + 1..].trim().to_string());
            }
        } else if line.contains("active sync")
            || line.contains("spare")
            || line.contains("rebuilding")
        {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                devices.push(parts[parts.len() - 1].to_string());
            }
        }
    }

    Ok(ArrayInfo {
        device: device.to_string(),
        name,
        level,
        devices,
        state,
        size,
        uuid,
    })
}

fn validate_device(device: &str) -> Result<()> {
    if device.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "device cannot be empty"));
    }

    if !device.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "device must be an absolute path",
        ));
    }

    Ok(())
}

fn mdadm_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if let Some(device) = &params.device {
        validate_device(device)?;
    }

    let client = MdadmClient::new(check_mode);

    let (changed, array_info) = match params.action {
        Action::Create => (client.create_array(&params)?, None),
        Action::Assemble => (client.assemble_array(&params)?, None),
        Action::Stop => (client.stop_array(&params)?, None),
        Action::Destroy => (client.destroy_array(&params)?, None),
        Action::Info => (false, client.get_info(&params)?),
    };

    let mut extra = serde_json::Map::new();

    if let Some(info) = array_info {
        extra.insert(
            "device".to_string(),
            serde_json::Value::String(info.device.clone()),
        );
        extra.insert(
            "level".to_string(),
            serde_json::Value::String(info.level.unwrap_or_default()),
        );
        extra.insert(
            "devices".to_string(),
            serde_json::Value::Array(
                info.devices
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        if let Some(state) = info.state {
            extra.insert("state".to_string(), serde_json::Value::String(state));
        }
        if let Some(size) = info.size {
            extra.insert("size".to_string(), serde_json::Value::String(size));
        }
        if let Some(uuid) = info.uuid {
            extra.insert("uuid".to_string(), serde_json::Value::String(uuid));
        }
        if let Some(name) = info.name {
            extra.insert("name".to_string(), serde_json::Value::String(name));
        }
    } else if let Some(device) = &params.device {
        extra.insert(
            "device".to_string(),
            serde_json::Value::String(device.clone()),
        );
    }

    Ok(ModuleResult {
        changed,
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
            device: /dev/md0
            name: data
            level: 1
            devices:
              - /dev/sdb1
              - /dev/sdc1
            metadata: "1.2"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Create);
        assert_eq!(params.device, Some("/dev/md0".to_string()));
        assert_eq!(params.name, Some("data".to_string()));
        assert_eq!(params.level, Some(1));
        assert_eq!(
            params.devices,
            Some(vec!["/dev/sdb1".to_string(), "/dev/sdc1".to_string()])
        );
        assert_eq!(params.metadata, "1.2");
    }

    #[test]
    fn test_parse_params_assemble() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: assemble
            device: /dev/md0
            devices:
              - /dev/sdb1
              - /dev/sdc1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Assemble);
        assert_eq!(params.device, Some("/dev/md0".to_string()));
    }

    #[test]
    fn test_parse_params_stop() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: stop
            device: /dev/md0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Stop);
        assert_eq!(params.device, Some("/dev/md0".to_string()));
    }

    #[test]
    fn test_parse_params_destroy() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: destroy
            devices:
              - /dev/sdb1
              - /dev/sdc1
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Destroy);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: info
            device: /dev/md0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Info);
    }

    #[test]
    fn test_parse_params_default_action() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/md0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Info);
    }

    #[test]
    fn test_parse_params_with_chunk() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            device: /dev/md0
            level: 5
            devices:
              - /dev/sdb1
              - /dev/sdc1
              - /dev/sdd1
            chunk: 512K
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chunk, Some("512K".to_string()));
    }

    #[test]
    fn test_parse_params_with_bitmap() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            device: /dev/md0
            level: 1
            devices:
              - /dev/sdb1
              - /dev/sdc1
            bitmap: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.bitmap);
    }

    #[test]
    fn test_parse_params_with_spare_devices() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            device: /dev/md0
            level: 5
            devices:
              - /dev/sdb1
              - /dev/sdc1
              - /dev/sdd1
            spare_devices:
              - /dev/sde1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.spare_devices, Some(vec!["/dev/sde1".to_string()]));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: create
            device: /dev/md0
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device() {
        assert!(validate_device("/dev/md0").is_ok());
        assert!(validate_device("/dev/md/name").is_ok());
        assert!(validate_device("").is_err());
        assert!(validate_device("dev/md0").is_err());
    }

    #[test]
    fn test_parse_mdadm_detail() {
        let output = r#"        Version : 1.2
    Creation Time : Mon Jan  1 12:00:00 2024
       Raid Level : raid1
       Array Size : 1047552 (1023.00 MiB 1072.69 MB)
    Used Dev Size : 1047552 (1023.00 MiB 1072.69 MB)
      Raid Devices : 2
     Total Devices : 2
       Persistence : Superblock is persistent
             Name : data
             UUID : 12345678:abcdef00:12345678:abcdef00
           Events : 10
    Number   Major   Minor   RaidDevice State
       0       8       17        0      active sync   /dev/sdb1
       1       8       33        1      active sync   /dev/sdc1
"#;
        let info = parse_mdadm_detail(output, "/dev/md0").unwrap();
        assert_eq!(info.device, "/dev/md0");
        assert_eq!(info.name, Some("data".to_string()));
        assert_eq!(info.level, Some("raid1".to_string()));
        assert_eq!(
            info.uuid,
            Some("12345678:abcdef00:12345678:abcdef00".to_string())
        );
        assert!(info.devices.contains(&"/dev/sdb1".to_string()));
        assert!(info.devices.contains(&"/dev/sdc1".to_string()));
    }
}
