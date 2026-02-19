/// ANCHOR: module
/// # sysctl
///
/// Manage kernel parameters via sysctl.
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
/// - name: Enable IP forwarding
///   sysctl:
///     name: net.ipv4.ip_forward
///     value: "1"
///     state: present
///     reload: true
///
/// - name: Set vm.swappiness
///   sysctl:
///     name: vm.swappiness
///     value: "10"
///
/// - name: Remove kernel.panic entry
///   sysctl:
///     name: kernel.panic
///     state: absent
///
/// - name: Set kernel parameter in custom file
///   sysctl:
///     name: net.core.somaxconn
///     value: "65535"
///     sysctl_file: /etc/sysctl.d/99-custom.conf
///     reload: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_SYSCTL_FILE: &str = "/etc/sysctl.conf";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The dot-separated path (key) specifying the sysctl variable.
    pub name: String,
    /// Desired value of the sysctl key. Required if state=present.
    pub value: Option<String>,
    /// Whether the entry should be present or absent in the sysctl file.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// If true, performs a sysctl -p if the sysctl_file is updated.
    /// **[default: `true`]**
    pub reload: Option<bool>,
    /// Specifies the absolute path to sysctl.conf.
    /// **[default: `"/etc/sysctl.conf"`]**
    pub sysctl_file: Option<String>,
    /// Use this option to ignore errors about unknown keys.
    /// **[default: `false`]**
    pub ignoreerrors: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone)]
struct SysctlEntry {
    key: String,
    value: String,
    line_number: usize,
}

fn parse_sysctl_content(content: &str) -> (Vec<SysctlEntry>, Vec<String>) {
    let mut entries: Vec<SysctlEntry> = Vec::new();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            entries.push(SysctlEntry {
                key,
                value,
                line_number: idx,
            });
        }
    }

    (entries, lines)
}

fn find_entry<'a>(entries: &'a [SysctlEntry], key: &str) -> Option<&'a SysctlEntry> {
    entries.iter().find(|e| e.key == key)
}

fn get_sysctl_value(name: &str) -> Result<String> {
    let output = Command::new("sysctl")
        .args(["-n", name])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute sysctl: {e}"),
            )
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "sysctl -n {} failed: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}

fn set_sysctl_value(name: &str, value: &str, ignoreerrors: bool) -> Result<()> {
    let output = Command::new("sysctl")
        .args(["-w", &format!("{name}={value}")])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute sysctl: {e}"),
            )
        })?;

    if !output.status.success() && !ignoreerrors {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "sysctl -w {}={} failed: {}",
                name,
                value,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn reload_sysctl(sysctl_file: &str) -> Result<()> {
    let output = Command::new("sysctl")
        .args(["-p", sysctl_file])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute sysctl: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "sysctl -p {} failed: {}",
                sysctl_file,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

pub fn sysctl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let reload = params.reload.unwrap_or(true);
    let sysctl_file = params.sysctl_file.as_deref().unwrap_or(DEFAULT_SYSCTL_FILE);
    let ignoreerrors = params.ignoreerrors.unwrap_or(false);

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let path = Path::new(sysctl_file);

    let (entries, mut lines) = if path.exists() {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let content: String = reader
            .lines()
            .map(|l| l.unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        parse_sysctl_content(&content)
    } else {
        (Vec::new(), Vec::new())
    };

    let original_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let mut changed = false;
    let mut file_changed = false;

    match state {
        State::Present => {
            let value = params.value.as_ref().unwrap();
            let existing = find_entry(&entries, &params.name);

            if let Some(entry) = existing {
                if entry.value != *value {
                    lines[entry.line_number] = format!("{} = {}", params.name, value);
                    file_changed = true;
                }
            } else {
                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                lines.push(format!("{} = {}", params.name, value));
                file_changed = true;
            }

            if !check_mode {
                match get_sysctl_value(&params.name) {
                    Ok(current) if current != *value => {
                        set_sysctl_value(&params.name, value, ignoreerrors)?;
                        changed = true;
                    }
                    Ok(_) => {}
                    Err(e) if !ignoreerrors => return Err(e),
                    Err(_) => {}
                }
            }

            if file_changed {
                changed = true;
            }
        }
        State::Absent => {
            if let Some(entry) = find_entry(&entries, &params.name) {
                lines.remove(entry.line_number);
                file_changed = true;
                changed = true;
            }
        }
    }

    if file_changed {
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            let trimmed: Vec<String> = lines
                .into_iter()
                .filter(|l| !l.trim().is_empty() || !l.is_empty())
                .collect();

            let mut result = String::new();
            let mut prev_empty = false;
            for line in trimmed {
                if line.trim().is_empty() {
                    if !prev_empty {
                        result.push_str(&line);
                        result.push('\n');
                        prev_empty = true;
                    }
                } else {
                    result.push_str(&line);
                    result.push('\n');
                    prev_empty = false;
                }
            }
            result
        };

        diff(&original_content, &new_content);

        if !check_mode {
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content.as_bytes())?;

            if reload && state == State::Present {
                reload_sysctl(sysctl_file)?;
            }
        }
    }

    Ok(ModuleResult::new(changed, None, Some(params.name)))
}

