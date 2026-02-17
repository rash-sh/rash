/// ANCHOR: module
/// # mount
///
/// Control filesystem mounts.
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
/// - name: Mount data volume
///   mount:
///     path: /mnt/data
///     src: /dev/sdb1
///     fstype: ext4
///     state: mounted
///
/// - name: Unmount data volume
///   mount:
///     path: /mnt/data
///     state: unmounted
///
/// - name: Mount NFS share
///   mount:
///     path: /mnt/nfs
///     src: 192.168.1.100:/export/data
///     fstype: nfs
///     opts: rw,hard,intr
///     state: mounted
///
/// - name: Remount with new options
///   mount:
///     path: /mnt/data
///     state: remounted
///
/// - name: Get mount info
///   mount:
///     path: /mnt/data
///     state: mounted
///   register: mount_info
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
    Absent,
    Mounted,
    Unmounted,
    Remounted,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the mount point.
    path: String,
    /// Device to be mounted on path. Required when state is mounted.
    src: Option<String>,
    /// Filesystem type. Required when state is mounted.
    fstype: Option<String>,
    /// Mount options.
    opts: Option<String>,
    /// State of the mount point.
    /// If _mounted_, the device will be actively mounted.
    /// If _unmounted_, the device will be unmounted without modifying fstab.
    /// If _absent_, the mount point will be unmounted and removed from fstab (fstab not yet supported).
    /// If _remounted_, the mount point will be remounted.
    /// **[default: `"mounted"`]**
    state: Option<State>,
}

#[derive(Debug)]
pub struct Mount;

impl Module for Mount {
    fn get_name(&self) -> &str {
        "mount"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            mount_module(parse_params(optional_params)?, check_mode)?,
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

struct MountClient {
    check_mode: bool,
}

impl MountClient {
    pub fn new(check_mode: bool) -> Self {
        MountClient { check_mode }
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
                    "Error executing mount command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn is_mounted(&self, path: &str) -> Result<bool> {
        let output = self.exec_cmd(Command::new("mountpoint").arg("-q").arg(path), false)?;
        Ok(output.status.success())
    }

    pub fn get_mount_info(&self, path: &str) -> Result<Option<MountInfo>> {
        let output = self.exec_cmd(
            Command::new("findmnt").args([
                "-n",
                "-o",
                "SOURCE,TARGET,FSTYPE,OPTIONS",
                "--target",
                path,
            ]),
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        if line.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            return Ok(None);
        }

        let source = parts[0].to_string();
        let rest = parts[1].trim();

        let remaining_parts: Vec<&str> = rest.splitn(3, ' ').collect();
        let (_target, fstype, opts) = match remaining_parts.len() {
            3 => (
                remaining_parts[0].to_string(),
                remaining_parts[1].to_string(),
                remaining_parts[2].to_string(),
            ),
            2 => (
                remaining_parts[0].to_string(),
                remaining_parts[1].to_string(),
                String::new(),
            ),
            1 => (remaining_parts[0].to_string(), String::new(), String::new()),
            _ => return Ok(None),
        };

        Ok(Some(MountInfo {
            source,
            fstype,
            opts,
        }))
    }

    pub fn mount(&self, params: &Params) -> Result<MountResult> {
        let src = params.src.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "src is required when state is mounted",
            )
        })?;

