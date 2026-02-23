/// ANCHOR: module
/// # debconf
///
/// Configure a .deb package using debconf-set-selections.
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
/// - name: Set default locale to fr_FR.UTF-8
///   debconf:
///     name: locales
///     question: locales/default_environment_locale
///     value: fr_FR.UTF-8
///     vtype: select
///
/// - name: Set to generate locales
///   debconf:
///     name: locales
///     question: locales/locales_to_be_generated
///     value: en_US.UTF-8 UTF-8, fr_FR.UTF-8 UTF-8
///     vtype: multiselect
///
/// - name: Accept oracle license
///   debconf:
///     name: oracle-java7-installer
///     question: shared/accepted-oracle-license-v1-1
///     value: "true"
///     vtype: select
///
/// - name: Query package settings
///   debconf:
///     name: tzdata
///   register: tzdata_settings
///
/// - name: Pre-configure tripwire site passphrase
///   debconf:
///     name: tripwire
///     question: tripwire/site-passphrase
///     value: "{{ site_passphrase }}"
///     vtype: password
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
use serde_norway::value;
use std::process::Command;

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
    Seen,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of package to configure.
    pub name: String,
    /// A debconf configuration setting.
    pub question: Option<String>,
    /// Value to set the configuration to.
    pub value: Option<String>,
    /// The type of the value supplied (string, password, boolean, select, multiselect, note, text, error, title, seen).
    pub vtype: Option<Vtype>,
    /// Do not set 'seen' flag when pre-seeding.
    #[serde(default)]
    pub unseen: bool,
}

#[derive(Debug, Clone)]
struct DebconfEntry {
    #[allow(dead_code)]
    package: String,
    question: String,
    vtype: String,
    value: String,
}

fn parse_debconf_show(output: &str) -> Vec<DebconfEntry> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix('*').unwrap_or(line).trim();
            let colon_pos = line.find(':')?;
            let pkg_question = &line[..colon_pos];
            let type_value = &line[colon_pos + 1..].trim();
            let space_pos = pkg_question.find(' ')?;
            let package = pkg_question[..space_pos].to_string();
            let question = pkg_question[space_pos + 1..].to_string();
            let (vtype, value) = type_value
                .find(' ')
                .map(|pos| {
                    (
                        type_value[..pos].to_string(),
                        type_value[pos + 1..].to_string(),
                    )
                })
                .unwrap_or((type_value.to_string(), String::new()));
            Some(DebconfEntry {
                package,
                question,
                vtype,
                value,
            })
        })
        .collect()
}

fn get_current_selection(package: &str, question: &str) -> Result<Option<DebconfEntry>> {
    let output = Command::new("debconf-show")
        .arg(package)
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
    let entries = parse_debconf_show(&stdout);

    Ok(entries.into_iter().find(|e| e.question == question))
}

fn get_all_selections(package: &str) -> Result<Vec<DebconfEntry>> {
    let output = Command::new("debconf-show")
        .arg(package)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute debconf-show: {}", e),
            )
        })?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_debconf_show(&stdout))
}

fn vtype_to_string(vtype: &Vtype) -> &'static str {
    match vtype {
        Vtype::String => "string",
        Vtype::Password => "password",
        Vtype::Boolean => "boolean",
        Vtype::Select => "select",
        Vtype::Multiselect => "multiselect",
        Vtype::Note => "note",
        Vtype::Text => "text",
        Vtype::Error => "error",
        Vtype::Title => "title",
        Vtype::Seen => "seen",
    }
}

