/// ANCHOR: module
/// # sshd_config
///
/// Manage SSH server configuration in /etc/ssh/sshd_config.
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
/// - name: Set SSH port
///   sshd_config:
///     option: Port
///     value: "22"
///
/// - name: Disable root login
///   sshd_config:
///     option: PermitRootLogin
///     value: "no"
///
/// - name: Disable password authentication
///   sshd_config:
///     option: PasswordAuthentication
///     value: "no"
///
/// - name: Remove an option
///   sshd_config:
///     option: PermitRootLogin
///     state: absent
///
/// - name: Configure option within Match block
///   sshd_config:
///     option: PasswordAuthentication
///     value: "yes"
///     match_criteria: User admin
///
/// - name: Set multiple options in custom path
///   sshd_config:
///     option: MaxAuthTries
///     value: "3"
///     path: /etc/ssh/sshd_config.d/custom.conf
///     validate: true
///
/// - name: Create backup before change
///   sshd_config:
///     option: PermitRootLogin
///     value: "no"
///     backup: true
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_SSHD_CONFIG_PATH: &str = "/etc/ssh/sshd_config";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The SSH server configuration option name.
    pub option: String,
    /// The value to set for the option. Required when state=present.
    pub value: Option<String>,
    /// Whether the option should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the sshd_config file.
    /// **[default: `"/etc/ssh/sshd_config"`]**
    pub path: Option<String>,
    /// Match block criteria (e.g., "User admin", "Group ssh-users").
    /// When specified, the option is managed within this Match block.
    pub match_criteria: Option<String>,
    /// Validate configuration with sshd -t before applying.
    /// **[default: `false`]**
    pub validate: Option<bool>,
    /// Create a backup file before making changes.
    /// **[default: `false`]**
    pub backup: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
struct SshdOption {
    key: String,
    value: String,
    line_idx: usize,
    match_block: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct MatchBlock {
    criteria: String,
    line_start: usize,
    line_end: usize,
}

fn parse_sshd_config(content: &str) -> (Vec<SshdOption>, Vec<MatchBlock>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut options: Vec<SshdOption> = Vec::new();
    let mut match_blocks: Vec<MatchBlock> = Vec::new();
    let mut current_match: Option<String> = None;
    let mut current_match_start: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let lower = trimmed.to_lowercase();

        if lower.starts_with("match ") {
            if let Some(ref criteria) = current_match {
                if let Some(start) = current_match_start {
                    match_blocks.push(MatchBlock {
                        criteria: criteria.clone(),
                        line_start: start,
                        line_end: idx.saturating_sub(1),
                    });
                }
            }
            current_match = Some(trimmed[6..].trim().to_string());
            current_match_start = Some(idx);
            continue;
        }

        if let Some((key, value)) = parse_option_line(trimmed) {
            options.push(SshdOption {
                key: key.to_lowercase(),
                value,
                line_idx: idx,
                match_block: current_match.clone(),
            });
        }
    }

    if let Some(ref criteria) = current_match {
        if let Some(start) = current_match_start {
            match_blocks.push(MatchBlock {
                criteria: criteria.clone(),
                line_start: start,
                line_end: lines.len().saturating_sub(1),
            });
        }
    }

    (options, match_blocks)
}

fn parse_option_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
    if parts.len() == 2 {
        let key = parts[0].trim().to_lowercase();
        let value = parts[1].trim().to_string();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    if let Some(pos) = line.find('=') {
        let key = line[..pos].trim().to_lowercase();
        let value = line[pos + 1..].trim().to_string();
        if !key.is_empty() && !value.is_empty() {
            return Some((key, value));
        }
    }
    None
}

fn find_option<'a>(
    options: &'a [SshdOption],
    key: &str,
    match_criteria: &Option<String>,
) -> Option<&'a SshdOption> {
    let key_lower = key.to_lowercase();
    options
        .iter()
        .find(|o| o.key == key_lower && o.match_block == *match_criteria)
}

fn find_match_block<'a>(blocks: &'a [MatchBlock], criteria: &str) -> Option<&'a MatchBlock> {
    let criteria_lower = criteria.trim().to_lowercase();
    blocks
        .iter()
        .find(|b| b.criteria.to_lowercase() == criteria_lower)
}

