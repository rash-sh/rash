/// ANCHOR: module
/// # ini_file
///
/// Manage settings in INI-style configuration files.
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
/// - ini_file:
///     path: /etc/app/config.ini
///     section: database
///     option: host
///     value: localhost
///     state: present
///
/// - ini_file:
///     path: /etc/app/config.ini
///     section: database
///     option: port
///     value: "5432"
///
/// - ini_file:
///     path: /etc/app/config.ini
///     section: database
///     option: deprecated_option
///     state: absent
///
/// - ini_file:
///     path: /etc/app/config.ini
///     section: cache
///     option: enabled
///     value: "true"
///     no_extra_spaces: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The absolute path to the INI file to modify.
    pub path: String,
    /// The section name to modify. If not specified, the option will be
    /// placed before the first section.
    pub section: Option<String>,
    /// The option (key) name to modify.
    pub option: String,
    /// The value to set for the option. Required if state=present.
    pub value: Option<String>,
    /// Whether the option should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Remove spaces around the = sign.
    /// **[default: `false`]**
    pub no_extra_spaces: Option<bool>,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone)]
struct IniEntry {
    section: Option<String>,
    option: String,
    value: String,
    line_number: usize,
}

fn parse_ini_content(content: &str) -> (Vec<IniEntry>, Vec<String>) {
    let mut entries: Vec<IniEntry> = Vec::new();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut current_section: Option<String> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed[1..trimmed.len() - 1].to_string());
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let option = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim().to_string();
            entries.push(IniEntry {
                section: current_section.clone(),
                option,
                value,
                line_number: idx,
            });
        }
    }

    (entries, lines)
}

fn find_option_entry<'a>(
    entries: &'a [IniEntry],
    section: &Option<String>,
    option: &str,
) -> Option<&'a IniEntry> {
    entries
        .iter()
        .find(|e| &e.section == section && e.option == option)
}

fn find_section_line(lines: &[String], section: &str) -> Option<usize> {
    let section_header = format!("[{section}]");
    lines.iter().position(|l| l.trim() == section_header)
}

fn format_option_value(option: &str, value: &str, no_extra_spaces: bool) -> String {
    if no_extra_spaces {
        format!("{option}={value}")
    } else {
        format!("{option} = {value}")
    }
}

pub fn ini_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let no_extra_spaces = params.no_extra_spaces.unwrap_or(false);

    if state == State::Present && params.value.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        ));
    }

    let path = Path::new(&params.path);

    let (entries, mut lines) = if path.exists() {
        let content = read_to_string(path)?;
        parse_ini_content(&content)
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
            let value = params.value.as_ref().unwrap();
            let existing = find_option_entry(&entries, &params.section, &params.option);

            if let Some(entry) = existing {
                if entry.value != *value {
                    let new_line = format_option_value(&params.option, value, no_extra_spaces);
                    lines[entry.line_number] = new_line;
                    changed = true;
                }
            } else {
                let new_line = format_option_value(&params.option, value, no_extra_spaces);

                match &params.section {
                    Some(section_name) => {
                        if let Some(section_idx) = find_section_line(&lines, section_name) {
                            let mut insert_idx = section_idx + 1;

                            while insert_idx < lines.len() {
                                let trimmed = lines[insert_idx].trim();
                                if trimmed.starts_with('[') {
                                    break;
                                }
                                if !trimmed.is_empty()
                                    && !trimmed.starts_with('#')
                                    && !trimmed.starts_with(';')
                                {
                                    insert_idx += 1;
                                    continue;
                                }
                                break;
                            }

                            while insert_idx < lines.len() {
                                let trimmed = lines[insert_idx].trim();
                                if trimmed.starts_with('[') {
                                    break;
                                }
                                insert_idx += 1;
                            }

                            lines.insert(insert_idx, new_line);
                        } else {
                            if !lines.is_empty()
                                && !lines.last().map(|l| l.is_empty()).unwrap_or(true)
                            {
                                lines.push(String::new());
                            }
                            lines.push(format!("[{section_name}]"));
                            lines.push(new_line);
                        }
                    }
                    None => {
                        let insert_idx = lines
                            .iter()
                            .position(|l| {
                                let trimmed = l.trim();
                                trimmed.starts_with('[')
                            })
                            .unwrap_or(lines.len());

                        if insert_idx > 0 && !lines[insert_idx - 1].is_empty() {
                            lines.insert(insert_idx, String::new());
                            lines.insert(insert_idx + 1, new_line);
                        } else {
                            lines.insert(insert_idx, new_line);
                        }
                    }
                }
                changed = true;
            }
        }
        State::Absent => {
            if let Some(entry) = find_option_entry(&entries, &params.section, &params.option) {
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

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.path),
        extra: None,
    })
}

