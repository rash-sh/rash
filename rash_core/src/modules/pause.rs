/// ANCHOR: module
/// # pause
///
/// Pause execution for a duration or prompt a human for input. Human input is returned as the
/// module output and can be captured with `register`.
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
///     prompt: "Environment name: "
///     input: true
///   register: answer
///
/// - pause:
///     prompt: "Password: "
///     input: true
///     echo: false
///   register: password
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

use std::io::{self, Write};
use std::time::Duration;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Number of seconds to pause.
    #[serde(default)]
    seconds: u64,
    /// Number of minutes to pause.
    #[serde(default)]
    minutes: u64,
    /// Optional message to display.
    #[serde(default)]
    prompt: Option<String>,
    /// Read one line of input from the user after displaying the prompt.
    #[serde(default)]
    input: bool,
    /// Echo interactive input. Set false for passwords/secrets.
    #[serde(default = "default_echo")]
    echo: bool,
}

fn default_echo() -> bool {
    true
}

fn read_input(prompt: Option<&str>, echo: bool) -> Result<String> {
    if let Some(prompt) = prompt {
        eprint!("{prompt}");
        io::stderr()
            .flush()
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
    }

    if echo {
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| Error::new(ErrorKind::IOError, e))?;
        Ok(input.trim_end_matches(['\r', '\n']).to_owned())
    } else {
        rpassword::read_password().map_err(|e| Error::new(ErrorKind::IOError, e))
    }
}

fn pause(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let total_seconds = params.minutes * 60 + params.seconds;

    if check_mode {
        let action = if params.input {
            "Would prompt for input".to_owned()
        } else {
            format!("Would pause for {total_seconds} seconds")
        };
        return Ok(ModuleResult::new(false, None, Some(action)));
    }

    let input = if params.input {
        Some(read_input(params.prompt.as_deref(), params.echo)?)
    } else {
        if let Some(prompt) = &params.prompt {
            eprintln!("{prompt}");
        }
        None
    };

    if total_seconds > 0 {
        std::thread::sleep(Duration::from_secs(total_seconds));
    }

    Ok(ModuleResult::new(
        false,
        None,
        input.or_else(|| (total_seconds > 0).then(|| total_seconds.to_string())),
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

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str("{}").unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.seconds, 0);
        assert_eq!(params.minutes, 0);
        assert!(!params.input);
        assert!(params.echo);
    }

    #[test]
    fn test_parse_input_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            prompt: "Password: "
            input: true
            echo: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.input);
        assert!(!params.echo);
        assert_eq!(params.prompt.as_deref(), Some("Password: "));
    }

    #[test]
    fn test_pause_zero() {
        let result = pause(
            Params {
                seconds: 0,
                minutes: 0,
                prompt: None,
                input: false,
                echo: true,
            },
            false,
        )
        .unwrap();
        assert!(!result.get_changed());
        assert_eq!(result.get_output(), None);
    }

    #[test]
    fn test_pause_check_mode_does_not_wait_or_prompt() {
        let result = pause(
            Params {
                seconds: 5,
                minutes: 0,
                prompt: Some("Question: ".into()),
                input: true,
                echo: true,
            },
            true,
        )
        .unwrap();
        assert!(!result.get_changed());
        assert_eq!(
            result.get_output().as_deref(),
            Some("Would prompt for input")
        );
    }

    #[test]
    fn test_pause_random_field() {
        let yaml: YamlValue = serde_norway::from_str("seconds: 5\ninvalid: field").unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
