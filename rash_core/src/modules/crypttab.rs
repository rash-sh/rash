/// ANCHOR: module
/// # crypttab
///
/// Manage encrypted filesystem entries in /etc/crypttab.
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
/// - name: Add encrypted swap partition
///   crypttab:
///     name: cryptswap
///     backing_device: /dev/sda2
///     password: /dev/urandom
///     opts: swap
///     state: present
///
/// - name: Add encrypted data volume with keyfile
///   crypttab:
///     name: cryptdata
///     backing_device: /dev/sdb1
///     password: /root/keyfile
///     opts: luks
///     state: present
///
/// - name: Add encrypted volume without password (will be prompted)
///   crypttab:
///     name: cryptdata
///     backing_device: /dev/sdb1
///     password: none
///     state: present
///
/// - name: Remove encrypted volume entry
///   crypttab:
///     name: cryptdata
///     state: absent
///
/// - name: Use custom crypttab file
///   crypttab:
///     name: cryptdata
///     backing_device: /dev/sdb1
///     password: /root/keyfile
///     state: present
///     path: /etc/crypttab.custom
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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

const DEFAULT_CRYPTTAB_PATH: &str = "/etc/crypttab";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the encrypted device mapping.
    pub name: String,
    /// Device containing encrypted data.
    /// Required when state=present.
    pub backing_device: Option<String>,
    /// Password/keyfile for decryption.
    /// Use 'none' for interactive password prompt.
    /// **[default: `"none"`]**
    pub password: Option<String>,
    /// Options for cryptsetup.
    pub opts: Option<String>,
    /// Whether the entry should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the crypttab file.
    /// **[default: `"/etc/crypttab"`]**
    pub path: Option<String>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
struct CrypttabEntry {
    name: String,
    backing_device: String,
    password: String,
    opts: Option<String>,
}

impl CrypttabEntry {
    fn to_line(&self) -> String {
        let opts_part = match &self.opts {
            Some(o) if !o.is_empty() => format!(" {}", o),
            _ => String::new(),
        };
        format!(
            "{} {} {}{}",
            self.name, self.backing_device, self.password, opts_part
        )
    }

    fn from_line(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let name = parts[0].to_string();
        let backing_device = parts[1].to_string();
        let password = if parts.len() > 2 {
            parts[2].to_string()
        } else {
            "none".to_string()
        };
        let opts = if parts.len() > 3 {
            Some(parts[3..].join(" "))
        } else {
            None
        };

        Some(CrypttabEntry {
            name,
            backing_device,
            password,
            opts,
        })
    }
}

fn read_crypttab_file(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .map_while(std::result::Result::ok)
                .collect()
        })
        .unwrap_or_default()
}

fn find_entry_in_lines(lines: &[String], entry_name: &str) -> Option<(usize, CrypttabEntry)> {
    lines.iter().enumerate().find_map(|(idx, line)| {
        let entry = CrypttabEntry::from_line(line)?;
        if entry.name == entry_name {
            Some((idx, entry))
        } else {
            None
        }
    })
}

