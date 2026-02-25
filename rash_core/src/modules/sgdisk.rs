/// ANCHOR: module
/// # sgdisk
///
/// Manage GPT disk partitions using sgdisk (part of gdisk/gptfdisk).
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
/// - name: Wipe disk and create GPT
///   sgdisk:
///     device: /dev/nvme0n1
///     zap: true
///
/// - name: Create BIOS boot partition
///   sgdisk:
///     device: /dev/nvme0n1
///     number: 1
///     state: present
///     part_start: 0
///     part_end: +1M
///     part_type: EF02
///     part_name: BIOS-BOOT
///
/// - name: Create EFI system partition
///   sgdisk:
///     device: /dev/nvme0n1
///     number: 2
///     state: present
///     part_start: 1M
///     part_end: +512M
///     part_type: EF00
///     part_name: EFI-SYSTEM
///
/// - name: Create ZFS partition
///   sgdisk:
///     device: /dev/nvme0n1
///     number: 3
///     state: present
///     part_start: 513M
///     part_end: 100%
///     part_type: BF00
///     part_name: ZFS
///
/// - name: Get partition info
///   sgdisk:
///     device: /dev/nvme0n1
///     state: info
///   register: part_info
///
/// - name: Remove partition
///   sgdisk:
///     device: /dev/nvme0n1
///     number: 1
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
    /// The block device (e.g., /dev/nvme0n1, /dev/sda).
    device: String,
    /// The partition number (1-128 for GPT).
    number: Option<u32>,
    /// Desired state of the partition.
    /// If _present_, create the partition if it doesn't exist.
    /// If _absent_, remove the partition if it exists.
    /// If _info_, return information about partitions on the device.
    /// **[default: `"info"`]**
    state: Option<State>,
    /// Start of the partition as sector number or size (e.g., "0", "1M", "2048").
    /// **[default: `"0"`]**
    part_start: Option<String>,
    /// End of the partition as sector number or size (e.g., "100%", "512M", "+1G").
    /// **[default: `"100%"`]**
    part_end: Option<String>,
    /// Partition type GUID or code (e.g., EF00 for EFI, 8300 for Linux, BF00 for ZFS).
    part_type: Option<String>,
    /// Partition name/label.
    part_name: Option<String>,
    /// Specific partition GUID.
    part_guid: Option<String>,
    /// Wipe all partitions on the device.
    /// **[default: `false`]**
    zap: Option<bool>,
}

#[derive(Debug)]
pub struct Sgdisk;

