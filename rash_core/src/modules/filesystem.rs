/// ANCHOR: module
/// # filesystem
///
/// Create and manage filesystems on block devices.
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
/// - name: Create ext4 filesystem
///   filesystem:
///     dev: /dev/sdb1
///     fstype: ext4
///
/// - name: Create xfs filesystem with force
///   filesystem:
///     dev: /dev/sdc1
///     fstype: xfs
///     force: true
///
/// - name: Create filesystem with custom options
///   filesystem:
///     dev: /dev/sdb1
///     fstype: ext4
///     opts: -L mylabel -m reserved_blocks=0
///
/// - name: Resize filesystem to match device size
///   filesystem:
///     dev: /dev/sdb1
///     fstype: ext4
///     resizefs: true
///
/// - name: Check if filesystem exists (wipes if different type)
///   filesystem:
///     dev: /dev/sdb1
///     fstype: ext4
///   register: fs_info
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
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Target block device path.
    dev: String,
    /// Filesystem type to create (ext4, xfs, btrfs, ext3, ext2, vfat, etc.).
    /// Required when state is present.
    fstype: Option<String>,
    /// State of the filesystem.
    /// If _present_, the filesystem will be created if it doesn't exist.
    /// If _absent_, any filesystem on the device will be wiped.
    /// **[default: `"present"`]**
    state: Option<State>,
    /// Force filesystem creation even if the device already has a filesystem.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Additional options to pass to mkfs command.
    opts: Option<String>,
    /// Resize the filesystem to match the device size.
    /// Only works for ext2/ext3/ext4 filesystems.
    /// **[default: `false`]**
    #[serde(default)]
    resizefs: bool,
}

#[derive(Debug)]
pub struct Filesystem;

impl Module for Filesystem {
    fn get_name(&self) -> &str {
        "filesystem"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            filesystem_module(parse_params(optional_params)?, check_mode)?,
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

struct FilesystemClient {
    check_mode: bool,
}

impl FilesystemClient {
    pub fn new(check_mode: bool) -> Self {
        FilesystemClient { check_mode }
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
                    "Error executing filesystem command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn get_filesystem_info(&self, dev: &str) -> Result<Option<FilesystemInfo>> {
        let output = self.exec_cmd(
            Command::new("blkid").arg("-o").arg("export").arg(dev),
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut fstype = None;
        let mut uuid = None;
        let mut label = None;

        for line in stdout.lines() {
            if let Some(stripped) = line.strip_prefix("TYPE=") {
                fstype = Some(stripped.trim_matches('"').to_string());
            } else if let Some(stripped) = line.strip_prefix("UUID=") {
                uuid = Some(stripped.trim_matches('"').to_string());
            } else if let Some(stripped) = line.strip_prefix("LABEL=") {
                label = Some(stripped.trim_matches('"').to_string());
            }
        }

        Ok(fstype.map(|fstype| FilesystemInfo {
            fstype,
            uuid,
            label,
        }))
    }

    pub fn device_exists(&self, dev: &str) -> Result<bool> {
        Ok(Path::new(dev).exists())
    }

    pub fn create_filesystem(&self, params: &Params) -> Result<FilesystemResult> {
        let fstype = params.fstype.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "fstype is required when state is present",
            )
        })?;

        let existing_fs = self.get_filesystem_info(&params.dev)?;

        if let Some(ref info) = existing_fs
            && !params.force
            && info.fstype == *fstype
        {
            if params.resizefs {
                return self.resize_filesystem(&params.dev, fstype);
            }
            return Ok(FilesystemResult::no_change());
        }

        let mkfs_cmd = self.get_mkfs_command(fstype)?;
        let mut cmd = Command::new(mkfs_cmd);

        if params.force {
            let fstype_str = fstype.as_str();
            if fstype_str == "xfs" || fstype_str == "btrfs" {
                cmd.arg("-f");
            } else {
                cmd.arg("-F");
            }
        }

        if let Some(opts) = &params.opts {
            for opt in opts.split_whitespace() {
                cmd.arg(opt);
            }
        }

        cmd.arg(&params.dev);

        diff(
            format!(
                "filesystem: {} ({})",
                existing_fs
                    .as_ref()
                    .map(|i| i.fstype.as_str())
                    .unwrap_or("none"),
                &params.dev
            ),
            format!("filesystem: {} ({})", fstype, &params.dev),
        );

        if self.check_mode {
            return Ok(FilesystemResult::new(true, None));
        }

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(FilesystemResult::new(true, output_str))
    }

