/// ANCHOR: module
/// # ssh_config
///
/// Manage SSH client configuration in ~/.ssh/config or /etc/ssh/ssh_config.
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
/// - ssh_config:
///     host: github.com
///     options:
///       hostname: github.com
///       user: git
///       identityfile: ~/.ssh/github_key
///
/// - ssh_config:
///     host: "*.example.com"
///     options:
///       user: deploy
///       port: "2222"
///
/// - ssh_config:
///     host: old-server
///     state: absent
///
/// - ssh_config:
///     host: tunnel-server
///     options:
///       hostname: 192.168.1.100
///       localforward: "8080:localhost:80"
///     ssh_config_file: /etc/ssh/ssh_config
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_SSH_CONFIG_PATH: &str = "~/.ssh/config";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The host pattern to configure (e.g., "github.com", "*.example.com").
    pub host: String,
    /// SSH options to set as a dictionary of key-value pairs.
    pub options: Option<OptionsInput>,
    /// Whether the host entry should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the SSH config file.
    /// **[default: `"~/.ssh/config"`]**
    pub ssh_config_file: Option<String>,
    /// Order of host entry placement (first, last, or None for in-place update).
    pub order: Option<Order>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum OptionsInput {
    Single(serde_json::Value),
    Map(std::collections::HashMap<String, String>),
}

