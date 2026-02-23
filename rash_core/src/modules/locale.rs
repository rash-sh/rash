/// ANCHOR: module
/// # locale
///
/// Manage system locale settings.
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
/// - name: Set system locale
///   locale:
///     name: en_US.UTF-8
///     state: present
///
/// - name: Set all locale variables
///   locale:
///     lang: en_US.UTF-8
///     lc_all: en_US.UTF-8
///
/// - name: Generate a locale
///   locale:
///     name: de_DE.UTF-8
///     state: present
///
/// - name: Set specific locale variables
///   locale:
///     lang: en_US.UTF-8
///     lc_ctype: en_US.UTF-8
///     lc_messages: C
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

const LOCALE_GEN_PATH: &str = "/etc/locale.gen";
const LOCALE_DEF_PATH: &str = "/usr/lib/locale";
const DEFAULT_ENV_PATH: &str = "/etc/default/locale";
const ENVIRONMENT_PATH: &str = "/etc/environment";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum State {
    /// Locale should be present (generated if needed).
    #[default]
    Present,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The locale name to manage (e.g., en_US.UTF-8).
    pub name: Option<String>,
    /// Whether the locale should be present.
    #[serde(default)]
    pub state: State,
    /// Set the LANG environment variable.
    pub lang: Option<String>,
    /// Set the LC_ALL environment variable.
    pub lc_all: Option<String>,
    /// Set the LC_CTYPE environment variable.
    pub lc_ctype: Option<String>,
    /// Set the LC_MESSAGES environment variable.
    pub lc_messages: Option<String>,
    /// Set the LC_COLLATE environment variable.
    pub lc_collate: Option<String>,
    /// Set the LC_NUMERIC environment variable.
    pub lc_numeric: Option<String>,
    /// Set the LC_TIME environment variable.
    pub lc_time: Option<String>,
    /// Set the LC_MONETARY environment variable.
    pub lc_monetary: Option<String>,
    /// Set the LC_PAPER environment variable.
    pub lc_paper: Option<String>,
    /// Set the LC_NAME environment variable.
    pub lc_name: Option<String>,
    /// Set the LC_ADDRESS environment variable.
    pub lc_address: Option<String>,
    /// Set the LC_TELEPHONE environment variable.
    pub lc_telephone: Option<String>,
    /// Set the LC_MEASUREMENT environment variable.
    pub lc_measurement: Option<String>,
    /// Set the LC_IDENTIFICATION environment variable.
    pub lc_identification: Option<String>,
}

#[derive(Debug)]
pub struct Locale;

impl Module for Locale {
    fn get_name(&self) -> &str {
        "locale"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            manage_locale(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn locale_exists(name: &str) -> bool {
    let locale_path = Path::new(LOCALE_DEF_PATH).join(name);
    locale_path.exists() || locale_available_via_locale_a(name)
}

fn locale_available_via_locale_a(name: &str) -> bool {
    Command::new("locale")
        .arg("-a")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.trim() == name || line.trim().replace('-', "") == name.replace('-', "")
            })
        })
        .unwrap_or(false)
}

fn is_locale_enabled_in_gen(name: &str) -> bool {
    if !Path::new(LOCALE_GEN_PATH).exists() {
        return true;
    }

    fs::read_to_string(LOCALE_GEN_PATH)
        .map(|content| {
            let locale_base = name.split('.').next().unwrap_or(name);
            content.lines().any(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with('#') && trimmed.contains(locale_base)
            })
        })
        .unwrap_or(false)
}

fn enable_locale_in_gen(name: &str, check_mode: bool) -> Result<bool> {
    if !Path::new(LOCALE_GEN_PATH).exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(LOCALE_GEN_PATH).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read {LOCALE_GEN_PATH}: {e}"),
        )
    })?;

    let locale_base = name.split('.').next().unwrap_or(name);
    let mut modified = false;
    let mut new_content = String::new();

    for line in content.lines() {
        if line.trim().starts_with('#')
            && (line.contains(&format!("{locale_base} "))
                || line.contains(&format!("{locale_base}\t"))
                || line.ends_with(locale_base))
        {
            let uncommented = line.trim().trim_start_matches('#').trim();
            if uncommented.starts_with(locale_base) {
                if !check_mode {
                    new_content.push_str(uncommented);
                    new_content.push('\n');
                }
                modified = true;
                continue;
            }
        }
        new_content.push_str(line);
        new_content.push('\n');
    }

    if modified && !check_mode {
        fs::write(LOCALE_GEN_PATH, new_content).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write {LOCALE_GEN_PATH}: {e}"),
            )
        })?;
    }

    Ok(modified)
}

