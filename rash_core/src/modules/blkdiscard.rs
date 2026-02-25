/// ANCHOR: module
/// # blkdiscard
///
/// Securely erase SSDs and NVMe drives using the blkdiscard command.
///
/// This module discards (TRIMs) blocks on a device, which is essential for
/// SSDs and NVMe drives to maintain performance and longevity. Unlike
/// traditional hard drives, SSDs need TRIM/DISCARD commands for proper reset.
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
/// - name: Secure erase SSD
///   blkdiscard:
///     device: /dev/nvme0n1
///
/// - name: Secure erase with zeroout
///   blkdiscard:
///     device: /dev/nvme0n1
///     zeroout: true
///
/// - name: Discard specific range
///   blkdiscard:
///     device: /dev/nvme0n1
///     offset: 0
///     length: 1073741824
///
/// - name: Force discard (dangerous)
///   blkdiscard:
///     device: /dev/nvme0n1
///     force: true
///
/// - name: Secure erase multiple disks
///   blkdiscard:
///     device: "{{ item }}"
///   loop:
///     - /dev/nvme0n1
///     - /dev/nvme1n1
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::path::Path;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Block device path (e.g., /dev/nvme0n1).
    device: String,
    /// Force discard even if device is mounted (dangerous).
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Perform secure erase.
    /// **[default: `false`]**
    #[serde(default)]
    secure: bool,
    /// Zero out instead of discard.
    /// **[default: `false`]**
    #[serde(default)]
    zeroout: bool,
    /// Starting offset in bytes.
    offset: Option<u64>,
    /// Length in bytes to discard.
    length: Option<u64>,
    /// Step size for incremental discard.
    step: Option<u64>,
}

#[derive(Debug)]
pub struct Blkdiscard;

impl Module for Blkdiscard {
    fn get_name(&self) -> &str {
        "blkdiscard"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            blkdiscard_module(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct BlkdiscardClient;

impl BlkdiscardClient {
    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn get_device_size(&self, device: &str) -> Result<u64> {
        let output = self.exec_cmd(
            Command::new("blockdev")
                .args(["--getsize64", device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to get device size: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .trim()
            .parse::<u64>()
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }

    fn is_device_mounted(&self, device: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("findmnt")
                .args(["-n", "-o", "TARGET", "-S", device])
                .env("LC_ALL", "C"),
        )?;

        Ok(output.status.success() && !output.stdout.is_empty())
    }

    fn supports_discard(&self, device: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("lsblk")
                .args(["-d", "-n", "-o", "DISC-GRAN", device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to check discard support: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let gran = stdout.trim();

        Ok(!gran.is_empty() && gran != "0B")
    }

    fn run_blkdiscard(&self, params: &Params) -> Result<u64> {
        let mut cmd = Command::new("blkdiscard");

        if params.force {
            cmd.arg("--force");
        }

        if params.secure {
            cmd.arg("--secure");
        }

        if params.zeroout {
            cmd.arg("--zeroout");
        }

        if let Some(offset) = params.offset {
            cmd.arg("--offset").arg(offset.to_string());
        }

        if let Some(length) = params.length {
            cmd.arg("--length").arg(length.to_string());
        }

        if let Some(step) = params.step {
            cmd.arg("--step").arg(step.to_string());
        }

        cmd.arg(&params.device);
        cmd.env("LC_ALL", "C");

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("blkdiscard failed: {}", stderr.trim()),
            ));
        }

        let bytes_discarded = if let Some(length) = params.length {
            length
        } else {
            self.get_device_size(&params.device)?
        };

        Ok(bytes_discarded)
    }
}

fn validate_device(device: &str) -> Result<()> {
    if device.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path cannot be empty",
        ));
    }

    if device.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path contains null character",
        ));
    }

    if !device.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path must be absolute",
        ));
    }

    let path = Path::new(device);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Device {} does not exist", device),
        ));
    }

    Ok(())
}

fn blkdiscard_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.device)?;

    let client = BlkdiscardClient;

    if !client.supports_discard(&params.device)? {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Device {} does not support discard/TRIM operations",
                params.device
            ),
        ));
    }

    if client.is_device_mounted(&params.device)? && !params.force {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Device {} is mounted. Use force=true to discard anyway (dangerous).",
                params.device
            ),
        ));
    }

    let bytes_discarded = if let Some(length) = params.length {
        length
    } else {
        client.get_device_size(&params.device)?
    };

    let mode_desc = if params.zeroout {
        "zeroout"
    } else if params.secure {
        "secure erase"
    } else {
        "discard"
    };

    diff(
        format!("{}: {}", params.device, mode_desc),
        format!(
            "{}: {} ({} bytes)",
            params.device, mode_desc, bytes_discarded
        ),
    );

    if check_mode {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "device".to_string(),
            serde_json::Value::String(params.device.clone()),
        );
        extra.insert(
            "bytes_discarded".to_string(),
            serde_json::Value::Number(bytes_discarded.into()),
        );

        return Ok(ModuleResult::new(
            true,
            Some(serde_norway::to_value(extra).map_err(|e| Error::new(ErrorKind::InvalidData, e))?),
            None,
        ));
    }

    let actual_bytes = client.run_blkdiscard(&params)?;

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );
    extra.insert(
        "bytes_discarded".to_string(),
        serde_json::Value::Number(actual_bytes.into()),
    );

    Ok(ModuleResult::new(
        true,
        Some(serde_norway::to_value(extra).map_err(|e| Error::new(ErrorKind::InvalidData, e))?),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                force: false,
                secure: false,
                zeroout: false,
                offset: None,
                length: None,
                step: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            force: true
            secure: true
            offset: 0
            length: 1073741824
            step: 1048576
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                force: true,
                secure: true,
                zeroout: false,
                offset: Some(0),
                length: Some(1073741824),
                step: Some(1048576),
            }
        );
    }

    #[test]
    fn test_parse_params_zeroout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            zeroout: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.zeroout);
        assert!(!params.secure);
    }

    #[test]
    fn test_parse_params_no_device() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            force: true
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_empty() {
        let error = validate_device("").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_null_char() {
        let error = validate_device("/dev/nvme0n1\0").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_relative_path() {
        let error = validate_device("nvme0n1").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_not_found() {
        let error = validate_device("/dev/nonexistent123").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }
}
