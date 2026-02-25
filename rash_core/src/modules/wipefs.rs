/// ANCHOR: module
/// # wipefs
///
/// Wipe filesystem, RAID, or partition table signatures from block devices.
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
/// - name: Wipe all signatures from disk
///   wipefs:
///     device: /dev/nvme0n1
///     all: true
///
/// - name: Wipe specific signature types
///   wipefs:
///     device: /dev/nvme0n1
///     types:
///       - zfs
///       - raid
///       - swap
///
/// - name: Wipe partition
///   wipefs:
///     device: /dev/nvme0n1p1
///
/// - name: Wipe multiple disks
///   wipefs:
///     device: "{{ item }}"
///   loop:
///     - /dev/nvme0n1
///     - /dev/nvme1n1
///
/// - name: Dry run to check signatures
///   wipefs:
///     device: /dev/sdb
///     no_act: true
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
    /// The block device path to wipe (e.g., /dev/sdb, /dev/nvme0n1).
    device: String,
    /// Wipe all signatures.
    /// **[default: `true`]**
    #[serde(default = "default_all")]
    all: bool,
    /// List of signature types to wipe (e.g., ext4, zfs, swap, raid).
    types: Option<Vec<String>>,
    /// Dry run / check mode - do not actually wipe.
    /// **[default: `false`]**
    #[serde(default)]
    no_act: bool,
    /// Force wipe even if the device is mounted.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Create a signature backup file before wiping.
    backup: Option<String>,
    /// Offset to start wiping (in bytes).
    offset: Option<u64>,
}

fn default_all() -> bool {
    true
}

#[derive(Debug)]
pub struct Wipefs;

impl Module for Wipefs {
    fn get_name(&self) -> &str {
        "wipefs"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            wipefs_module(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct WipefsClient {
    check_mode: bool,
}

impl WipefsClient {
    pub fn new(check_mode: bool) -> Self {
        WipefsClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn get_signatures(&self, device: &str) -> Result<Vec<SignatureInfo>> {
        let output = self.exec_cmd(
            Command::new("wipefs")
                .arg("-o")
                .arg("TYPE,UUID,LABEL,OFFSET")
                .arg(device)
                .env("LC_ALL", "C"),
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_wipefs_output(&stdout)
    }

    fn wipe_signatures(&self, params: &Params) -> Result<WipefsResult> {
        let signatures = self.get_signatures(&params.device)?;

        if signatures.is_empty() {
            return Ok(WipefsResult::no_change());
        }

        let signature_types: Vec<String> = signatures.iter().map(|s| s.type_str.clone()).collect();

        diff(
            format!(
                "signatures on {}: present ({:?})",
                params.device, signature_types
            ),
            format!("signatures on {}: absent", params.device),
        );

        if self.check_mode || params.no_act {
            return Ok(WipefsResult::with_signatures(true, signatures));
        }

        let mut cmd = Command::new("wipefs");

        if params.all {
            cmd.arg("--all");
        } else if let Some(types) = &params.types {
            for t in types {
                cmd.arg("--types").arg(t);
            }
        } else {
            cmd.arg("--all");
        }

        if params.force {
            cmd.arg("--force");
        }

        if let Some(backup) = &params.backup {
            cmd.arg("--backup").arg(backup);
        }

        if let Some(offset) = params.offset {
            cmd.arg("--offset").arg(offset.to_string());
        }

        cmd.arg(&params.device);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to wipe signatures: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(WipefsResult::with_signatures(true, signatures))
    }
}

#[derive(Debug, Clone)]
struct SignatureInfo {
    type_str: String,
    uuid: Option<String>,
    label: Option<String>,
    offset: Option<String>,
}

fn parse_wipefs_output(output: &str) -> Result<Vec<SignatureInfo>> {
    let mut signatures = Vec::new();

    for line in output.lines() {
        if line.starts_with("TYPE") || line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let type_str = parts.first().unwrap_or(&"").to_string();
        if type_str.is_empty() {
            continue;
        }

        let uuid = parts.get(1).and_then(|s| {
            if *s == "-" {
                None
            } else {
                Some((*s).to_string())
            }
        });
        let label = parts.get(2).and_then(|s| {
            if *s == "-" {
                None
            } else {
                Some((*s).to_string())
            }
        });
        let offset = parts.get(3).map(|s| s.to_string());

        signatures.push(SignatureInfo {
            type_str,
            uuid,
            label,
            offset,
        });
    }

    Ok(signatures)
}

#[derive(Debug)]
struct WipefsResult {
    changed: bool,
    signatures: Vec<SignatureInfo>,
}

impl WipefsResult {
    fn no_change() -> Self {
        WipefsResult {
            changed: false,
            signatures: Vec::new(),
        }
    }

    fn with_signatures(changed: bool, signatures: Vec<SignatureInfo>) -> Self {
        WipefsResult {
            changed,
            signatures,
        }
    }
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

    if device.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "device path contains null character",
        ));
    }

    if !Path::new(device).exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Device {} does not exist", device),
        ));
    }

    Ok(())
}