impl OptionsInput {
    pub fn to_map(&self) -> std::collections::HashMap<String, String> {
        match self {
            OptionsInput::Map(m) => m.clone(),
            OptionsInput::Single(v) => {
                if let serde_json::Value::Object(obj) = v {
                    obj.iter()
                        .map(|(k, v)| {
                            let val = match v {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Number(n) => n.to_string(),
                                serde_json::Value::Bool(b) => b.to_string(),
                                _ => v.to_string(),
                            };
                            (k.clone(), val)
                        })
                        .collect()
                } else {
                    std::collections::HashMap::new()
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Order {
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostEntry {
    pub host_pattern: String,
    pub options: std::collections::HashMap<String, String>,
    pub line_start: usize,
    pub line_end: usize,
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(&path[2..]);
    }
    PathBuf::from(path)
}

fn get_ssh_config_path(params: &Params) -> PathBuf {
    if let Some(ref path) = params.ssh_config_file {
        expand_tilde(path)
    } else {
        expand_tilde(DEFAULT_SSH_CONFIG_PATH)
    }
}

fn parse_ssh_config(content: &str) -> Vec<HostEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let mut entries: Vec<HostEntry> = Vec::new();
    let mut current_entry: Option<HostEntry> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.to_lowercase().starts_with("host ") {
            if let Some(entry) = current_entry.take() {
                entries.push(entry);
            }

            let host_pattern = trimmed[5..].trim().to_string();
            current_entry = Some(HostEntry {
                host_pattern,
                options: std::collections::HashMap::new(),
                line_start: idx,
                line_end: idx,
            });
        } else if let Some(ref mut entry) = current_entry
            && let Some((key, value)) = parse_option_line(trimmed)
        {
            entry.options.insert(key.to_lowercase(), value);
            entry.line_end = idx;
        }
    }

    if let Some(entry) = current_entry {
        entries.push(entry);
    }

    entries
}

fn parse_option_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() == 2 {
        let key = parts[0].trim().to_lowercase();
        let value = parts[1].trim().to_string();
        Some((key, value))
    } else if parts.len() == 1 && !parts[0].is_empty() {
        let trimmed = parts[0].trim();
        for sep in ['=', ' '] {
            if let Some(pos) = trimmed.find(sep) {
                let key = trimmed[..pos].trim().to_lowercase();
                let value = trimmed[pos + 1..].trim().to_string();
                if !key.is_empty() {
                    return Some((key, value));
                }
            }
        }
        None
    } else {
        None
    }
}

fn format_host_entry(
    host_pattern: &str,
    options: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = format!("Host {host_pattern}\n");
    for (key, value) in options {
        result.push_str(&format!("    {key} {value}\n"));
    }
    result
}

fn host_matches_pattern(host_pattern: &str, target_host: &str) -> bool {
    if host_pattern == target_host {
        return true;
    }

    let patterns: Vec<&str> = host_pattern.split_whitespace().collect();
    for pattern in patterns {
        if pattern == target_host {
            return true;
        }
        if (pattern.contains('*') || pattern.contains('?'))
            && matches_ssh_pattern(pattern, target_host)
        {
            return true;
        }
    }

    false
}

fn matches_ssh_pattern(pattern: &str, hostname: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    let hostname_lower = hostname.to_lowercase();

    if pattern_lower == hostname_lower {
        return true;
    }

    if pattern_lower == "*" {
        return true;
    }

    if let Some(domain) = pattern_lower.strip_prefix("*.")
        && (hostname_lower.ends_with(domain) || hostname_lower == domain)
    {
        return true;
    }

    let pattern_chars: Vec<char> = pattern_lower.chars().collect();
    let hostname_chars: Vec<char> = hostname_lower.chars().collect();
    let mut dp = vec![vec![false; hostname_chars.len() + 1]; pattern_chars.len() + 1];
    dp[0][0] = true;

    for i in 1..=pattern_chars.len() {
        if pattern_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pattern_chars.len() {
        for j in 1..=hostname_chars.len() {
            if pattern_chars[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pattern_chars[i - 1] == '?' || pattern_chars[i - 1] == hostname_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[pattern_chars.len()][hostname_chars.len()]
}

fn rebuild_config(
    original_lines: &[&str],
    entries: &[HostEntry],
    new_entries: &[HostEntry],
    target_host: &str,
    order: &Option<Order>,
) -> String {
    let mut result_lines: Vec<String> = Vec::new();

    for (idx, line) in original_lines.iter().enumerate() {
        let is_in_host_block = entries
            .iter()
            .any(|e| idx >= e.line_start && idx <= e.line_end);
        if !is_in_host_block {
            result_lines.push(line.to_string());
        }
    }

    let existing_entries: Vec<HostEntry> = entries
        .iter()
        .filter(|e| !host_matches_pattern(&e.host_pattern, target_host))
        .cloned()
        .collect();

    match order {
        Some(Order::First) => {
            for entry in &existing_entries {
                if !result_lines.is_empty() && !result_lines[0].is_empty() {
                    result_lines.insert(0, String::new());
                }
                let entry_lines: Vec<String> =
                    format_host_entry(&entry.host_pattern, &entry.options)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                for line in entry_lines.iter().rev() {
                    result_lines.insert(0, line.clone());
                }
            }

            for new_entry in new_entries {
                if !result_lines.is_empty() && !result_lines[0].is_empty() {
                    result_lines.insert(0, String::new());
                }
                let entry_lines: Vec<String> =
                    format_host_entry(&new_entry.host_pattern, &new_entry.options)
                        .lines()
                        .map(|s| s.to_string())
                        .collect();
                for line in entry_lines.iter().rev() {
                    result_lines.insert(0, line.clone());
                }
            }
        }
        Some(Order::Last) | None => {
            for entry in &existing_entries {
                if !result_lines.is_empty()
                    && !result_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    result_lines.push(String::new());
                }
                for line in format_host_entry(&entry.host_pattern, &entry.options).lines() {
                    result_lines.push(line.to_string());
                }
            }

            for new_entry in new_entries {
                if !result_lines.is_empty()
                    && !result_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    result_lines.push(String::new());
                }
                for line in format_host_entry(&new_entry.host_pattern, &new_entry.options).lines() {
                    result_lines.push(line.to_string());
                }
            }
        }
    }

    if !result_lines.is_empty() && !result_lines.last().map(|l| l.is_empty()).unwrap_or(true) {
        result_lines.push(String::new());
    }

    result_lines.join("\n")
}

pub fn ssh_config(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let ssh_config_path = get_ssh_config_path(&params);

    match state {
        State::Present => {
            let options = params.options.clone().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "options parameter is required when state=present",
                )
            })?;

            let options_map = options.to_map();

            if options_map.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "options must contain at least one option",
                ));
            }

