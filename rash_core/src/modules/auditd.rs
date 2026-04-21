/// ANCHOR: module
/// # auditd
///
/// Manage Linux audit daemon rules.
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
/// - name: Add audit rules for identity files
///   auditd:
///     rules_file: /etc/audit/rules.d/audit.rules
///     rules:
///       - -w /etc/passwd -p wa -k identity
///       - -w /etc/group -p wa -k identity
///       - -w /etc/shadow -p wa -k identity
///     state: present
///
/// - name: Add syscall audit rule
///   auditd:
///     rules_file: /etc/audit/rules.d/audit.rules
///     rules:
///       - -a always,exit -F arch=b64 -S execve -k exec
///     state: present
///
/// - name: Remove specific audit rules
///   auditd:
///     rules_file: /etc/audit/rules.d/audit.rules
///     rules:
///       - -w /var/log -p wa -k logs
///     state: absent
///
/// - name: Add rule without reload
///   auditd:
///     rules_file: /etc/audit/rules.d/audit.rules
///     rules:
///       - -w /etc/ssh/sshd_config -p wa -k ssh
///     state: present
///     reload: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_RULES_FILE: &str = "/etc/audit/rules.d/audit.rules";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the audit rules file.
    /// **[default: `"/etc/audit/rules.d/audit.rules"`]**
    pub rules_file: Option<String>,
    /// List of audit rules to add or remove.
    pub rules: Vec<String>,
    /// Whether the rules should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Whether to reload auditd after changes.
    /// **[default: `true`]**
    pub reload: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn read_existing_rules(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|line| line.trim().to_string())
        .collect()
}

fn normalize_rule(rule: &str) -> String {
    rule.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn reload_auditd() -> Result<()> {
    let status = std::process::Command::new("augenrules")
        .arg("--load")
        .status()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute augenrules: {e}"),
            )
        })?;

    if !status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "augenrules --load failed".to_string(),
        ));
    }

    Ok(())
}

pub fn auditd(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let reload = params.reload.unwrap_or(true);
    let rules_file = params.rules_file.as_deref().unwrap_or(DEFAULT_RULES_FILE);
    let path = Path::new(rules_file);

    let mut existing_rules = read_existing_rules(path);
    let original_content = fs::read_to_string(path).unwrap_or_default();
    let mut changed = false;

    match state {
        State::Present => {
            for rule in &params.rules {
                let normalized = normalize_rule(rule);
                if !existing_rules
                    .iter()
                    .any(|r| normalize_rule(r) == normalized)
                {
                    existing_rules.push(normalized);
                    changed = true;
                }
            }
        }
        State::Absent => {
            let before_len = existing_rules.len();
            existing_rules.retain(|existing| {
                let normalized_existing = normalize_rule(existing);
                !params
                    .rules
                    .iter()
                    .any(|r| normalize_rule(r) == normalized_existing)
            });
            if existing_rules.len() != before_len {
                changed = true;
            }
        }
    }

    if !changed {
        return Ok(ModuleResult::new(false, None, None));
    }

    let mut new_content = String::new();

    let original_lines: Vec<&str> = original_content.lines().collect();
    let mut comment_lines = Vec::new();
    for line in &original_lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || (trimmed.is_empty() && !comment_lines.is_empty()) {
            comment_lines.push(*line);
        }
    }

    for comment in &comment_lines {
        new_content.push_str(comment);
        new_content.push('\n');
    }

    if !comment_lines.is_empty() && !existing_rules.is_empty() {
        new_content.push('\n');
    }

    for rule in &existing_rules {
        new_content.push_str(rule);
        new_content.push('\n');
    }

    diff(&original_content, &new_content);

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(new_content.as_bytes())?;

    if reload {
        reload_auditd()?;
    }

    Ok(ModuleResult::new(changed, None, None))
}

#[derive(Debug)]
pub struct Auditd;

