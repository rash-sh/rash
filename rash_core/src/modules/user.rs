/// ANCHOR: module
/// # user
///
/// Manage user accounts and user attributes.
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
/// - user:
///     name: johnd
///     comment: John Doe
///     uid: 1040
///     group: admin
///     shell: /bin/bash
///
/// - user:
///     name: myservice
///     system: yes
///     create_home: no
///     shell: /sbin/nologin
///
/// - user:
///     name: james
///     groups:
///       - docker
///       - wheel
///     append: yes
///
/// - user:
///     name: olduser
///     state: absent
///     remove: yes
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
    /// Name of the user to create, remove or modify.
    pub name: String,
    /// Whether the account should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// User ID of the user.
    pub uid: Option<u32>,
    /// Primary group name.
    pub group: Option<String>,
    /// List of supplementary groups.
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    /// If true, add the user to the groups specified in groups.
    /// If false, user will only be in the groups specified.
    /// **[default: `false`]**
    #[serde(default)]
    pub append: Option<bool>,
    /// Home directory path.
    pub home: Option<String>,
    /// Create home directory if it doesn't exist.
    /// **[default: `true`]**
    #[serde(default)]
    pub create_home: Option<bool>,
    /// Login shell path.
    pub shell: Option<String>,
    /// User description (GECOS field).
    pub comment: Option<String>,
    /// Create as system user (uid < 1000).
    /// **[default: `false`]**
    #[serde(default)]
    pub system: Option<bool>,
    /// Encrypted password hash.
    pub password: Option<String>,
    /// Remove home directory when state=absent.
    /// **[default: `false`]**
    #[serde(default)]
    pub remove: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Present,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserInfo {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub comment: String,
    pub home: String,
    pub shell: String,
}

/// Parse a line from /etc/passwd into UserInfo
fn parse_passwd_line(line: &str) -> Option<UserInfo> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 7 {
        return None;
    }
    Some(UserInfo {
        name: parts[0].to_string(),
        uid: parts[2].parse().ok()?,
        gid: parts[3].parse().ok()?,
        comment: parts[4].to_string(),
        home: parts[5].to_string(),
        shell: parts[6].trim().to_string(),
    })
}

/// Get user info from /etc/passwd (or test mock file if it exists)
fn get_user_info(username: &str) -> Option<UserInfo> {
    // Check for test mock file first (for e2e testing without root)
    // Use environment variable if set, otherwise check default test path
    let passwd_path = if let Ok(test_file) = std::env::var("RASH_TEST_PASSWD_FILE") {
        test_file
    } else if std::path::Path::new("/tmp/rash_test_passwd").exists() {
        "/tmp/rash_test_passwd".to_string()
    } else {
        "/etc/passwd".to_string()
    };

    if let Ok(passwd) = fs::read_to_string(&passwd_path) {
        for line in passwd.lines() {
            if let Some(info) = parse_passwd_line(line)
                && info.name == username
            {
                return Some(info);
            }
        }
    }
    None
}

/// Get supplementary groups for a user from /etc/group
fn get_user_groups(username: &str) -> Vec<String> {
    let mut groups = Vec::new();

    let group_path = if let Ok(test_file) = std::env::var("RASH_TEST_GROUP_FILE") {
        test_file
    } else if std::path::Path::new("/tmp/rash_test_group").exists() {
        "/tmp/rash_test_group".to_string()
    } else {
        "/etc/group".to_string()
    };

    if let Ok(groupfile) = fs::read_to_string(&group_path) {
        for line in groupfile.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() < 4 {
                continue;
            }
            let groupname = parts[0];
            let members = parts[3];
            if members.split(',').any(|m| m == username) {
                groups.push(groupname.to_string());
            }
        }
    }
    groups
}

/// Build useradd command arguments
fn build_useradd_command(params: &Params) -> Vec<String> {
    let mut cmd = vec!["useradd".to_string()];

    if let Some(uid) = params.uid {
        cmd.push("-u".to_string());
        cmd.push(uid.to_string());
    }
    if let Some(ref group) = params.group {
        cmd.push("-g".to_string());
        cmd.push(group.clone());
    }
    if let Some(ref groups) = params.groups
        && !groups.is_empty()
    {
        cmd.push("-G".to_string());
        cmd.push(groups.join(","));
    }
    if let Some(ref home) = params.home {
        cmd.push("-d".to_string());
        cmd.push(home.clone());
    }
    match params.create_home {
        Some(true) | None => cmd.push("-m".to_string()),
        Some(false) => cmd.push("-M".to_string()),
    }
    if let Some(ref shell) = params.shell {
        cmd.push("-s".to_string());
        cmd.push(shell.clone());
    }
    if let Some(ref comment) = params.comment {
        cmd.push("-c".to_string());
        cmd.push(comment.clone());
    }
    if let Some(true) = params.system {
        cmd.push("-r".to_string());
    }
    if let Some(ref password) = params.password {
        cmd.push("-p".to_string());
        cmd.push(password.clone());
    }

    cmd.push(params.name.clone());
    cmd
}