            let original_content = if ssh_config_path.exists() {
                fs::read_to_string(&ssh_config_path)?
            } else {
                String::new()
            };

            let original_lines: Vec<&str> = original_content.lines().collect();
            let entries = parse_ssh_config(&original_content);

            let existing_entry = entries
                .iter()
                .find(|e| host_matches_pattern(&e.host_pattern, &params.host));

            let mut changed = false;
            let new_entries: Vec<HostEntry> = vec![HostEntry {
                host_pattern: params.host.clone(),
                options: options_map.clone(),
                line_start: 0,
                line_end: 0,
            }];

            if let Some(existing) = existing_entry {
                for (key, value) in &options_map {
                    let normalized_key = key.to_lowercase();
                    match existing.options.get(&normalized_key) {
                        Some(existing_value) if existing_value == value => {}
                        _ => changed = true,
                    }
                }
                for key in existing.options.keys() {
                    if !options_map.contains_key(key) {
                        changed = true;
                    }
                }
            } else {
                changed = true;
            }

            if changed {
                let new_content = rebuild_config(
                    &original_lines,
                    &entries,
                    &new_entries,
                    &params.host,
                    &params.order,
                );

                diff(&original_content, &new_content);

                if !check_mode {
                    if let Some(parent) = ssh_config_path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }

                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&ssh_config_path)?;
                    file.write_all(new_content.as_bytes())?;
                }
            }

            Ok(ModuleResult {
                changed,
                output: Some(ssh_config_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
        State::Absent => {
            let original_content = if ssh_config_path.exists() {
                fs::read_to_string(&ssh_config_path)?
            } else {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(ssh_config_path.to_string_lossy().to_string()),
                    extra: None,
                });
            };

            let original_lines: Vec<&str> = original_content.lines().collect();
            let entries = parse_ssh_config(&original_content);

            let existing_entry = entries
                .iter()
                .find(|e| host_matches_pattern(&e.host_pattern, &params.host));

            if existing_entry.is_none() {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(ssh_config_path.to_string_lossy().to_string()),
                    extra: None,
                });
            }

            let new_content =
                rebuild_config(&original_lines, &entries, &[], &params.host, &params.order);

            diff(&original_content, &new_content);

            if !check_mode {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&ssh_config_path)?;
                file.write_all(new_content.as_bytes())?;
            }

            Ok(ModuleResult {
                changed: true,
                output: Some(ssh_config_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct SshConfig;

impl Module for SshConfig {
    fn get_name(&self) -> &str {
        "ssh_config"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            ssh_config(parse_params(optional_params)?, check_mode)?,
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
            host: github.com
            options:
              hostname: github.com
              user: git
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "github.com");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_ssh_config() {
        let content = "Host github.com\n    hostname github.com\n    user git\n\nHost *.example.com\n    user deploy\n";
        let entries = parse_ssh_config(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].host_pattern, "github.com");
        assert_eq!(
            entries[0].options.get("hostname"),
            Some(&"github.com".to_string())
        );
        assert_eq!(entries[1].host_pattern, "*.example.com");
    }

    #[test]
    fn test_parse_ssh_config_with_comments() {
        let content = "# Global config\nHost github.com\n    hostname github.com\n# Comment inside\n    user git\n";
        let entries = parse_ssh_config(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].options.len(), 2);
    }

    #[test]
    fn test_host_matches_pattern_exact() {
        assert!(host_matches_pattern("github.com", "github.com"));
        assert!(!host_matches_pattern("github.com", "gitlab.com"));
    }

    #[test]
    fn test_host_matches_pattern_wildcard() {
        assert!(host_matches_pattern("*.example.com", "test.example.com"));
        assert!(host_matches_pattern("*.example.com", "sub.example.com"));
        assert!(!host_matches_pattern("*.example.com", "example.org"));
    }

    #[test]
    fn test_host_matches_pattern_multiple() {
        assert!(host_matches_pattern("github.com gitlab.com", "github.com"));
        assert!(host_matches_pattern("github.com gitlab.com", "gitlab.com"));
        assert!(!host_matches_pattern(
            "github.com gitlab.com",
            "bitbucket.com"
        ));
    }

    #[test]
    fn test_ssh_config_add_entry() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");

        let params = Params {
            host: "github.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("hostname".to_string(), "github.com".to_string());
                m.insert("user".to_string(), "git".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Host github.com"));
        assert!(content.contains("hostname github.com"));
        assert!(content.contains("user git"));
    }

    #[test]
    fn test_ssh_config_add_existing_no_change() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "Host github.com\n    hostname github.com\n    user git\n",
        )
        .unwrap();

        let params = Params {
            host: "github.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("hostname".to_string(), "github.com".to_string());
                m.insert("user".to_string(), "git".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_ssh_config_update_option() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "Host github.com\n    hostname github.com\n    user olduser\n",
        )
        .unwrap();

        let params = Params {
            host: "github.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("hostname".to_string(), "github.com".to_string());
                m.insert("user".to_string(), "git".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("user git"));
        assert!(!content.contains("olduser"));
    }

    #[test]
    fn test_ssh_config_remove_entry() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "Host github.com\n    hostname github.com\n    user git\n\nHost gitlab.com\n    hostname gitlab.com\n",
        )
        .unwrap();

        let params = Params {
            host: "github.com".to_string(),
            options: None,
            state: Some(State::Absent),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("Host github.com"));
        assert!(content.contains("Host gitlab.com"));
    }

    #[test]
    fn test_ssh_config_remove_not_found() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "Host github.com\n    hostname github.com\n").unwrap();

        let params = Params {
            host: "nonexistent.com".to_string(),
            options: None,
            state: Some(State::Absent),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_ssh_config_wildcard_pattern() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");

        let params = Params {
            host: "*.example.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("user".to_string(), "deploy".to_string());
                m.insert("port".to_string(), "2222".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Host *.example.com"));
        assert!(content.contains("port 2222"));
    }

    #[test]
    fn test_ssh_config_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");

        let params = Params {
            host: "github.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("hostname".to_string(), "github.com".to_string());
                m.insert("user".to_string(), "git".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: None,
        };

        let result = ssh_config(params, true).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_ssh_config_order_first() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join(".ssh/config");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "Host existing.com\n    hostname existing.com\n",
        )
        .unwrap();

        let params = Params {
            host: "github.com".to_string(),
            options: Some(OptionsInput::Map({
                let mut m = std::collections::HashMap::new();
                m.insert("hostname".to_string(), "github.com".to_string());
                m
            })),
            state: Some(State::Present),
            ssh_config_file: Some(config_path.to_string_lossy().to_string()),
            order: Some(Order::First),
        };

        let result = ssh_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.starts_with("Host github.com"));
    }

    #[test]
    fn test_matches_ssh_pattern() {
        assert!(matches_ssh_pattern("*.example.com", "test.example.com"));
        assert!(matches_ssh_pattern("*.example.com", "sub.example.com"));
        assert!(!matches_ssh_pattern("*.example.com", "example.org"));
        assert!(matches_ssh_pattern("host?", "host1"));
        assert!(matches_ssh_pattern("host?", "host2"));
        assert!(!matches_ssh_pattern("host?", "host10"));
        assert!(matches_ssh_pattern("*", "anything.com"));
    }

    #[test]
    fn test_options_input_json() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host: test.com
            options:
              hostname: test.com
              port: 22
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let options_map = params.options.unwrap().to_map();
        assert_eq!(options_map.get("hostname"), Some(&"test.com".to_string()));
        assert_eq!(options_map.get("port"), Some(&"22".to_string()));
    }
}