impl Module for Sgdisk {
    fn get_name(&self) -> &str {
        "sgdisk"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            sgdisk_module(parse_params(optional_params)?, check_mode)?,
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

struct SgdiskClient {
    check_mode: bool,
}

impl SgdiskClient {
    pub fn new(check_mode: bool) -> Self {
        SgdiskClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn get_partition_info(&self, device: &str) -> Result<(Vec<PartitionInfo>, Option<String>)> {
        let output = self.exec_cmd(
            Command::new("sgdisk")
                .args(["-p", device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("disk is invalid") || stderr.contains("does not exist") {
                return Ok((Vec::new(), None));
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to get partition info: {}", stderr.trim()),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_sgdisk_output(&stdout)
    }

    fn partition_exists(&self, device: &str, number: u32) -> Result<bool> {
        let (partitions, _) = self.get_partition_info(device)?;
        Ok(partitions.iter().any(|p| p.number == number))
    }

    fn zap_disk(&self, params: &Params) -> Result<SgdiskResult> {
        diff(
            format!("partitions on {}: present", params.device),
            format!("partitions on {}: wiped", params.device),
        );

        if self.check_mode {
            return Ok(SgdiskResult::new(true));
        }

        let output = self.exec_cmd(
            Command::new("sgdisk")
                .args(["-Z", &params.device])
                .env("LC_ALL", "C"),
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to zap disk: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(SgdiskResult::new(true))
    }

    fn create_partition(&self, params: &Params) -> Result<SgdiskResult> {
        let number = params.number.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "number is required when state is present",
            )
        })?;

        if self.partition_exists(&params.device, number)? {
            return Ok(SgdiskResult::no_change());
        }

        let part_start = params.part_start.as_deref().unwrap_or("0");
        let part_end = params.part_end.as_deref().unwrap_or("100%");

        diff(
            format!("partition {} on {}: absent", number, params.device),
            format!(
                "partition {} on {}: present ({} - {})",
                number, params.device, part_start, part_end
            ),
        );

        if self.check_mode {
            return Ok(SgdiskResult::new(true));
        }

        let mut args: Vec<String> = vec!["-n".to_string()];
        args.push(format!(
            "{}:{}:{}",
            number,
            part_start,
            part_end.trim_start_matches('+')
        ));

        if let Some(part_type) = &params.part_type {
            args.push("-t".to_string());
            args.push(format!("{}:{}", number, part_type));
        }

        if let Some(part_name) = &params.part_name {
            args.push("-c".to_string());
            args.push(format!("{}:{}", number, part_name));
        }

        if let Some(part_guid) = &params.part_guid {
            args.push("-u".to_string());
            args.push(format!("{}:{}", number, part_guid));
        }

        args.push(params.device.clone());

        let output = self.exec_cmd(Command::new("sgdisk").args(&args).env("LC_ALL", "C"))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to create partition: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(SgdiskResult::new(true))
    }

    fn remove_partition(&self, params: &Params) -> Result<SgdiskResult> {
        let number = params.number.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "number is required when state is absent",
            )
        })?;

        if !self.partition_exists(&params.device, number)? {
            return Ok(SgdiskResult::no_change());
        }

        diff(
            format!("partition {} on {}: present", number, params.device),
            format!("partition {} on {}: absent", number, params.device),
        );

        if self.check_mode {
            return Ok(SgdiskResult::new(true));
        }

        let output = self.exec_cmd(
            Command::new("sgdisk")
                .args(["-d", &number.to_string(), &params.device])
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

        Ok(SgdiskResult::new(true))
    }

    fn get_info(&self, params: &Params) -> Result<SgdiskResult> {
        let (partitions, disk_guid) = self.get_partition_info(&params.device)?;
        Ok(SgdiskResult::with_info(false, partitions, disk_guid))
    }
}

#[derive(Debug, Clone)]
struct PartitionInfo {
    number: u32,
    start: String,
    end: String,
    size: String,
    code: Option<String>,
    name: Option<String>,
    guid: Option<String>,
}

fn parse_sgdisk_output(output: &str) -> Result<(Vec<PartitionInfo>, Option<String>)> {
    let mut partitions = Vec::new();
    let mut disk_guid: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Disk identifier (GUID):") {
            if let Some(guid) = trimmed.split(':').nth(1) {
                disk_guid = Some(guid.trim().to_string());
            }
            continue;
        }

        if let Some(_rest) = trimmed.strip_prefix("Number ") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4
            && let Ok(number) = parts[0].parse::<u32>()
        {
            let code = if parts.len() > 5 {
                Some(parts[5].to_string())
            } else {
                None
            };
            let name = if parts.len() > 6 {
                Some(parts[6..].join(" "))
            } else {
                None
            };

            partitions.push(PartitionInfo {
                number,
                start: parts[1].to_string(),
                end: parts[2].to_string(),
                size: parts[3].to_string(),
                code,
                name,
                guid: None,
            });
        }
    }

    Ok((partitions, disk_guid))
}

#[derive(Debug)]
struct SgdiskResult {
    changed: bool,
    partitions: Option<Vec<PartitionInfo>>,
    disk_guid: Option<String>,
}

impl SgdiskResult {
    fn new(changed: bool) -> Self {
        SgdiskResult {
            changed,
            partitions: None,
            disk_guid: None,
        }
    }

    fn no_change() -> Self {
        SgdiskResult {
            changed: false,
            partitions: None,
            disk_guid: None,
        }
    }