    fn get_mkfs_command(&self, fstype: &str) -> Result<&'static str> {
        match fstype {
            "ext4" => Ok("mkfs.ext4"),
            "ext3" => Ok("mkfs.ext3"),
            "ext2" => Ok("mkfs.ext2"),
            "xfs" => Ok("mkfs.xfs"),
            "btrfs" => Ok("mkfs.btrfs"),
            "vfat" | "fat32" => Ok("mkfs.vfat"),
            "ntfs" => Ok("mkfs.ntfs"),
            "swap" => Ok("mkswap"),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unsupported filesystem type: {fstype}"),
            )),
        }
    }

    pub fn wipe_filesystem(&self, dev: &str) -> Result<FilesystemResult> {
        let existing_fs = self.get_filesystem_info(dev)?;

        if existing_fs.is_none() {
            return Ok(FilesystemResult::no_change());
        }

        diff(
            format!(
                "filesystem: {} ({})",
                existing_fs
                    .as_ref()
                    .map(|i| i.fstype.as_str())
                    .unwrap_or("none"),
                dev
            ),
            format!("filesystem: absent ({dev})"),
        );

        if self.check_mode {
            return Ok(FilesystemResult::new(true, None));
        }

        let mut cmd = Command::new("wipefs");
        cmd.arg("--all").arg(dev);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(FilesystemResult::new(true, output_str))
    }

    pub fn resize_filesystem(&self, dev: &str, fstype: &str) -> Result<FilesystemResult> {
        if !matches!(fstype, "ext2" | "ext3" | "ext4") {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("resizefs is only supported for ext2/ext3/ext4, got: {fstype}"),
            ));
        }

        diff(
            format!("filesystem size: current ({dev})"),
            format!("filesystem size: resized ({dev})"),
        );

        if self.check_mode {
            return Ok(FilesystemResult::new(true, None));
        }

        let mut cmd = Command::new("resize2fs");
        cmd.arg(dev);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(FilesystemResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct FilesystemResult {
    changed: bool,
    output: Option<String>,
}

impl FilesystemResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        FilesystemResult { changed, output }
    }

    fn no_change() -> Self {
        FilesystemResult {
            changed: false,
            output: None,
        }
    }
}

#[derive(Debug)]
struct FilesystemInfo {
    fstype: String,
    uuid: Option<String>,
    label: Option<String>,
}

fn validate_dev(dev: &str) -> Result<()> {
    if dev.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path cannot be empty",
        ));
    }

    if dev.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path contains null character",
        ));
    }

    if !dev.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Device path must be absolute",
        ));
    }

    Ok(())
}

fn filesystem_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_dev(&params.dev)?;

    let client = FilesystemClient::new(check_mode);

    if !client.device_exists(&params.dev)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Device {} does not exist", &params.dev),
        ));
    }

    let result = match params.state.unwrap_or(State::Present) {
        State::Present => client.create_filesystem(&params)?,
        State::Absent => client.wipe_filesystem(&params.dev)?,
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "dev".to_string(),
        serde_json::Value::String(params.dev.clone()),
    );

    if let Some(info) = client.get_filesystem_info(&params.dev)? {
        extra.insert("fstype".to_string(), serde_json::Value::String(info.fstype));
        if let Some(uuid) = info.uuid {
            extra.insert("uuid".to_string(), serde_json::Value::String(uuid));
        }
        if let Some(label) = info.label {
            extra.insert("label".to_string(), serde_json::Value::String(label));
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
            dev: /dev/sdb1
            fstype: ext4
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                dev: "/dev/sdb1".to_owned(),
                fstype: Some("ext4".to_owned()),
                state: None,
                force: false,
                opts: None,
                resizefs: false,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            fstype: xfs
            state: present
            force: true
            opts: "-L mylabel"
            resizefs: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                dev: "/dev/sdb1".to_owned(),
                fstype: Some("xfs".to_owned()),
                state: Some(State::Present),
                force: true,
                opts: Some("-L mylabel".to_owned()),
                resizefs: false,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                dev: "/dev/sdb1".to_owned(),
                fstype: None,
                state: Some(State::Absent),
                force: false,
                opts: None,
                resizefs: false,
            }
        );
    }

    #[test]
    fn test_parse_params_no_dev() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            fstype: ext4
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
            dev: /dev/sdb1
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_dev() {
        assert!(validate_dev("/dev/sdb1").is_ok());
        assert!(validate_dev("/dev/mapper/vg-lv").is_ok());
        assert!(validate_dev("").is_err());
        assert!(validate_dev("dev/sdb1").is_err());
        assert!(validate_dev("path\0with\0null").is_err());
    }

    #[test]
    fn test_get_mkfs_command() {
        let client = FilesystemClient::new(false);
        assert_eq!(client.get_mkfs_command("ext4").unwrap(), "mkfs.ext4");
        assert_eq!(client.get_mkfs_command("ext3").unwrap(), "mkfs.ext3");
        assert_eq!(client.get_mkfs_command("ext2").unwrap(), "mkfs.ext2");
        assert_eq!(client.get_mkfs_command("xfs").unwrap(), "mkfs.xfs");
        assert_eq!(client.get_mkfs_command("btrfs").unwrap(), "mkfs.btrfs");
        assert_eq!(client.get_mkfs_command("vfat").unwrap(), "mkfs.vfat");
        assert_eq!(client.get_mkfs_command("fat32").unwrap(), "mkfs.vfat");
        assert_eq!(client.get_mkfs_command("ntfs").unwrap(), "mkfs.ntfs");
        assert_eq!(client.get_mkfs_command("swap").unwrap(), "mkswap");
        assert!(client.get_mkfs_command("unknown").is_err());
    }
}
