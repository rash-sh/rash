/// ANCHOR: module
/// # lvm_snapshot
///
/// Manage LVM snapshots for backup and rollback operations.
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
/// - name: Create a snapshot of root logical volume
///   lvm_snapshot:
///     vg: vg0
///     lv: root
///     snapshot_name: root_backup
///     size: 5G
///
/// - name: Remove a snapshot
///   lvm_snapshot:
///     vg: vg0
///     lv: root
///     snapshot_name: root_backup
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
#[derive(Default)]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Volume group name.
    vg: String,
    /// Logical volume name to snapshot.
    lv: String,
    /// Name for the snapshot.
    snapshot_name: String,
    /// Size of the snapshot (e.g., 5G, 512M).
    /// Required when state is present.
    size: Option<String>,
    /// Whether the snapshot should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
}

#[derive(Debug)]
pub struct LvmSnapshot;

impl Module for LvmSnapshot {
    fn get_name(&self) -> &str {
        "lvm_snapshot"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            lvm_snapshot_module(parse_params(optional_params)?, check_mode)?,
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

struct LvmSnapshotClient {
    check_mode: bool,
}

impl LvmSnapshotClient {
    pub fn new(check_mode: bool) -> Self {
        LvmSnapshotClient { check_mode }
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
                    "Error executing LVM command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn snapshot_exists(&self, vg: &str, snapshot_name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("lvs").args([
                "--noheadings",
                "-o",
                "lv_name",
                "--select",
                &format!("vg_name={vg} && lv_name={snapshot_name} && lv_attr=~^s"),
            ]),
            false,
        )?;
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    pub fn lv_exists(&self, vg: &str, lv: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("lvs").args([
                "--noheadings",
                "-o",
                "lv_name",
                "--select",
                &format!("vg_name={vg} && lv_name={lv}"),
            ]),
            false,
        )?;
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    pub fn create_snapshot(&self, params: &Params) -> Result<SnapshotResult> {
        let size = params.size.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "size is required when state is present",
            )
        })?;

        let snapshot_path = format!("/dev/{}/{}", params.vg, params.snapshot_name);

        diff(
            format!("state: absent ({snapshot_path})"),
            format!("state: present ({snapshot_path})"),
        );

        if self.check_mode {
            return Ok(SnapshotResult::new(true, None));
        }

        let mut cmd = Command::new("lvcreate");
        cmd.args(["-s", "-n", &params.snapshot_name])
            .args(["-L", size])
            .arg(format!("/dev/{}/{}", params.vg, params.lv));

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(SnapshotResult::new(true, output_str))
    }

    pub fn remove_snapshot(&self, params: &Params) -> Result<SnapshotResult> {
        let snapshot_path = format!("/dev/{}/{}", params.vg, params.snapshot_name);

        diff(
            format!("state: present ({snapshot_path})"),
            format!("state: absent ({snapshot_path})"),
        );

        if self.check_mode {
            return Ok(SnapshotResult::new(true, None));
        }

        let mut cmd = Command::new("lvremove");
        cmd.arg("-f").arg(&snapshot_path);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(SnapshotResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct SnapshotResult {
    changed: bool,
    output: Option<String>,
}

impl SnapshotResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        SnapshotResult { changed, output }
    }

    fn no_change() -> Self {
        SnapshotResult {
            changed: false,
            output: None,
        }
    }
}

fn validate_params(params: &Params) -> Result<()> {
    if params.vg.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "vg cannot be empty"));
    }
    if params.lv.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "lv cannot be empty"));
    }
    if params.snapshot_name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "snapshot_name cannot be empty",
        ));
    }
    if params.state == State::Present && params.size.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "size is required when state is present",
        ));
    }
    Ok(())
}

fn lvm_snapshot_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = LvmSnapshotClient::new(check_mode);

    if params.state == State::Present && !client.lv_exists(&params.vg, &params.lv)? {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Logical volume {} in volume group {} does not exist",
                params.lv, params.vg
            ),
        ));
    }

    let snapshot_exists = client.snapshot_exists(&params.vg, &params.snapshot_name)?;

    let (result, final_exists) = match params.state {
        State::Present => {
            if snapshot_exists {
                (SnapshotResult::no_change(), true)
            } else {
                (client.create_snapshot(&params)?, true)
            }
        }
        State::Absent => {
            if snapshot_exists {
                (client.remove_snapshot(&params)?, false)
            } else {
                (SnapshotResult::no_change(), false)
            }
        }
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "vg".to_string(),
        serde_json::Value::String(params.vg.clone()),
    );
    extra.insert(
        "lv".to_string(),
        serde_json::Value::String(params.lv.clone()),
    );
    extra.insert(
        "snapshot_name".to_string(),
        serde_json::Value::String(params.snapshot_name.clone()),
    );
    extra.insert("exists".to_string(), serde_json::Value::Bool(final_exists));

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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg0
            lv: root
            snapshot_name: root_backup
            size: 5G
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                vg: "vg0".to_owned(),
                lv: "root".to_owned(),
                snapshot_name: "root_backup".to_owned(),
                size: Some("5G".to_owned()),
                state: State::Present,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg0
            lv: root
            snapshot_name: root_backup
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.size, None);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg0
            lv: root
            snapshot_name: root_backup
            size: 5G
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_validate_params_empty_vg() {
        let params = Params {
            vg: "".to_string(),
            lv: "root".to_string(),
            snapshot_name: "root_backup".to_string(),
            size: Some("5G".to_string()),
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_empty_lv() {
        let params = Params {
            vg: "vg0".to_string(),
            lv: "".to_string(),
            snapshot_name: "root_backup".to_string(),
            size: Some("5G".to_string()),
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_empty_snapshot_name() {
        let params = Params {
            vg: "vg0".to_string(),
            lv: "root".to_string(),
            snapshot_name: "".to_string(),
            size: Some("5G".to_string()),
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_missing_size() {
        let params = Params {
            vg: "vg0".to_string(),
            lv: "root".to_string(),
            snapshot_name: "root_backup".to_string(),
            size: None,
            state: State::Present,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_absent_no_size_ok() {
        let params = Params {
            vg: "vg0".to_string(),
            lv: "root".to_string(),
            snapshot_name: "root_backup".to_string(),
            size: None,
            state: State::Absent,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg0
            lv: root
            snapshot_name: root_backup
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_snapshot_result_no_change() {
        let result = SnapshotResult::no_change();
        assert!(!result.changed);
        assert!(result.output.is_none());
    }

    #[test]
    fn test_snapshot_result_new() {
        let result = SnapshotResult::new(true, Some("output".to_string()));
        assert!(result.changed);
        assert_eq!(result.output, Some("output".to_string()));
    }
}
