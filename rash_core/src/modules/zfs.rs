/// ANCHOR: module
/// # zfs
///
/// Manage ZFS datasets.
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
/// - name: Create encrypted root dataset
///   zfs:
///     name: rpool/ROOT
///     state: present
///     properties:
///       mountpoint: legacy
///       canmount: on
///       encryption: aes-256-gcm
///       keylocation: file:///etc/zfs/zfs-key
///       keyformat: passphrase
///
/// - name: Create dataset with compression
///   zfs:
///     name: rpool/ROOT/ubuntu
///     state: present
///     create_parent: true
///     properties:
///       mountpoint: /
///       compression: zstd
///       atime: off
///       recordsize: 32K
///
/// - name: Create unmounted dataset for OpenEBS
///   zfs:
///     name: rpool/openebs
///     state: present
///     properties:
///       mountpoint: none
///       canmount: off
///
/// - name: Create snapshot
///   zfs:
///     name: rpool/ROOT/ubuntu
///     state: snapshot
///     snapshot_suffix: pre-upgrade
///     recursive: true
///
/// - name: Mount dataset
///   zfs:
///     name: rpool/ROOT/ubuntu
///     state: mounted
///
/// - name: Unmount dataset
///   zfs:
///     name: rpool/ROOT/ubuntu
///     state: unmounted
///
/// - name: Destroy dataset
///   zfs:
///     name: rpool/old
///     state: absent
///     recursive: true
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
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
enum State {
    #[default]
    Info,
    Present,
    Absent,
    Mounted,
    Unmounted,
    Snapshot,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Dataset name (e.g., rpool/ROOT/ubuntu).
    name: String,
    /// State of the dataset.
    /// **[default: `"info"`]**
    #[serde(default)]
    state: State,
    /// Dict of dataset properties (mountpoint, compression, encryption, etc.).
    properties: Option<HashMap<String, String>>,
    /// Dict of properties that trigger change on any modification.
    extra_properties: Option<HashMap<String, String>>,
    /// Create parent datasets.
    /// **[default: `false`]**
    #[serde(default)]
    create_parent: bool,
    /// Apply recursively.
    /// **[default: `false`]**
    #[serde(default)]
    recursive: bool,
    /// Force unmount.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Snapshot suffix (used with state: snapshot).
    snapshot_suffix: Option<String>,
}

#[derive(Debug)]
pub struct Zfs;

