/// ANCHOR: module
/// # debconf
///
/// Configure Debian packages using debconf.
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
/// - name: Set MySQL root password
///   debconf:
///     name: mysql-server
///     question: mysql-server/root_password
///     value: secret
///     vtype: password
///
/// - name: Set keyboard layout
///   debconf:
///     name: keyboard-configuration
///     question: keyboard-configuration/layoutcode
///     value: us
///     vtype: select
///
/// - name: Set timezone for tzdata
///   debconf:
///     name: tzdata
///     question: tzdata/Areas
///     value: Etc
///     vtype: select
///
/// - name: Set a question as unseen
///   debconf:
///     name: some-package
///     question: some-package/some-question
///     value: "some value"
///     vtype: string
///     unseen: true
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
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Vtype {
    #[default]
    String,
    Password,
    Boolean,
    Select,
    Multiselect,
    Note,
    Text,
    Error,
    Title,
}

impl Vtype {
    fn as_str(&self) -> &'static str {
        match self {
            Vtype::String => "string",
            Vtype::Password => "password",
            Vtype::Boolean => "boolean",
            Vtype::Select => "select",
            Vtype::Multiselect => "multiselect",
            Vtype::Note => "note",
            Vtype::Text => "text",
            Vtype::Error => "error",
            Vtype::Title => "title",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the package to configure
    pub name: String,
    /// The debconf question to set
    pub question: String,
    /// The value to set for the question
    pub value: String,
    /// The type of the value (string, password, boolean, select, multiselect, note, text, error, title)
    #[serde(default)]
    pub vtype: Vtype,
    /// Do not set the question as seen (default: false)
    #[serde(default)]
    pub unseen: bool,
}

fn get_current_value(name: &str, question: &str) -> Result<Option<String>> {
    let output = Command::new("debconf-show")
        .arg(name)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute debconf-show: {}", e),
            )
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('*') || trimmed.starts_with('-') {
            let rest = trimmed[1..].trim_start();
            if rest.starts_with(&format!("{}:", question)) {
                let value = rest
                    .strip_prefix(&format!("{}:", question))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                return Ok(Some(value));
            }
        }
    }

    Ok(None)
}

fn debconf_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let name = params.name.trim();
    let question = params.question.trim();
    let value = params.value.trim();
    let vtype = params.vtype.as_str();

    if name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    if question.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "question cannot be empty",
        ));
    }

    let current_value = get_current_value(name, question)?;

    if current_value.as_deref() == Some(value) {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "Question '{}' for '{}' already set to '{}'",
                question, name, value
            )),
            extra: None,
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would set question '{}' for '{}' to '{}'",
                question, name, value
            )),
            extra: None,
        });
    }

    let input = if params.unseen {
        format!("-{} {} {} {}\n", name, question, vtype, value)
    } else {
        format!("{} {} {} {}\n", name, question, vtype, value)
    };

    let mut child = Command::new("debconf-set-selections")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute debconf-set-selections: {}", e),
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write to debconf-set-selections: {}", e),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to wait for debconf-set-selections: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("debconf-set-selections failed: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Set question '{}' for '{}' to '{}'",
            question, name, value
        )),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Debconf;

impl Module for Debconf {
    fn get_name(&self) -> &str {
        "debconf"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((debconf_impl(params, check_mode)?, None))
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
    fn test_parse_params_basic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: mysql-server
            question: mysql-server/root_password
            value: secret
            vtype: password
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "mysql-server".to_string(),
                question: "mysql-server/root_password".to_string(),
                value: "secret".to_string(),
                vtype: Vtype::Password,
                unseen: false,
            }
        );
    }

    #[test]
    fn test_parse_params_with_unseen() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: some-package
            question: some-package/some-question
            value: "some value"
            vtype: string
            unseen: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "some-package".to_string(),
                question: "some-package/some-question".to_string(),
                value: "some value".to_string(),
                vtype: Vtype::String,
                unseen: true,
            }
        );
    }

    #[test]
    fn test_parse_params_default_vtype() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: tzdata
            question: tzdata/Areas
            value: Etc
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "tzdata".to_string(),
                question: "tzdata/Areas".to_string(),
                value: "Etc".to_string(),
                vtype: Vtype::String,
                unseen: false,
            }
        );
    }

    #[test]
    fn test_parse_params_boolean() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: some-package
            question: some-package/boolean-question
            value: "true"
            vtype: boolean
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vtype, Vtype::Boolean);
    }

    #[test]
    fn test_parse_params_select() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: keyboard-configuration
            question: keyboard-configuration/layoutcode
            value: us
            vtype: select
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vtype, Vtype::Select);
    }

    #[test]
    fn test_parse_params_empty_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: ""
            question: "test"
            value: "test"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "");
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: "test"
            question: "test"
            value: "test"
            unknown: "field"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_vtype_as_str() {
        assert_eq!(Vtype::String.as_str(), "string");
        assert_eq!(Vtype::Password.as_str(), "password");
        assert_eq!(Vtype::Boolean.as_str(), "boolean");
        assert_eq!(Vtype::Select.as_str(), "select");
        assert_eq!(Vtype::Multiselect.as_str(), "multiselect");
        assert_eq!(Vtype::Note.as_str(), "note");
        assert_eq!(Vtype::Text.as_str(), "text");
        assert_eq!(Vtype::Error.as_str(), "error");
        assert_eq!(Vtype::Title.as_str(), "title");
    }
}
