/// ANCHOR: module
/// # btrfs
///
/// Manage Btrfs subvolumes, snapshots, and properties.
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
/// - name: Create Btrfs subvolume
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/app
///     state: present
///
/// - name: Create subvolume with compression
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/compressed
///     state: present
///     compression: zstd
///
/// - name: Create read-only snapshot
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/app
///     snapshot: /data/app-snap
///     readonly: true
///
/// - name: Create read-write snapshot
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/app
///     snapshot: /data/app-rw-snap
///     readonly: false
///
/// - name: Set subvolume properties
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/app
///     state: present
///     properties:
///       compression: zstd
///
/// - name: Delete subvolume
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/old
///     state: absent
///
/// - name: Delete snapshot
///   btrfs:
///     device: /dev/sda1
///     subvolume: /data/app-snap
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
use std::collections::HashMap;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the Btrfs device or mount point.
    device: String,
    /// Subvolume path relative to the mount point.
    subvolume: String,
    /// Whether the subvolume should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
    /// Destination path for a snapshot of the subvolume.
    snapshot: Option<String>,
    /// Whether the snapshot should be read-only.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    readonly: bool,
    /// Dict of subvolume properties to set.
    properties: Option<HashMap<String, String>>,
    /// Compression algorithm (e.g., zstd, lzo, zlib).
    compression: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug)]
pub struct Btrfs;

impl Module for Btrfs {
    fn get_name(&self) -> &str {
        "btrfs"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            btrfs_module(parse_params(optional_params)?, check_mode)?,
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

struct BtrfsClient {
    check_mode: bool,
}

impl BtrfsClient {
    pub fn new(check_mode: bool) -> Self {
        BtrfsClient { check_mode }
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
                    "Error executing btrfs command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn get_mount_point(&self, device: &str) -> Result<String> {
        let output = self.exec_cmd(
            Command::new("findmnt")
                .args(["-n", "-o", "TARGET", "-S", device]),
            true,
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }

    pub fn subvolume_exists(&self, mount_point: &str, subvolume: &str) -> Result<bool> {
        let full_path = format!("{mount_point}{subvolume}");
        let output = self.exec_cmd(
            Command::new("btrfs")
                .args(["subvolume", "show", &full_path]),
            false,
        );
        Ok(output.map(|o| o.status.success()).unwrap_or(false))
    }

    pub fn snapshot_exists(&self, mount_point: &str, snapshot: &str) -> Result<bool> {
        self.subvolume_exists(mount_point, snapshot)
    }

    fn get_property(&self, mount_point: &str, subvolume: &str, property: &str) -> Result<Option<String>> {
        let full_path = format!("{mount_point}{subvolume}");
        let output = self.exec_cmd(
            Command::new("btrfs")
                .args(["property", "get", "-ts", &full_path, property]),
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        Ok(trimmed.split_once('=').map(|(_, v)| v.trim().to_string()))
    }

    pub fn create_subvolume(&self, mount_point: &str, params: &Params) -> Result<BtrfsResult> {
        let full_path = format!("{}{}", mount_point, params.subvolume);

        diff(
            format!("subvolume: absent ({})", params.subvolume),
            format!("subvolume: present ({})", params.subvolume),
        );

        if self.check_mode {
            return Ok(BtrfsResult::new(true, None));
        }

        let output = self.exec_cmd(
            Command::new("btrfs")
                .args(["subvolume", "create", &full_path]),
            true,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(BtrfsResult::new(true, output_str))
    }

    pub fn delete_subvolume(&self, mount_point: &str, params: &Params) -> Result<BtrfsResult> {
        let full_path = format!("{}{}", mount_point, params.subvolume);

        diff(
            format!("subvolume: present ({})", params.subvolume),
            format!("subvolume: absent ({})", params.subvolume),
        );

        if self.check_mode {
            return Ok(BtrfsResult::new(true, None));
        }

        let output = self.exec_cmd(
            Command::new("btrfs")
                .args(["subvolume", "delete", &full_path]),
            true,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(BtrfsResult::new(true, output_str))
    }

    pub fn create_snapshot(
        &self,
        mount_point: &str,
        params: &Params,
    ) -> Result<BtrfsResult> {
        let snapshot_dest = params.snapshot.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "snapshot is required when creating a snapshot",
            )
        })?;

        let source_path = format!("{}{}", mount_point, params.subvolume);
        let dest_path = format!("{}{}", mount_point, snapshot_dest);

        if self.snapshot_exists(mount_point, snapshot_dest)? {
            return Ok(BtrfsResult::no_change());
        }

        diff(
            format!("snapshot: absent ({snapshot_dest})"),
            format!(
                "snapshot: present ({snapshot_dest}) (readonly={})",
                params.readonly
            ),
        );

        if self.check_mode {
            return Ok(BtrfsResult::new(true, None));
        }

        let mut cmd = Command::new("btrfs");
        cmd.args(["subvolume", "snapshot"]);

        if params.readonly {
            cmd.arg("-r");
        }

        cmd.args([&source_path, &dest_path]);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(BtrfsResult::new(true, output_str))
    }

    pub fn set_properties(
        &self,
        mount_point: &str,
        params: &Params,
    ) -> Result<BtrfsResult> {
        let full_path = format!("{}{}", mount_point, params.subvolume);

        let mut changed = false;
        let mut changes = Vec::new();

        if let Some(compression) = &params.compression {
            let current_compression = self
                .get_property(mount_point, &params.subvolume, "compression")?
                .unwrap_or_else(|| "none".to_string());
            if current_compression != *compression {
                changes.push(format!("compression: {current_compression} -> {compression}"));
                changed = true;
            }
        }

        if let Some(props) = &params.properties {
            for (key, value) in props {
                changes.push(format!("property: {key}={value}"));
                changed = true;
            }
        }

        if !changed {
            return Ok(BtrfsResult::no_change());
        }

        for change in &changes {
            diff("properties", change);
        }

        if self.check_mode {
            return Ok(BtrfsResult::new(true, None));
        }

        if let Some(compression) = &params.compression {
            self.exec_cmd(
                Command::new("btrfs")
                    .args(["property", "set", &full_path, "compression", compression]),
                true,
            )?;
        }

        if let Some(props) = &params.properties {
            for (key, value) in props {
                self.exec_cmd(
                    Command::new("btrfs")
                        .args(["property", "set", &full_path, key, value]),
                    true,
                )?;
            }
        }

        Ok(BtrfsResult::new(true, None))
    }
}

#[derive(Debug)]
struct BtrfsResult {
    changed: bool,
    output: Option<String>,
}

impl BtrfsResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        BtrfsResult { changed, output }
    }

    fn no_change() -> Self {
        BtrfsResult {
            changed: false,
            output: None,
        }
    }
}

fn validate_params(params: &Params) -> Result<()> {
    if params.device.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "device cannot be empty"));
    }

    if params.subvolume.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "subvolume cannot be empty",
        ));
    }

    if !params.subvolume.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "subvolume must be an absolute path starting with /",
        ));
    }

    Ok(())
}

