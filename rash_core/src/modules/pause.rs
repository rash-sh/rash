/// ANCHOR: module
/// # pause
///
/// Pause execution for a given duration.
///
/// This module is useful for debugging, rate limiting, or waiting for
/// external processes that don't have a clear signal.
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
/// - pause:
///     seconds: 5
///
/// - pause:
///     minutes: 1
///
/// - pause:
///     seconds: 30
///     prompt: "Waiting for service to start..."
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

use std::time::Duration;

const DEFAULT_SECONDS: u64 = 0;
const DEFAULT_MINUTES: u64 = 0;

fn default_seconds() -> u64 {
    DEFAULT_SECONDS
}

fn default_minutes() -> u64 {
    DEFAULT_MINUTES
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Number of seconds to pause.
    #[serde(default = "default_seconds")]
    seconds: u64,
    /// Number of minutes to pause.
    #[serde(default = "default_minutes")]
    minutes: u64,
    /// Optional message to display during pause.
    #[serde(default)]
    prompt: Option<String>,
}

fn pause(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let total_seconds = params.minutes * 60 + params.seconds;

    if total_seconds == 0 {
        return Ok(ModuleResult::new(false, None, Some("0".to_string())));
    }

    if !check_mode {
        if let Some(ref prompt) = params.prompt {
            eprintln!("{}", prompt);
        }
        std::thread::sleep(Duration::from_secs(total_seconds));
    }

    Ok(ModuleResult::new(
        !check_mode,
        None,
        Some(total_seconds.to_string()),
    ))
}

#[derive(Debug)]
pub struct Pause;

impl Module for Pause {
    fn get_name(&self) -> &str {
        "pause"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((pause(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn test_parse_params_seconds() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            seconds: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                seconds: 5,
                minutes: 0,
                prompt: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minutes() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            minutes: 2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                seconds: 0,
                minutes: 2,
                prompt: None,
            }
        );
    }

    #[test]
    fn test_parse_params_both() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            seconds: 30
            minutes: 1
            prompt: "Waiting..."
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                seconds: 30,
                minutes: 1,
                prompt: Some("Waiting...".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_params_default() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                seconds: 0,
                minutes: 0,
                prompt: None,
            }
        );
    }

    #[test]
    fn test_pause_zero() {
        let params = Params {
            seconds: 0,
            minutes: 0,
            prompt: None,
        };
        let result = pause(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_pause_check_mode() {
        let params = Params {
            seconds: 5,
            minutes: 0,
            prompt: None,
        };
        let result = pause(params, true).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_pause_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            seconds: 5
            invalid: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
