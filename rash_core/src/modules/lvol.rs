/// ANCHOR: module
/// # lvol
///
/// Manage LVM logical volumes.
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
/// - name: Create logical volume
///   lvol:
///     vg: data_vg
///     lv: data_lv
///     size: 10G
///
/// - name: Create thin logical volume
///   lvol:
///     vg: data_vg
///     lv: thin_lv
///     size: 5G
///     thinpool: thin_pool
///
/// - name: Remove logical volume
///   lvol:
///     vg: data_vg
///     lv: old_lv
///     state: absent
///
/// - name: Force remove logical volume
///   lvol:
///     vg: data_vg
///     lv: old_lv
///     state: absent
///     force: true
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

fn default_state() -> Option<State> {
    Some(State::Present)
}

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
    /// Volume group name.
    vg: String,
    /// Logical volume name.
    lv: String,
    /// Size of the logical volume (e.g., 10G, 512M, 100%FREE).
    size: Option<String>,
    /// State of the logical volume.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: Option<State>,
    /// Force removal of logical volume (for state=absent).
    /// **[default: `false`]**
    #[serde(default)]
    force: Option<bool>,
    /// Thin pool name to create the logical volume in.
    thinpool: Option<String>,
}

#[derive(Debug)]
pub struct Lvol;

impl Module for Lvol {
    fn get_name(&self) -> &str {
        "lvol"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            lvol_module(parse_params(optional_params)?, check_mode)?,
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

struct LvolClient {
    check_mode: bool,
}

impl LvolClient {
    pub fn new(check_mode: bool) -> Self {
        LvolClient { check_mode }
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
                    "Error executing lvol command: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    pub fn lv_exists(&self, vg: &str, lv: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("lvs").args([
                "--noheadings",
                "-o",
                "lv_name",
                "--select",
                &format!("vg_name={} && lv_name={}", vg, lv),
            ]),
            false,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    pub fn get_lv_info(&self, vg: &str, lv: &str) -> Result<Option<LvInfo>> {
        let output = self.exec_cmd(
            Command::new("lvs")
                .args(["--noheadings", "-o", "vg_name,lv_name,lv_size,lv_attr"])
                .args([&format!("{}/{}", vg, lv)]),
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

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return Ok(None);
        }

        Ok(Some(LvInfo {
            vg: parts[0].to_string(),
            name: parts[1].to_string(),
            size: parts[2].to_string(),
            attr: parts[3].to_string(),
        }))
    }

    pub fn create_lv(&self, params: &Params) -> Result<LvolResult> {
        let size = params.size.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "size is required when state is present",
            )
        })?;

        if self.lv_exists(&params.vg, &params.lv)? {
            return Ok(LvolResult::no_change());
        }

        let lv_type = if params.thinpool.is_some() {
            "thin"
        } else {
            "linear"
        };

        diff(
            format!("state: absent ({}/{})", &params.vg, &params.lv),
            format!(
                "state: present ({}/{} - {} - {})",
                &params.vg, &params.lv, lv_type, size
            ),
        );

        if self.check_mode {
            return Ok(LvolResult::new(true, None));
        }

        let mut cmd = Command::new("lvcreate");
        cmd.arg("-n").arg(&params.lv);
        cmd.arg("-L").arg(size);

        if let Some(ref thinpool) = params.thinpool {
            cmd.arg("--thinpool").arg(thinpool);
        }

        cmd.arg(&params.vg);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(LvolResult::new(true, output_str))
    }

    pub fn remove_lv(&self, params: &Params) -> Result<LvolResult> {
        if !self.lv_exists(&params.vg, &params.lv)? {
            return Ok(LvolResult::no_change());
        }

        diff(
            format!("state: present ({}/{})", &params.vg, &params.lv),
            format!("state: absent ({}/{})", &params.vg, &params.lv),
        );

        if self.check_mode {
            return Ok(LvolResult::new(true, None));
        }

        let mut cmd = Command::new("lvremove");
        cmd.arg("-y");

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }

        cmd.arg(format!("{}/{}", &params.vg, &params.lv));

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(LvolResult::new(true, output_str))
    }
}

#[derive(Debug)]
struct LvolResult {
    changed: bool,
    output: Option<String>,
}

impl LvolResult {
    fn new(changed: bool, output: Option<String>) -> Self {
        LvolResult { changed, output }
    }

    fn no_change() -> Self {
        LvolResult {
            changed: false,
            output: None,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct LvInfo {
    vg: String,
    name: String,
    size: String,
    attr: String,
}

fn validate_params(params: &Params) -> Result<()> {
    if params.vg.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Volume group name cannot be empty",
        ));
    }

    if params.lv.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Logical volume name cannot be empty",
        ));
    }

    if params.vg.contains('\0') || params.lv.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Names cannot contain null characters",
        ));
    }

    Ok(())
}

fn lvol_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = LvolClient::new(check_mode);

    let result = match params.state.unwrap_or(State::Present) {
        State::Present => client.create_lv(&params)?,
        State::Absent => client.remove_lv(&params)?,
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
        "exists".to_string(),
        serde_json::Value::Bool(client.lv_exists(&params.vg, &params.lv)?),
    );

    if let Some(info) = client.get_lv_info(&params.vg, &params.lv)? {
        extra.insert("size".to_string(), serde_json::Value::String(info.size));
        extra.insert("attr".to_string(), serde_json::Value::String(info.attr));
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            lv: data_lv
            size: 10G
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "data_vg");
        assert_eq!(params.lv, "data_lv");
        assert_eq!(params.size, Some("10G".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            lv: old_lv
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "data_vg");
        assert_eq!(params.lv, "old_lv");
        assert_eq!(params.state, Some(State::Absent));
        assert_eq!(params.size, None);
    }

    #[test]
    fn test_parse_params_thinpool() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            lv: thin_lv
            size: 5G
            thinpool: thin_pool
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "data_vg");
        assert_eq!(params.lv, "thin_lv");
        assert_eq!(params.size, Some("5G".to_string()));
        assert_eq!(params.thinpool, Some("thin_pool".to_string()));
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            lv: old_lv
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.force, Some(true));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            lv: data_lv
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params() {
        assert!(
            validate_params(&Params {
                vg: "data_vg".to_string(),
                lv: "data_lv".to_string(),
                size: Some("10G".to_string()),
                state: Some(State::Present),
                force: None,
                thinpool: None,
            })
            .is_ok()
        );

        assert!(
            validate_params(&Params {
                vg: "".to_string(),
                lv: "data_lv".to_string(),
                size: Some("10G".to_string()),
                state: Some(State::Present),
                force: None,
                thinpool: None,
            })
            .is_err()
        );

        assert!(
            validate_params(&Params {
                vg: "data_vg".to_string(),
                lv: "".to_string(),
                size: Some("10G".to_string()),
                state: Some(State::Present),
                force: None,
                thinpool: None,
            })
            .is_err()
        );
    }
}
