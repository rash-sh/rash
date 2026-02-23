/// ANCHOR: module
/// # parted
///
/// Configure block device partitions using GNU Parted.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: limited
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Read device information
///   parted:
///     device: /dev/sdb
///     unit: MiB
///
/// - name: Create a new ext4 primary partition
///   parted:
///     device: /dev/sdb
///     number: 1
///     state: present
///     fs_type: ext4
///
/// - name: Create a new primary partition with a size of 1GiB
///   parted:
///     device: /dev/sdb
///     number: 1
///     state: present
///     part_end: 1GiB
///
/// - name: Create a new primary partition for LVM
///   parted:
///     device: /dev/sdb
///     number: 2
///     flags: [lvm]
///     state: present
///     part_start: 1GiB
///
/// - name: Remove partition number 1
///   parted:
///     device: /dev/sdb
///     number: 1
///     state: absent
///
/// - name: Create a new GPT partition table
///   parted:
///     device: /dev/sdb
///     label: gpt
///
/// - name: Extend an existing partition to fill all available space
///   parted:
///     device: /dev/sdb
///     number: 1
///     part_end: "100%"
///     resize: true
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{parse_params, Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;

use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    #[default]
    Info,
    Present,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Align {
    Cylinder,
    Minimal,
    None,
    #[default]
    Optimal,
    Undefined,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Label {
    Aix,
    Amiga,
    Bsd,
    Dvh,
    Gpt,
    Loop,
    Mac,
    #[default]
    Msdos,
    Pc98,
    Sun,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum PartType {
    Extended,
    Logical,
    #[default]
    Primary,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Unit {
    S,
    B,
    KB,
    #[default]
    KiB,
    MB,
    MiB,
    GB,
    GiB,
    TB,
    TiB,
    Percent,
    Cyl,
    Chs,
    Compact,
}

fn default_part_start() -> String {
    "0%".to_string()
}

fn default_part_end() -> String {
    "100%".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Set alignment for newly created partitions.
    /// **[default: `"optimal"`]**
    #[serde(default)]
    align: Option<Align>,
    /// The block device (disk) where to operate.
    device: String,
    /// A list of the flags that has to be set on the partition.
    flags: Option<Vec<String>>,
    /// If specified and the partition does not exist, sets filesystem type to given partition.
    fs_type: Option<String>,
    /// Disk label type or partition table to use.
    /// **[default: `"msdos"`]**
    #[serde(default)]
    label: Option<Label>,
    /// Sets the name for the partition number (GPT, Mac, MIPS and PC98 only).
    name: Option<String>,
    /// The partition number being affected.
    /// Required when performing any action on the disk, except fetching information.
    number: Option<i64>,
    /// Where the partition ends as offset from the beginning of the disk.
    /// **[default: `"100%"`]**
    #[serde(default = "default_part_end")]
    part_end: String,
    /// Where the partition starts as offset from the beginning of the disk.
    /// **[default: `"0%"`]**
    #[serde(default = "default_part_start")]
    part_start: String,
    /// May be specified only with label=msdos or label=dvh.
    /// **[default: `"primary"`]**
    #[serde(default)]
    part_type: Option<PartType>,
    /// Call resizepart on existing partitions to match the size specified by part_end.
    #[serde(default)]
    resize: Option<bool>,
    /// Whether to create or delete a partition.
    /// If set to info the module only returns the device information.
    /// **[default: `"info"`]**
    #[serde(default)]
    state: Option<State>,
    /// Selects the current default unit that Parted uses to display locations and capacities.
    /// **[default: `"KiB"`]**
    #[serde(default)]
    unit: Option<Unit>,
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
        Ok((parted(parse_params(optional_params)?, check_mode)?, None))
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

    fn get_partition_info(&self, device: &str, unit: &Unit) -> Result<PartitionInfo> {
        let unit_str = match unit {
            Unit::S => "s",
            Unit::B => "B",
            Unit::KB => "KB",
            Unit::KiB => "KiB",
            Unit::MB => "MB",
            Unit::MiB => "MiB",
            Unit::GB => "GB",
            Unit::GiB => "GiB",
            Unit::TB => "TB",
            Unit::TiB => "TiB",
            Unit::Percent => "%",
            Unit::Cyl => "cyl",
            Unit::Chs => "chs",
            Unit::Compact => "compact",
        };

        let mut cmd = Command::new("parted");
        cmd.args(["-s", "-m", device, "unit", unit_str, "print"]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error executing parted: {}", stderr.trim()),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_parted_output(&stdout, device, unit_str)
    }

    fn create_partition_table(&self, device: &str, label: &Label) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let label_str = match label {
            Label::Aix => "aix",
            Label::Amiga => "amiga",
            Label::Bsd => "bsd",
            Label::Dvh => "dvh",
            Label::Gpt => "gpt",
            Label::Loop => "loop",
            Label::Mac => "mac",
            Label::Msdos => "msdos",
            Label::Pc98 => "pc98",
            Label::Sun => "sun",
        };

        let mut cmd = Command::new("parted");
        cmd.args(["-s", device, "mklabel", label_str]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error creating partition table: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn create_partition(
        &self,
        device: &str,
        _number: i64,
        part_start: &str,
        part_end: &str,
        part_type: &PartType,
        fs_type: Option<&str>,
        align: &Align,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let align_str = match align {
            Align::Cylinder => "cylinder",
            Align::Minimal => "minimal",
            Align::None => "none",
            Align::Optimal => "optimal",
            Align::Undefined => "undefined",
        };

        let part_type_str = match part_type {
            PartType::Extended => "extended",
            PartType::Logical => "logical",
            PartType::Primary => "primary",
        };

        let fs_type_arg = fs_type.unwrap_or("");

        let mut cmd = Command::new("parted");
        cmd.args(["-s", "-a", align_str, device, "mkpart", part_type_str]);

        if !fs_type_arg.is_empty() {
            cmd.arg(fs_type_arg);
        }

        cmd.args([part_start, part_end]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error creating partition: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn delete_partition(&self, device: &str, number: i64) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = Command::new("parted");
        cmd.args(["-s", device, "rm", &number.to_string()]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error deleting partition: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn set_partition_flag(
        &self,
        device: &str,
        number: i64,
        flag: &str,
        state: bool,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let state_str = if state { "on" } else { "off" };

        let mut cmd = Command::new("parted");
        cmd.args(["-s", device, "set", &number.to_string(), flag, state_str]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error setting partition flag: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn resize_partition(&self, device: &str, number: i64, part_end: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = Command::new("parted");
        cmd.args(["-s", device, "resizepart", &number.to_string(), part_end]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error resizing partition: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn set_partition_name(&self, device: &str, number: i64, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = Command::new("parted");
        cmd.args(["-s", device, "name", &number.to_string(), name]);

        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error setting partition name: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }
}

#[derive(Debug, Clone)]
struct Partition {
    num: i64,
    begin: f64,
    end: f64,
    size: f64,
    fs_type: String,
    name: String,
    flags: Vec<String>,
}

#[derive(Debug)]
struct PartitionInfo {
    disk: DiskInfo,
    partitions: Vec<Partition>,
    script: String,
}

#[derive(Debug)]
struct DiskInfo {
    dev: String,
    size: f64,
    unit: String,
    table: String,
    model: String,
    logical_block: u64,
    physical_block: u64,
}

fn parse_parted_output(output: &str, device: &str, unit: &str) -> Result<PartitionInfo> {
    let lines: Vec<&str> = output.lines().collect();
    let mut disk_info = DiskInfo {
        dev: device.to_string(),
        size: 0.0,
        unit: unit.to_string(),
        table: String::new(),
        model: String::new(),
        logical_block: 512,
        physical_block: 512,
    };
    let mut partitions = Vec::new();

    for line in &lines {
        if line.starts_with("BYT;") {
            continue;
        }

        if line.contains(':')
            && !line.starts_with(' ')
            && !line.starts_with("1")
            && !line.starts_with("2")
            && !line.starts_with("3")
            && !line.starts_with("4")
            && !line.starts_with("5")
            && !line.starts_with("6")
            && !line.starts_with("7")
            && !line.starts_with("8")
            && !line.starts_with("9")
        {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                disk_info.dev = device.to_string();
                if let Ok(size) = parts[1].parse::<f64>() {
                    disk_info.size = size;
                }
                disk_info.table = parts.get(2).unwrap_or(&"").to_string();
                disk_info.model = parts.get(3).unwrap_or(&"").to_string();
                if parts.len() >= 5
                    && let Ok(lb) = parts[4].parse::<u64>()
                {
                    disk_info.logical_block = lb;
                }
                if parts.len() >= 6
                    && let Ok(pb) = parts[5].parse::<u64>()
                {
                    disk_info.physical_block = pb;
                }
            }
        } else if line.contains(':') {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 5 {
                let num = parts[0].parse::<i64>().unwrap_or(0);
                let begin = parts[1].parse::<f64>().unwrap_or(0.0);
                let end = parts[2].parse::<f64>().unwrap_or(0.0);
                let size = parts[3].parse::<f64>().unwrap_or(0.0);
                let fs_type = parts.get(4).unwrap_or(&"").to_string();
                let name = parts.get(5).unwrap_or(&"").to_string();
                let flags_str = parts.get(6).unwrap_or(&"").to_string();
                let flags: Vec<String> = if flags_str.is_empty() {
                    Vec::new()
                } else {
                    flags_str.split(',').map(|s| s.trim().to_string()).collect()
                };

                partitions.push(Partition {
                    num,
                    begin,
                    end,
                    size,
                    fs_type,
                    name,
                    flags,
                });
            }
        }
    }

    Ok(PartitionInfo {
        disk: disk_info,
        partitions,
        script: format!("unit {} print ", unit),
    })
}

fn validate_device_path(device: &str) -> Result<()> {
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

    Ok(())
}

fn parted(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device_path(&params.device)?;

    let client = PartedClient::new(check_mode);
    let state = params.state.clone().unwrap_or_default();
    let unit = params.unit.clone().unwrap_or_default();

    match state {
        State::Info => {
            let info = client.get_partition_info(&params.device, &unit)?;

            let disk_json = serde_json::json!({
                "dev": info.disk.dev,
                "size": info.disk.size,
                "unit": info.disk.unit,
                "table": info.disk.table,
                "model": info.disk.model,
                "logical_block": info.disk.logical_block,
                "physical_block": info.disk.physical_block,
            });

            let partitions_json: Vec<serde_json::Value> = info
                .partitions
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "num": p.num,
                        "begin": p.begin,
                        "end": p.end,
                        "size": p.size,
                        "fstype": p.fs_type,
                        "name": p.name,
                        "flags": p.flags,
                    })
                })
                .collect();

            let extra = serde_json::json!({
                "disk": disk_json,
                "partitions": partitions_json,
                "script": info.script,
            });

            Ok(ModuleResult {
                changed: false,
                output: None,
                extra: Some(serde_norway::to_value(extra)?),
            })
        }
        State::Present => {
            let number = params.number.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "number is required when state=present",
                )
            })?;

            let mut changed = false;

            if let Some(label) = &params.label {
                let info = client.get_partition_info(&params.device, &unit)?;
                let current_table = info.disk.table.to_lowercase();
                let new_label = match label {
                    Label::Aix => "aix",
                    Label::Amiga => "amiga",
                    Label::Bsd => "bsd",
                    Label::Dvh => "dvh",
                    Label::Gpt => "gpt",
                    Label::Loop => "loop",
                    Label::Mac => "mac",
                    Label::Msdos => "msdos",
                    Label::Pc98 => "pc98",
                    Label::Sun => "sun",
                };

                if current_table != new_label {
                    diff(
                        format!("label: {}", current_table),
                        format!("label: {}", new_label),
                    );
                    client.create_partition_table(&params.device, label)?;
                    changed = true;
                }
            }

            let info = client.get_partition_info(&params.device, &unit)?;
            let existing_partition = info.partitions.iter().find(|p| p.num == number);

            if let Some(_partition) = existing_partition {
                if params.resize.unwrap_or(false) {
                    let resize_result =
                        client.resize_partition(&params.device, number, &params.part_end)?;
                    if resize_result {
                        diff(
                            "partition size: old",
                            format!("partition size: {}", params.part_end),
                        );
                        changed = true;
                    }
                }
            } else {
                let part_type = params.part_type.clone().unwrap_or_default();
                let align = params.align.clone().unwrap_or_default();

                diff(
                    format!("partition {} absent", number),
                    format!("partition {} present", number),
                );
                client.create_partition(
                    &params.device,
                    number,
                    &params.part_start,
                    &params.part_end,
                    &part_type,
                    params.fs_type.as_deref(),
                    &align,
                )?;
                changed = true;
            }

            if let Some(name) = &params.name {
                let set_result = client.set_partition_name(&params.device, number, name)?;
                if set_result {
                    changed = true;
                }
            }

            if let Some(flags) = &params.flags {
                for flag in flags {
                    let flag_result =
                        client.set_partition_flag(&params.device, number, flag, true)?;
                    if flag_result {
                        diff(format!("flag {} off", flag), format!("flag {} on", flag));
                        changed = true;
                    }
                }
            }

            let final_info = client.get_partition_info(&params.device, &unit)?;

            let disk_json = serde_json::json!({
                "dev": final_info.disk.dev,
                "size": final_info.disk.size,
                "unit": final_info.disk.unit,
                "table": final_info.disk.table,
                "model": final_info.disk.model,
                "logical_block": final_info.disk.logical_block,
                "physical_block": final_info.disk.physical_block,
            });

            let partitions_json: Vec<serde_json::Value> = final_info
                .partitions
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "num": p.num,
                        "begin": p.begin,
                        "end": p.end,
                        "size": p.size,
                        "fstype": p.fs_type,
                        "name": p.name,
                        "flags": p.flags,
                    })
                })
                .collect();

            let extra = serde_json::json!({
                "disk": disk_json,
                "partitions": partitions_json,
                "script": final_info.script,
            });

            Ok(ModuleResult {
                changed,
                output: None,
                extra: Some(serde_norway::to_value(extra)?),
            })
        }
        State::Absent => {
            let number = params.number.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "number is required when state=absent",
                )
            })?;

            let info = client.get_partition_info(&params.device, &unit)?;
            let existing_partition = info.partitions.iter().find(|p| p.num == number);

            if existing_partition.is_some() {
                diff(
                    format!("partition {} present", number),
                    format!("partition {} absent", number),
                );
                client.delete_partition(&params.device, number)?;

                let final_info = client.get_partition_info(&params.device, &unit)?;

                let disk_json = serde_json::json!({
                    "dev": final_info.disk.dev,
                    "size": final_info.disk.size,
                    "unit": final_info.disk.unit,
                    "table": final_info.disk.table,
                    "model": final_info.disk.model,
                    "logical_block": final_info.disk.logical_block,
                    "physical_block": final_info.disk.physical_block,
                });

                let partitions_json: Vec<serde_json::Value> = final_info
                    .partitions
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "num": p.num,
                            "begin": p.begin,
                            "end": p.end,
                            "size": p.size,
                            "fstype": p.fs_type,
                            "name": p.name,
                            "flags": p.flags,
                        })
                    })
                    .collect();

                let extra = serde_json::json!({
                    "disk": disk_json,
                    "partitions": partitions_json,
                    "script": final_info.script,
                });

                Ok(ModuleResult {
                    changed: true,
                    output: None,
                    extra: Some(serde_norway::to_value(extra)?),
                })
            } else {
                let disk_json = serde_json::json!({
                    "dev": info.disk.dev,
                    "size": info.disk.size,
                    "unit": info.disk.unit,
                    "table": info.disk.table,
                    "model": info.disk.model,
                    "logical_block": info.disk.logical_block,
                    "physical_block": info.disk.physical_block,
                });

                let partitions_json: Vec<serde_json::Value> = info
                    .partitions
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "num": p.num,
                            "begin": p.begin,
                            "end": p.end,
                            "size": p.size,
                            "fstype": p.fs_type,
                            "name": p.name,
                            "flags": p.flags,
                        })
                    })
                    .collect();

                let extra = serde_json::json!({
                    "disk": disk_json,
                    "partitions": partitions_json,
                    "script": info.script,
                });

                Ok(ModuleResult {
                    changed: false,
                    output: None,
                    extra: Some(serde_norway::to_value(extra)?),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "/dev/sdb");
        assert_eq!(params.state, None);
        assert_eq!(params.number, None);
    }

    #[test]
    fn test_parse_params_present() {
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
        assert_eq!(params.device, "/dev/sdb");
        assert_eq!(params.number, Some(1));
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.fs_type, Some("ext4".to_string()));
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
        assert_eq!(params.device, "/dev/sdb");
        assert_eq!(params.number, Some(1));
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_label() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            label: gpt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.label, Some(Label::Gpt));
    }

    #[test]
    fn test_parse_params_with_flags() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            flags:
              - lvm
              - boot
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.flags,
            Some(vec!["lvm".to_string(), "boot".to_string()])
        );
    }

    #[test]
    fn test_parse_params_with_part_start_end() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            part_start: 0%
            part_end: 50%
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.part_start, "0%");
        assert_eq!(params.part_end, "50%");
    }

    #[test]
    fn test_parse_params_with_align() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            align: optimal
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.align, Some(Align::Optimal));
    }

    #[test]
    fn test_parse_params_with_resize() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            resize: true
            part_end: 100%
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.resize, Some(true));
    }

    #[test]
    fn test_parse_params_with_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb
            number: 1
            state: present
            name: root
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("root".to_string()));
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
    fn test_validate_device_path() {
        assert!(validate_device_path("/dev/sdb").is_ok());
        assert!(validate_device_path("").is_err());
        assert!(validate_device_path("/dev/null\0").is_err());
    }

    #[test]
    fn test_default_values() {
        assert_eq!(default_part_start(), "0%");
        assert_eq!(default_part_end(), "100%");
    }
}
