/// ANCHOR: module
/// # pids
///
/// Find process IDs matching criteria.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: always
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Find nginx processes
///   pids:
///     pattern: nginx
///   register: nginx_pids
///
/// - name: Find processes by user
///   pids:
///     user: root
///   register: root_pids
///
/// - name: Find processes with command pattern
///   pids:
///     command: python.*script
///   register: python_pids
///
/// - name: Find all processes excluding current
///   pids:
///     pattern: .*
///     exclude: rash
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_with::{OneOrMany, serde_as};

#[serde_as]
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Regex pattern to match process name (comm field from /proc/[pid]/comm).
    #[serde(default)]
    pattern: Option<String>,
    /// User name or UID running the process.
    #[serde(default)]
    user: Option<String>,
    /// Regex pattern to match against full command line.
    #[serde(default)]
    command: Option<String>,
    /// Regex pattern to exclude processes from the result.
    #[serde_as(deserialize_as = "Option<OneOrMany<_>>")]
    #[serde(default)]
    exclude: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ProcessInfo {
    pid: u32,
    name: String,
    user: String,
    command: String,
}

fn get_process_comm(pid: u32) -> Result<String> {
    let comm_path = Path::new("/proc").join(pid.to_string()).join("comm");
    fs::read_to_string(&comm_path)
        .map(|s| s.trim().to_string())
        .map_err(|e| Error::new(ErrorKind::Other, e))
}

fn get_process_cmdline(pid: u32) -> Result<String> {
    let cmdline_path = Path::new("/proc").join(pid.to_string()).join("cmdline");
    fs::read_to_string(&cmdline_path)
        .map(|s| s.replace('\0', " ").trim().to_string())
        .map_err(|e| Error::new(ErrorKind::Other, e))
}

fn get_process_user(pid: u32) -> Result<String> {
    let status_path = Path::new("/proc").join(pid.to_string()).join("status");
    let content = fs::read_to_string(&status_path).map_err(|e| Error::new(ErrorKind::Other, e))?;

    for line in content.lines() {
        if line.starts_with("Uid:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let uid: u32 = parts[1]
                    .parse()
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                return get_username_from_uid(uid);
            }
        }
    }
    Err(Error::new(
        ErrorKind::InvalidData,
        "Could not find Uid in process status",
    ))
}

fn get_username_from_uid(uid: u32) -> Result<String> {
    let passwd_path = Path::new("/etc/passwd");
    if passwd_path.exists() {
        let content =
            fs::read_to_string(passwd_path).map_err(|e| Error::new(ErrorKind::Other, e))?;

        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                let entry_uid: u32 = parts[2]
                    .parse()
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
                if entry_uid == uid {
                    return Ok(parts[0].to_string());
                }
            }
        }
    }
    Ok(uid.to_string())
}

fn get_all_pids() -> Result<Vec<u32>> {
    let proc_path = Path::new("/proc");
    let entries = fs::read_dir(proc_path).map_err(|e| Error::new(ErrorKind::Other, e))?;

    let pids: Vec<u32> = entries
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
            })
        })
        .collect();

    Ok(pids)
}

fn match_regex(value: &str, pattern: &str) -> Result<bool> {
    use regex::Regex;
    let re = Regex::new(pattern).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    Ok(re.is_match(value))
}

fn resolve_username(name_or_uid: &str) -> Result<String> {
    if let Ok(uid) = name_or_uid.parse::<u32>() {
        get_username_from_uid(uid)
    } else {
        Ok(name_or_uid.to_string())
    }
}

fn match_user(actual_user: &str, target_user: &str) -> Result<bool> {
    let actual_resolved = resolve_username(actual_user)?;
    let target_resolved = resolve_username(target_user)?;
    Ok(actual_resolved == target_resolved)
}

pub fn pids(params: Params) -> Result<ModuleResult> {
    let all_pids = get_all_pids()?;

    let pattern_regex = params.pattern.as_ref();
    let user_filter = params.user.as_ref();
    let command_regex = params.command.as_ref();
    let exclude_patterns = params.exclude.as_ref();

    let matched_processes: Vec<ProcessInfo> = all_pids
        .iter()
        .filter_map(|pid| {
            let comm = get_process_comm(*pid).ok()?;
            let cmdline = get_process_cmdline(*pid).ok()?;
            let user = get_process_user(*pid).ok()?;

            let matches_pattern = pattern_regex
                .map(|p| match_regex(&comm, p).unwrap_or(false))
                .unwrap_or(true);

            let matches_user = user_filter
                .map(|u| match_user(&user, u).unwrap_or(false))
                .unwrap_or(true);

            let matches_command = command_regex
                .map(|c| match_regex(&cmdline, c).unwrap_or(false))
                .unwrap_or(true);

            if matches_pattern && matches_user && matches_command {
                let excluded = exclude_patterns
                    .map(|patterns| {
                        patterns.iter().any(|ex| {
                            match_regex(&comm, ex).unwrap_or(false)
                                || match_regex(&cmdline, ex).unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                if !excluded {
                    return Some(ProcessInfo {
                        pid: *pid,
                        name: comm,
                        user,
                        command: cmdline,
                    });
                }
            }
            None
        })
        .collect();

    let pids_list: Vec<u32> = matched_processes.iter().map(|p| p.pid).collect();

    Ok(ModuleResult::new(
        false,
        Some(serde_norway::value::to_value(serde_json::json!({
            "pids": pids_list,
            "processes": matched_processes
        }))?),
        None,
    ))
}

#[derive(Debug)]
pub struct Pids;

impl Module for Pids {
    fn get_name(&self) -> &str {
        "pids"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((pids(parse_params(params)?)?, None))
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
            pattern: nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                pattern: Some("nginx".to_owned()),
                user: None,
                command: None,
                exclude: None,
            }
        );
    }

    #[test]
    fn test_parse_params_all_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            pattern: python
            user: root
            command: script.py
            exclude: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                pattern: Some("python".to_owned()),
                user: Some("root".to_owned()),
                command: Some("script.py".to_owned()),
                exclude: Some(vec!["test".to_owned()]),
            }
        );
    }

    #[test]
    fn test_parse_params_exclude_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            pattern: .*
            exclude:
              - test
              - debug
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                pattern: Some(".*".to_owned()),
                user: None,
                command: None,
                exclude: Some(vec!["test".to_owned(), "debug".to_owned()]),
            }
        );
    }

    #[test]
    fn test_match_regex() {
        assert!(match_regex("nginx", "nginx").unwrap());
        assert!(match_regex("nginx-worker", "nginx.*").unwrap());
        assert!(!match_regex("apache", "nginx").unwrap());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_match_user() {
        assert!(match_user("root", "root").unwrap());
        assert!(match_user("0", "root").unwrap());
        assert!(match_user("root", "0").unwrap());
        assert!(!match_user("root", "admin").unwrap());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_get_all_pids() {
        let pids = get_all_pids().unwrap();
        assert!(!pids.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_pids_no_filter() {
        let result = pids(Params {
            pattern: None,
            user: None,
            command: None,
            exclude: None,
        })
        .unwrap();
        assert!(!result.get_changed());
        let extra = result.get_extra().unwrap();
        let pids_list = extra.get("pids").unwrap().as_sequence().unwrap();
        assert!(!pids_list.is_empty());
    }
}
