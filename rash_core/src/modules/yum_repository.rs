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
/// ## Examples
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
/// - name: Add repository with multiple baseurls
///   yum_repository:
///     name: myrepo
///     description: My Custom Repository
///     baseurl:
///       - http://mirror1.example.com/repo/
///       - http://mirror2.example.com/repo/
///
/// - name: Remove old repository
///   yum_repository:
///     name: old-repo
///     state: absent
///
/// - name: Disable a repository
///   yum_repository:
///     name: epel
///     description: EPEL YUM repo
///     baseurl: https://download.fedoraproject.org/pub/epel/$releasever/$basearch/
///     enabled: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::BTreeMap;
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

const YUM_REPOS_DIR: &str = "/etc/yum.repos.d";

fn default_true() -> Option<bool> {
    Some(true)
}

fn default_file(name: &str) -> String {
    format!("{name}.repo")
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Repository name (section name in the .repo file).
    pub name: String,
    /// Base URL for the repository. Can be a single URL or a list of URLs.
    pub baseurl: Option<StringOrList>,
    /// A human-readable description of the repository.
    /// Maps to the `name` key in the repository file.
    pub description: Option<String>,
    /// Whether the repository is enabled.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub enabled: Option<bool>,
    /// Whether to check GPG signatures on packages.
    pub gpgcheck: Option<bool>,
    /// URL to the GPG key for the repository.
    pub gpgkey: Option<String>,
    /// Whether the repository should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Repository file name (without .repo extension).
    /// Defaults to the repository name.
    pub file: Option<String>,
    /// Repository mirror list URL.
    pub mirrorlist: Option<String>,
    /// Metalink URL for the repository.
    pub metalink: Option<String>,
    /// Repository priority (lower = higher priority).
    pub priority: Option<i32>,
    /// Cost of this repository relative to others.
    pub cost: Option<i32>,
    /// Exclude specific packages from this repository.
    pub exclude: Option<String>,
    /// Include only specific packages from this repository.
    pub includepkgs: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum StringOrList {
    Single(String),
    List(Vec<String>),
}

impl StringOrList {
    fn to_ini_value(&self) -> String {
        match self {
            StringOrList::Single(s) => s.clone(),
            StringOrList::List(v) => v.join("\n"),
        }
    }
}

#[derive(Debug, Clone)]
struct RepoEntry {
    section: String,
    key: String,
    value: String,
}

fn parse_repo_content(content: &str) -> (Vec<RepoEntry>, Vec<String>) {
    let mut entries: Vec<RepoEntry> = Vec::new();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut current_section: Option<String> = None;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed[1..trimmed.len() - 1].to_string());
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            if let Some(ref section) = current_section {
                entries.push(RepoEntry {
                    section: section.clone(),
                    key,
                    value,
                });
            }
        }
    }

    (entries, lines)
}

fn find_repo_entries<'a>(entries: &'a [RepoEntry], section: &str) -> Vec<&'a RepoEntry> {
    entries.iter().filter(|e| e.section == section).collect()
}

fn find_section_line(lines: &[String], section: &str) -> Option<usize> {
    let section_header = format!("[{section}]");
    lines.iter().position(|l| l.trim() == section_header)
}

fn format_key_value(key: &str, value: &str) -> String {
    format!("{key}={value}")
}

