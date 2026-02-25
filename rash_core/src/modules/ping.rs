/// ANCHOR: module
/// # ping
///
/// Try to connect to a host, verify a usable python and return `pong` on success.
///
/// This is a simple connectivity test module. It returns `pong` on success.
/// If the module is called but the connection fails, the task will fail.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - ping:
///
/// - name: Ping with custom data
///   ping:
///     data: "custom_data"
///   register: result
///
/// - name: Verify ping response
///   debug:
///     var: result.ping
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::Result;
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
    /// Data to return in the ping result.
    /// **[default: `"pong"`]**
    #[serde(default = "default_data")]
    data: String,
}

fn default_data() -> String {
    "pong".to_string()
}

pub fn ping(params: Params) -> Result<ModuleResult> {
    Ok(ModuleResult {
        changed: false,
        output: Some(params.data),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Ping;

impl Module for Ping {
    fn get_name(&self) -> &str {
        "ping"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((ping(parse_params(optional_params)?)?, None))
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
            data: test_data
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.data, "test_data");
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.data, "pong");
    }

    #[test]
    fn test_ping_default() {
        let result = ping(Params {
            data: default_data(),
        })
        .unwrap();
        assert!(!result.get_changed());
        assert_eq!(result.get_output(), Some("pong".to_string()));
    }

    #[test]
    fn test_ping_custom_data() {
        let result = ping(Params {
            data: "hello".to_string(),
        })
        .unwrap();
        assert!(!result.get_changed());
        assert_eq!(result.get_output(), Some("hello".to_string()));
    }
}
