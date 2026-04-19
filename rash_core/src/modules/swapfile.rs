/// ANCHOR: module
/// # swapfile
///
/// Manage swap files on Linux systems.
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
/// - name: Create a 1GB swap file
///   swapfile:
///     path: /swapfile
///     size: 1G
///     state: present
///
/// - name: Create swap with custom priority
///   swapfile:
///     path: /swapfile
///     size: 512M
///     priority: 100
///     state: present
///
/// - name: Remove swap file
///   swapfile:
///     path: /swapfile
///     state: absent
///
/// - name: Disable existing swap
///   swapfile:
///     path: /swapfile
///     state: disabled
///
/// - name: Create swap file without enabling it
///   swapfile:
///     path: /swapfile
///     size: 1G
///     state: created
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use log::trace;
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the swap file.
    pub path: String,
    /// Size of the swap file. Supports suffixes like M (megabytes) and G (gigabytes).
    /// Required when state is present or created.
    pub size: Option<String>,
    /// State of the swap file.
    /// If _present_, the swap file will be created and enabled.
    /// If _created_, the swap file will be created but not enabled.
    /// If _absent_, the swap file will be disabled and removed.
    /// If _disabled_, the swap file will be disabled but not removed.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Priority of the swap file. Higher values indicate higher priority.
    /// Range: -1 to 32767. Default is -1 (auto priority).
    pub priority: Option<i32>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Created,
    Absent,
    Disabled,
}

fn parse_size(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    let (num_part, multiplier) = if size_str.ends_with('G') {
        (size_str.trim_end_matches('G'), 1024 * 1024 * 1024)
    } else if size_str.ends_with('M') {
        (size_str.trim_end_matches('M'), 1024 * 1024)
    } else if size_str.ends_with('K') {
        (size_str.trim_end_matches('K'), 1024)
    } else {
        (size_str.as_str(), 1)
    };

    let num: u64 = num_part.parse().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid size format '{}': {}", size_str, e),
        )
    })?;

    Ok(num * multiplier)
}

fn is_swap_enabled(path: &str) -> Result<bool> {
    let output = Command::new("swapon")
        .args(["--show", "--noheadings"])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute swapon: {e}"),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if !parts.is_empty() && parts[0] == path {
            return Ok(true);
        }
    }

    Ok(false)
}

fn get_swap_info(path: &str) -> Result<Option<SwapInfo>> {
    let output = Command::new("swapon")
        .args(["--show", "--noheadings"])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute swapon: {e}"),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if !parts.is_empty() && parts[0] == path {
            let size = parts
                .get(2)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let used = parts
                .get(3)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let priority = parts
                .get(4)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(-1);
            return Ok(Some(SwapInfo {
                path: path.to_string(),
                size,
                used,
                priority,
            }));
        }
    }

    Ok(None)
}

#[derive(Debug)]
#[allow(dead_code)]
struct SwapInfo {
    path: String,
    size: u64,
    used: u64,
    priority: i32,
}

fn disable_swap(path: &str, check_mode: bool) -> Result<bool> {
    if !is_swap_enabled(path)? {
        return Ok(false);
    }

    diff(
        format!("swap enabled: {}", path),
        format!("swap disabled: {}", path),
    );

    if check_mode {
        return Ok(true);
    }

    let output = Command::new("swapoff").arg(path).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute swapoff: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "swapoff {} failed: {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn enable_swap(path: &str, priority: Option<i32>, check_mode: bool) -> Result<bool> {
    if is_swap_enabled(path)? {
        return Ok(false);
    }

    let priority_str = if let Some(p) = priority {
        format!(" (priority {})", p)
    } else {
        String::new()
    };

    diff(
        format!("swap disabled: {}", path),
        format!("swap enabled: {}{}", path, priority_str),
    );

    if check_mode {
        return Ok(true);
    }

    let mut cmd = Command::new("swapon");
    if let Some(p) = priority {
        cmd.args(["-p", &p.to_string()]);
    }
    cmd.arg(path);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute swapon: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "swapon {} failed: {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn create_swap_file(path: &str, size_bytes: u64, check_mode: bool) -> Result<bool> {
    let file_path = Path::new(path);

    if file_path.exists() {
        let metadata = fs::metadata(file_path)?;
        let existing_size = metadata.len();
        if existing_size == size_bytes {
            let mode = metadata.permissions().mode() & 0o7777;
            if mode != 0o600 {
                if !check_mode {
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o600);
                    fs::set_permissions(file_path, permissions)?;
                }
                diff(format!("mode: {:o}", mode), "mode: 600");
                return Ok(true);
            }
            return Ok(false);
        }

        if !check_mode {
            fs::remove_file(file_path)?;
        }
        diff(
            format!("swap file exists (size: {} bytes)", existing_size),
            format!("swap file will be recreated (size: {} bytes)", size_bytes),
        );
    }

    diff(
        format!("swap file absent: {}", path),
        format!("swap file created: {} ({} bytes)", path, size_bytes),
    );

    if check_mode {
        return Ok(true);
    }

    let file = File::create(file_path)?;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(file_path, permissions)?;

    let file = OpenOptions::new().write(true).open(file_path)?;
    file.set_len(size_bytes)?;

    Ok(true)
}