fn btrfs_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = BtrfsClient::new(check_mode);
    let mount_point = client.get_mount_point(&params.device)?;

    if mount_point.is_empty() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Device {} is not mounted", params.device),
        ));
    }

    let subvol_exists = client.subvolume_exists(&mount_point, &params.subvolume)?;

    let result = match params.state {
        State::Present => {
            if let Some(ref _snapshot) = params.snapshot {
                if !subvol_exists {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "Source subvolume {} does not exist",
                            params.subvolume
                        ),
                    ));
                }
                client.create_snapshot(&mount_point, &params)?
            } else if subvol_exists {
                let mut overall_result = BtrfsResult::no_change();

                if params.compression.is_some() || params.properties.is_some() {
                    let prop_result = client.set_properties(&mount_point, &params)?;
                    if prop_result.changed {
                        overall_result = prop_result;
                    }
                }

                overall_result
            } else {
                let mut create_result = client.create_subvolume(&mount_point, &params)?;

                if create_result.changed
                    && (params.compression.is_some() || params.properties.is_some())
                {
                    let prop_result = client.set_properties(&mount_point, &params)?;
                    if prop_result.changed {
                        create_result = prop_result;
                    }
                }

                create_result
            }
        }
        State::Absent => {
            if subvol_exists {
                client.delete_subvolume(&mount_point, &params)?
            } else {
                BtrfsResult::no_change()
            }
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );
    extra.insert(
        "subvolume".to_string(),
        serde_json::Value::String(params.subvolume.clone()),
    );
    extra.insert(
        "exists".to_string(),
        serde_json::Value::Bool(client.subvolume_exists(&mount_point, &params.subvolume)?),
    );

    if let Some(ref snapshot) = params.snapshot {
        extra.insert(
            "snapshot".to_string(),
            serde_json::Value::String(snapshot.clone()),
        );
        extra.insert(
            "snapshot_exists".to_string(),
            serde_json::Value::Bool(client.snapshot_exists(&mount_point, snapshot)?),
        );
    }

    Ok(ModuleResult {
        changed: result.changed,
        output: result.output,
        extra: Some(serde_norway::value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "/dev/sda1");
        assert_eq!(params.subvolume, "/data/app");
        assert_eq!(params.state, State::Present);
        assert!(params.snapshot.is_none());
        assert!(params.properties.is_none());
        assert!(params.compression.is_none());
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/old
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_snapshot_readonly() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            snapshot: /data/app-snap
            readonly: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.snapshot, Some("/data/app-snap".to_string()));
        assert!(params.readonly);
    }

    #[test]
    fn test_parse_params_snapshot_readwrite() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            snapshot: /data/app-rw-snap
            readonly: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.snapshot, Some("/data/app-rw-snap".to_string()));
        assert!(!params.readonly);
    }

    #[test]
    fn test_parse_params_with_compression() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/compressed
            state: present
            compression: zstd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.compression, Some("zstd".to_string()));
    }

    #[test]
    fn test_parse_params_with_properties() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            state: present
            properties:
              compression: zstd
              label: mydata
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let props = params.properties.unwrap();
        assert_eq!(props.get("compression"), Some(&"zstd".to_string()));
        assert_eq!(props.get("label"), Some(&"mydata".to_string()));
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_default_readonly() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            snapshot: /data/app-snap
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.readonly);
    }

    #[test]
    fn test_validate_params_empty_device() {
        let params = Params {
            device: "".to_string(),
            subvolume: "/data/app".to_string(),
            state: State::Present,
            snapshot: None,
            readonly: true,
            properties: None,
            compression: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_empty_subvolume() {
        let params = Params {
            device: "/dev/sda1".to_string(),
            subvolume: "".to_string(),
            state: State::Present,
            snapshot: None,
            readonly: true,
            properties: None,
            compression: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_relative_subvolume() {
        let params = Params {
            device: "/dev/sda1".to_string(),
            subvolume: "data/app".to_string(),
            state: State::Present,
            snapshot: None,
            readonly: true,
            properties: None,
            compression: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            subvolume: /data/app
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_device() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            subvolume: /data/app
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_subvolume() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda1
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