fn build_repo_content(params: &Params) -> BTreeMap<String, String> {
    let mut options: BTreeMap<String, String> = BTreeMap::new();

    if let Some(ref desc) = params.description {
        options.insert("name".to_string(), desc.clone());
    }

    if let Some(ref baseurl) = params.baseurl {
        options.insert("baseurl".to_string(), baseurl.to_ini_value());
    }

    if let Some(ref mirrorlist) = params.mirrorlist {
        options.insert("mirrorlist".to_string(), mirrorlist.clone());
    }

    if let Some(ref metalink) = params.metalink {
        options.insert("metalink".to_string(), metalink.clone());
    }

    if let Some(enabled) = params.enabled {
        options.insert(
            "enabled".to_string(),
            if enabled {
                "1".to_string()
            } else {
                "0".to_string()
            },
        );
    }

    if let Some(gpgcheck) = params.gpgcheck {
        options.insert(
            "gpgcheck".to_string(),
            if gpgcheck {
                "1".to_string()
            } else {
                "0".to_string()
            },
        );
    }

    if let Some(ref gpgkey) = params.gpgkey {
        options.insert("gpgkey".to_string(), gpgkey.clone());
    }

    if let Some(priority) = params.priority {
        options.insert("priority".to_string(), priority.to_string());
    }

    if let Some(cost) = params.cost {
        options.insert("cost".to_string(), cost.to_string());
    }

    if let Some(ref exclude) = params.exclude {
        options.insert("exclude".to_string(), exclude.clone());
    }

    if let Some(ref includepkgs) = params.includepkgs {
        options.insert("includepkgs".to_string(), includepkgs.clone());
    }

    options
}

fn entries_to_map(entries: &[&RepoEntry]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect()
}

fn compare_repo_options(
    existing: &BTreeMap<String, String>,
    desired: &BTreeMap<String, String>,
) -> bool {
    for (key, desired_value) in desired {
        match existing.get(key) {
            Some(existing_value) if existing_value == desired_value => continue,
            _ => return false,
        }
    }
    true
}

