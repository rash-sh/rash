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
/// - name: Create a logical volume
///   lvol:
///     vg: vgdata
///     lv: lvdata
///     size: 10G
///
/// - name: Create logical volume with filesystem
///   lvol:
///     vg: vgdata
///     lv: lvdata
///     size: 50G
///     filesystem: ext4
///
/// - name: Resize logical volume with filesystem
///   lvol:
///     vg: vgdata
///     lv: lvdata
///     size: 100G
///     resizefs: true
///
/// - name: Remove logical volume
///   lvol:
///     vg: vgdata
///     lv: lvdata
///     state: absent
///
/// - name: Force remove logical volume
///   lvol:
///     vg: vgdata
///     lv: lvdata
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
    /// Logical volume name.
    lv: String,
    /// Size of the logical volume (e.g., 10G, 512M).
    size: Option<String>,
    /// Whether the logical volume should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
    /// Force removal of logical volume.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Filesystem type to create on the logical volume.
    filesystem: Option<String>,
    /// Allow shrinking of the logical volume.
    /// **[default: `false`]**
    #[serde(default)]
    shrink: bool,
    /// Resize the filesystem with the logical volume.
    /// **[default: `false`]**
    #[serde(default)]
    resizefs: bool,
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
                    "Error executing LVM command: {}",
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
                &format!("vg_name={vg} && lv_name={lv}"),
            ]),
            false,
        )?;
        Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    pub fn get_lv_size(&self, vg: &str, lv: &str) -> Result<Option<String>> {
        let output = self.exec_cmd(
            Command::new("lvs")
                .args([
                    "--noheadings",
                    "-o",
                    "lv_size",
                    "--units",
                    "b",
                    "--nosuffix",
                ])
                .args(["--select", &format!("vg_name={vg} && lv_name={lv}")]),
            false,
        )?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            return Ok(None);
        }
        Ok(Some(stdout))
    }

    pub fn create_lv(&self, params: &Params) -> Result<LvolResult> {
        let size = params.size.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "size is required when state is present",
            )
        })?;

        let lv_path = format!("/dev/{}/{}", params.vg, params.lv);

        diff(
            format!("state: absent ({lv_path})"),
            format!("state: present ({lv_path})"),
        );

        if self.check_mode {
            return Ok(LvolResult::new(true, None));
        }

        let mut cmd = Command::new("lvcreate");
        cmd.args(["-n", &params.lv])
            .args(["-L", size])
            .arg(&params.vg);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        if let Some(ref fs) = params.filesystem {
            self.create_filesystem(&lv_path, fs)?;
        }

        Ok(LvolResult::new(true, output_str))
    }

    pub fn resize_lv(&self, params: &Params) -> Result<LvolResult> {
        let size = params.size.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "size is required when state is present",
            )
        })?;

        let lv_path = format!("/dev/{}/{}", params.vg, params.lv);
        let current_size = self.get_lv_size(&params.vg, &params.lv)?;

        let current_size_str = match current_size {
            Some(ref s) => s.clone(),
            None => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Cannot resize: logical volume does not exist",
                ));
            }
        };

        let target_size_bytes = self.parse_size_to_bytes(size)?;
        let current_size_bytes: u64 = current_size_str
            .parse()
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        if target_size_bytes == current_size_bytes {
            return Ok(LvolResult::no_change());
        }

        if target_size_bytes < current_size_bytes && !params.shrink {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Shrinking requires shrink=true",
            ));
        }

        diff(
            format!("size: {current_size_str} ({lv_path})"),
            format!("size: {size} ({lv_path})"),
        );

        if self.check_mode {
            return Ok(LvolResult::new(true, None));
        }

        let mut cmd = Command::new("lvresize");
        cmd.args(["-L", size]).arg(&lv_path);

        if params.resizefs {
            cmd.arg("--resizefs");
        }

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
        let lv_path = format!("/dev/{}/{}", params.vg, params.lv);

        diff(
            format!("state: present ({lv_path})"),
            format!("state: absent ({lv_path})"),
        );

        if self.check_mode {
            return Ok(LvolResult::new(true, None));
        }

        let mut cmd = Command::new("lvremove");
        if params.force {
            cmd.arg("-f");
        }
        cmd.arg(&lv_path);

        let output = self.exec_cmd(&mut cmd, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_str = if stdout.trim().is_empty() {
            None
        } else {
            Some(stdout.trim().to_string())
        };

        Ok(LvolResult::new(true, output_str))
    }

    fn create_filesystem(&self, device: &str, fs_type: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = Command::new("mkfs");
        cmd.arg("-t").arg(fs_type).arg(device);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    fn parse_size_to_bytes(&self, size: &str) -> Result<u64> {
        let size = size.trim();
        let (num_part, unit) = if size.ends_with('T') || size.ends_with('t') {
            (&size[..size.len() - 1], 1024u64 * 1024 * 1024 * 1024)
        } else if size.ends_with('G') || size.ends_with('g') {
            (&size[..size.len() - 1], 1024u64 * 1024 * 1024)
        } else if size.ends_with('M') || size.ends_with('m') {
            (&size[..size.len() - 1], 1024u64 * 1024)
        } else if size.ends_with('K') || size.ends_with('k') {
            (&size[..size.len() - 1], 1024u64)
        } else if size.ends_with('B') || size.ends_with('b') {
            (&size[..size.len() - 1], 1u64)
        } else {
            (size, 1u64)
        };

        let num: u64 = num_part
            .parse()
            .map_err(|_| Error::new(ErrorKind::InvalidData, format!("Invalid size: {size}")))?;

        Ok(num * unit)
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

fn validate_params(params: &Params) -> Result<()> {
    if params.vg.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "vg cannot be empty"));
    }
    if params.lv.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "lv cannot be empty"));
    }
    if params.state == State::Present && params.size.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "size is required when state is present",
        ));
    }
    Ok(())
}

