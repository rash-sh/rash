/// ANCHOR: module
/// # filesystem
///
/// Create filesystems on block devices.
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
/// - name: Create btrfs filesystem with options
///   filesystem:
///     dev: /dev/sdd1
///     fstype: btrfs
///     opts: -L mydata
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
use serde_norway::Value as YamlValue;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
enum FSType {
    Ext4,
    Ext3,
    Ext2,
    Xfs,
    Btrfs,
    Vfat,
    Swap,
}

impl FSType {
    fn mkfs_cmd(&self) -> &'static str {
        match self {
            FSType::Ext4 => "mkfs.ext4",
            FSType::Ext3 => "mkfs.ext3",
            FSType::Ext2 => "mkfs.ext2",
            FSType::Xfs => "mkfs.xfs",
            FSType::Btrfs => "mkfs.btrfs",
            FSType::Vfat => "mkfs.vfat",
            FSType::Swap => "mkswap",
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            FSType::Ext4 => "ext4",
            FSType::Ext3 => "ext3",
            FSType::Ext2 => "ext2",
            FSType::Xfs => "xfs",
            FSType::Btrfs => "btrfs",
            FSType::Vfat => "vfat",
            FSType::Swap => "swap",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Target block device path.
    dev: String,
    /// Filesystem type to create.
    fstype: FSType,
    /// Force filesystem creation even if the device already has a filesystem.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Additional options to pass to the mkfs command.
    opts: Option<String>,
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
            create_filesystem(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct FilesystemClient;

impl FilesystemClient {
    pub fn new() -> Self {
        FilesystemClient
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if !output.status.success() {
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

    pub fn has_filesystem(&self, dev: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("blkid")
                .arg("-o")
                .arg("value")
                .arg("-s")
                .arg("TYPE")
                .arg(dev),
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let fs_type = stdout.trim();

        Ok(!fs_type.is_empty())
    }

    pub fn create_filesystem(&self, params: &Params) -> Result<String> {
        let mkfs_cmd = params.fstype.mkfs_cmd();
        let mut cmd = Command::new(mkfs_cmd);

        if params.force {
            match params.fstype {
                FSType::Ext4 | FSType::Ext3 | FSType::Ext2 => {
                    cmd.arg("-F");
                }
                FSType::Xfs => {
                    cmd.arg("-f");
                }
                FSType::Btrfs => {
                    cmd.arg("-f");
                }
                FSType::Vfat => {
                    cmd.arg("-I");
                }
                FSType::Swap => {
                    cmd.arg("-f");
                }
            }
        }

        if let Some(opts) = &params.opts {
            for opt in opts.split_whitespace() {
                cmd.arg(opt);
            }
        }

        cmd.arg(&params.dev);

        let output = self.exec_cmd(&mut cmd)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }
}

fn validate_device(dev: &str) -> Result<()> {
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

    let path = Path::new(dev);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Device {} does not exist", dev),
        ));
    }

    Ok(())
}

fn create_filesystem(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_device(&params.dev)?;

    let client = FilesystemClient::new();

    let has_fs = client.has_filesystem(&params.dev)?;

    if has_fs && !params.force {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Device {} already has a filesystem. Use force=true to overwrite.",
                &params.dev
            ),
        ));
    }

    if has_fs && params.force {
        diff(
            format!("filesystem: present on {}", &params.dev),
            format!("filesystem: {} (will overwrite)", params.fstype.as_str()),
        );
    } else {
        diff(
            format!("filesystem: absent on {}", &params.dev),
            format!("filesystem: {}", params.fstype.as_str()),
        );
    }

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let output = client.create_filesystem(&params)?;

    Ok(ModuleResult::new(true, None, Some(output)))
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
                fstype: FSType::Ext4,
                force: false,
                opts: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            fstype: xfs
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                dev: "/dev/sdb1".to_owned(),
                fstype: FSType::Xfs,
                force: true,
                opts: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_opts() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            fstype: ext4
            opts: "-L mylabel"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                dev: "/dev/sdb1".to_owned(),
                fstype: FSType::Ext4,
                force: false,
                opts: Some("-L mylabel".to_owned()),
            }
        );
    }

    #[test]
    fn test_parse_params_all_fstypes() {
        for (fstype_str, expected) in [
            ("ext4", FSType::Ext4),
            ("ext3", FSType::Ext3),
            ("ext2", FSType::Ext2),
            ("xfs", FSType::Xfs),
            ("btrfs", FSType::Btrfs),
            ("vfat", FSType::Vfat),
            ("swap", FSType::Swap),
        ] {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                dev: /dev/sdb1
                fstype: {}
                "#,
                fstype_str
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.fstype, expected);
        }
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
    fn test_parse_params_no_fstype() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_fstype() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dev: /dev/sdb1
            fstype: invalid
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
            fstype: ext4
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
        let error = validate_device("/dev/sdb1\0").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_device_not_found() {
        let error = validate_device("/dev/nonexistent123").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_fstype_mkfs_cmd() {
        assert_eq!(FSType::Ext4.mkfs_cmd(), "mkfs.ext4");
        assert_eq!(FSType::Xfs.mkfs_cmd(), "mkfs.xfs");
        assert_eq!(FSType::Btrfs.mkfs_cmd(), "mkfs.btrfs");
        assert_eq!(FSType::Swap.mkfs_cmd(), "mkswap");
    }

    #[test]
    fn test_fstype_as_str() {
        assert_eq!(FSType::Ext4.as_str(), "ext4");
        assert_eq!(FSType::Xfs.as_str(), "xfs");
        assert_eq!(FSType::Btrfs.as_str(), "btrfs");
    }
}
