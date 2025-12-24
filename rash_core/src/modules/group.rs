/// ANCHOR: module
/// # group
///
/// Manage groups and group attributes.
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
/// - group:
///     name: docker
///     gid: 999
///
/// - group:
///     name: myservice
///     system: yes
///
/// - group:
///     name: oldgroup
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_state() -> Option<State> {
    Some(State::Present)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the group to create, remove or modify.
    pub name: String,
    /// Whether the group should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// Group ID (GID) of the group.
    pub gid: Option<u32>,
    /// Create as system group (gid < 1000).
    /// **[default: `false`]**
    #[serde(default)]
    pub system: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupInfo {
    pub name: String,
    pub gid: u32,
}

/// Parse a line from /etc/group into GroupInfo
fn parse_group_line(line: &str) -> Option<GroupInfo> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    Some(GroupInfo {
        name: parts[0].to_string(),
        gid: parts[2].parse().ok()?,
    })
}

/// Get group info from /etc/group (or test mock file if it exists)
fn get_group_info(groupname: &str) -> Option<GroupInfo> {
    // Check for test mock file first (for e2e testing without root)
    // Use environment variable if set, otherwise check default test path
    let group_path = if let Ok(test_file) = std::env::var("RASH_TEST_GROUP_FILE") {
        test_file
    } else if std::path::Path::new("/tmp/rash_test_group").exists() {
        "/tmp/rash_test_group".to_string()
    } else {
        "/etc/group".to_string()
    };

    if let Ok(groupfile) = fs::read_to_string(&group_path) {
        for line in groupfile.lines() {
            if let Some(info) = parse_group_line(line)
                && info.name == groupname
            {
                return Some(info);
            }
        }
    }
    None
}

/// Build groupadd command arguments
fn build_groupadd_command(params: &Params) -> Vec<String> {
    let mut cmd = vec!["groupadd".to_string()];

    if let Some(gid) = params.gid {
        cmd.push("-g".to_string());
        cmd.push(gid.to_string());
    }
    if let Some(true) = params.system {
        cmd.push("-r".to_string());
    }

    cmd.push(params.name.clone());
    cmd
}

/// Build groupmod command arguments
fn build_groupmod_command(params: &Params, current: &GroupInfo) -> Vec<String> {
    let mut cmd = vec!["groupmod".to_string()];

    if let Some(gid) = params.gid
        && gid != current.gid
    {
        cmd.push("-g".to_string());
        cmd.push(gid.to_string());
    }

    cmd.push(params.name.clone());
    cmd
}

/// Build groupdel command arguments
fn build_groupdel_command(params: &Params) -> Vec<String> {
    vec!["groupdel".to_string(), params.name.clone()]
}

/// Execute a group management command
fn exec_group_command(cmd: &[String], check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
    if check_mode {
        return Ok((
            ModuleResult {
                changed: true,
                output: Some(format!("Would run: {}", cmd.join(" "))),
                extra: None,
            },
            None,
        ));
    }

    let mut command = std::process::Command::new(&cmd[0]);
    command.args(&cmd[1..]);

    // Pass through environment variables for test mocks
    if let Ok(test_file) = std::env::var("RASH_TEST_GROUP_FILE") {
        command.env("RASH_TEST_GROUP_FILE", test_file);
    }

    let output = command
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(Error::new(ErrorKind::InvalidData, stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok((
        ModuleResult {
            changed: true,
            output: Some(stdout.into_owned()),
            extra: None,
        },
        None,
    ))
}

#[derive(Debug)]
pub struct Group;

impl Module for Group {
    fn get_name(&self) -> &str {
        "group"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = serde_norway::from_value(params)?;
        let current = get_group_info(&params.name);

        match params.state.clone().unwrap_or(State::Present) {
            State::Present => match current {
                None => {
                    // Group does not exist, create
                    let cmd = build_groupadd_command(&params);
                    exec_group_command(&cmd, check_mode)
                }
                Some(ref info) => {
                    // Group exists, modify if needed
                    let cmd = build_groupmod_command(&params, info);
                    if cmd.len() > 2 {
                        // groupmod + name + at least one change
                        exec_group_command(&cmd, check_mode)
                    } else {
                        Ok((
                            ModuleResult {
                                changed: false,
                                output: None,
                                extra: None,
                            },
                            None,
                        ))
                    }
                }
            },
            State::Absent => match current {
                None => Ok((
                    ModuleResult {
                        changed: false,
                        output: Some("Group already absent".to_string()),
                        extra: None,
                    },
                    None,
                )),
                Some(_) => {
                    let cmd = build_groupdel_command(&params);
                    exec_group_command(&cmd, check_mode)
                }
            },
        }
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(r#"name: docker"#).unwrap();
        let params: Params = serde_norway::from_value(yaml).unwrap();
        assert_eq!(params.name, "docker");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_group_line() {
        let line = "docker:x:999:user1,user2";
        let info = parse_group_line(line).unwrap();
        assert_eq!(info.name, "docker");
        assert_eq!(info.gid, 999);
    }

    #[test]
    fn test_build_groupadd_command_basic() {
        let params = Params {
            name: "docker".to_string(),
            state: Some(State::Present),
            gid: Some(999),
            system: None,
        };
        let cmd = build_groupadd_command(&params);
        assert!(cmd.contains(&"groupadd".to_string()));
        assert!(cmd.contains(&"-g".to_string()));
        assert!(cmd.contains(&"999".to_string()));
        assert!(cmd.contains(&"docker".to_string()));
    }

    #[test]
    fn test_build_groupadd_command_system() {
        let params = Params {
            name: "myservice".to_string(),
            state: Some(State::Present),
            gid: None,
            system: Some(true),
        };
        let cmd = build_groupadd_command(&params);
        assert!(cmd.contains(&"groupadd".to_string()));
        assert!(cmd.contains(&"-r".to_string()));
        assert!(cmd.contains(&"myservice".to_string()));
    }

    #[test]
    fn test_build_groupdel_command() {
        let params = Params {
            name: "docker".to_string(),
            state: Some(State::Absent),
            gid: None,
            system: None,
        };
        let cmd = build_groupdel_command(&params);
        assert_eq!(cmd, vec!["groupdel", "docker"]);
    }
}