fn run_locale_gen(name: &str, check_mode: bool) -> Result<bool> {
    if check_mode {
        return Ok(true);
    }

    let output = Command::new("locale-gen")
        .arg(name)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to generate locale {}: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn generate_locale(name: &str, check_mode: bool) -> Result<bool> {
    if locale_exists(name) {
        return Ok(false);
    }

    let was_enabled = enable_locale_in_gen(name, check_mode)?;
    if was_enabled || !is_locale_enabled_in_gen(name) {
        run_locale_gen(name, check_mode)?;
    }

    Ok(true)
}

fn read_locale_file(path: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();

    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                vars.insert(key, value);
            }
        }
    }

    vars
}

fn write_locale_file(
    path: &str,
    vars: &std::collections::HashMap<String, String>,
    check_mode: bool,
) -> Result<bool> {
    let existing = read_locale_file(path);
    let mut changed = false;
    let mut final_vars = existing.clone();

    for (key, value) in vars {
        if final_vars.get(key) != Some(value) {
            final_vars.insert(key.clone(), value.clone());
            changed = true;
        }
    }

    if !changed || check_mode {
        return Ok(changed);
    }

    let content: String = final_vars
        .iter()
        .map(|(k, v)| format!("{k}=\"{v}\"\n"))
        .collect();

    if let Some(parent) = Path::new(path).parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create directory {}: {e}", parent.display()),
            )
        })?;
    }

    fs::write(path, content).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to write {path}: {e}"),
        )
    })?;

    Ok(changed)
}

fn set_locale_env_var(key: &str, value: &str, check_mode: bool) -> Result<bool> {
    let mut vars = std::collections::HashMap::new();
    vars.insert(key.to_string(), value.to_string());

    let mut changed = false;

    if Path::new(DEFAULT_ENV_PATH)
        .parent()
        .map(|p| p.exists())
        .unwrap_or(false)
        && write_locale_file(DEFAULT_ENV_PATH, &vars, check_mode)?
    {
        changed = true;
    }

    if write_locale_file(ENVIRONMENT_PATH, &vars, check_mode)? {
        changed = true;
    }

    Ok(changed)
}

fn manage_locale(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let mut changed = false;
    let mut messages = Vec::new();

    if let Some(name) = &params.name {
        match params.state {
            State::Present => {
                if generate_locale(name, check_mode)? {
                    changed = true;
                    messages.push(format!("Locale {name} generated"));
                }
            }
        }
    }

    let locale_vars = [
        ("LANG", params.lang),
        ("LC_ALL", params.lc_all),
        ("LC_CTYPE", params.lc_ctype),
        ("LC_MESSAGES", params.lc_messages),
        ("LC_COLLATE", params.lc_collate),
        ("LC_NUMERIC", params.lc_numeric),
        ("LC_TIME", params.lc_time),
        ("LC_MONETARY", params.lc_monetary),
        ("LC_PAPER", params.lc_paper),
        ("LC_NAME", params.lc_name),
        ("LC_ADDRESS", params.lc_address),
        ("LC_TELEPHONE", params.lc_telephone),
        ("LC_MEASUREMENT", params.lc_measurement),
        ("LC_IDENTIFICATION", params.lc_identification),
    ];

    for (var_name, var_value) in locale_vars {
        if let Some(value) = var_value
            && set_locale_env_var(var_name, &value, check_mode)?
        {
            changed = true;
            messages.push(format!("{var_name} set to {value}"));
        }
    }

    let output = if messages.is_empty() {
        if params.name.is_some() {
            Some("Locale already present and configured".to_string())
        } else {
            Some("No changes needed".to_string())
        }
    } else {
        Some(messages.join("; "))
    };

    Ok(ModuleResult::new(changed, None, output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: en_US.UTF-8
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_env_vars() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            lang: en_US.UTF-8
            lc_all: en_US.UTF-8
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.lang, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.lc_all, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.name, None);
    }

    #[test]
    fn test_parse_params_all_vars() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: de_DE.UTF-8
            lang: en_US.UTF-8
            lc_all: C
            lc_ctype: en_US.UTF-8
            lc_messages: C
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("de_DE.UTF-8".to_string()));
        assert_eq!(params.lang, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.lc_all, Some("C".to_string()));
        assert_eq!(params.lc_ctype, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.lc_messages, Some("C".to_string()));
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: en_US.UTF-8
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: en_US.UTF-8
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_read_locale_file() {
        let vars = read_locale_file("/nonexistent/file");
        assert!(vars.is_empty());
    }

    #[test]
    fn test_locale_exists_existing() {
        if Path::new("/usr/lib/locale/C.UTF-8").exists() || Path::new("/usr/lib/locale/C").exists()
        {
            assert!(locale_exists("C"));
        }
    }
}
