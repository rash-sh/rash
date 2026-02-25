/// ANCHOR: module
/// # yum_repository
///
/// Manage YUM/DNF repositories on RHEL/Fedora systems.
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
/// - name: Add EPEL repository
///   yum_repository:
///     name: epel
///     description: EPEL YUM repo
///     baseurl: https://download.fedoraproject.org/pub/epel/$releasever/$basearch/
///     gpgcheck: true
///     gpgkey: https://download.fedoraproject.org/pub/epel/RPM-GPG-KEY-EPEL-$releasever
///
/// - name: Remove old repository
///   yum_repository:
///     name: old-repo
///     state: absent
///
/// - name: Add repository with custom file
///   yum_repository:
///     name: myrepo
///     file: my-repos
///     description: My custom repo
///     baseurl: https://myrepo.example.com/
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::Result;
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{OpenOptions, read_to_string};
use std::io::prelude::*;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const REPO_DIR: &str = "/etc/yum.repos.d";

fn default_enabled() -> Option<bool> {
    Some(true)
}

#[derive(Debug, PartialEq, Default, Clone, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Repository ID. This is the section name in the .repo file.
    pub name: String,
    /// URL to the directory where the yum repository's 'repodata' directory lives.
    pub baseurl: Option<String>,
    /// A human readable string describing the repository.
    pub description: Option<String>,
    /// Whether the repository is enabled.
    /// **[default: `true`]**
    #[serde(default = "default_enabled")]
    pub enabled: Option<bool>,
    /// Whether to check GPG signatures on packages.
    pub gpgcheck: Option<bool>,
    /// URL pointing to the GPG key for the repository.
    pub gpgkey: Option<String>,
    /// Whether the repository should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// File name (without .repo extension) to use for the repository.
    /// Defaults to the value of `name`.
    pub file: Option<String>,
}

#[derive(Debug, Clone)]
struct RepoEntry {
    option: String,
    value: String,
    line_number: usize,
}

fn parse_repo_content(content: &str, section: &str) -> (Vec<RepoEntry>, Vec<String>, bool) {
    let mut entries: Vec<RepoEntry> = Vec::new();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut in_section = false;
    let mut section_exists = false;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current_section = &trimmed[1..trimmed.len() - 1];
            in_section = current_section == section;
            if in_section {
                section_exists = true;
            }
            continue;
        }

        if in_section {
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            if trimmed.starts_with('[') {
                in_section = false;
                continue;
            }

            if let Some(eq_pos) = trimmed.find('=') {
                let option = trimmed[..eq_pos].trim().to_string();
                let value = trimmed[eq_pos + 1..].trim().to_string();
                entries.push(RepoEntry {
                    option,
                    value,
                    line_number: idx,
                });
            }
        }
    }

    (entries, lines, section_exists)
}

fn find_option_entry<'a>(entries: &'a [RepoEntry], option: &str) -> Option<&'a RepoEntry> {
    entries.iter().find(|e| e.option == option)
}

fn get_repo_path(params: &Params) -> String {
    let filename = params.file.as_ref().unwrap_or(&params.name);
    format!("{}/{}.repo", REPO_DIR, filename)
}

fn build_repo_content(params: &Params) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("[{}]", params.name));

    if let Some(ref desc) = params.description {
        lines.push(format!("name={}", desc));
    }

    if let Some(ref baseurl) = params.baseurl {
        lines.push(format!("baseurl={}", baseurl));
    }

    if let Some(enabled) = params.enabled {
        lines.push(format!("enabled={}", if enabled { 1 } else { 0 }));
    }

    if let Some(gpgcheck) = params.gpgcheck {
        lines.push(format!("gpgcheck={}", if gpgcheck { 1 } else { 0 }));
    }

    if let Some(ref gpgkey) = params.gpgkey {
        lines.push(format!("gpgkey={}", gpgkey));
    }

    lines.join("\n") + "\n"
}

fn remove_section(lines: &[String], section: &str) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut in_section = false;
    let mut skip_next_empty = false;

    for line in lines.iter() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let current_section = &trimmed[1..trimmed.len() - 1];
            in_section = current_section == section;
            if !in_section {
                result.push(line.clone());
            }
            continue;
        }

        if in_section {
            skip_next_empty = true;
            continue;
        }

        if trimmed.is_empty() && skip_next_empty {
            skip_next_empty = false;
            continue;
        }

        result.push(line.clone());
    }

    if result.last().map(|l| l.is_empty()).unwrap_or(false) {
        result.pop();
    }

    result
}

