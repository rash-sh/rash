/// ANCHOR: module
/// # smartctl
///
/// Monitor disk health using SMART (Self-Monitoring, Analysis and Reporting Technology).
/// Requires smartmontools to be installed.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Check disk health
///   smartctl:
///     device: /dev/sda
///     attributes: true
///   register: disk_health
///
/// - name: Get disk info
///   smartctl:
///     device: /dev/sda
///     info: true
///   register: disk_info
///
/// - name: Run short self-test
///   smartctl:
///     device: /dev/sda
///     test: short
///
/// - name: Run long self-test
///   smartctl:
///     device: /dev/sda
///     test: long
///
/// - name: Run conveyance self-test
///   smartctl:
///     device: /dev/sda
///     test: conveyance
///
/// - name: Check SMART health status
///   smartctl:
///     device: /dev/sda
///     health: true
///   register: health_status
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

const SMARTCTL_BIN: &str = "smartctl";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Block device path (e.g., /dev/sda).
    pub device: String,
    /// Return SMART attributes for the device.
    /// **[default: `false`]**
    pub attributes: Option<bool>,
    /// Return device identity and capabilities information.
    /// **[default: `false`]**
    pub info: Option<bool>,
    /// Return overall SMART health assessment.
    /// **[default: `false`]**
    pub health: Option<bool>,
    /// Run a SMART self-test on the device.
    pub test: Option<SelfTest>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SelfTest {
    Short,
    Long,
    Conveyance,
}

fn build_args(params: &Params) -> Vec<String> {
    let mut args = vec![];

    if params.attributes.unwrap_or(false) {
        args.push("-A".to_string());
    }

    if params.info.unwrap_or(false) {
        args.push("-i".to_string());
    }

    if params.health.unwrap_or(false) {
        args.push("-H".to_string());
    }

    if let Some(ref test_type) = params.test {
        match test_type {
            SelfTest::Short => args.push("-t".to_string()),
            SelfTest::Long => args.push("-t".to_string()),
            SelfTest::Conveyance => args.push("-t".to_string()),
        }
    }

    args
}

fn run_smartctl(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let args = build_args(params);

    let mut cmd = Command::new(SMARTCTL_BIN);

    if !args.is_empty() {
        cmd.args(&args);
    }

    let test_arg = match &params.test {
        Some(SelfTest::Short) => Some("short"),
        Some(SelfTest::Long) => Some("long"),
        Some(SelfTest::Conveyance) => Some("conveyance"),
        None => None,
    };

    if let Some(t) = test_arg {
        cmd.arg(t);
    }

    cmd.arg(&params.device);

    let cmd_str = format!("{SMARTCTL_BIN} {}", get_cmd_args_string(params));

    if check_mode {
        let changed = params.test.is_some();
        return Ok(ModuleResult::new(
            changed,
            None,
            Some(format!("Would run: {cmd_str}")),
        ));
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute smartctl: {e}"),
        )
    })?;

    trace!("smartctl output: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let changed = params.test.is_some();

    let extra = Some(value::to_value(json!({
        "rc": output.status.code(),
        "stderr": stderr,
    }))?);

    let module_output = if stdout.is_empty() {
        None
    } else {
        Some(stdout.into_owned())
    };

    Ok(ModuleResult::new(changed, extra, module_output))
}

fn get_cmd_args_string(params: &Params) -> String {
    let mut parts = vec![];

    if params.attributes.unwrap_or(false) {
        parts.push("-A".to_string());
    }
    if params.info.unwrap_or(false) {
        parts.push("-i".to_string());
    }
    if params.health.unwrap_or(false) {
        parts.push("-H".to_string());
    }
    if let Some(ref test_type) = params.test {
        parts.push("-t".to_string());
        parts.push(match test_type {
            SelfTest::Short => "short".to_string(),
            SelfTest::Long => "long".to_string(),
            SelfTest::Conveyance => "conveyance".to_string(),
        });
    }
    parts.push(params.device.clone());
    parts.join(" ")
}

#[derive(Debug)]
pub struct Smartctl;

impl Module for Smartctl {
    fn get_name(&self) -> &str {
        "smartctl"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        let result = run_smartctl(&params, check_mode)?;
        Ok((result, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_attributes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            attributes: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "/dev/sda");
        assert_eq!(params.attributes, Some(true));
        assert_eq!(params.info, None);
        assert_eq!(params.health, None);
        assert_eq!(params.test, None);
    }

    #[test]
    fn test_parse_params_test_short() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            test: short
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.device, "/dev/sda");
        assert_eq!(params.test, Some(SelfTest::Short));
    }

    #[test]
    fn test_parse_params_test_long() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            test: long
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.test, Some(SelfTest::Long));
    }

    #[test]
    fn test_parse_params_test_conveyance() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            test: conveyance
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.test, Some(SelfTest::Conveyance));
    }

    #[test]
    fn test_parse_params_health() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/nvme0
            health: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.health, Some(true));
    }

    #[test]
    fn test_parse_params_info() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            info: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.info, Some(true));
    }

    #[test]
    fn test_parse_params_combined() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            attributes: true
            health: true
            info: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.attributes, Some(true));
        assert_eq!(params.health, Some(true));
        assert_eq!(params.info, Some(true));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            nonexistent: true
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_check_mode_query() {
        let smartctl = Smartctl;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            attributes: true
            "#,
        )
        .unwrap();
        let (result, _) = smartctl
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("Would run:"));
    }

    #[test]
    fn test_check_mode_test() {
        let smartctl = Smartctl;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sda
            test: short
            "#,
        )
        .unwrap();
        let (result, _) = smartctl
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would run:"));
    }

    #[test]
    fn test_build_args() {
        let params = Params {
            device: "/dev/sda".to_string(),
            attributes: Some(true),
            info: Some(true),
            health: Some(true),
            test: None,
        };
        let args = build_args(&params);
        assert_eq!(args, vec!["-A", "-i", "-H"]);
    }

    #[test]
    fn test_build_args_with_test() {
        let params = Params {
            device: "/dev/sda".to_string(),
            attributes: None,
            info: None,
            health: None,
            test: Some(SelfTest::Long),
        };
        let args = build_args(&params);
        assert_eq!(args, vec!["-t"]);
    }

    #[test]
    fn test_build_args_empty() {
        let params = Params {
            device: "/dev/sda".to_string(),
            attributes: None,
            info: None,
            health: None,
            test: None,
        };
        let args = build_args(&params);
        assert!(args.is_empty());
    }
}