fn rebuild_config(
    original_content: &str,
    parsed_options: &[SshdOption],
    parsed_blocks: &[MatchBlock],
    target_key: &str,
    target_value: &Option<String>,
    target_match: &Option<String>,
    state: &State,
) -> String {
    let lines: Vec<&str> = original_content.lines().collect();
    let target_key_lower = target_key.to_lowercase();

    let existing_option = find_option(parsed_options, target_key, target_match);

    match state {
        State::Present => {
            let value = target_value
                .as_deref()
                .unwrap_or("");

            if let Some(existing) = existing_option {
                if existing.value == value {
                    return original_content.to_string();
                }
                let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                new_lines[existing.line_idx] = format!("{target_key} {value}");
                return new_lines.join("\n");
            }

            let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

            if let Some(criteria) = target_match {
                if let Some(block) = find_match_block(parsed_blocks, criteria) {
                    let insert_pos = block.line_end + 1;
                    new_lines.insert(insert_pos, format!("{target_key} {value}"));
                } else {
                    if !new_lines.is_empty()
                        && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                    {
                        new_lines.push(String::new());
                    }
                    new_lines.push(format!("Match {criteria}"));
                    new_lines.push(format!("    {target_key} {value}"));
                    if !new_lines.is_empty()
                        && new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                    {
                        new_lines.push(String::new());
                    }
                }
            } else {
                let insert_pos = find_global_insert_position(parsed_options, parsed_blocks, &lines);
                new_lines.insert(insert_pos, format!("{target_key} {value}"));
            }

            new_lines.join("\n")
        }
        State::Absent => {
            if let Some(existing) = existing_option {
                let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                new_lines.remove(existing.line_idx);

                if let Some(criteria) = target_match {
                    if let Some(block) = find_match_block(parsed_blocks, criteria) {
                        let remaining_in_block = parsed_options.iter().any(|o| {
                            o.match_block.as_deref() == Some(criteria.as_str())
                                && o.key != target_key_lower
                                && o.line_idx != existing.line_idx
                        });

                        if !remaining_in_block {
                            let adjusted_block_start = if existing.line_idx < block.line_start {
                                block.line_start
                            } else {
                                block.line_start
                            };
                            if adjusted_block_start < new_lines.len() {
                                new_lines.remove(adjusted_block_start);
                            }

                            new_lines = clean_empty_lines(new_lines);
                        }
                    }
                }

                new_lines = clean_trailing_empty(new_lines);
                return new_lines.join("\n");
            }

            original_content.to_string()
        }
    }
}

fn find_global_insert_position(
    parsed_options: &[SshdOption],
    parsed_blocks: &[MatchBlock],
    lines: &[&str],
) -> usize {
    let first_match_line = parsed_blocks.iter().map(|b| b.line_start).min();

    let last_global_option = parsed_options
        .iter()
        .filter(|o| o.match_block.is_none())
        .map(|o| o.line_idx)
        .max();

    match (last_global_option, first_match_line) {
        (Some(last_opt), Some(first_match)) => {
            let mut insert_pos = last_opt + 1;
            while insert_pos < first_match
                && insert_pos < lines.len()
                && lines[insert_pos].trim().is_empty()
            {
                insert_pos += 1;
            }
            insert_pos.min(first_match)
        }
        (Some(last_opt), None) => {
            let mut insert_pos = last_opt + 1;
            while insert_pos < lines.len()
                && lines[insert_pos].trim().is_empty()
            {
                insert_pos += 1;
            }
            insert_pos
        }
        (None, Some(first_match)) => first_match,
        (None, None) => lines.len(),
    }
}

fn clean_empty_lines(lines: Vec<String>) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut prev_empty = false;
    for line in lines {
        let is_empty = line.trim().is_empty();
        if is_empty && prev_empty {
            continue;
        }
        result.push(line);
        prev_empty = is_empty;
    }
    result
}

fn clean_trailing_empty(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn create_backup(path: &PathBuf) -> Result<()> {
    use std::time::SystemTime;
    let timestamp = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new(ErrorKind::Other, e))?
        .as_secs();
    let backup_path = PathBuf::from(format!("{}.{}", path.display(), timestamp));
    fs::copy(path, &backup_path)?;
    Ok(())
}

fn validate_config(path: &PathBuf) -> Result<()> {
    let output = std::process::Command::new("sshd")
        .arg("-t")
        .arg("-f")
        .arg(path.as_os_str())
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("sshd config validation failed: {stderr}"),
        ));
    }

    Ok(())
}