fn update_crypttab_file(params: &Params, crypttab_path: &str, check_mode: bool) -> Result<bool> {
    let path = Path::new(crypttab_path);
    let lines = read_crypttab_file(path);
    let original = lines.join("\n");

    let state = params.state.clone().unwrap_or_default();

    let mut changed = false;
    let mut new_lines = lines.clone();

    match state {
        State::Present => {
            let backing_device = params.backing_device.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "backing_device is required when state=present",
                )
            })?;

            let password = params
                .password
                .clone()
                .unwrap_or_else(|| "none".to_string());

            let new_entry = CrypttabEntry {
                name: params.name.clone(),
                backing_device: backing_device.clone(),
                password,
                opts: params.opts.clone(),
            };

            if let Some((idx, existing_entry)) = find_entry_in_lines(&lines, &params.name) {
                if existing_entry != new_entry {
                    new_lines[idx] = new_entry.to_line();
                    changed = true;
                }
            } else {
                if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    new_lines.push(String::new());
                }
                new_lines.push(new_entry.to_line());
                changed = true;
            }
        }
        State::Absent => {
            while let Some((idx, _)) = find_entry_in_lines(&new_lines, &params.name) {
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

pub fn crypttab(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let crypttab_path = params
        .path
        .clone()
        .unwrap_or_else(|| DEFAULT_CRYPTTAB_PATH.to_string());

    let changed = update_crypttab_file(&params, &crypttab_path, check_mode)?;

    Ok(ModuleResult::new(changed, None, Some(params.name)))
}

#[derive(Debug)]
pub struct Crypttab;

impl Module for Crypttab {
    fn get_name(&self) -> &str {
        "crypttab"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((crypttab(parse_params(optional_params)?, check_mode)?, None))
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
            name: cryptswap
            backing_device: /dev/sda2
            password: /dev/urandom
            opts: swap
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "cryptswap".to_owned(),
                backing_device: Some("/dev/sda2".to_owned()),
                password: Some("/dev/urandom".to_owned()),
                opts: Some("swap".to_owned()),
                state: Some(State::Present),
                path: None,
            }
        );
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: cryptdata
            backing_device: /dev/sdb1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "cryptdata".to_owned(),
                backing_device: Some("/dev/sdb1".to_owned()),
                password: None,
                opts: None,
                state: None,
                path: None,
            }
        );
    }

    #[test]
    fn test_crypttab_entry_to_line() {
        let entry = CrypttabEntry {
            name: "cryptswap".to_string(),
            backing_device: "/dev/sda2".to_string(),
            password: "/dev/urandom".to_string(),
            opts: Some("swap".to_string()),
        };
        assert_eq!(entry.to_line(), "cryptswap /dev/sda2 /dev/urandom swap");

        let entry_no_opts = CrypttabEntry {
            name: "cryptdata".to_string(),
            backing_device: "/dev/sdb1".to_string(),
            password: "none".to_string(),
            opts: None,
        };
        assert_eq!(entry_no_opts.to_line(), "cryptdata /dev/sdb1 none");
    }

    #[test]
    fn test_crypttab_entry_from_line() {
        let entry = CrypttabEntry::from_line("cryptswap /dev/sda2 /dev/urandom swap").unwrap();
        assert_eq!(entry.name, "cryptswap");
        assert_eq!(entry.backing_device, "/dev/sda2");
        assert_eq!(entry.password, "/dev/urandom");
        assert_eq!(entry.opts, Some("swap".to_string()));

        let entry_no_opts = CrypttabEntry::from_line("cryptdata /dev/sdb1 none").unwrap();
        assert_eq!(entry_no_opts.name, "cryptdata");
        assert_eq!(entry_no_opts.password, "none");
        assert_eq!(entry_no_opts.opts, None);

        let entry_minimal = CrypttabEntry::from_line("cryptdata /dev/sdb1").unwrap();
        assert_eq!(entry_minimal.password, "none");
    }

    #[test]
    fn test_crypttab_entry_from_line_ignores_comments() {
        assert!(CrypttabEntry::from_line("# comment line").is_none());
        assert!(CrypttabEntry::from_line("").is_none());
    }

    #[test]
    fn test_find_entry_in_lines() {
        let lines = vec![
            "# comment".to_string(),
            "cryptswap /dev/sda2 /dev/urandom swap".to_string(),
            "cryptdata /dev/sdb1 none luks".to_string(),
        ];
        assert!(find_entry_in_lines(&lines, "cryptswap").is_some());
        assert!(find_entry_in_lines(&lines, "cryptdata").is_some());
        assert!(find_entry_in_lines(&lines, "unknown").is_none());
    }

    #[test]
    fn test_update_crypttab_file_add() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("crypttab");

        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: Some("/dev/sda2".to_string()),
            password: Some("/dev/urandom".to_string()),
            opts: Some("swap".to_string()),
            state: Some(State::Present),
            path: Some(test_path.to_str().unwrap().to_string()),
        };

        let changed = update_crypttab_file(&params, test_path.to_str().unwrap(), false).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&test_path).unwrap();
        assert!(content.contains("cryptswap /dev/sda2 /dev/urandom swap"));
    }

    #[test]
    fn test_update_crypttab_file_no_change_when_exists() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("crypttab");
        fs::write(&test_path, "cryptswap /dev/sda2 /dev/urandom swap\n").unwrap();

        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: Some("/dev/sda2".to_string()),
            password: Some("/dev/urandom".to_string()),
            opts: Some("swap".to_string()),
            state: Some(State::Present),
            path: Some(test_path.to_str().unwrap().to_string()),
        };

        let changed = update_crypttab_file(&params, test_path.to_str().unwrap(), false).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_update_crypttab_file_change_when_different() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("crypttab");
        fs::write(&test_path, "cryptswap /dev/sda2 none\n").unwrap();

        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: Some("/dev/sda2".to_string()),
            password: Some("/dev/urandom".to_string()),
            opts: Some("swap".to_string()),
            state: Some(State::Present),
            path: Some(test_path.to_str().unwrap().to_string()),
        };

        let changed = update_crypttab_file(&params, test_path.to_str().unwrap(), false).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&test_path).unwrap();
        assert!(content.contains("/dev/urandom"));
    }

    #[test]
    fn test_update_crypttab_file_remove() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("crypttab");
        fs::write(
            &test_path,
            "cryptswap /dev/sda2 /dev/urandom swap\ncryptdata /dev/sdb1 none\n",
        )
        .unwrap();

        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: None,
            password: None,
            opts: None,
            state: Some(State::Absent),
            path: Some(test_path.to_str().unwrap().to_string()),
        };

        let changed = update_crypttab_file(&params, test_path.to_str().unwrap(), false).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&test_path).unwrap();
        assert!(!content.contains("cryptswap"));
        assert!(content.contains("cryptdata"));
    }

    #[test]
    fn test_update_crypttab_file_check_mode() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("crypttab");

        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: Some("/dev/sda2".to_string()),
            password: Some("/dev/urandom".to_string()),
            opts: Some("swap".to_string()),
            state: Some(State::Present),
            path: Some(test_path.to_str().unwrap().to_string()),
        };

        let changed = update_crypttab_file(&params, test_path.to_str().unwrap(), true).unwrap();
        assert!(changed);
        assert!(!test_path.exists());
    }

    #[test]
    fn test_update_crypttab_file_missing_backing_device() {
        let params = Params {
            name: "cryptswap".to_string(),
            backing_device: None,
            password: None,
            opts: None,
            state: Some(State::Present),
            path: None,
        };

        let result = update_crypttab_file(&params, "/tmp/test", false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("backing_device is required")
        );
    }
}