pub fn yum_repository(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let file_name = params
        .file
        .clone()
        .unwrap_or_else(|| default_file(&params.name));
    let repo_path = Path::new(YUM_REPOS_DIR).join(&file_name);

    let (entries, mut lines) = if repo_path.exists() {
        let content = read_to_string(&repo_path)?;
        parse_repo_content(&content)
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
            let desired_options = build_repo_content(&params);
            let existing_entries = find_repo_entries(&entries, &params.name);
            let existing_map = entries_to_map(&existing_entries);

            if existing_entries.is_empty() {
                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }
                lines.push(format!("[{}]", params.name));
                for (key, value) in &desired_options {
                    lines.push(format_key_value(key, value));
                }
                changed = true;
            } else if !compare_repo_options(&existing_map, &desired_options) {
                let section_line = find_section_line(&lines, &params.name).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Section header not found")
                })?;

                let mut section_end = lines.len();
                for (idx, line) in lines.iter().enumerate().skip(section_line + 1) {
                    let trimmed = line.trim();
                    if trimmed.starts_with('[') {
                        section_end = idx;
                        break;
                    }
                }

                let mut new_section_lines: Vec<String> = Vec::new();
                new_section_lines.push(lines[section_line].clone());

                for (key, value) in &desired_options {
                    new_section_lines.push(format_key_value(key, value));
                }

                lines.splice(section_line..section_end, new_section_lines);
                changed = true;
            }
        }
        State::Absent => {
            let existing_entries = find_repo_entries(&entries, &params.name);
            if !existing_entries.is_empty()
                && let Some(section_line) = find_section_line(&lines, &params.name)
            {
                let mut section_end = lines.len();
                for (idx, line) in lines.iter().enumerate().skip(section_line + 1) {
                    let trimmed = line.trim();
                    if trimmed.starts_with('[') {
                        section_end = idx;
                        break;
                    }
                }

                while section_end > section_line {
                    lines.remove(section_line);
                    section_end -= 1;
                }

                while section_line > 0 && section_line < lines.len() {
                    if lines[section_line - 1].trim().is_empty() {
                        lines.remove(section_line - 1);
                    } else {
                        break;
                    }
                }

                changed = true;
            }
        }
    }

    if changed {
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            let mut result = String::new();
            let mut prev_empty = false;
            for line in &lines {
                if line.is_empty() {
                    if !prev_empty {
                        result.push_str(line);
                        result.push('\n');
                        prev_empty = true;
                    }
                } else {
                    result.push_str(line);
                    result.push('\n');
                    prev_empty = false;
                }
            }
            result
        };

        diff(&original_content, &new_content);

        if !check_mode {
            if let Some(parent) = repo_path.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&repo_path)?;
            file.write_all(new_content.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(repo_path.to_string_lossy().to_string()),
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
        assert_eq!(params.description, Some("EPEL YUM repo".to_string()));
        assert_eq!(params.gpgcheck, Some(true));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_with_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: epel
            file: external-repos
            description: EPEL YUM repo
            baseurl: https://example.com/repo/
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "epel");
        assert_eq!(params.file, Some("external-repos".to_string()));
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
    fn test_parse_params_baseurl_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myrepo
            baseurl:
              - http://mirror1.example.com/repo/
              - http://mirror2.example.com/repo/
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        match params.baseurl {
            Some(StringOrList::List(urls)) => {
                assert_eq!(urls.len(), 2);
                assert_eq!(urls[0], "http://mirror1.example.com/repo/");
            }
            _ => panic!("Expected list of baseurls"),
        }
    }

    #[test]
    fn test_parse_repo_content() {
        let content = "[epel]\nname=EPEL\nbaseurl=https://example.com/\nenabled=1\n";
        let (entries, lines) = parse_repo_content(content);

        assert_eq!(lines.len(), 4);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].section, "epel");
        assert_eq!(entries[0].key, "name");
        assert_eq!(entries[0].value, "EPEL");
    }

    #[test]
    fn test_find_repo_entries() {
        let content = "[epel]\nname=EPEL\nbaseurl=https://example.com/\n\n[other]\nname=Other\n";
        let (entries, _) = parse_repo_content(content);

        let epel_entries = find_repo_entries(&entries, "epel");
        assert_eq!(epel_entries.len(), 2);

        let other_entries = find_repo_entries(&entries, "other");
        assert_eq!(other_entries.len(), 1);
    }

    #[test]
    fn test_build_repo_content() {
        let params = Params {
            name: "epel".to_string(),
            description: Some("EPEL repo".to_string()),
            baseurl: Some(StringOrList::Single("https://example.com/".to_string())),
            enabled: Some(false),
            gpgcheck: Some(true),
            gpgkey: Some("https://example.com/key".to_string()),
            state: None,
            file: None,
            mirrorlist: None,
            metalink: None,
            priority: None,
            cost: None,
            exclude: None,
            includepkgs: None,
        };

        let options = build_repo_content(&params);
        assert_eq!(options.get("name"), Some(&"EPEL repo".to_string()));
        assert_eq!(
            options.get("baseurl"),
            Some(&"https://example.com/".to_string())
        );
        assert_eq!(options.get("enabled"), Some(&"0".to_string()));
        assert_eq!(options.get("gpgcheck"), Some(&"1".to_string()));
        assert_eq!(
            options.get("gpgkey"),
            Some(&"https://example.com/key".to_string())
        );
    }

    #[test]
    fn test_format_key_value() {
        assert_eq!(format_key_value("name", "EPEL"), "name=EPEL");
        assert_eq!(
            format_key_value("baseurl", "https://example.com/"),
            "baseurl=https://example.com/"
        );
    }

    #[test]
    fn test_string_or_list_to_ini_value() {
        let single = StringOrList::Single("https://example.com/".to_string());
        assert_eq!(single.to_ini_value(), "https://example.com/");

        let list = StringOrList::List(vec![
            "http://mirror1.example.com/".to_string(),
            "http://mirror2.example.com/".to_string(),
        ]);
        assert_eq!(
            list.to_ini_value(),
            "http://mirror1.example.com/\nhttp://mirror2.example.com/"
        );
    }

    #[test]
    fn test_compare_repo_options() {
        let mut existing: BTreeMap<String, String> = BTreeMap::new();
        existing.insert("name".to_string(), "EPEL".to_string());
        existing.insert("enabled".to_string(), "1".to_string());

        let mut desired: BTreeMap<String, String> = BTreeMap::new();
        desired.insert("name".to_string(), "EPEL".to_string());
        desired.insert("enabled".to_string(), "1".to_string());

        assert!(compare_repo_options(&existing, &desired));

        desired.insert("enabled".to_string(), "0".to_string());
        assert!(!compare_repo_options(&existing, &desired));
    }

    #[test]
    fn test_default_file() {
        assert_eq!(default_file("epel"), "epel.repo");
        assert_eq!(default_file("my-repo"), "my-repo.repo");
    }
}
