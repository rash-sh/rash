/// ANCHOR: module
/// # pam_limits
///
/// Manage Linux PAM limits (ulimits).
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
/// - name: Set max open files limit for nginx user
///   pam_limits:
///     domain: nginx
///     limit_type: soft
///     item: nofile
///     value: "65535"
///
/// - name: Set hard limit for max processes
///   pam_limits:
///     domain: '*'
///     limit_type: hard
///     item: nproc
///     value: "4096"
///
/// - name: Remove memlock limit for user
///   pam_limits:
///     domain: myuser
///     limit_type: soft
///     item: memlock
///     value: unlimited
///
/// - name: Set limits in a custom file with comment
///   pam_limits:
///     domain: "@developers"
///     limit_type: "-"
///     item: nofile
///     value: "100000"
///     dest: /etc/security/limits.d/99-developers.conf
///     comment: Custom limits for developers
///
/// - name: Ensure limit does not exist
///   pam_limits:
///     domain: olduser
///     limit_type: soft
///     item: nofile
///     state: absent
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
use std::time::{SystemTime, UNIX_EPOCH};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_DEST: &str = "/etc/security/limits.conf";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// A username, @groupname, wildcard *, or UID/GID range.
    pub domain: String,
    /// Limit type: hard, soft, or - (both).
    pub limit_type: LimitType,
    /// The limit item to set.
    pub item: LimitItem,
    /// The value of the limit. Required when state=present.
    pub value: Option<String>,
    /// Whether the entry should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the limits.conf file.
    /// **[default: `"/etc/security/limits.conf"`]**
    pub dest: Option<String>,
    /// Comment associated with the limit.
    pub comment: Option<String>,
    /// Create a backup file before modifying.
    /// **[default: `false`]**
    pub backup: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
pub enum LimitType {
    #[serde(rename = "hard")]
    Hard,
    #[serde(rename = "soft")]
    Soft,
    #[serde(rename = "-")]
    Both,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum LimitItem {
    Core,
    Data,
    Fsize,
    Memlock,
    Nofile,
    Rss,
    Stack,
    Cpu,
    Nproc,
    #[serde(rename = "as")]
    As,
    Maxlogins,
    Maxsyslogins,
    Priority,
    Locks,
    Sigpending,
    Msgqueue,
    Nice,
    Rtprio,
    Chroot,
}

impl LimitItem {
    fn as_str(&self) -> &'static str {
        match self {
            LimitItem::Core => "core",
            LimitItem::Data => "data",
            LimitItem::Fsize => "fsize",
            LimitItem::Memlock => "memlock",
            LimitItem::Nofile => "nofile",
            LimitItem::Rss => "rss",
            LimitItem::Stack => "stack",
            LimitItem::Cpu => "cpu",
            LimitItem::Nproc => "nproc",
            LimitItem::As => "as",
            LimitItem::Maxlogins => "maxlogins",
            LimitItem::Maxsyslogins => "maxsyslogins",
            LimitItem::Priority => "priority",
            LimitItem::Locks => "locks",
            LimitItem::Sigpending => "sigpending",
            LimitItem::Msgqueue => "msgqueue",
            LimitItem::Nice => "nice",
            LimitItem::Rtprio => "rtprio",
            LimitItem::Chroot => "chroot",
        }
    }
}

impl LimitType {
    fn as_str(&self) -> &'static str {
        match self {
            LimitType::Hard => "hard",
            LimitType::Soft => "soft",
            LimitType::Both => "-",
        }
    }
}

#[derive(Debug, Clone)]
struct LimitsEntry {
    domain: String,
    limit_type: String,
    item: String,
    value: String,
    line_number: usize,
}

fn parse_limits_content(content: &str) -> (Vec<LimitsEntry>, Vec<String>) {
    let mut entries: Vec<LimitsEntry> = Vec::new();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4 {
            entries.push(LimitsEntry {
                domain: parts[0].to_string(),
                limit_type: parts[1].to_string(),
                item: parts[2].to_string(),
                value: parts[3].to_string(),
                line_number: idx,
            });
        }
    }

    (entries, lines)
}