fn debconf_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let package = params.name.trim();

    if package.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    if let Some(question) = &params.question {
        let question = question.trim();
        let value = params.value.as_ref().map(|v| v.trim()).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "value is required when question is specified",
            )
        })?;

        let vtype = params.vtype.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "vtype is required when question is specified",
            )
        })?;

        let current = get_current_selection(package, question)?;

        if let Some(ref entry) = current
            && entry.value == *value
        {
            let extra = Some(value::to_value(json!({
                "current_value": entry.value,
                "vtype": entry.vtype.clone(),
            }))?);

            return Ok(ModuleResult {
                changed: false,
                output: Some(format!(
                    "Package '{}' question '{}' already set to '{}'",
                    package, question, value
                )),
                extra,
            });
        }

        if check_mode {
            return Ok(ModuleResult {
                changed: true,
                output: Some(format!(
                    "Would set package '{}' question '{}' to '{}'",
                    package, question, value
                )),
                extra: None,
            });
        }

        let vtype_str = vtype_to_string(vtype);
        let seen = if params.unseen { "false" } else { "true" };

        let selection = format!("{} {} {} {}", package, question, vtype_str, value);

        let mut cmd = Command::new("debconf-set-selections");
        let child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute debconf-set-selections: {}", e),
                )
            })?;

        if let Some(mut stdin) = child.stdin.as_ref() {
            use std::io::Write;
            writeln!(stdin, "{}", selection).map_err(|e| {
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

        if !params.unseen {
            let seen_selection = format!("{} {} seen {}", package, question, seen);
            let mut seen_cmd = Command::new("debconf-set-selections");
            let seen_child = seen_cmd
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to execute debconf-set-selections: {}", e),
                    )
                })?;

            if let Some(mut stdin) = seen_child.stdin.as_ref() {
                use std::io::Write;
                writeln!(stdin, "{}", seen_selection).map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to write to debconf-set-selections: {}", e),
                    )
                })?;
            }

            let seen_output = seen_child.wait_with_output().map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to wait for debconf-set-selections: {}", e),
                )
            })?;

            if !seen_output.status.success() {
                let stderr = String::from_utf8_lossy(&seen_output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "debconf-set-selections failed when setting seen flag: {}",
                        stderr
                    ),
                ));
            }
        }

        let extra = Some(value::to_value(json!({
            "current_value": value,
            "vtype": vtype_str.to_string(),
        }))?);

        Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Set package '{}' question '{}' to '{}'",
                package, question, value
            )),
            extra,
        })
    } else {
        let entries = get_all_selections(package)?;

        if entries.is_empty() {
            return Ok(ModuleResult {
                changed: false,
                output: Some(format!("Package '{}' has no debconf settings", package)),
                extra: Some(value::to_value(json!({
                    "settings": []
                }))?),
            });
        }

        let settings: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "question": e.question,
                    "vtype": e.vtype,
                    "value": e.value,
                })
            })
            .collect();

        let extra = Some(value::to_value(json!({
            "settings": settings
        }))?);

        Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "Package '{}' has {} debconf settings",
                package,
                entries.len()
            )),
            extra,
        })
    }
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
    fn test_parse_params_name_only() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: tzdata
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "tzdata".to_string(),
                question: None,
                value: None,
                vtype: None,
                unseen: false,
            }
        );
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: locales
            question: locales/default_environment_locale
            value: fr_FR.UTF-8
            vtype: select
            unseen: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "locales");
        assert_eq!(
            params.question,
            Some("locales/default_environment_locale".to_string())
        );
        assert_eq!(params.value, Some("fr_FR.UTF-8".to_string()));
        assert_eq!(params.vtype, Some(Vtype::Select));
        assert!(params.unseen);
    }

    #[test]
    fn test_parse_params_boolean() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oracle-java7-installer
            question: shared/accepted-oracle-license-v1-1
            value: "true"
            vtype: boolean
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vtype, Some(Vtype::Boolean));
    }

    #[test]
    fn test_parse_params_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: tripwire
            question: tripwire/site-passphrase
            value: secret
            vtype: password
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vtype, Some(Vtype::Password));
    }

    #[test]
    fn test_parse_params_multiselect() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: locales
            question: locales/locales_to_be_generated
            value: "en_US.UTF-8 UTF-8, fr_FR.UTF-8 UTF-8"
            vtype: multiselect
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vtype, Some(Vtype::Multiselect));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: tzdata
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_debconf_show() {
        let output = r#"tzdata tzdata/Areas: select Europe
tzdata tzdata/Zones/Europe: select Paris
locales locales/default_environment_locale: select en_US.UTF-8"#;
        let entries = parse_debconf_show(output);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].package, "tzdata");
        assert_eq!(entries[0].question, "tzdata/Areas");
        assert_eq!(entries[0].vtype, "select");
        assert_eq!(entries[0].value, "Europe");
    }

    #[test]
    fn test_vtype_to_string() {
        assert_eq!(vtype_to_string(&Vtype::String), "string");
        assert_eq!(vtype_to_string(&Vtype::Password), "password");
        assert_eq!(vtype_to_string(&Vtype::Boolean), "boolean");
        assert_eq!(vtype_to_string(&Vtype::Select), "select");
        assert_eq!(vtype_to_string(&Vtype::Multiselect), "multiselect");
    }
}