impl Module for Zfs {
    fn get_name(&self) -> &str {
        "zfs"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            zfs_module(parse_params(optional_params)?, check_mode)?,
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

struct ZfsClient {
    check_mode: bool,
}

impl ZfsClient {
    pub fn new(check_mode: bool) -> Self {
        ZfsClient { check_mode }
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
                    "Error executing ZFS command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn dataset_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("zfs").args(["list", "-H", "-o", "name", name]),
            false,
        )?;
        Ok(output.status.success())
    }

    pub fn get_properties(&self, name: &str) -> Result<Option<DatasetInfo>> {
        let props = [
            "name",
            "mountpoint",
            "mounted",
            "compression",
            "compressratio",
            "atime",
            "relatime",
            "recordsize",
            "encryption",
            "keylocation",
            "keyformat",
            "encryptionroot",
            "canmount",
            "xattr",
            "acltype",
            "quota",
            "refquota",
            "reservation",
            "refreservation",
            "snapdir",
            "snapshot_limit",
            "used",
            "available",
            "referenced",
        ];

        let output = self.exec_cmd(
            Command::new("zfs")
                .args(["list", "-H", "-o", &props.join(",")])
                .arg(name),
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

        let values: Vec<&str> = line.split('\t').collect();
        if values.len() < props.len() {
            return Ok(None);
        }

        let mut properties = HashMap::new();
        for (i, prop) in props.iter().enumerate() {
            if i < values.len() {
                properties.insert(prop.to_string(), values[i].to_string());
            }
        }

        Ok(Some(DatasetInfo {
            mountpoint: values[1].to_string(),
            mounted: values[2] == "yes",
            properties,
        }))
    }

    pub fn get_all_properties(&self, name: &str) -> Result<HashMap<String, String>> {
        let output = self.exec_cmd(
            Command::new("zfs")
                .args(["get", "-H", "-o", "property,value", "all"])
                .arg(name),
            false,
        )?;

        if !output.status.success() {
            return Ok(HashMap::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut props = HashMap::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            if parts.len() == 2 {
                props.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        Ok(props)
    }

    pub fn create_dataset(&self, params: &Params) -> Result<ZfsResult> {
        diff(
            format!("state: absent ({})", params.name),
            format!("state: present ({})", params.name),
        );

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let mut cmd = Command::new("zfs");
        cmd.arg("create");

        if params.create_parent {
            cmd.arg("-p");
        }

        if let Some(props) = &params.properties {
            for (key, value) in props {
                cmd.args(["-o", &format!("{key}={value}")]);
            }
        }

        cmd.arg(&params.name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }

    pub fn set_properties(&self, params: &Params) -> Result<ZfsResult> {
        let current_props = self.get_all_properties(&params.name)?;
        let desired_props = params.properties.as_ref();

        let mut changed = false;
        let mut changes = Vec::new();

        if let Some(props) = desired_props {
            for (key, value) in props {
                let current = current_props.get(key).map(|s| s.as_str()).unwrap_or("-");
                if current != value {
                    changes.push(format!("{key}: {current} -> {value}"));
                    changed = true;
                }
            }
        }

        if let Some(extra_props) = &params.extra_properties {
            for (key, value) in extra_props {
                let current = current_props.get(key).map(|s| s.as_str()).unwrap_or("-");
                if current != value {
                    changes.push(format!("{key}: {current} -> {value} (extra)"));
                    changed = true;
                }
            }
        }

        if !changed {
            return Ok(ZfsResult::no_change());
        }

        for change in &changes {
            diff("properties", change);
        }

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let mut cmd = Command::new("zfs");
        cmd.arg("set");

        if let Some(props) = desired_props {
            for (key, value) in props {
                cmd.arg(format!("{key}={value}"));
            }
        }

        if let Some(extra_props) = &params.extra_properties {
            for (key, value) in extra_props {
                cmd.arg(format!("{key}={value}"));
            }
        }

        cmd.arg(&params.name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }

    pub fn destroy_dataset(&self, params: &Params) -> Result<ZfsResult> {
        diff(
            format!("state: present ({})", params.name),
            format!("state: absent ({})", params.name),
        );

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let mut cmd = Command::new("zfs");
        cmd.arg("destroy");

        if params.recursive {
            cmd.arg("-r");
        }

        if params.force {
            cmd.arg("-f");
        }

        cmd.arg(&params.name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }

    pub fn mount_dataset(&self, name: &str) -> Result<ZfsResult> {
        let info = self.get_properties(name)?;
        if let Some(ref i) = info
            && i.mounted
        {
            return Ok(ZfsResult::no_change());
        }

        diff(
            format!("mounted: false ({name})"),
            format!("mounted: true ({name})"),
        );

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let output = self.exec_cmd(Command::new("zfs").arg("mount").arg(name), true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }

    pub fn unmount_dataset(&self, name: &str, force: bool) -> Result<ZfsResult> {
        let info = self.get_properties(name)?;
        if let Some(ref i) = info
            && !i.mounted
        {
            return Ok(ZfsResult::no_change());
        }

        diff(
            format!("mounted: true ({name})"),
            format!("mounted: false ({name})"),
        );

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let mut cmd = Command::new("zfs");
        cmd.arg("unmount");

        if force {
            cmd.arg("-f");
        }

        cmd.arg(name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }

    pub fn create_snapshot(&self, params: &Params) -> Result<ZfsResult> {
        let suffix = params.snapshot_suffix.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "snapshot_suffix is required when state is snapshot",
            )
        })?;

        let snapshot_name = format!("{}@{}", params.name, suffix);

        let output = self.exec_cmd(
            Command::new("zfs")
                .args(["list", "-H", "-o", "name", "-t", "snapshot"])
                .arg(&snapshot_name),
            false,
        );

        let exists = output.map(|o| o.status.success()).unwrap_or(false);
        if exists {
            return Ok(ZfsResult::no_change());
        }

        diff(
            format!("snapshot: absent ({snapshot_name})"),
            format!("snapshot: present ({snapshot_name})"),
        );

        if self.check_mode {
            return Ok(ZfsResult::new(true, None));
        }

        let mut cmd = Command::new("zfs");
        cmd.arg("snapshot");

        if params.recursive {
            cmd.arg("-r");
        }

        cmd.arg(&snapshot_name);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(ZfsResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct ZfsResult {
    changed: bool,
    output: Option<String>,
}

impl ZfsResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        ZfsResult { changed, output }
    }

    fn no_change() -> Self {
        ZfsResult {
            changed: false,
            output: None,
        }
    }
}

#[derive(Debug)]
struct DatasetInfo {
    mountpoint: String,
    mounted: bool,
    properties: HashMap<String, String>,
}

fn validate_params(params: &Params) -> Result<()> {
    if params.name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    if params.state == State::Snapshot && params.snapshot_suffix.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "snapshot_suffix is required when state is snapshot",
        ));
    }

    Ok(())
}

fn zfs_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = ZfsClient::new(check_mode);
    let dataset_exists = client.dataset_exists(&params.name)?;

    let result = match params.state {
        State::Info => ZfsResult::no_change(),
        State::Present => {
            if dataset_exists {
                client.set_properties(&params)?
            } else {
                client.create_dataset(&params)?
            }
        }
        State::Absent => {
            if dataset_exists {
                client.destroy_dataset(&params)?
            } else {
                ZfsResult::no_change()
            }
        }
        State::Mounted => {
            if !dataset_exists {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Dataset {} does not exist", params.name),
                ));
            }
            client.mount_dataset(&params.name)?
        }
        State::Unmounted => {
            if !dataset_exists {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Dataset {} does not exist", params.name),
                ));
            }
            client.unmount_dataset(&params.name, params.force)?
        }
        State::Snapshot => {
            if !dataset_exists {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Dataset {} does not exist", params.name),
                ));
            }
            client.create_snapshot(&params)?
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "name".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert(
        "exists".to_string(),
        serde_json::Value::Bool(client.dataset_exists(&params.name)?),
    );

