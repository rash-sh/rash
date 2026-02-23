/// ANCHOR: module
/// # lvg
///
/// Manage LVM (Logical Volume Manager) volume groups.
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
/// - name: Create a volume group on top of /dev/sda1 with physical extent size = 32MB
///   lvg:
///     vg: vg_services
///     pvs: /dev/sda1
///     pesize: 32
///
/// - name: Create a volume group on top of /dev/sdb with physical extent size = 128KiB
///   lvg:
///     vg: vg_services
///     pvs: /dev/sdb
///     pesize: 128K
///
/// - name: Create or resize a volume group on top of /dev/sdb1 and /dev/sdc5
///   lvg:
///     vg: vg_services
///     pvs:
///       - /dev/sdb1
///       - /dev/sdc5
///
/// - name: Remove a volume group
///   lvg:
///     vg: vg_services
///     state: absent
///
/// - name: Force remove a volume group with logical volumes
///   lvg:
///     vg: vg_services
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
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_state() -> Option<State> {
    Some(State::Present)
}

fn default_pesize() -> Option<String> {
    Some("4".to_string())
}

fn deserialize_pvs<'de, D>(deserializer: D) -> std::result::Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let opt = Option::<serde_norway::Value>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(YamlValue::String(s)) => Ok(Some(vec![s])),
        Some(YamlValue::Sequence(seq)) => {
            let pvs: std::result::Result<Vec<String>, _> = seq
                .into_iter()
                .map(|v| {
                    if let YamlValue::String(s) = v {
                        Ok(s)
                    } else {
                        Err(D::Error::custom("expected string in pvs list"))
                    }
                })
                .collect();
            Ok(Some(pvs?))
        }
        Some(_) => Err(D::Error::custom("pvs must be a string or list of strings")),
    }
}

fn deserialize_pesize<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_norway::Value::deserialize(deserializer)?;
    match value {
        YamlValue::Null => Ok(Some("4".to_string())),
        YamlValue::String(s) => Ok(Some(s)),
        YamlValue::Number(n) => Ok(Some(n.to_string())),
        _ => Err(serde::de::Error::custom(
            "pesize must be a string or number",
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The name of the volume group.
    pub vg: String,
    /// List of comma-separated devices to use as physical devices in this volume group.
    /// Required when creating or resizing volume group.
    #[serde(default, deserialize_with = "deserialize_pvs")]
    pub pvs: Option<Vec<String>>,
    /// Control if the volume group exists.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// If true, allows to remove volume group with logical volumes.
    /// **[default: `false`]**
    #[serde(default)]
    pub force: Option<bool>,
    /// The size of the physical extent. Must be a power of 2.
    /// Can be optionally suffixed by a UNIT (k/K/m/M/g/G), default unit is megabyte.
    /// **[default: `"4"`]**
    #[serde(default = "default_pesize", deserialize_with = "deserialize_pesize")]
    pub pesize: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
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
            Command::new("vgs").args([
                "--noheadings",
                "-o",
                "pv_name",
                "--separator",
                ",",
                vg_name,
            ]),
            false,
        )?;

        if !output.status.success() {
            return Ok(Vec::new());
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

    pub fn pv_exists(&self, pv_path: &str) -> Result<bool> {
        let output = self.exec_cmd(
            Command::new("pvs").args(["--noheadings", "-o", "pv_name", pv_path]),
            false,
        )?;
        Ok(output.status.success())
    }

    pub fn pvcreate(&self, pv_path: &str) -> Result<()> {
        if self.pv_exists(pv_path)? {
            return Ok(());
        }

        if self.check_mode {
            trace!("check_mode: would run pvcreate {}", pv_path);
            return Ok(());
        }

        self.exec_cmd(Command::new("pvcreate").arg(pv_path), true)?;
        Ok(())
    }

    pub fn vgcreate(&self, params: &Params) -> Result<ModuleResult> {
        let pvs = params.pvs.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "pvs is required when creating a volume group",
            )
        })?;

        for pv in pvs {
            self.pvcreate(pv)?;
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Would create volume group {} with PVs: {:?}",
                    params.vg, pvs
                )),
            ));
        }

        let pesize = params.pesize.as_deref().unwrap_or("4");
        let mut cmd = Command::new("vgcreate");
        cmd.args(["-s", &format!("{}M", pesize)]).arg(&params.vg);

        for pv in pvs {
            cmd.arg(pv);
        }

        self.exec_cmd(&mut cmd, true)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Created volume group {}", params.vg)),
        ))
    }

    pub fn vgextend(&self, vg_name: &str, new_pvs: &[String]) -> Result<ModuleResult> {
        if new_pvs.is_empty() {
            return Ok(ModuleResult::new(false, None, None));
        }

        for pv in new_pvs {
            self.pvcreate(pv)?;
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Would extend volume group {} with PVs: {:?}",
                    vg_name, new_pvs
                )),
            ));
        }

        let mut cmd = Command::new("vgextend");
        cmd.arg(vg_name);
        for pv in new_pvs {
            cmd.arg(pv);
        }

        self.exec_cmd(&mut cmd, true)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Extended volume group {}", vg_name)),
        ))
    }

    pub fn vgreduce(&self, vg_name: &str, remove_pvs: &[String]) -> Result<ModuleResult> {
        if remove_pvs.is_empty() {
            return Ok(ModuleResult::new(false, None, None));
        }

        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!(
                    "Would reduce volume group {} by removing PVs: {:?}",
                    vg_name, remove_pvs
                )),
            ));
        }

        let mut cmd = Command::new("vgreduce");
        cmd.arg(vg_name);
        for pv in remove_pvs {
            cmd.arg(pv);
        }

        self.exec_cmd(&mut cmd, true)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Reduced volume group {}", vg_name)),
        ))
    }

    pub fn vgremove(&self, vg_name: &str, force: bool) -> Result<ModuleResult> {
        if self.check_mode {
            return Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Would remove volume group {}", vg_name)),
            ));
        }

        let mut cmd = Command::new("vgremove");
        if force {
            cmd.arg("-f");
        }
        cmd.arg(vg_name);

        self.exec_cmd(&mut cmd, true)?;

        Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Removed volume group {}", vg_name)),
        ))
    }
}