fn find_entry<'a>(
    entries: &'a [LimitsEntry],
    domain: &str,
    limit_type: &str,
    item: &str,
) -> Option<&'a LimitsEntry> {
    entries
        .iter()
        .find(|e| e.domain == domain && e.limit_type == limit_type && e.item == item)
}

fn normalize_value(value: &str) -> String {
    let lower = value.to_lowercase();
    if lower == "unlimited" || lower == "infinity" {
        "unlimited".to_string()
    } else {
        value.to_string()
    }
}

fn create_backup(path: &Path) -> Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = format!("{}.{}.bak", path.display(), timestamp);
    fs::copy(path, &backup_path)?;
    Ok(())
}

pub fn pam_limits(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let dest = params.dest.as_deref().unwrap_or(DEFAULT_DEST);
    let backup = params.backup.unwrap_or(false);

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let path = Path::new(dest);
    let limit_type_str = params.limit_type.as_str();
    let item_str = params.item.as_str();

    let (entries, mut lines) = if path.exists() {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let content: String = reader
            .lines()
            .map(|l| l.unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n");
        parse_limits_content(&content)
    } else {
        (Vec::new(), Vec::new())
    };

    let original_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let mut changed = false;

    match state {
        State::Present => {
            let value = normalize_value(params.value.as_ref().unwrap());
            let existing = find_entry(&entries, &params.domain, limit_type_str, item_str);

            if let Some(entry) = existing {
                if entry.value != value {
                    let comment_suffix = params
                        .comment
                        .as_ref()
                        .map(|c| format!("  # {}", c))
                        .unwrap_or_default();
                    lines[entry.line_number] = format!(
                        "{}\t{}\t{}\t{}{}",
                        params.domain, limit_type_str, item_str, value, comment_suffix
                    );
                    changed = true;
                }
            } else {
                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                let comment_suffix = params
                    .comment
                    .as_ref()
                    .map(|c| format!("  # {}", c))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}\t{}\t{}\t{}{}",
                    params.domain, limit_type_str, item_str, value, comment_suffix
                ));
                changed = true;
            }
        }
        State::Absent => {
            if let Some(entry) = find_entry(&entries, &params.domain, limit_type_str, item_str) {
                lines.remove(entry.line_number);
                changed = true;
            }
        }
    }

    if changed {
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            let trimmed: Vec<String> = lines.into_iter().collect();

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
            if backup && path.exists() {
                create_backup(path)?;
            }

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
        }
    }

    Ok(ModuleResult::new(changed, None, Some(dest.to_string())))
}

#[derive(Debug)]
pub struct PamLimits;

