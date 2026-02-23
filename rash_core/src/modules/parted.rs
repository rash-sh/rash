/// ANCHOR: module
/// # parted
///
/// Manage disk partitions using parted.
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
/// - name: Create partition
///   parted:
///     device: /dev/sdb
///     number: 1
///     state: present
///     part_start: 0%
///     part_end: 100%
///
/// - name: Create partition with filesystem type
///   parted:
///     device: /dev/sdb
///     number: 2
///     state: present
///     part_start: 50%
///     part_end: 100%
///     fs_type: ext4
///
/// - name: Remove partition
///   parted:
///     device: /dev/sdb
///     number: 1
///     state: absent
///
/// - name: Get partition info
///   parted:
///     device: /dev/sdb
///     state: info
///   register: part_info
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
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
    Info,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The block device (e.g., /dev/sdb).
    device: String,
    /// The partition number (1-128 for GPT, 1-4 for MBR).
    number: Option<u32>,
    /// Desired state of the partition.
    /// If _present_, create the partition if it doesn't exist.
    /// If _absent_, remove the partition if it exists.
    /// If _info_, return information about partitions on the device.
    /// **[default: `"info"`]**
    state: Option<State>,
    /// Start of the partition (e.g., "0%", "1GB", "100MB").
    /// **[default: `"0%"`]**
    part_start: Option<String>,
    /// End of the partition (e.g., "100%", "10GB", "500MB").
    /// **[default: `"100%"`]**
    part_end: Option<String>,
    /// Filesystem type for the partition (e.g., ext4, xfs, fat32).
    fs_type: Option<String>,
    /// Disk label type (e.g., gpt, msdos). Only used when creating a new partition table.
    label: Option<String>,
}

#[derive(Debug)]
pub struct Parted;

impl Module for Parted {
    fn get_name(&self) -> &str {
        "parted"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            parted_module(parse_params(optional_params)?, check_mode)?,
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

struct PartedClient {
    check_mode: bool,
}

impl PartedClient {
    pub fn new(check_mode: bool) -> Self {
        PartedClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn get_partition_info(&self, device: &str) -> Result<Vec<PartitionInfo>> {
        let output = self.exec_cmd(
            Command::new("parted")
                .args(["-s", "-m", device, "print"])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("unrecognised disk label") {
                return Ok(Vec::new());
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to get partition info: {}", stderr.trim()),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_parted_output(&stdout)
    }

    fn partition_exists(&self, device: &str, number: u32) -> Result<bool> {
        let partitions = self.get_partition_info(device)?;
        Ok(partitions.iter().any(|p| p.number == number))
    }

    fn create_partition(&self, params: &Params) -> Result<PartedResult> {
        let number = params.number.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "number is required when state is present",
            )
        })?;

        if self.partition_exists(&params.device, number)? {
            return Ok(PartedResult::no_change());
        }

        let part_start = params.part_start.as_deref().unwrap_or("0%");
        let part_end = params.part_end.as_deref().unwrap_or("100%");

        diff(
            format!("partition {} on {}: absent", number, params.device),
            format!(
                "partition {} on {}: present ({} - {})",
                number, params.device, part_start, part_end
            ),
        );

        if self.check_mode {
            return Ok(PartedResult::new(true));
        }

        if let Some(label) = &params.label {
            let output = self.exec_cmd(
                Command::new("parted")
                    .args(["-s", &params.device, "mklabel", label])
                    .env("LC_ALL", "C"),
            )?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create disk label: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        }

        let mut cmd = Command::new("parted");
        cmd.args(["-s", "-a", "optimal", &params.device, "mkpart"]);

        if let Some(fs_type) = &params.fs_type {
            cmd.arg(fs_type);
        }

        cmd.args([part_start, part_end]).env("LC_ALL", "C");

        let output = self.exec_cmd(&mut cmd)?;
        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to create partition: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(PartedResult::new(true))
    }

    fn remove_partition(&self, params: &Params) -> Result<PartedResult> {
        let number = params.number.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "number is required when state is absent",
            )
        })?;

        if !self.partition_exists(&params.device, number)? {
            return Ok(PartedResult::no_change());
        }

        diff(
            format!("partition {} on {}: present", number, params.device),
            format!("partition {} on {}: absent", number, params.device),
        );

        if self.check_mode {
            return Ok(PartedResult::new(true));
        }

        let output = self.exec_cmd(
            Command::new("parted")
                .args(["-s", &params.device, "rm", &number.to_string()])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to remove partition: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(PartedResult::new(true))
    }

    fn get_info(&self, params: &Params) -> Result<PartedResult> {
        let partitions = self.get_partition_info(&params.device)?;
        Ok(PartedResult::with_info(false, partitions))
    }
}

