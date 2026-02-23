/// ANCHOR: module
/// # lvg
///
/// Manage LVM volume groups.
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
/// - name: Create volume group
///   lvg:
///     vg: data_vg
///     pvs: /dev/sdb1,/dev/sdc1
///
/// - name: Create volume group with single PV
///   lvg:
///     vg: system_vg
///     pvs: /dev/sda2
///
/// - name: Remove volume group
///   lvg:
///     vg: old_vg
///     state: absent
///
/// - name: Force remove volume group
///   lvg:
///     vg: old_vg
///     state: absent
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use std::process::{Command, Output};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_state() -> State {
    State::Present
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
    /// Name of the volume group.
    vg: String,
    /// List of comma-separated physical volumes.
    /// Required when state is present.
    pvs: Option<String>,
    /// Whether the volume group should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: State,
    /// Force removal of volume group.
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
}

#[derive(Debug)]
pub struct Lvg;

impl Module for Lvg {
    fn get_name(&self) -> &str {
        "lvg"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            lvg_module(parse_params(optional_params)?, check_mode)?,
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

struct LvgClient {
    check_mode: bool,
}

impl LvgClient {
    pub fn new(check_mode: bool) -> Self {
        LvgClient { check_mode }
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

    pub fn vg_exists(&self, vg_name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("vgs").args(["--noheadings", "-o", "vg_name", vg_name]),
            false,
        )?;
        Ok(output.status.success())
    }

    pub fn get_vg_pvs(&self, vg_name: &str) -> Result<Vec<String>> {
        let output = self.exec_cmd(
            Command::new("vgs")
                .args(["--noheadings", "-o", "pv_name", "--separator", ","])
                .arg(vg_name),
            false,
        )?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pvs: Vec<String> = stdout
            .trim()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(pvs)
    }

    pub fn create_vg(&self, vg_name: &str, pvs: &[String]) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = Command::new("vgcreate");
        cmd.arg(vg_name);
        for pv in pvs {
            cmd.arg(pv);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn extend_vg(&self, vg_name: &str, new_pvs: &[String]) -> Result<()> {
        if self.check_mode || new_pvs.is_empty() {
            return Ok(());
        }

        let mut cmd = Command::new("vgextend");
        cmd.arg(vg_name);
        for pv in new_pvs {
            cmd.arg(pv);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn remove_vg(&self, vg_name: &str, force: bool) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = Command::new("vgremove");
        if force {
            cmd.arg("-f");
        }
        cmd.arg(vg_name);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }
}

fn parse_pvs(pvs_str: &str) -> Vec<String> {
    pvs_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn lvg_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = LvgClient::new(check_mode);

    match params.state {
        State::Present => {
            let pvs_str = params.pvs.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "pvs is required when state is present",
                )
            })?;
            let desired_pvs = parse_pvs(pvs_str);

            if desired_pvs.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "At least one physical volume must be specified",
                ));
            }

            if client.vg_exists(&params.vg)? {
                let current_pvs = client.get_vg_pvs(&params.vg)?;
                let new_pvs: Vec<String> = desired_pvs
                    .into_iter()
                    .filter(|pv| !current_pvs.contains(pv))
                    .collect();

                if new_pvs.is_empty() {
                    return Ok(ModuleResult::new(false, None, None));
                }

                client.extend_vg(&params.vg, &new_pvs)?;
                Ok(ModuleResult::new(
                    true,
                    None,
                    Some(format!("Extended VG {} with new PVs", params.vg)),
                ))
            } else {
                client.create_vg(&params.vg, &desired_pvs)?;
                Ok(ModuleResult::new(
                    true,
                    None,
                    Some(format!("Created VG {}", params.vg)),
                ))
            }
        }
        State::Absent => {
            if !client.vg_exists(&params.vg)? {
                return Ok(ModuleResult::new(false, None, None));
            }

            client.remove_vg(&params.vg, params.force)?;
            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Removed VG {}", params.vg)),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            pvs: /dev/sdb1,/dev/sdc1
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "data_vg");
        assert_eq!(params.pvs, Some("/dev/sdb1,/dev/sdc1".to_string()));
        assert_eq!(params.state, State::Present);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: old_vg
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "old_vg");
        assert_eq!(params.pvs, None);
        assert_eq!(params.state, State::Absent);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: old_vg
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            pvs: /dev/sdb1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_pvs() {
        let pvs = parse_pvs("/dev/sdb1,/dev/sdc1,/dev/sdd1");
        assert_eq!(pvs, vec!["/dev/sdb1", "/dev/sdc1", "/dev/sdd1"]);
    }

    #[test]
    fn test_parse_pvs_with_spaces() {
        let pvs = parse_pvs("/dev/sdb1, /dev/sdc1 , /dev/sdd1");
        assert_eq!(pvs, vec!["/dev/sdb1", "/dev/sdc1", "/dev/sdd1"]);
    }

    #[test]
    fn test_parse_pvs_empty() {
        let pvs = parse_pvs("");
        assert!(pvs.is_empty());
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: data_vg
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