impl Module for PamLimits {
    fn get_name(&self) -> &str {
        "pam_limits"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            pam_limits(parse_params(optional_params)?, check_mode)?,
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
            domain: nginx
            limit_type: soft
            item: nofile
            value: "65535"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.domain, "nginx");
        assert_eq!(params.limit_type, LimitType::Soft);
        assert_eq!(params.item, LimitItem::Nofile);
        assert_eq!(params.value, Some("65535".to_owned()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domain: "*"
            limit_type: hard
            item: nproc
            value: "4096"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.domain, "*");
        assert_eq!(params.limit_type, LimitType::Hard);
        assert_eq!(params.item, LimitItem::Nproc);
        assert_eq!(params.value, Some("4096".to_owned()));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_group() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domain: "@developers"
            limit_type: "-"
            item: memlock
            value: unlimited
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.domain, "@developers");
        assert_eq!(params.limit_type, LimitType::Both);
        assert_eq!(params.item, LimitItem::Memlock);
        assert_eq!(params.value, Some("unlimited".to_owned()));
    }

    #[test]
    fn test_parse_limits_content() {
        let content = "# PAM limits\nnginx soft nofile 65535\n* hard nproc 4096\n";
        let (entries, lines) = parse_limits_content(content);

        assert_eq!(lines.len(), 3);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].domain, "nginx");
        assert_eq!(entries[0].limit_type, "soft");
        assert_eq!(entries[0].item, "nofile");
        assert_eq!(entries[0].value, "65535");

        assert_eq!(entries[1].domain, "*");
        assert_eq!(entries[1].limit_type, "hard");
        assert_eq!(entries[1].item, "nproc");
        assert_eq!(entries[1].value, "4096");
    }

    #[test]
    fn test_find_entry() {
        let content = "nginx soft nofile 65535\n* hard nproc 4096\n";
        let (entries, _) = parse_limits_content(content);

        let found = find_entry(&entries, "nginx", "soft", "nofile");
        assert!(found.is_some());
        assert_eq!(found.unwrap().value, "65535");

        let not_found = find_entry(&entries, "nginx", "hard", "nofile");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_normalize_value() {
        assert_eq!(normalize_value("unlimited"), "unlimited");
        assert_eq!(normalize_value("Unlimited"), "unlimited");
        assert_eq!(normalize_value("UNLIMITED"), "unlimited");
        assert_eq!(normalize_value("infinity"), "unlimited");
        assert_eq!(normalize_value("INFINITY"), "unlimited");
        assert_eq!(normalize_value("65535"), "65535");
        assert_eq!(normalize_value("-1"), "-1");
    }

    #[test]
    fn test_pam_limits_add_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "* soft nofile 1024\n").unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("nginx\tsoft\tnofile\t65535"));
        assert!(content.contains("* soft nofile 1024"));
    }

    #[test]
    fn test_pam_limits_modify_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "nginx soft nofile 1024\n").unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("nginx\tsoft\tnofile\t65535"));
        assert!(!content.contains("1024"));
    }

    #[test]
    fn test_pam_limits_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "nginx soft nofile 65535\n").unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_pam_limits_remove_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "nginx soft nofile 65535\n* hard nproc 4096\n").unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: None,
            state: Some(State::Absent),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("nginx"));
        assert!(content.contains("* hard nproc 4096"));
    }

    #[test]
    fn test_pam_limits_remove_nonexistent_entry() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "* hard nproc 4096\n").unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: None,
            state: Some(State::Absent),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_pam_limits_missing_value_for_present() {
        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: None,
            state: Some(State::Present),
            dest: None,
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_pam_limits_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("nginx\tsoft\tnofile\t65535"));
    }

    #[test]
    fn test_pam_limits_with_comment() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: Some("High file descriptor limit".to_string()),
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("# High file descriptor limit"));
    }

    #[test]
    fn test_pam_limits_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        fs::write(&file_path, "nginx soft nofile 1024\n").unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Nofile,
            value: Some("65535".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_pam_limits_unlimited_value() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        let params = Params {
            domain: "nginx".to_string(),
            limit_type: LimitType::Soft,
            item: LimitItem::Memlock,
            value: Some("unlimited".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("nginx\tsoft\tmemlock\tunlimited"));
    }

    #[test]
    fn test_pam_limits_wildcard_domain() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        let params = Params {
            domain: "*".to_string(),
            limit_type: LimitType::Hard,
            item: LimitItem::Nproc,
            value: Some("4096".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("*\thard\tnproc\t4096"));
    }

    #[test]
    fn test_pam_limits_group_domain() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("limits.conf");

        let params = Params {
            domain: "@developers".to_string(),
            limit_type: LimitType::Both,
            item: LimitItem::Nofile,
            value: Some("100000".to_string()),
            state: Some(State::Present),
            dest: Some(file_path.to_str().unwrap().to_string()),
            comment: None,
            backup: None,
        };

        let result = pam_limits(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("@developers\t-\tnofile\t100000"));
    }
}
