/// ANCHOR: module
/// # fail
///
/// Fail execution with a custom error message.
///
/// This module is useful for explicitly failing execution in conditional
/// logic to provide meaningful error messages.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Check for required config
///   stat:
///     path: /etc/app/required.conf
///   register: config_check
///
/// - name: Fail if config missing
///   fail:
///     msg: "Required configuration file /etc/app/required.conf not found"
///   when: not config_check.stat.exists
///
/// - name: Fail with templated message
///   fail:
///     msg: "Unsupported architecture: {{ rash.arch }}"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The error message to display.
    #[serde(default = "default_msg")]
    msg: String,
}

fn default_msg() -> String {
    "Failed as requested".to_owned()
}

#[derive(Debug)]
pub struct Fail;

impl Module for Fail {
    fn get_name(&self) -> &str {
        "fail"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Err(Error::new(ErrorKind::Other, params.msg))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: Custom error message
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                msg: "Custom error message".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params, Params { msg: default_msg() });
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: error
            invalid: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_fail_returns_error() {
        let fail = Fail;
        let result = fail.exec(
            &GlobalParams::default(),
            YamlValue::Null,
            &Value::UNDEFINED,
            false,
        );
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
    }

    #[test]
    fn test_fail_custom_message() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            msg: "Custom failure message"
            "#,
        )
        .unwrap();
        let fail = Fail;
        let result = fail.exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("Custom failure message"));
    }
}