fn lvol_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = LvolClient::new(check_mode);
    let lv_exists = client.lv_exists(&params.vg, &params.lv)?;

    let result = match params.state {
        State::Present => {
            if lv_exists {
                client.resize_lv(&params)?
            } else {
                client.create_lv(&params)?
            }
        }
        State::Absent => {
            if lv_exists {
                client.remove_lv(&params)?
            } else {
                LvolResult::no_change()
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
        "exists".to_string(),
        serde_json::Value::Bool(client.lv_exists(&params.vg, &params.lv)?),
    );

    if let Some(size) = client.get_lv_size(&params.vg, &params.lv)? {
        extra.insert("size".to_string(), serde_json::Value::String(size));
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
            vg: vgdata
            lv: lvdata
            size: 10G
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                vg: "vgdata".to_owned(),
                lv: "lvdata".to_owned(),
                size: Some("10G".to_owned()),
                state: State::Present,
                force: false,
                filesystem: None,
                shrink: false,
                resizefs: false,
            }
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vgdata
            lv: lvdata
            size: 10G
            state: absent
            force: true
            filesystem: ext4
            shrink: true
            resizefs: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert!(params.force);
        assert_eq!(params.filesystem, Some("ext4".to_owned()));
        assert!(params.shrink);
        assert!(params.resizefs);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vgdata
            lv: lvdata
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.size, None);
    }

    #[test]
    fn test_validate_params_empty_vg() {
        let params = Params {
            vg: "".to_string(),
            lv: "lvdata".to_string(),
            size: Some("10G".to_string()),
            state: State::Present,
            force: false,
            filesystem: None,
            shrink: false,
            resizefs: false,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_empty_lv() {
        let params = Params {
            vg: "vgdata".to_string(),
            lv: "".to_string(),
            size: Some("10G".to_string()),
            state: State::Present,
            force: false,
            filesystem: None,
            shrink: false,
            resizefs: false,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_missing_size() {
        let params = Params {
            vg: "vgdata".to_string(),
            lv: "lvdata".to_string(),
            size: None,
            state: State::Present,
            force: false,
            filesystem: None,
            shrink: false,
            resizefs: false,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_parse_size_to_bytes() {
        let client = LvolClient::new(false);

        assert_eq!(
            client.parse_size_to_bytes("1T").unwrap(),
            1024u64 * 1024 * 1024 * 1024
        );
        assert_eq!(
            client.parse_size_to_bytes("1G").unwrap(),
            1024u64 * 1024 * 1024
        );
        assert_eq!(client.parse_size_to_bytes("1M").unwrap(), 1024u64 * 1024);
        assert_eq!(client.parse_size_to_bytes("1K").unwrap(), 1024u64);
        assert_eq!(client.parse_size_to_bytes("1024").unwrap(), 1024u64);
        assert_eq!(
            client.parse_size_to_bytes("10G").unwrap(),
            10u64 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vgdata
            lv: lvdata
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