fn lvg_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = LvgClient::new(check_mode);

    match params.state.clone().unwrap_or(State::Present) {
        State::Present => {
            let vg_exists = client.vg_exists(&params.vg)?;

            if !vg_exists {
                return client.vgcreate(&params);
            }

            let pvs = match &params.pvs {
                Some(pvs) => pvs,
                None => {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some(format!("Volume group {} already exists", params.vg)),
                    ));
                }
            };

            let current_pvs = client.get_vg_pvs(&params.vg)?;

            let new_pvs: Vec<String> = pvs
                .iter()
                .filter(|pv| !current_pvs.contains(pv))
                .cloned()
                .collect();

            let remove_pvs: Vec<String> = current_pvs
                .iter()
                .filter(|pv| !pvs.contains(pv))
                .cloned()
                .collect();

            if new_pvs.is_empty() && remove_pvs.is_empty() {
                return Ok(ModuleResult::new(
                    false,
                    None,
                    Some(format!("Volume group {} is already up to date", params.vg)),
                ));
            }

            if !remove_pvs.is_empty() {
                client.vgreduce(&params.vg, &remove_pvs)?;
            }

            if !new_pvs.is_empty() {
                client.vgextend(&params.vg, &new_pvs)?;
            }

            Ok(ModuleResult::new(
                true,
                None,
                Some(format!("Volume group {} updated", params.vg)),
            ))
        }
        State::Absent => {
            let vg_exists = client.vg_exists(&params.vg)?;

            if !vg_exists {
                return Ok(ModuleResult::new(
                    false,
                    None,
                    Some(format!("Volume group {} does not exist", params.vg)),
                ));
            }

            client.vgremove(&params.vg, params.force.unwrap_or(false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            pvs:
              - /dev/sdb1
              - /dev/sdc5
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "vg_services");
        assert_eq!(
            params.pvs,
            Some(vec!["/dev/sdb1".to_string(), "/dev/sdc5".to_string()])
        );
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vg, "vg_services");
        assert_eq!(params.pvs, None);
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.pesize, Some("4".to_string()));
    }

    #[test]
    fn test_parse_params_with_pesize() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            pvs: /dev/sda1
            pesize: 32
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.pesize, Some("32".to_string()));
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.force, Some(true));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_pvs_string() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            pvs: /dev/sda1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.pvs, Some(vec!["/dev/sda1".to_string()]));
    }

    #[test]
    fn test_parse_params_pvs_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            vg: vg_services
            pvs:
              - /dev/sdb1
              - /dev/sdc5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.pvs,
            Some(vec!["/dev/sdb1".to_string(), "/dev/sdc5".to_string()])
        );
    }
}