    if let Some(info) = client.get_properties(&params.name)? {
        extra.insert(
            "mountpoint".to_string(),
            serde_json::Value::String(info.mountpoint),
        );
        extra.insert("mounted".to_string(), serde_json::Value::Bool(info.mounted));

        let mut props = serde_json::Map::new();
        for (key, value) in &info.properties {
            props.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        extra.insert("properties".to_string(), serde_json::Value::Object(props));
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
            name: rpool/ROOT/ubuntu
            state: present
            properties:
              mountpoint: /
              compression: zstd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "rpool/ROOT/ubuntu");
        assert_eq!(params.state, State::Present);
        assert!(params.properties.is_some());
        let props = params.properties.unwrap();
        assert_eq!(props.get("mountpoint"), Some(&"/".to_string()));
        assert_eq!(props.get("compression"), Some(&"zstd".to_string()));
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool/ROOT
            state: present
            create_parent: true
            recursive: true
            force: true
            properties:
              mountpoint: legacy
              canmount: on
            extra_properties:
              custom: value
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.create_parent);
        assert!(params.recursive);
        assert!(params.force);
        assert!(params.extra_properties.is_some());
    }

    #[test]
    fn test_parse_params_snapshot() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool/ROOT/ubuntu
            state: snapshot
            snapshot_suffix: pre-upgrade
            recursive: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Snapshot);
        assert_eq!(params.snapshot_suffix, Some("pre-upgrade".to_string()));
        assert!(params.recursive);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool/ROOT/ubuntu
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Info);
    }

    #[test]
    fn test_validate_params_empty_name() {
        let params = Params {
            name: "".to_string(),
            state: State::Present,
            properties: None,
            extra_properties: None,
            create_parent: false,
            recursive: false,
            force: false,
            snapshot_suffix: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_snapshot_without_suffix() {
        let params = Params {
            name: "rpool/ROOT".to_string(),
            state: State::Snapshot,
            properties: None,
            extra_properties: None,
            create_parent: false,
            recursive: false,
            force: false,
            snapshot_suffix: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: rpool/ROOT
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