impl Module for Auditd {
    fn get_name(&self) -> &str {
        "auditd"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((auditd(parse_params(optional_params)?, check_mode)?, None))
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
            rules_file: /etc/audit/rules.d/audit.rules
            rules:
              - -w /etc/passwd -p wa -k identity
              - -w /var/log -p wa -k logs
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.rules_file,
            Some("/etc/audit/rules.d/audit.rules".to_string())
        );
        assert_eq!(params.rules.len(), 2);
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rules:
              - -w /var/log -p wa -k logs
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
        assert_eq!(params.rules.len(), 1);
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rules:
              - -w /etc/passwd -p wa -k identity
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
        assert_eq!(params.reload, None);
        assert_eq!(params.rules_file, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            rules:
              - "-w /etc/passwd -p wa"
            invalid: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_normalize_rule() {
        assert_eq!(
            normalize_rule("  -w   /etc/passwd   -p wa   -k identity  "),
            "-w /etc/passwd -p wa -k identity"
        );
    }

    #[test]
    fn test_read_existing_rules_empty() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        let rules = read_existing_rules(file_path.as_path());
        assert!(rules.is_empty());
    }

    #[test]
    fn test_read_existing_rules_with_content() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(
            &file_path,
            "# Header comment\n-w /etc/passwd -p wa -k identity\n\n-w /var/log -p wa -k logs\n",
        )
        .unwrap();

        let rules = read_existing_rules(&file_path);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0], "-w /etc/passwd -p wa -k identity");
        assert_eq!(rules[1], "-w /var/log -p wa -k logs");
    }

    #[test]
    fn test_auditd_add_rules_to_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec![
                "-w /etc/passwd -p wa -k identity".to_string(),
                "-w /var/log -p wa -k logs".to_string(),
            ],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("-w /etc/passwd -p wa -k identity"));
        assert!(content.contains("-w /var/log -p wa -k logs"));
    }

    #[test]
    fn test_auditd_add_rules_to_existing_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(&file_path, "-w /etc/passwd -p wa -k identity\n").unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /var/log -p wa -k logs".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("-w /etc/passwd -p wa -k identity"));
        assert!(content.contains("-w /var/log -p wa -k logs"));
    }

    #[test]
    fn test_auditd_no_change_when_rule_exists() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(
            &file_path,
            "-w /etc/passwd -p wa -k identity\n-w /var/log -p wa -k logs\n",
        )
        .unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /var/log -p wa -k logs".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_auditd_remove_rules() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(
            &file_path,
            "-w /etc/passwd -p wa -k identity\n-w /var/log -p wa -k logs\n",
        )
        .unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /var/log -p wa -k logs".to_string()],
            state: Some(State::Absent),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("-w /etc/passwd -p wa -k identity"));
        assert!(!content.contains("-w /var/log -p wa -k logs"));
    }

    #[test]
    fn test_auditd_remove_nonexistent_rule() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(&file_path, "-w /etc/passwd -p wa -k identity\n").unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /etc/shadow -p wa -k identity".to_string()],
            state: Some(State::Absent),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_auditd_check_mode_add() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /etc/passwd -p wa -k identity".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, true).unwrap();
        assert!(result.get_changed());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_auditd_check_mode_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(&file_path, "-w /etc/passwd -p wa -k identity\n").unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /etc/passwd -p wa -k identity".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, true).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_auditd_preserves_comments() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(
            &file_path,
            "# This is a comment\n-w /etc/passwd -p wa -k identity\n",
        )
        .unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /var/log -p wa -k logs".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("# This is a comment"));
        assert!(content.contains("-w /etc/passwd -p wa -k identity"));
        assert!(content.contains("-w /var/log -p wa -k logs"));
    }

    #[test]
    fn test_auditd_normalizes_whitespace() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(&file_path, "-w   /etc/passwd   -p wa   -k identity\n").unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /etc/passwd -p wa -k identity".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_auditd_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("subdir").join("audit.rules");

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec!["-w /etc/passwd -p wa -k identity".to_string()],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(result.get_changed());
        assert!(file_path.exists());
    }

    #[test]
    fn test_auditd_add_duplicate_rule() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("audit.rules");
        fs::write(&file_path, "-w /etc/passwd -p wa -k identity\n").unwrap();

        let params = Params {
            rules_file: Some(file_path.to_str().unwrap().to_string()),
            rules: vec![
                "-w /etc/passwd -p wa -k identity".to_string(),
                "-w /etc/passwd -p wa -k identity".to_string(),
            ],
            state: Some(State::Present),
            reload: Some(false),
        };

        let result = auditd(params, false).unwrap();
        assert!(!result.get_changed());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content
                .lines()
                .filter(|l| l.trim() == "-w /etc/passwd -p wa -k identity")
                .count(),
            1
        );
    }
}