/// Build usermod command arguments
fn build_usermod_command(params: &Params, current: &UserInfo) -> Vec<String> {
    let mut cmd = vec!["usermod".to_string()];

    if let Some(uid) = params.uid
        && uid != current.uid
    {
        cmd.push("-u".to_string());
        cmd.push(uid.to_string());
    }
    if let Some(ref group) = params.group {
        cmd.push("-g".to_string());
        cmd.push(group.clone());
    }
    if let Some(ref groups) = params.groups
        && !groups.is_empty()
    {
        // Determine which groups need to be added
        let groups_to_add = if params.append.unwrap_or(false) {
            // In append mode, only add groups the user doesn't already have
            let current_groups = get_user_groups(&params.name);
            groups
                .iter()
                .filter(|g| !current_groups.contains(g))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            // In replace mode, always set groups
            groups.clone()
        };

        if !groups_to_add.is_empty() {
            if params.append.unwrap_or(false) {
                cmd.push("-a".to_string());
            }
            cmd.push("-G".to_string());
            cmd.push(groups_to_add.join(","));
        }
    }
    if let Some(ref home) = params.home
        && home != &current.home
    {
        cmd.push("-d".to_string());
        cmd.push(home.clone());
    }
    if let Some(ref shell) = params.shell
        && shell != &current.shell
    {
        cmd.push("-s".to_string());
        cmd.push(shell.clone());
    }
    if let Some(ref comment) = params.comment
        && comment != &current.comment
    {
        cmd.push("-c".to_string());
        cmd.push(comment.clone());
    }
    if let Some(ref password) = params.password {
        cmd.push("-p".to_string());
        cmd.push(password.clone());
    }

    cmd.push(params.name.clone());
    cmd
}

/// Build userdel command arguments
fn build_userdel_command(params: &Params) -> Vec<String> {
    let mut cmd = vec!["userdel".to_string()];

    if let Some(true) = params.remove {
        cmd.push("-r".to_string());
    }

    cmd.push(params.name.clone());
    cmd
}

/// Execute a user management command
fn exec_user_command(cmd: &[String], check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
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
pub struct User;

impl Module for User {
    fn get_name(&self) -> &str {
        "user"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = serde_norway::from_value(params)?;
        let current = get_user_info(&params.name);

        match params.state.clone().unwrap_or(State::Present) {
            State::Present => match current {
                None => {
                    // User does not exist, create
                    let cmd = build_useradd_command(&params);
                    exec_user_command(&cmd, check_mode)
                }
                Some(ref info) => {
                    // User exists, modify if needed
                    let cmd = build_usermod_command(&params, info);
                    if cmd.len() > 2 {
                        // usermod + name + at least one change
                        exec_user_command(&cmd, check_mode)
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
                        output: Some("User already absent".to_string()),
                        extra: None,
                    },
                    None,
                )),
                Some(_) => {
                    let cmd = build_userdel_command(&params);
                    exec_user_command(&cmd, check_mode)
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
        let yaml: YamlValue = serde_norway::from_str(r#"name: johnd"#).unwrap();
        let params: Params = serde_norway::from_value(yaml).unwrap();
        assert_eq!(params.name, "johnd");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_passwd_line() {
        let line = "johnd:x:1040:1000:John Doe:/home/johnd:/bin/bash";
        let info = parse_passwd_line(line).unwrap();
        assert_eq!(info.name, "johnd");
        assert_eq!(info.uid, 1040);
        assert_eq!(info.gid, 1000);
        assert_eq!(info.comment, "John Doe");
        assert_eq!(info.home, "/home/johnd");
        assert_eq!(info.shell, "/bin/bash");
    }

    #[test]
    fn test_build_useradd_command_basic() {
        let params = Params {
            name: "johnd".to_string(),
            state: Some(State::Present),
            uid: Some(1040),
            group: Some("admin".to_string()),
            groups: Some(vec!["docker".to_string(), "wheel".to_string()]),
            append: None,
            home: Some("/home/johnd".to_string()),
            create_home: Some(true),
            shell: Some("/bin/bash".to_string()),
            comment: Some("John Doe".to_string()),
            system: None,
            password: None,
            remove: None,
        };
        let cmd = build_useradd_command(&params);
        assert!(cmd.contains(&"useradd".to_string()));
        assert!(cmd.contains(&"-u".to_string()));
        assert!(cmd.contains(&"1040".to_string()));
        assert!(cmd.contains(&"-g".to_string()));
        assert!(cmd.contains(&"admin".to_string()));
        assert!(cmd.contains(&"-G".to_string()));
        assert!(cmd.contains(&"docker,wheel".to_string()));
        assert!(cmd.contains(&"johnd".to_string()));
    }

    #[test]
    fn test_build_userdel_command() {
        let params = Params {
            name: "johnd".to_string(),
            state: Some(State::Absent),
            uid: None,
            group: None,
            groups: None,
            append: None,
            home: None,
            create_home: None,
            shell: None,
            comment: None,
            system: None,
            password: None,
            remove: Some(true),
        };
        let cmd = build_userdel_command(&params);
        assert_eq!(cmd, vec!["userdel", "-r", "johnd"]);
    }
}
