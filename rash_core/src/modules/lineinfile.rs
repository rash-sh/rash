/// ANCHOR: module
/// # lineinfile
///
/// Ensure a particular line is in a file, or replace an existing line using a back-referenced regular expression.
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
/// - lineinfile:
///     path: /etc/sudoers
///     line: '%wheel ALL=(ALL) NOPASSWD: ALL'
///     state: present
///
/// - lineinfile:
///     path: /etc/hosts
///     regexp: '^127\.0\.0\.1'
///     line: '127.0.0.1 localhost'
///     state: present
///
/// - lineinfile:
///     path: /tmp/testfile
///     regexp: '^#?banana'
///     state: absent
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
use regex::Regex;
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
    /// The absolute path to the file to modify.
    pub path: String,
    /// The regular expression to look for in every line of the file.
    /// If the regular expression is not matched, the line will be added to the file.
    /// Uses Python regular expressions.
    pub regexp: Option<String>,
    /// The line to insert/replace into the file.
    /// Required unless `state=absent`.
    pub line: Option<String>,
    /// Whether the line should be there or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
}

#[derive(Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

pub fn lineinfile(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();

    // Validate parameters based on state
    match state {
        State::Present => {
            if params.line.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "line parameter is required when state=present",
                ));
            }
        }
        State::Absent => {
            if params.regexp.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "regexp parameter is required when state=absent",
                ));
            }
        }
    }

    let path = Path::new(&params.path);

    // Read existing file content or create empty if it doesn't exist
    let original_content = if path.exists() {
        read_to_string(path)?
    } else {
        if state == State::Absent {
            // File doesn't exist and we want to remove lines - nothing to do
            return Ok(ModuleResult {
                changed: false,
                output: Some(params.path),
                extra: None,
            });
        }
        String::new()
    };

    let mut lines: Vec<String> = original_content.lines().map(|s| s.to_string()).collect();
    let mut changed = false;

    match state {
        State::Present => {
            let line_to_add = params.line.as_ref().unwrap();

            if let Some(regexp_str) = &params.regexp {
                // Try to find and replace existing line matching regexp
                let regex = Regex::new(regexp_str).map_err(|e| {
                    Error::new(ErrorKind::InvalidData, format!("Invalid regexp: {e}"))
                })?;

                let mut found_match = false;
                for existing_line in &mut lines {
                    if regex.is_match(existing_line) {
                        if existing_line != line_to_add {
                            trace!("replacing line: {existing_line} -> {line_to_add}");
                            *existing_line = line_to_add.clone();
                            changed = true;
                        }
                        found_match = true;
                        break;
                    }
                }

                if !found_match {
                    // No matching line found, add the new line
                    trace!("adding line: {line_to_add}");
                    lines.push(line_to_add.clone());
                    changed = true;
                }
            } else {
                // No regexp provided, check if line already exists
                if !lines.contains(line_to_add) {
                    trace!("adding line: {line_to_add}");
                    lines.push(line_to_add.clone());
                    changed = true;
                }
            }
        }
        State::Absent => {
            let regexp_str = params.regexp.as_ref().unwrap();
            let regex = Regex::new(regexp_str)
                .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Invalid regexp: {e}")))?;

            let original_len = lines.len();
            lines.retain(|line| !regex.is_match(line));

            if lines.len() != original_len {
                trace!(
                    "removed {} line(s) matching regexp: {}",
                    original_len - lines.len(),
                    regexp_str
                );
                changed = true;
            }
        }
    }

    if changed {
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };

        // Show diff
        diff(&original_content, &new_content);

        if !check_mode {
            // Create parent directories if they don't exist
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)?;
            }

            // Write the new content
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
pub struct Lineinfile;

impl Module for Lineinfile {
    fn get_name(&self) -> &str {
        "lineinfile"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            lineinfile(parse_params(optional_params)?, check_mode)?,
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
            path: "/tmp/test.txt"
            line: "test line"
            regexp: "^test"
            state: "present"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                path: "/tmp/test.txt".to_owned(),
                line: Some("test line".to_owned()),
                regexp: Some("^test".to_owned()),
                state: Some(State::Present),
            }
        );
    }

    #[test]
    fn test_lineinfile_add_new_line() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file content
        fs::write(&file_path, "line1\nline2\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: Some("line3".to_string()),
            regexp: None,
            state: Some(State::Present),
        };

        let result = lineinfile(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("line3"));
    }

    #[test]
    fn test_lineinfile_replace_existing_line() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file content
        fs::write(&file_path, "line1\nold line\nline3\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: Some("new line".to_string()),
            regexp: Some("^old".to_string()),
            state: Some(State::Present),
        };

        let result = lineinfile(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("new line"));
        assert!(!content.contains("old line"));
    }

    #[test]
    fn test_lineinfile_remove_line() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file content
        fs::write(&file_path, "line1\nremove this\nline3\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: None,
            regexp: Some("remove".to_string()),
            state: Some(State::Absent),
        };

        let result = lineinfile(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("remove this"));
        assert!(content.contains("line1"));
        assert!(content.contains("line3"));
    }

    #[test]
    fn test_lineinfile_no_change_when_line_exists() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file content
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: Some("line2".to_string()),
            regexp: None,
            state: Some(State::Present),
        };

        let result = lineinfile(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_lineinfile_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Create initial file content
        fs::write(&file_path, "line1\nline2\n").unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: Some("line3".to_string()),
            regexp: None,
            state: Some(State::Present),
        };

        let result = lineinfile(params, true).unwrap(); // check_mode = true
        assert!(result.changed);

        // File should not have been modified in check mode
        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_lineinfile_invalid_regexp() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        fs::write(&file_path, "test content\n").unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            line: Some("new line".to_string()),
            regexp: Some("[invalid".to_string()), // Invalid regex
            state: Some(State::Present),
        };

        let result = lineinfile(params, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regexp"));
    }

    #[test]
    fn test_lineinfile_missing_line_for_present() {
        let params = Params {
            path: "/tmp/test.txt".to_string(),
            line: None,
            regexp: Some("test".to_string()),
            state: Some(State::Present),
        };

        let result = lineinfile(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("line parameter is required")
        );
    }

    #[test]
    fn test_lineinfile_missing_regexp_for_absent() {
        let params = Params {
            path: "/tmp/test.txt".to_string(),
            line: Some("test".to_string()),
            regexp: None,
            state: Some(State::Absent),
        };

        let result = lineinfile(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("regexp parameter is required")
        );
    }
}