#[derive(Debug)]
pub struct IniFile;

impl Module for IniFile {
    fn get_name(&self) -> &str {
        "ini_file"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((ini_file(parse_params(optional_params)?, check_mode)?, None))
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
            path: "/etc/config.ini"
            section: "database"
            option: "host"
            value: "localhost"
            state: "present"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/etc/config.ini".to_owned(),
                section: Some("database".to_owned()),
                option: "host".to_owned(),
                value: Some("localhost".to_owned()),
                state: Some(State::Present),
                no_extra_spaces: None,
            }
        );
    }

    #[test]
    fn test_ini_file_add_option() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = oldhost\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "port".to_string(),
            value: Some("5432".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("port = 5432"));
    }

    #[test]
    fn test_ini_file_modify_option() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = oldhost\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "host".to_string(),
            value: Some("localhost".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("host = localhost"));
        assert!(!content.contains("oldhost"));
    }

    #[test]
    fn test_ini_file_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = localhost\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "host".to_string(),
            value: Some("localhost".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_ini_file_remove_option() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = localhost\nport = 5432\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "port".to_string(),
            value: None,
            state: Some(State::Absent),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("host = localhost"));
        assert!(!content.contains("port"));
    }

    #[test]
    fn test_ini_file_add_new_section() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = localhost\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("cache".to_string()),
            option: "enabled".to_string(),
            value: Some("true".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("[cache]"));
        assert!(content.contains("enabled = true"));
    }

    #[test]
    fn test_ini_file_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "host".to_string(),
            value: Some("localhost".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("[database]"));
        assert!(content.contains("host = localhost"));
    }

    #[test]
    fn test_ini_file_no_extra_spaces() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost=localhost\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "port".to_string(),
            value: Some("5432".to_string()),
            state: Some(State::Present),
            no_extra_spaces: Some(true),
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("port=5432"));
    }

    #[test]
    fn test_ini_file_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "[database]\nhost = oldhost\n").unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: Some("database".to_string()),
            option: "host".to_string(),
            value: Some("localhost".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_ini_file_missing_value_for_present() {
        let params = Params {
            path: "/tmp/test.ini".to_string(),
            section: Some("database".to_string()),
            option: "host".to_string(),
            value: None,
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("value parameter is required")
        );
    }

    #[test]
    fn test_ini_file_no_section() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.ini");

        fs::write(&file_path, "global = value\n[section]\nkey = val\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            section: None,
            option: "global".to_string(),
            value: Some("newvalue".to_string()),
            state: Some(State::Present),
            no_extra_spaces: None,
        };

        let result = ini_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("global = newvalue"));
    }

    #[test]
    fn test_parse_ini_content() {
        let content = "[database]\nhost = localhost\nport = 5432\n\n[cache]\nenabled = true\n";
        let (entries, lines) = parse_ini_content(content);

        assert_eq!(lines.len(), 6);
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].section, Some("database".to_string()));
        assert_eq!(entries[0].option, "host");
        assert_eq!(entries[0].value, "localhost");

        assert_eq!(entries[1].section, Some("database".to_string()));
        assert_eq!(entries[1].option, "port");
        assert_eq!(entries[1].value, "5432");

        assert_eq!(entries[2].section, Some("cache".to_string()));
        assert_eq!(entries[2].option, "enabled");
        assert_eq!(entries[2].value, "true");
    }

    #[test]
    fn test_format_option_value() {
        assert_eq!(format_option_value("key", "value", false), "key = value");
        assert_eq!(format_option_value("key", "value", true), "key=value");
    }
}
