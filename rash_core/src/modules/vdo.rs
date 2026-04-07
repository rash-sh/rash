/// ANCHOR: module
/// # vdo
///
/// Manage VDO (Virtual Data Optimizer) volumes.
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
/// - name: Create VDO volume
///   vdo:
///     name: vdo_vol
///     device: /dev/sdb
///     logicalsize: 100G
///
/// - name: Create VDO volume with compression disabled
///   vdo:
///     name: vdo_vol
///     device: /dev/sdb
///     logicalsize: 200G
///     compression: false
///
/// - name: Create VDO volume with deduplication disabled
///   vdo:
///     name: vdo_vol
///     device: /dev/sdb
///     logicalsize: 50G
///     deduplication: false
///
/// - name: Remove VDO volume
///   vdo:
///     name: vdo_vol
///     state: absent
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
#[derive(Default)]
enum State {
    #[default]
    Present,
    Absent,
}

fn default_compression() -> bool {
    true
}

fn default_deduplication() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// VDO volume name.
    name: String,
    /// Underlying block device.
    device: Option<String>,
    /// Logical size of the VDO volume (e.g., 100G, 1T).
    logicalsize: Option<String>,
    /// Enable compression.
    /// **[default: `true`]**
    #[serde(default = "default_compression")]
    compression: bool,
    /// Enable deduplication.
    /// **[default: `true`]**
    #[serde(default = "default_deduplication")]
    deduplication: bool,
    /// Whether the VDO volume should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
}

#[derive(Debug)]
pub struct Vdo;

impl Module for Vdo {
    fn get_name(&self) -> &str {
        "vdo"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            vdo_module(parse_params(optional_params)?, check_mode)?,
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

struct VdoClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct VdoInfo {
    #[allow(dead_code)]
    name: String,
    device: String,
    logical_size: Option<String>,
    physical_size: Option<String>,
    compression: Option<String>,
    deduplication: Option<String>,
    operating_mode: Option<String>,
    write_policy: Option<String>,
}

impl VdoClient {
    pub fn new(check_mode: bool) -> Self {
        VdoClient { check_mode }
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
                    "Error executing VDO command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn vdo_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(Command::new("vdo").args(["list"]).env("LC_ALL", "C"), false)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.trim() == name {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn get_vdo_info(&self, name: &str) -> Result<Option<VdoInfo>> {
        let output = self.exec_cmd(
            Command::new("vdo")
                .args(["status", "--name", name])
                .env("LC_ALL", "C"),
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Some(parse_vdo_status(&stdout, name)?))
    }

    pub fn create_vdo(&self, params: &Params) -> Result<VdoResult> {
        let device = params.device.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "device is required when state is present",
            )
        })?;