fn make_swap(path: &str, _check_mode: bool) -> Result<bool> {
    let output = Command::new("mkswap").arg(path).output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mkswap: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "mkswap {} failed: {}",
                path,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn remove_swap_file(path: &str, check_mode: bool) -> Result<bool> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Ok(false);
    }

    diff(
        format!("swap file present: {}", path),
        format!("swap file removed: {}", path),
    );

    if check_mode {
        return Ok(true);
    }

    fs::remove_file(file_path)?;

    Ok(true)
}

pub fn swapfile(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("swapfile params: {:?}", params);

    let state = params.state.unwrap_or_default();
    let path = params.path.as_str();

    if (state == State::Present || state == State::Created) && params.size.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "size parameter is required when state is present or created",
        ));
    }

    if params.priority.is_some()
        && (*params.priority.as_ref().unwrap() < -1 || *params.priority.as_ref().unwrap() > 32767)
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "priority must be between -1 and 32767",
        ));
    }

    let mut changed = false;
    let mut output_messages: Vec<String> = Vec::new();

    match state {
        State::Present => {
            let size_bytes = parse_size(params.size.as_ref().unwrap())?;

            if is_swap_enabled(path)? {
                if let Some(info) = get_swap_info(path)? {
                    let current_priority = info.priority;
                    let desired_priority = params.priority.unwrap_or(-1);
                    if current_priority != desired_priority {
                        if disable_swap(path, check_mode)? {
                            output_messages
                                .push(format!("Disabled swap {} to change priority", path));
                        }
                        if enable_swap(path, params.priority, check_mode)? {
                            changed = true;
                            output_messages.push(format!(
                                "Enabled swap {} with priority {}",
                                path,
                                params.priority.unwrap_or(-1)
                            ));
                        }
                    }
                }
            } else {
                if create_swap_file(path, size_bytes, check_mode)? {
                    changed = true;
                    output_messages
                        .push(format!("Created swap file {} ({} bytes)", path, size_bytes));
                    if !check_mode {
                        make_swap(path, check_mode)?;
                    }
                }
                if enable_swap(path, params.priority, check_mode)? {
                    changed = true;
                    output_messages.push(format!("Enabled swap {}", path));
                }
            }
        }
        State::Created => {
            let size_bytes = parse_size(params.size.as_ref().unwrap())?;
            if create_swap_file(path, size_bytes, check_mode)? {
                changed = true;
                output_messages.push(format!("Created swap file {} ({} bytes)", path, size_bytes));
                if !check_mode {
                    make_swap(path, check_mode)?;
                }
            }
        }
        State::Absent => {
            if disable_swap(path, check_mode)? {
                changed = true;
                output_messages.push(format!("Disabled swap {}", path));
            }
            if remove_swap_file(path, check_mode)? {
                changed = true;
                output_messages.push(format!("Removed swap file {}", path));
            }
        }
        State::Disabled => {
            if disable_swap(path, check_mode)? {
                changed = true;
                output_messages.push(format!("Disabled swap {}", path));
            }
        }
    }

    let output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult::new(changed, None, output))
}