        let fstype = params.fstype.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "fstype is required when state is mounted",
            )
        })?;

        if self.is_mounted(&params.path)? {
            return Ok(MountResult::no_change());
        }

        let path = Path::new(&params.path);
        if !path.exists() {
            if self.check_mode {
                diff(
                    "path: absent",
                    format!("path: {} (will be created)", &params.path),
                );
            } else {
                std::fs::create_dir_all(path)?;
            }
        }

        diff(
            format!("state: unmounted ({})", &params.path),
            format!("state: mounted ({})", &params.path),
        );

        if self.check_mode {
            return Ok(MountResult::new(true, None));
        }

        let mut cmd = Command::new("mount");
        cmd.arg("-t").arg(fstype);

        if let Some(opts) = &params.opts {
            cmd.arg("-o").arg(opts);
        }

        cmd.arg(src).arg(&params.path);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(MountResult::new(true, output_str))
    }

    pub fn unmount(&self, path: &str) -> Result<MountResult> {
        if !self.is_mounted(path)? {
            return Ok(MountResult::no_change());
        }

        diff(
            format!("state: mounted ({path})"),
            format!("state: unmounted ({path})"),
        );

        if self.check_mode {
            return Ok(MountResult::new(true, None));
        }

        let mut cmd = Command::new("umount");
        cmd.arg(path);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(MountResult::new(true, output_str))
    }

    pub fn remount(&self, path: &str) -> Result<MountResult> {
        if !self.is_mounted(path)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Path {path} is not mounted, cannot remount"),
            ));
        }

        diff(
            format!("state: mounted ({path})"),
            format!("state: remounted ({path})"),
        );

        if self.check_mode {
            return Ok(MountResult::new(true, None));
        }

        let mut cmd = Command::new("mount");
        cmd.arg("-o").arg("remount").arg(path);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(MountResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct MountResult {
    changed: bool,
    output: Option<String>,
}

impl MountResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        MountResult { changed, output }
    }

    fn no_change() -> Self {
        MountResult {
            changed: false,
            output: None,
        }
    }
}

#[derive(Debug)]
struct MountInfo {
    source: String,
    fstype: String,
    opts: String,
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Path cannot be empty"));
    }

    if path.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Path contains null character",
        ));
    }

    Ok(())
}

fn mount_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_path(&params.path)?;

    let client = MountClient::new(check_mode);

    let result = match params.state.unwrap_or(State::Mounted) {
        State::Mounted => client.mount(&params)?,
        State::Unmounted => client.unmount(&params.path)?,
        State::Remounted => client.remount(&params.path)?,
        State::Absent => {
            let mut changed = false;
            let mut output = None;

            if client.is_mounted(&params.path)? {
                let unmount_result = client.unmount(&params.path)?;
                changed = unmount_result.changed;
                output = unmount_result.output;
            }

            MountResult::new(changed, output)
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "path".to_string(),
        serde_json::Value::String(params.path.clone()),
    );
    extra.insert(
        "mounted".to_string(),
        serde_json::Value::Bool(client.is_mounted(&params.path)?),
    );

    if let Some(info) = client.get_mount_info(&params.path)? {
        extra.insert("source".to_string(), serde_json::Value::String(info.source));
        extra.insert("fstype".to_string(), serde_json::Value::String(info.fstype));
        extra.insert("opts".to_string(), serde_json::Value::String(info.opts));
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
            path: /mnt/data
            src: /dev/sdb1
            fstype: ext4
            state: mounted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/mnt/data".to_owned(),
                src: Some("/dev/sdb1".to_owned()),
                fstype: Some("ext4".to_owned()),
                opts: None,
                state: Some(State::Mounted),
            }
        );
    }

    #[test]
    fn test_parse_params_with_opts() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /mnt/data
            src: /dev/sdb1
            fstype: ext4
            opts: rw,noatime
            state: mounted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/mnt/data".to_owned(),
                src: Some("/dev/sdb1".to_owned()),
                fstype: Some("ext4".to_owned()),
                opts: Some("rw,noatime".to_owned()),
                state: Some(State::Mounted),
            }
        );
    }

    #[test]
    fn test_parse_params_unmounted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /mnt/data
            state: unmounted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/mnt/data".to_owned(),
                src: None,
                fstype: None,
                opts: None,
                state: Some(State::Unmounted),
            }
        );
    }

    #[test]
    fn test_parse_params_no_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /mnt/data
            src: /dev/sdb1
            fstype: ext4
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
            path: /mnt/data
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_path() {
        assert!(validate_path("/mnt/data").is_ok());
        assert!(validate_path("/").is_ok());
        assert!(validate_path("").is_err());
        assert!(validate_path("path\0with\0null").is_err());
    }
}