        let logicalsize = params.logicalsize.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "logicalsize is required when state is present",
            )
        })?;

        let _vdo_path = format!("/dev/mapper/{}", params.name);

        diff(
            format!("VDO volume {}: absent", params.name),
            format!(
                "VDO volume {}: present (device={}, logicalsize={})",
                params.name, device, logicalsize
            ),
        );

        if self.check_mode {
            return Ok(VdoResult::new(true, None));
        }

        let mut cmd = Command::new("vdo");
        cmd.args(["create", "--name", &params.name])
            .args(["--device", device])
            .args(["--vdoLogicalSize", logicalsize]);

        if !params.compression {
            cmd.arg("--compression");
            cmd.arg("disabled");
        }

        if !params.deduplication {
            cmd.arg("--deduplication");
            cmd.arg("disabled");
        }

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(VdoResult::new(true, output_str))
    }

    pub fn remove_vdo(&self, params: &Params) -> Result<VdoResult> {
        let vdo_path = format!("/dev/mapper/{}", params.name);

        diff(
            format!("VDO volume {}: present ({})", params.name, vdo_path),
            format!("VDO volume {}: absent", params.name),
        );

        if self.check_mode {
            return Ok(VdoResult::new(true, None));
        }

        let mut cmd = Command::new("vdo");
        cmd.args(["remove", "--name", &params.name]);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(VdoResult::new(true, output_str))
    }

    pub fn update_vdo_settings(&self, params: &Params) -> Result<VdoResult> {
        let current_info = self.get_vdo_info(&params.name)?;
        let current_info = current_info.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "Cannot update settings: VDO volume does not exist",
            )
        })?;

        let compression_changed = current_info.compression.as_ref().is_some_and(|c| {
            let enabled = c == "enabled" || c == "online";
            enabled != params.compression
        });

        let deduplication_changed = current_info.deduplication.as_ref().is_some_and(|d| {
            let enabled = d == "enabled" || d == "online";
            enabled != params.deduplication
        });

        if !compression_changed && !deduplication_changed {
            return Ok(VdoResult::no_change());
        }

        if compression_changed {
            diff(
                format!(
                    "compression: {}",
                    current_info
                        .compression
                        .as_ref()
                        .map_or("unknown", |s| s.as_str())
                ),
                format!(
                    "compression: {}",
                    if params.compression {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
        }

        if deduplication_changed {
            diff(
                format!(
                    "deduplication: {}",
                    current_info
                        .deduplication
                        .as_ref()
                        .map_or("unknown", |s| s.as_str())
                ),
                format!(
                    "deduplication: {}",
                    if params.deduplication {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
        }

        if self.check_mode {
            return Ok(VdoResult::new(true, None));
        }

        let mut cmd = Command::new("vdo");
        cmd.args(["modify", "--name", &params.name]);

        if compression_changed {
            cmd.arg("--compression");
            cmd.arg(if params.compression {
                "enabled"
            } else {
                "disabled"
            });
        }

        if deduplication_changed {
            cmd.arg("--deduplication");
            cmd.arg(if params.deduplication {
                "enabled"
            } else {
                "disabled"
            });
        }

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(VdoResult::new(true, output_str))
    }
}

fn parse_vdo_status(output: &str, name: &str) -> Result<VdoInfo> {
    let mut device = String::new();
    let mut logical_size = None;
    let mut physical_size = None;
    let mut compression = None;
    let mut deduplication = None;
    let mut operating_mode = None;
    let mut write_policy = None;

    let mut in_vdo_section = false;

    for line in output.lines() {
        let line = line.trim();

        if line.starts_with("VDO volume") && line.contains(name) {
            in_vdo_section = true;
            continue;
        }

        if in_vdo_section {
            if line.is_empty() || line.starts_with("VDO volume") {
                break;
            }

            if let Some(value) = parse_field(line, "Device mapper name") {
                device = value;
            } else if let Some(value) = parse_field(line, "Physical size") {
                physical_size = Some(value);
            } else if let Some(value) = parse_field(line, "Logical size") {
                logical_size = Some(value);
            } else if let Some(value) = parse_field(line, "Compression") {
                compression = Some(value);
            } else if let Some(value) = parse_field(line, "Deduplication") {
                deduplication = Some(value);
            } else if let Some(value) = parse_field(line, "Operating mode") {
                operating_mode = Some(value);
            } else if let Some(value) = parse_field(line, "Write policy") {
                write_policy = Some(value);
            }
        }
    }

    Ok(VdoInfo {
        name: name.to_string(),
        device,
        logical_size,
        physical_size,
        compression,
        deduplication,
        operating_mode,
        write_policy,
    })
}

fn parse_field(line: &str, field_name: &str) -> Option<String> {
    if line.starts_with(field_name) {
        let colon_pos = line.find(':')?;
        Some(line[colon_pos + 1..].trim().to_string())
    } else {
        None
    }
}

#[derive(Debug)]
struct VdoResult {
    changed: bool,
    output: Option<String>,
}

impl VdoResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        VdoResult { changed, output }
    }

    fn no_change() -> Self {
        VdoResult {
            changed: false,
            output: None,
        }
    }
}

fn validate_params(params: &Params) -> Result<()> {
    if params.name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    if params.state == State::Present {
        if params.device.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "device is required when state is present",
            ));
        }
        if params.logicalsize.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "logicalsize is required when state is present",
            ));
        }
    }

    if let Some(device) = &params.device {
        if device.is_empty() {
            return Err(Error::new(ErrorKind::InvalidData, "device cannot be empty"));
        }
        if !device.starts_with('/') {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "device must be an absolute path",
            ));
        }
    }

    if let Some(size) = &params.logicalsize
        && size.is_empty()
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "logicalsize cannot be empty",
        ));
    }

    Ok(())
}