#[derive(Debug)]
pub struct Swapfile;

impl Module for Swapfile {
    fn get_name(&self) -> &str {
        "swapfile"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((swapfile(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /swapfile
            size: 1G
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/swapfile".to_owned(),
                size: Some("1G".to_owned()),
                state: Some(State::Present),
                priority: None,
            }
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /swapfile
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/swapfile".to_owned(),
                size: None,
                state: Some(State::Absent),
                priority: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_priority() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /swapfile
            size: 512M
            priority: 100
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.priority, Some(100));
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /swapfile
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_size_gigabytes() {
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("2G").unwrap(), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_size_megabytes() {
        assert_eq!(parse_size("512M").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_size("1M").unwrap(), 1024 * 1024);
    }

    #[test]
    fn test_parse_size_kilobytes() {
        assert_eq!(parse_size("1024K").unwrap(), 1024 * 1024);
    }

    #[test]
    fn test_parse_size_bytes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }

    #[test]
    fn test_parse_size_case_insensitive() {
        assert_eq!(parse_size("1g").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("512m").unwrap(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_parse_size_invalid() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("").is_err());
    }

    #[test]
    fn test_create_swap_file_check_mode() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");
        let path_str = swap_path.to_str().unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Created),
            priority: None,
        };

        let result = swapfile(params, true).unwrap();
        assert!(result.changed);
        assert!(!swap_path.exists());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_create_swap_file_real() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");
        let path_str = swap_path.to_str().unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Created),
            priority: None,
        };

        let result = swapfile(params, false).unwrap();
        assert!(result.changed);

        assert!(swap_path.exists());
        let metadata = fs::metadata(&swap_path).unwrap();
        assert_eq!(metadata.len(), 1024 * 1024);
        let mode = metadata.permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn test_create_swap_file_already_exists_same_size() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");
        let path_str = swap_path.to_str().unwrap();

        let file = File::create(&swap_path).unwrap();
        file.set_len(1024 * 1024).unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&swap_path, permissions).unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Created),
            priority: None,
        };

        let result = swapfile(params, true).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_swapfile_missing_size_for_present() {
        let params = Params {
            path: "/swapfile".to_string(),
            size: None,
            state: Some(State::Present),
            priority: None,
        };

        let result = swapfile(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("size parameter is required")
        );
    }

    #[test]
    fn test_swapfile_invalid_priority() {
        let params = Params {
            path: "/swapfile".to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Present),
            priority: Some(40000),
        };

        let result = swapfile(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("priority must be between")
        );
    }

    #[test]
    fn test_swapfile_negative_priority() {
        let params = Params {
            path: "/swapfile".to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Present),
            priority: Some(-2),
        };

        let result = swapfile(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_swapfile_valid_negative_priority() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");

        let params = Params {
            path: swap_path.to_str().unwrap().to_string(),
            size: Some("1M".to_string()),
            state: Some(State::Created),
            priority: Some(-1),
        };

        let result = swapfile(params, true);
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_remove_swap_file_check_mode() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");
        let path_str = swap_path.to_str().unwrap();

        File::create(&swap_path).unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: None,
            state: Some(State::Absent),
            priority: None,
        };

        let result = swapfile(params, true).unwrap();
        assert!(result.changed);
        assert!(swap_path.exists());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_remove_swap_file_real() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("swapfile");
        let path_str = swap_path.to_str().unwrap();

        File::create(&swap_path).unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: None,
            state: Some(State::Absent),
            priority: None,
        };

        let result = swapfile(params, false).unwrap();
        assert!(result.changed);
        assert!(!swap_path.exists());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_remove_swap_file_not_exists() {
        let dir = tempdir().unwrap();
        let swap_path = dir.path().join("nonexistent_swapfile");
        let path_str = swap_path.to_str().unwrap();

        let params = Params {
            path: path_str.to_string(),
            size: None,
            state: Some(State::Absent),
            priority: None,
        };

        let result = swapfile(params, false).unwrap();
        assert!(!result.changed);
    }
}