pub fn sshd_config(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let config_path = params
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SSHD_CONFIG_PATH));

    match state {
        State::Present => {
            let value = params.value.as_deref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "value parameter is required when state=present",
                )
            })?;

            let original_content = if config_path.exists() {
                fs::read_to_string(&config_path)?
            } else {
                String::new()
            };

            let (options, blocks) = parse_sshd_config(&original_content);

            let existing = find_option(&options, &params.option, &params.match_criteria);

            let changed = match existing {
                Some(e) => e.value != value,
                None => true,
            };

            if !changed {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(config_path.to_string_lossy().to_string()),
                    extra: None,
                });
            }

            let new_content = rebuild_config(
                &original_content,
                &options,
                &blocks,
                &params.option,
                &Some(value.to_string()),
                &params.match_criteria,
                &State::Present,
            );

            diff(&original_content, &new_content);

            if !check_mode {
                if params.backup.unwrap_or(false) && config_path.exists() {
                    create_backup(&config_path)?;
                }

                if let Some(parent) = config_path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }

                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&config_path)?;
                file.write_all(new_content.as_bytes())?;

                if params.validate.unwrap_or(false) {
                    validate_config(&config_path)?;
                }
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(config_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
        State::Absent => {
            if !config_path.exists() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(config_path.to_string_lossy().to_string()),
                    extra: None,
                });
            }

            let original_content = fs::read_to_string(&config_path)?;
            let (options, blocks) = parse_sshd_config(&original_content);

            let existing = find_option(&options, &params.option, &params.match_criteria);

            if existing.is_none() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(config_path.to_string_lossy().to_string()),
                    extra: None,
                });
            }

            let new_content = rebuild_config(
                &original_content,
                &options,
                &blocks,
                &params.option,
                &None,
                &params.match_criteria,
                &State::Absent,
            );

            diff(&original_content, &new_content);

            if !check_mode {
                if params.backup.unwrap_or(false) {
                    create_backup(&config_path)?;
                }

                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&config_path)?;
                file.write_all(new_content.as_bytes())?;

                if params.validate.unwrap_or(false) {
                    validate_config(&config_path)?;
                }
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(config_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct SshdConfig;

impl Module for SshdConfig {
    fn get_name(&self) -> &str {
        "sshd_config"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            sshd_config(parse_params(optional_params)?, check_mode)?,
            None,
        ))
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
            option: PermitRootLogin
            value: "no"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.option, "PermitRootLogin");
        assert_eq!(params.value, Some("no".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_match() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            option: PasswordAuthentication
            value: "yes"
            match_criteria: User admin
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.option, "PasswordAuthentication");
        assert_eq!(params.match_criteria, Some("User admin".to_string()));
    }

    #[test]
    fn test_parse_sshd_config_simple() {
        let content = "Port 22\nPermitRootLogin no\nPasswordAuthentication no\n";
        let (options, blocks) = parse_sshd_config(content);
        assert_eq!(options.len(), 3);
        assert_eq!(blocks.len(), 0);
        assert_eq!(options[0].key, "port");
        assert_eq!(options[0].value, "22");
        assert_eq!(options[0].match_block, None);
    }

    #[test]
    fn test_parse_sshd_config_with_comments() {
        let content = "# SSH config\nPort 22\n# Disable root login\nPermitRootLogin no\n";
        let (options, _) = parse_sshd_config(content);
        assert_eq!(options.len(), 2);
    }

    #[test]
    fn test_parse_sshd_config_with_match_block() {
        let content = "Port 22\nPermitRootLogin no\n\nMatch User admin\n    PasswordAuthentication yes\n";
        let (options, blocks) = parse_sshd_config(content);
        assert_eq!(options.len(), 3);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].criteria, "User admin");
        assert_eq!(options[2].key, "passwordauthentication");
        assert_eq!(options[2].value, "yes");
        assert_eq!(options[2].match_block, Some("User admin".to_string()));
    }

    #[test]
    fn test_parse_sshd_config_multiple_match_blocks() {
        let content = "Port 22\n\nMatch User admin\n    PasswordAuthentication yes\n\nMatch Group ssh-users\n    AllowTcpForwarding yes\n";
        let (options, blocks) = parse_sshd_config(content);
        assert_eq!(options.len(), 3);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].criteria, "User admin");
        assert_eq!(blocks[1].criteria, "Group ssh-users");
    }

    #[test]
    fn test_parse_option_line() {
        assert_eq!(
            parse_option_line("Port 22"),
            Some(("port".to_string(), "22".to_string()))
        );
        assert_eq!(
            parse_option_line("PermitRootLogin no"),
            Some(("permitrootlogin".to_string(), "no".to_string()))
        );
    }

    #[test]
    fn test_parse_option_line_with_equals() {
        assert_eq!(
            parse_option_line("Port=22"),
            Some(("port".to_string(), "22".to_string()))
        );
    }

    #[test]
    fn test_find_option() {
        let options = vec![
            SshdOption {
                key: "port".to_string(),
                value: "22".to_string(),
                line_idx: 0,
                match_block: None,
            },
            SshdOption {
                key: "passwordauthentication".to_string(),
                value: "yes".to_string(),
                line_idx: 4,
                match_block: Some("User admin".to_string()),
            },
        ];

        assert!(find_option(&options, "Port", &None).is_some());
        assert!(find_option(&options, "PasswordAuthentication", &Some("User admin".to_string())).is_some());
        assert!(find_option(&options, "PasswordAuthentication", &None).is_none());
    }

    #[test]
    fn test_sshd_config_set_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("PermitRootLogin no"));
    }

    #[test]
    fn test_sshd_config_update_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin yes\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("PermitRootLogin no"));
        assert!(!content.contains("PermitRootLogin yes"));
        assert!(content.contains("Port 22"));
    }

    #[test]
    fn test_sshd_config_no_change() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin no\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sshd_config_remove_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin no\nPasswordAuthentication no\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: None,
            state: Some(State::Absent),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("PermitRootLogin"));
        assert!(content.contains("Port 22"));
        assert!(content.contains("PasswordAuthentication no"));
    }

    #[test]
    fn test_sshd_config_remove_not_found() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: None,
            state: Some(State::Absent),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sshd_config_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, true).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_sshd_config_match_block_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(
            &config_path,
            "Port 22\nPermitRootLogin no\n\nMatch User admin\n    PasswordAuthentication yes\n",
        )
        .unwrap();

        let params = Params {
            option: "PasswordAuthentication".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: Some("User admin".to_string()),
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Match User admin"));
        assert!(content.contains("PasswordAuthentication no"));
    }

    #[test]
    fn test_sshd_config_create_match_block() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin no\n").unwrap();

        let params = Params {
            option: "PasswordAuthentication".to_string(),
            value: Some("yes".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: Some("User admin".to_string()),
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Match User admin"));
        assert!(content.contains("PasswordAuthentication yes"));
    }

    #[test]
    fn test_sshd_config_absent_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("nonexistent_sshd_config");

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: None,
            state: Some(State::Absent),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sshd_config_add_to_existing_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin no\n").unwrap();

        let params = Params {
            option: "MaxAuthTries".to_string(),
            value: Some("3".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Port 22"));
        assert!(content.contains("PermitRootLogin no"));
        assert!(content.contains("MaxAuthTries 3"));
    }

    #[test]
    fn test_sshd_config_case_insensitive_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "port 22\npermitrootlogin no\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("yes".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("PermitRootLogin yes"));
    }

    #[test]
    fn test_sshd_config_backup() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(&config_path, "Port 22\nPermitRootLogin yes\n").unwrap();

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: Some("no".to_string()),
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: Some(true),
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("sshd_config.")
            })
            .collect();
        assert_eq!(backup_files.len(), 1, "Expected exactly one backup file");

        let backup_content = fs::read_to_string(&backup_files[0].path()).unwrap();
        assert!(backup_content.contains("PermitRootLogin yes"));
    }

    #[test]
    fn test_sshd_config_remove_from_match_block() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");
        fs::write(
            &config_path,
            "Port 22\n\nMatch User admin\n    PasswordAuthentication yes\n    AllowTcpForwarding yes\n",
        )
        .unwrap();

        let params = Params {
            option: "PasswordAuthentication".to_string(),
            value: None,
            state: Some(State::Absent),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: Some("User admin".to_string()),
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("PasswordAuthentication"));
        assert!(content.contains("Match User admin"));
        assert!(content.contains("AllowTcpForwarding yes"));
    }

    #[test]
    fn test_sshd_config_value_required_for_present() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("sshd_config");

        let params = Params {
            option: "PermitRootLogin".to_string(),
            value: None,
            state: Some(State::Present),
            path: Some(config_path.to_string_lossy().to_string()),
            match_criteria: None,
            validate: None,
            backup: None,
        };

        let result = sshd_config(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("value parameter is required"));
    }
}