fn vdo_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = VdoClient::new(check_mode);
    let vdo_exists = client.vdo_exists(&params.name)?;

    let result = match params.state {
        State::Present => {
            if vdo_exists {
                client.update_vdo_settings(&params)?
            } else {
                client.create_vdo(&params)?
            }
        }
        State::Absent => {
            if vdo_exists {
                client.remove_vdo(&params)?
            } else {
                VdoResult::no_change()
            }
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert(
        "exists".to_string(),
        serde_json::Value::Bool(client.vdo_exists(&params.name)?),
    );

    if let Some(info) = client.get_vdo_info(&params.name)? {
        extra.insert("device".to_string(), serde_json::Value::String(info.device));
        if let Some(size) = info.logical_size {
            extra.insert("logical_size".to_string(), serde_json::Value::String(size));
        }
        if let Some(size) = info.physical_size {
            extra.insert("physical_size".to_string(), serde_json::Value::String(size));
        }
        if let Some(c) = info.compression {
            extra.insert("compression".to_string(), serde_json::Value::String(c));
        }
        if let Some(d) = info.deduplication {
            extra.insert("deduplication".to_string(), serde_json::Value::String(d));
        }
        if let Some(m) = info.operating_mode {
            extra.insert("operating_mode".to_string(), serde_json::Value::String(m));
        }
        if let Some(w) = info.write_policy {
            extra.insert("write_policy".to_string(), serde_json::Value::String(w));
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            device: /dev/sdb
            logicalsize: 100G
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "vdo_vol".to_owned(),
                device: Some("/dev/sdb".to_owned()),
                logicalsize: Some("100G".to_owned()),
                compression: true,
                deduplication: true,
                state: State::Present,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            device: /dev/sdb
            logicalsize: 100G
            compression: false
            deduplication: false
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert!(!params.compression);
        assert!(!params.deduplication);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.device, None);
        assert_eq!(params.logicalsize, None);
    }

    #[test]
    fn test_parse_params_compression_disabled() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            device: /dev/sdb
            logicalsize: 200G
            compression: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.compression);
        assert!(params.deduplication);
    }

    #[test]
    fn test_parse_params_deduplication_disabled() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            device: /dev/sdb
            logicalsize: 50G
            deduplication: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.compression);
        assert!(!params.deduplication);
    }

    #[test]
    fn test_validate_params_empty_name() {
        let params = Params {
            name: "".to_string(),
            device: Some("/dev/sdb".to_string()),
            logicalsize: Some("100G".to_string()),
            compression: true,
            deduplication: true,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_missing_device() {
        let params = Params {
            name: "vdo_vol".to_string(),
            device: None,
            logicalsize: Some("100G".to_string()),
            compression: true,
            deduplication: true,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_missing_logicalsize() {
        let params = Params {
            name: "vdo_vol".to_string(),
            device: Some("/dev/sdb".to_string()),
            logicalsize: None,
            compression: true,
            deduplication: true,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_empty_device() {
        let params = Params {
            name: "vdo_vol".to_string(),
            device: Some("".to_string()),
            logicalsize: Some("100G".to_string()),
            compression: true,
            deduplication: true,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_relative_device() {
        let params = Params {
            name: "vdo_vol".to_string(),
            device: Some("dev/sdb".to_string()),
            logicalsize: Some("100G".to_string()),
            compression: true,
            deduplication: true,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_absent_no_device() {
        let params = Params {
            name: "vdo_vol".to_string(),
            device: None,
            logicalsize: None,
            compression: true,
            deduplication: true,
            state: State::Absent,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vdo_vol
            device: /dev/sdb
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_field() {
        let line = "Compression: enabled";
        assert_eq!(
            parse_field(line, "Compression"),
            Some("enabled".to_string())
        );

        let line = "Logical size: 100G";
        assert_eq!(parse_field(line, "Logical size"), Some("100G".to_string()));

        let line = "Some other field: value";
        assert_eq!(parse_field(line, "Compression"), None);
    }

    #[test]
    fn test_parse_vdo_status() {
        let output = r#"VDO volume statistics:
VDO volume: vdo_vol
  Device mapper name: vdo_vol
  Physical size: 10G
  Logical size: 100G
  Compression: enabled
  Deduplication: enabled
  Operating mode: normal
  Write policy: sync
"#;
        let info = parse_vdo_status(output, "vdo_vol").unwrap();
        assert_eq!(info.name, "vdo_vol");
        assert_eq!(info.device, "vdo_vol");
        assert_eq!(info.physical_size, Some("10G".to_string()));
        assert_eq!(info.logical_size, Some("100G".to_string()));
        assert_eq!(info.compression, Some("enabled".to_string()));
        assert_eq!(info.deduplication, Some("enabled".to_string()));
        assert_eq!(info.operating_mode, Some("normal".to_string()));
        assert_eq!(info.write_policy, Some("sync".to_string()));
    }
}