    fn with_info(changed: bool, partitions: Vec<PartitionInfo>, disk_guid: Option<String>) -> Self {
        SgdiskResult {
            changed,
            partitions: Some(partitions),
            disk_guid,
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

fn sgdisk_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.device)?;

    let client = SgdiskClient::new(check_mode);

    let result = if params.zap.unwrap_or(false) {
        client.zap_disk(&params)?
    } else {
        match params.state.unwrap_or(State::Info) {
            State::Present => client.create_partition(&params)?,
            State::Absent => client.remove_partition(&params)?,
            State::Info => client.get_info(&params)?,
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );

    if let Some(disk_guid) = &result.disk_guid {
        extra.insert(
            "disk_guid".to_string(),
            serde_json::Value::String(disk_guid.clone()),
        );
    }

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
                if let Some(code) = &p.code {
                    map.insert("code".to_string(), serde_json::Value::String(code.clone()));
                }
                if let Some(name) = &p.name {
                    map.insert("name".to_string(), serde_json::Value::String(name.clone()));
                }
                if let Some(guid) = &p.guid {
                    map.insert("guid".to_string(), serde_json::Value::String(guid.clone()));
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
            device: /dev/nvme0n1
            number: 1
            state: present
            part_start: "0"
            part_end: 100%
            part_type: EF00
            part_name: EFI-SYSTEM
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                number: Some(1),
                state: Some(State::Present),
                part_start: Some("0".to_owned()),
                part_end: Some("100%".to_owned()),
                part_type: Some("EF00".to_owned()),
                part_name: Some("EFI-SYSTEM".to_owned()),
                part_guid: None,
                zap: None,
            }
        );
    }

    #[test]
    fn test_parse_params_zap() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            zap: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                number: None,
                state: None,
                part_start: None,
                part_end: None,
                part_type: None,
                part_name: None,
                part_guid: None,
                zap: Some(true),
            }
        );
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            state: info
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                number: None,
                state: Some(State::Info),
                part_start: None,
                part_end: None,
                part_type: None,
                part_name: None,
                part_guid: None,
                zap: None,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
            number: 1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/nvme0n1".to_owned(),
                number: Some(1),
                state: Some(State::Absent),
                part_start: None,
                part_end: None,
                part_type: None,
                part_name: None,
                part_guid: None,
                zap: None,
            }
        );
    }

    #[test]
    fn test_parse_params_no_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0n1
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
            device: /dev/nvme0n1
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
    fn test_parse_sgdisk_output() {
        let output = r#"Disk /dev/nvme0n1: 1953525168 sectors, 931.5 GiB
Model: Samsung SSD 970 EVO Plus 1TB
Sector size (logical/physical): 512/512 bytes
Disk identifier (GUID): A1B2C3D4-E5F6-7890-ABCD-EF1234567890
Partition table holds up to 128 entries
First usable sector is 34, last usable sector is 1953525134
Partitions will be aligned on 2048-sector boundaries
Total free space is 2047 sectors (1023.5 KiB)

Number  Start (sector)    End (sector)  Size       Code  Name
   1            2048         1050623   512.0 MiB  EF00  EFI-SYSTEM
   2         1050624        20973567   9.5 GiB    8200  SWAP
   3        20973568      1953523711   921.5 GiB  8300  LINUX
"#;
        let (partitions, disk_guid) = parse_sgdisk_output(output).unwrap();
        assert_eq!(partitions.len(), 3);
        assert_eq!(
            disk_guid,
            Some("A1B2C3D4-E5F6-7890-ABCD-EF1234567890".to_string())
        );
        assert_eq!(partitions[0].number, 1);
        assert_eq!(partitions[0].code, Some("EF00".to_string()));
        assert_eq!(partitions[0].name, Some("EFI-SYSTEM".to_string()));
        assert_eq!(partitions[1].number, 2);
        assert_eq!(partitions[1].code, Some("8200".to_string()));
        assert_eq!(partitions[2].number, 3);
        assert_eq!(partitions[2].code, Some("8300".to_string()));
    }
}
