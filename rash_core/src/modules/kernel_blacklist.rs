/// ANCHOR: module
/// # kernel_blacklist
///
/// Manage kernel module blacklist entries.
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
/// - name: Blacklist problematic module
///   kernel_blacklist:
///     name: nouveau
///     state: present
///
/// - name: Blacklist multiple modules
///   kernel_blacklist:
///     name: "{{ item }}"
///     state: present
///   loop:
///     - b43
///     - ssb
///
/// - name: Remove from blacklist
///   kernel_blacklist:
///     name: nouveau
///     state: absent
///
/// - name: Blacklist with custom file
///   kernel_blacklist:
///     name: floppy
///     state: present
///     blacklist_file: /etc/modprobe.d/no-floppy.conf
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::Result;
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_BLACKLIST_DIR: &str = "/etc/modprobe.d";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of kernel module to blacklist.
    pub name: String,
    /// Whether the module should be blacklisted or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the blacklist file.
    /// If not specified, `/etc/modprobe.d/rash-blacklist.conf` is used.
    /// **[default: `"/etc/modprobe.d/rash-blacklist.conf"`]**
    pub blacklist_file: Option<String>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[allow(clippy::lines_filter_map_ok)]
fn read_file_lines(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .filter_map(std::result::Result::ok)
                .collect()
        })
        .unwrap_or_default()
}

fn find_blacklist_in_lines(lines: &[String], module_name: &str) -> Option<usize> {
    let target = format!("blacklist {module_name}");
    lines.iter().position(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed == target
    })
}

fn update_blacklist_file(
    module_name: &str,
    state: &State,
    blacklist_file: &str,
    check_mode: bool,
) -> Result<bool> {
    let path = Path::new(blacklist_file);
    let lines = read_file_lines(path);
    let original = lines.join("\n");

    let mut changed = false;
    let mut new_lines = lines.clone();

    match state {
        State::Present => {
            if find_blacklist_in_lines(&lines, module_name).is_none() {
                if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    new_lines.push(String::new());
                }
                new_lines.push(format!("blacklist {module_name}"));
                changed = true;
            }
        }
        State::Absent => {
            while let Some(idx) = find_blacklist_in_lines(&new_lines, module_name) {
                new_lines.remove(idx);
                changed = true;
            }
        }
    }

    if changed && !check_mode {
        let new_content = new_lines.join("\n");
        diff(format!("{original}\n"), format!("{new_content}\n"));

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
        write!(file, "{new_content}")?;
    }

    Ok(changed)
}

pub fn kernel_blacklist(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let blacklist_file = params
        .blacklist_file
        .clone()
        .unwrap_or_else(|| format!("{DEFAULT_BLACKLIST_DIR}/rash-blacklist.conf"));

    let changed = update_blacklist_file(&params.name, &state, &blacklist_file, check_mode)?;

    Ok(ModuleResult::new(changed, None, Some(params.name)))
}

#[derive(Debug)]
pub struct KernelBlacklist;

impl Module for KernelBlacklist {
    fn get_name(&self) -> &str {
        "kernel_blacklist"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            kernel_blacklist(parse_params(optional_params)?, check_mode)?,
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
            name: nouveau
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "nouveau".to_owned(),
                state: Some(State::Present),
                blacklist_file: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: floppy
            state: present
            blacklist_file: /etc/modprobe.d/no-floppy.conf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "floppy".to_owned(),
                state: Some(State::Present),
                blacklist_file: Some("/etc/modprobe.d/no-floppy.conf".to_owned()),
            }
        );
    }

    #[test]
    fn test_find_blacklist_in_lines() {
        let lines = vec![
            "# Comment".to_string(),
            "blacklist nouveau".to_string(),
            "blacklist nvidia".to_string(),
        ];
        assert_eq!(find_blacklist_in_lines(&lines, "nouveau"), Some(1));
        assert_eq!(find_blacklist_in_lines(&lines, "nvidia"), Some(2));
        assert_eq!(find_blacklist_in_lines(&lines, "dummy"), None);
    }

    #[test]
    fn test_find_blacklist_in_lines_ignores_commented() {
        let lines = vec![
            "#blacklist nouveau".to_string(),
            "blacklist nouveau".to_string(),
        ];
        assert_eq!(find_blacklist_in_lines(&lines, "nouveau"), Some(1));
    }

    #[test]
    fn test_update_blacklist_file_add() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("blacklist.conf");

        let result =
            update_blacklist_file_at_path("nouveau", &State::Present, true, &test_path).unwrap();
        assert!(result);
    }

    #[test]
    fn test_update_blacklist_file_no_change() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("blacklist.conf");
        fs::write(&test_path, "blacklist nouveau\n").unwrap();

        let result =
            update_blacklist_file_at_path("nouveau", &State::Present, true, &test_path).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_update_blacklist_file_remove() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("blacklist.conf");
        fs::write(&test_path, "blacklist nouveau\nblacklist nvidia\n").unwrap();

        let result =
            update_blacklist_file_at_path("nouveau", &State::Absent, true, &test_path).unwrap();
        assert!(result);
    }

    #[test]
    fn test_update_blacklist_file_writes_correct_content() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("blacklist.conf");

        let result =
            update_blacklist_file_at_path("nouveau", &State::Present, false, &test_path).unwrap();
        assert!(result);

        let content = fs::read_to_string(&test_path).unwrap();
        assert!(content.contains("blacklist nouveau"));
    }

    fn update_blacklist_file_at_path(
        module_name: &str,
        state: &State,
        check_mode: bool,
        path: &Path,
    ) -> Result<bool> {
        let lines = read_file_lines(path);
        let original = lines.join("\n");

        let mut changed = false;
        let mut new_lines = lines.clone();

        match state {
            State::Present => {
                if find_blacklist_in_lines(&lines, module_name).is_none() {
                    if !new_lines.is_empty()
                        && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                    {
                        new_lines.push(String::new());
                    }
                    new_lines.push(format!("blacklist {module_name}"));
                    changed = true;
                }
            }
            State::Absent => {
                while let Some(idx) = find_blacklist_in_lines(&new_lines, module_name) {
                    new_lines.remove(idx);
                    changed = true;
                }
            }
        }

        if changed && !check_mode {
            let new_content = new_lines.join("\n");
            diff(format!("{original}\n"), format!("{new_content}\n"));

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
            write!(file, "{new_content}")?;
        }

        Ok(changed)
    }
}