#[derive(Debug, Clone)]
struct PartitionInfo {
    number: u32,
    start: String,
    end: String,
    size: String,
    fs_type: Option<String>,
    name: Option<String>,
    flags: Option<String>,
}

fn parse_parted_output(output: &str) -> Result<Vec<PartitionInfo>> {
    let mut partitions = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 4
            && let Ok(number) = parts[0].parse::<u32>()
        {
            let fs_type = if parts.len() > 4 && !parts[4].is_empty() {
                Some(parts[4].to_string())
            } else {
                None
            };
            let name = if parts.len() > 5 && !parts[5].is_empty() {
                Some(parts[5].to_string())
            } else {
                None
            };
            let flags = if parts.len() > 6 && !parts[6].is_empty() {
                Some(parts[6].to_string())
            } else {
                None
            };

            partitions.push(PartitionInfo {
                number,
                start: parts[1].to_string(),
                end: parts[2].to_string(),
                size: parts[3].to_string(),
                fs_type,
                name,
                flags,
            });
        }
    }

    Ok(partitions)
}

#[derive(Debug)]
struct PartedResult {
    changed: bool,
    partitions: Option<Vec<PartitionInfo>>,
}

impl PartedResult {
    fn new(changed: bool) -> Self {
        PartedResult {
            changed,
            partitions: None,
        }
    }

    fn no_change() -> Self {
        PartedResult {
            changed: false,
            partitions: None,
        }
    }

    fn with_info(changed: bool, partitions: Vec<PartitionInfo>) -> Self {
        PartedResult {
            changed,
            partitions: Some(partitions),
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

    Ok(())
}

fn parted_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.device)?;

    let client = PartedClient::new(check_mode);

    let result = match params.state.unwrap_or(State::Info) {
        State::Present => client.create_partition(&params)?,
        State::Absent => client.remove_partition(&params)?,
        State::Info => client.get_info(&params)?,
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );

    if let Some(partitions) = &result.partitions {
        let partitions_json: Vec<serde_json::Value> = partitions
            .iter()
            .map(|p| {
                let mut map = serde_json::Map::new();
                map.insert(
                    "number".to_string(),
                    serde_json::Value::Number(p.number.into()),
                );
                map.insert(
                    "start".to_string(),
                    serde_json::Value::String(p.start.clone()),
                );
                map.insert("end".to_string(), serde_json::Value::String(p.end.clone()));
                map.insert(
                    "size".to_string(),
                    serde_json::Value::String(p.size.clone()),
                );
                if let Some(fs_type) = &p.fs_type {
                    map.insert(
                        "fstype".to_string(),
                        serde_json::Value::String(fs_type.clone()),
                    );
                }
                if let Some(name) = &p.name {
                    map.insert("name".to_string(), serde_json::Value::String(name.clone()));
                }
                if let Some(flags) = &p.flags {
                    map.insert(
                        "flags".to_string(),
                        serde_json::Value::String(flags.clone()),
                    );
                }
                serde_json::Value::Object(map)
            })
            .collect();
        extra.insert(
            "partitions".to_string(),
            serde_json::Value::Array(partitions_json),
        );
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            part_start: 0%
            part_end: 100%
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/sdb".to_owned(),
                number: Some(1),
                state: Some(State::Present),
                part_start: Some("0%".to_owned()),
                part_end: Some("100%".to_owned()),
                fs_type: None,
                label: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_fs_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            fs_type: ext4
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/sdb".to_owned(),
                number: Some(1),
                state: Some(State::Present),
                part_start: None,
                part_end: None,
                fs_type: Some("ext4".to_owned()),
                label: None,
            }
        );
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            state: info
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/sdb".to_owned(),
                number: None,
                state: Some(State::Info),
                part_start: None,
                part_end: None,
                fs_type: None,
                label: None,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/sdb".to_owned(),
                number: Some(1),
                state: Some(State::Absent),
                part_start: None,
                part_end: None,
                fs_type: None,
                label: None,
            }
        );
    }

    #[test]
    fn test_parse_params_no_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device() {
        assert!(validate_device("/dev/sdb").is_ok());
        assert!(validate_device("/dev/nvme0n1").is_ok());
        assert!(validate_device("").is_err());
        assert!(validate_device("dev/sdb").is_err());
    }

    #[test]
    fn test_parse_parted_output() {
        let output = "1:1049kB:1074MB:1073MB:ext4::boot;\n2:1075MB:2149MB:1074MB:xfs::;\n";
        let partitions = parse_parted_output(output).unwrap();
        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[0].number, 1);
        assert_eq!(partitions[0].fs_type, Some("ext4".to_string()));
        assert_eq!(partitions[0].flags, Some("boot;".to_string()));
        assert_eq!(partitions[1].number, 2);
        assert_eq!(partitions[1].fs_type, Some("xfs".to_string()));
    }
}