fn wipefs_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.device)?;

    let client = WipefsClient::new(check_mode);
    let result = client.wipe_signatures(&params)?;

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );

    let signatures_removed: Vec<serde_json::Value> = result
        .signatures
        .iter()
        .map(|s| {
            let mut map = serde_json::Map::new();
            map.insert(
                "type".to_string(),
                serde_json::Value::String(s.type_str.clone()),
            );
            if let Some(uuid) = &s.uuid {
                map.insert("uuid".to_string(), serde_json::Value::String(uuid.clone()));
            }
            if let Some(label) = &s.label {
                map.insert(
                    "label".to_string(),
                    serde_json::Value::String(label.clone()),
                );
            }
            if let Some(offset) = &s.offset {
                map.insert(
                    "offset".to_string(),
                    serde_json::Value::String(offset.clone()),
                );
            }
            serde_json::Value::Object(map)
        })
        .collect();

    extra.insert(
        "signatures_removed".to_string(),
        serde_json::Value::Array(signatures_removed),
    );

    Ok(ModuleResult::new(
        result.changed,
        Some(serde_norway::to_value(extra)?),
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
                all: true,
                types: None,
                no_act: false,
                force: false,
                backup: None,
                offset: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_types() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            types:
              - zfs
              - raid
              - swap
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                all: true,
                types: Some(vec!["zfs".to_owned(), "raid".to_owned(), "swap".to_owned()]),
                no_act: false,
                force: false,
                backup: None,
                offset: None,
            }
        );
    }

    #[test]
    fn test_parse_params_all_false() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            all: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.all);
    }

    #[test]
    fn test_parse_params_with_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            all: true
            force: true
            no_act: true
            backup: /tmp/backup
            offset: 1024
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.all);
        assert!(params.force);
        assert!(params.no_act);
        assert_eq!(params.backup, Some("/tmp/backup".to_owned()));
        assert_eq!(params.offset, Some(1024));
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
    fn test_parse_params_no_device() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            all: true
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device() {
        assert!(
            validate_device("/dev/sdb").is_ok()
                || validate_device("/dev/sdb").unwrap_err().kind() == ErrorKind::NotFound
        );
        assert!(validate_device("").is_err());
        assert!(validate_device("dev/sdb").is_err());
        assert!(validate_device("/dev/sdb\0").is_err());
    }

    #[test]
    fn test_validate_device_empty() {
        let error = validate_device("").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_relative_path() {
        let error = validate_device("dev/sdb").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_null_char() {
        let error = validate_device("/dev/sdb\0").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_wipefs_output() {
        let output =
            "ext4    123e4567-e89b-12d3-a456-426614174000  mylabel  0x438\nzfs     -  -  0x0\n";
        let signatures = parse_wipefs_output(output).unwrap();
        assert_eq!(signatures.len(), 2);
        assert_eq!(signatures[0].type_str, "ext4");
        assert_eq!(
            signatures[0].uuid,
            Some("123e4567-e89b-12d3-a456-426614174000".to_string())
        );
        assert_eq!(signatures[0].label, Some("mylabel".to_string()));
        assert_eq!(signatures[1].type_str, "zfs");
        assert_eq!(signatures[1].uuid, None);
        assert_eq!(signatures[1].label, None);
    }

    #[test]
    fn test_parse_wipefs_output_empty() {
        let output = "";
        let signatures = parse_wipefs_output(output).unwrap();
        assert_eq!(signatures.len(), 0);
    }

    #[test]
    fn test_parse_wipefs_output_header_only() {
        let output = "TYPE UUID LABEL OFFSET\n";
        let signatures = parse_wipefs_output(output).unwrap();
        assert_eq!(signatures.len(), 0);
    }

    #[test]
    fn test_wipefs_result_no_change() {
        let result = WipefsResult::no_change();
        assert!(!result.changed);
        assert_eq!(result.signatures.len(), 0);
    }

    #[test]
    fn test_wipefs_result_with_signatures() {
        let signatures = vec![SignatureInfo {
            type_str: "ext4".to_string(),
            uuid: Some("uuid".to_string()),
            label: None,
            offset: Some("0x438".to_string()),
        }];
        let result = WipefsResult::with_signatures(true, signatures);
        assert!(result.changed);
        assert_eq!(result.signatures.len(), 1);
    }
}
