/// ANCHOR: module
/// # timezone
///
/// Configure system timezone.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Set timezone to UTC
///   timezone:
///     name: UTC
///
/// - name: Set timezone to Europe/Madrid
///   timezone:
///     name: Europe/Madrid
///
/// - name: Set timezone to America/New_York
///   timezone:
///     name: America/New_York
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

const ZONEINFO_PATH: &str = "/usr/share/zoneinfo";
const LOCALTIME_PATH: &str = "/etc/localtime";
const TIMEZONE_FILE: &str = "/etc/timezone";

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the timezone (e.g., UTC, Europe/Madrid, America/New_York).
    pub name: String,
}

fn get_timezone_link_target() -> Result<String> {
    let link_target = fs::read_link(LOCALTIME_PATH).map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read {LOCALTIME_PATH}: {e}"),
        )
    })?;

    let target_str = link_target.to_string_lossy().to_string();

    if let Some(stripped) = target_str.strip_prefix(ZONEINFO_PATH) {
        Ok(stripped.trim_start_matches('/').to_string())
    } else if link_target.is_absolute() {
        Ok(link_target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(target_str))
    } else {
        Ok(target_str)
    }
}

fn get_current_timezone() -> Result<String> {
    get_timezone_link_target()
}

fn timezone_file_exists(name: &str) -> Result<bool> {
    let tz_path = Path::new(ZONEINFO_PATH).join(name);
    Ok(tz_path.exists())
}

fn set_timezone(name: &str, check_mode: bool) -> Result<ModuleResult> {
    if !timezone_file_exists(name)? {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Timezone '{name}' not found in {ZONEINFO_PATH}"),
        ));
    }

    let current_tz = get_current_timezone().unwrap_or_default();

    if current_tz == name {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Timezone already set to {name}")),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!(
                "Timezone would be changed from {current_tz} to {name}"
            )),
        ));
    }

    let tz_path = Path::new(ZONEINFO_PATH).join(name);
    let localtime_path = Path::new(LOCALTIME_PATH);

    if localtime_path.exists() {
        fs::remove_file(localtime_path).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to remove {LOCALTIME_PATH}: {e}"),
            )
        })?;
    }

    if let Some(parent) = localtime_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create directory {}: {e}", parent.display()),
            )
        })?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&tz_path, localtime_path).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create symlink {LOCALTIME_PATH}: {e}"),
            )
        })?;
    }

    let _ = fs::write(TIMEZONE_FILE, name);

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Timezone changed from {current_tz} to {name}")),
    ))
}

#[derive(Debug)]
pub struct Timezone;

impl Module for Timezone {
    fn get_name(&self) -> &str {
        "timezone"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((set_timezone(&params.name, check_mode)?, None))
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
            name: UTC
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "UTC".to_owned(),
            }
        );
    }

    #[test]
    fn test_parse_params_timezone_with_slash() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: Europe/Madrid
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "Europe/Madrid".to_owned(),
            }
        );
    }

    #[test]
    fn test_timezone_file_exists_utc() {
        if Path::new(ZONEINFO_PATH).exists() {
            assert!(timezone_file_exists("UTC").unwrap());
        }
    }

    #[test]
    fn test_timezone_file_exists_invalid() {
        if Path::new(ZONEINFO_PATH).exists() {
            assert!(!timezone_file_exists("Invalid/Timezone").unwrap());
        }
    }

    #[test]
    fn test_set_timezone_invalid() {
        if Path::new(ZONEINFO_PATH).exists() {
            let result = set_timezone("Invalid/Timezone", false);
            assert!(result.is_err());
        }
    }
}