#[derive(Debug)]
pub struct Sysctl;

impl Module for Sysctl {
    fn get_name(&self) -> &str {
        "sysctl"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((sysctl(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: net.ipv4.ip_forward
            value: "1"
            state: present
            reload: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "net.ipv4.ip_forward".to_owned(),
                value: Some("1".to_owned()),
                state: Some(State::Present),
                reload: Some(true),
                sysctl_file: None,
                ignoreerrors: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: vm.swappiness
            value: "10"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "vm.swappiness");
        assert_eq!(params.value, Some("10".to_owned()));
        assert_eq!(params.state, None);
        assert_eq!(params.reload, None);
    }

    #[test]
    fn test_parse_sysctl_content() {
        let content = "# Kernel parameters\nnet.ipv4.ip_forward = 1\nvm.swappiness = 10\n\n# Empty line above\n";
        let (entries, lines) = parse_sysctl_content(content);

        assert_eq!(lines.len(), 5);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].key, "net.ipv4.ip_forward");
        assert_eq!(entries[0].value, "1");

        assert_eq!(entries[1].key, "vm.swappiness");
        assert_eq!(entries[1].value, "10");
    }

    #[test]
    fn test_find_entry() {
        let content = "net.ipv4.ip_forward = 1\nvm.swappiness = 10\n";
        let (entries, _) = parse_sysctl_content(content);

        let found = find_entry(&entries, "net.ipv4.ip_forward");
        assert!(found.is_some());
        assert_eq!(found.unwrap().value, "1");

        let not_found = find_entry(&entries, "kernel.panic");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_sysctl_add_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        fs::write(&file_path, "net.ipv4.ip_forward = 0\n").unwrap();

        let params = Params {
            name: "vm.swappiness".to_string(),
            value: Some("10".to_string()),
            state: Some(State::Present),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: Some(true),
        };

        let result = sysctl(params, true).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("net.ipv4.ip_forward = 0"));
    }

    #[test]
    fn test_sysctl_modify_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        fs::write(&file_path, "net.ipv4.ip_forward = 0\n").unwrap();

        let params = Params {
            name: "net.ipv4.ip_forward".to_string(),
            value: Some("1".to_string()),
            state: Some(State::Present),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: Some(true),
        };

        let result = sysctl(params, true).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_sysctl_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        fs::write(&file_path, "net.ipv4.ip_forward = 1\n").unwrap();

        let params = Params {
            name: "net.ipv4.ip_forward".to_string(),
            value: Some("1".to_string()),
            state: Some(State::Present),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: Some(true),
        };

        let result = sysctl(params, true).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sysctl_remove_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        fs::write(&file_path, "net.ipv4.ip_forward = 1\nvm.swappiness = 10\n").unwrap();

        let params = Params {
            name: "vm.swappiness".to_string(),
            value: None,
            state: Some(State::Absent),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: None,
        };

        let result = sysctl(params, true).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_sysctl_remove_nonexistent_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        fs::write(&file_path, "net.ipv4.ip_forward = 1\n").unwrap();

        let params = Params {
            name: "kernel.panic".to_string(),
            value: None,
            state: Some(State::Absent),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: None,
        };

        let result = sysctl(params, true).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sysctl_missing_value_for_present() {
        let params = Params {
            name: "net.ipv4.ip_forward".to_string(),
            value: None,
            state: Some(State::Present),
            reload: None,
            sysctl_file: None,
            ignoreerrors: None,
        };

        let result = sysctl(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_sysctl_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sysctl.conf");

        let params = Params {
            name: "net.ipv4.ip_forward".to_string(),
            value: Some("1".to_string()),
            state: Some(State::Present),
            reload: Some(false),
            sysctl_file: Some(file_path.to_str().unwrap().to_string()),
            ignoreerrors: Some(true),
        };

        let result = sysctl(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("net.ipv4.ip_forward = 1"));
    }
}