pub fn yum_repository(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let repo_path = get_repo_path(&params);
    let path = Path::new(repo_path.as_str());
    let section = &params.name;
    let state = params.state.clone().unwrap_or_default();

    let (entries, mut lines, section_exists) = if path.exists() {
        let content = read_to_string(path)?;
        parse_repo_content(&content, section)
    } else {
        (Vec::new(), Vec::new(), false)
    };

    let original_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let mut changed = false;

    match state {
        State::Present => {
            if !section_exists {
                let new_section = build_repo_content(&params);
                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                for line in new_section.lines() {
                    lines.push(line.to_string());
                }
                changed = true;
            } else {
                let enabled_str = params
                    .enabled
                    .map(|b| (if b { "1" } else { "0" }).to_string());
                let gpgcheck_str = params
                    .gpgcheck
                    .map(|b| (if b { "1" } else { "0" }).to_string());
                let options: [(&str, Option<&String>); 5] = [
                    ("name", params.description.as_ref()),
                    ("baseurl", params.baseurl.as_ref()),
                    ("enabled", enabled_str.as_ref()),
                    ("gpgcheck", gpgcheck_str.as_ref()),
                    ("gpgkey", params.gpgkey.as_ref()),
                ];

                for (option, value) in options.iter() {
                    if let Some(v) = value {
                        if let Some(entry) = find_option_entry(&entries, option) {
                            if entry.value != **v {
                                lines[entry.line_number] = format!("{}={}", option, v);
                                changed = true;
                            }
                        } else {
                            let section_start = lines.iter().position(|l| {
                                let trimmed = l.trim();
                                trimmed == format!("[{}]", section)
                            });

                            if let Some(start_idx) = section_start {
                                let mut insert_idx = start_idx + 1;
                                while insert_idx < lines.len() {
                                    let trimmed = lines[insert_idx].trim();
                                    if trimmed.starts_with('[') || trimmed.is_empty() {
                                        break;
                                    }
                                    insert_idx += 1;
                                }
                                lines.insert(insert_idx, format!("{}={}", option, v));
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        State::Absent => {
            if section_exists {
                lines = remove_section(&lines, section);
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
                if line.is_empty() {
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
                std::fs::create_dir_all(parent)?;
            }

            if new_content.is_empty() {
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(path)?;
                file.write_all(new_content.as_bytes())?;
            }
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(get_repo_path(&params)),
        extra: None,
    })
}

#[derive(Debug)]
pub struct YumRepository;

impl Module for YumRepository {
    fn get_name(&self) -> &str {
        "yum_repository"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            yum_repository(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
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
            name: epel
            description: EPEL YUM repo
            baseurl: https://download.fedoraproject.org/pub/epel/$releasever/$basearch/
            gpgcheck: true
            gpgkey: https://download.fedoraproject.org/pub/epel/RPM-GPG-KEY-EPEL-$releasever
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "epel");
        assert_eq!(params.description, Some("EPEL YUM repo".to_owned()));
        assert_eq!(params.gpgcheck, Some(true));
        assert_eq!(params.enabled, Some(true));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-repo
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old-repo");
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_get_repo_path_default() {
        let params = Params {
            name: "epel".to_string(),
            file: None,
            ..Default::default()
        };
        assert_eq!(get_repo_path(&params), "/etc/yum.repos.d/epel.repo");
    }

    #[test]
    fn test_get_repo_path_custom_file() {
        let params = Params {
            name: "myrepo".to_string(),
            file: Some("my-repos".to_string()),
            ..Default::default()
        };
        assert_eq!(get_repo_path(&params), "/etc/yum.repos.d/my-repos.repo");
    }

    #[test]
    fn test_build_repo_content() {
        let params = Params {
            name: "epel".to_string(),
            description: Some("EPEL YUM repo".to_string()),
            baseurl: Some("https://example.com/epel".to_string()),
            enabled: Some(true),
            gpgcheck: Some(true),
            gpgkey: Some("https://example.com/key".to_string()),
            state: None,
            file: None,
        };
        let content = build_repo_content(&params);
        assert!(content.contains("[epel]"));
        assert!(content.contains("name=EPEL YUM repo"));
        assert!(content.contains("baseurl=https://example.com/epel"));
        assert!(content.contains("enabled=1"));
        assert!(content.contains("gpgcheck=1"));
        assert!(content.contains("gpgkey=https://example.com/key"));
    }

    #[test]
    fn test_parse_repo_content() {
        let content =
            "[epel]\nname=EPEL\nbaseurl=https://example.com\nenabled=1\n\n[other]\nname=Other\n";
        let (entries, lines, exists) = parse_repo_content(content, "epel");

        assert!(exists);
        assert_eq!(lines.len(), 7);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].option, "name");
        assert_eq!(entries[0].value, "EPEL");
    }

    #[test]
    fn test_parse_repo_content_not_found() {
        let content = "[other]\nname=Other\n";
        let (entries, _, exists) = parse_repo_content(content, "epel");

        assert!(!exists);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_remove_section() {
        let lines: Vec<String> = vec![
            "[epel]".to_string(),
            "name=EPEL".to_string(),
            "baseurl=https://example.com".to_string(),
            "".to_string(),
            "[other]".to_string(),
            "name=Other".to_string(),
        ];
        let result = remove_section(&lines, "epel");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "[other]");
        assert_eq!(result[1], "name=Other");
    }

    #[test]
    fn test_build_repo_content_minimal() {
        let params = Params {
            name: "minimal".to_string(),
            description: None,
            baseurl: Some("https://example.com".to_string()),
            enabled: Some(false),
            gpgcheck: None,
            gpgkey: None,
            state: None,
            file: None,
        };
        let content = build_repo_content(&params);
        assert!(content.contains("[minimal]"));
        assert!(content.contains("baseurl=https://example.com"));
        assert!(content.contains("enabled=0"));
        assert!(!content.contains("gpgcheck"));
        assert!(!content.contains("gpgkey"));
    }

    #[test]
    fn test_find_option_entry() {
        let entries = vec![
            RepoEntry {
                option: "name".to_string(),
                value: "EPEL".to_string(),
                line_number: 1,
            },
            RepoEntry {
                option: "baseurl".to_string(),
                value: "https://example.com".to_string(),
                line_number: 2,
            },
        ];
        assert!(find_option_entry(&entries, "name").is_some());
        assert!(find_option_entry(&entries, "baseurl").is_some());
        assert!(find_option_entry(&entries, "gpgcheck").is_none());
    }

    #[test]
    fn test_parse_params_with_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myrepo
            file: my-custom-file
            description: My custom repo
            baseurl: https://myrepo.example.com/
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myrepo");
        assert_eq!(params.file, Some("my-custom-file".to_owned()));
        assert_eq!(params.description, Some("My custom repo".to_owned()));
    }
}
