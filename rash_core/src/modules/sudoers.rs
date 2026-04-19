/// ANCHOR: module
/// # sudoers
///
/// Manage sudoers configuration entries in /etc/sudoers.d.
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
/// - name: Allow nginx to restart service without password
///   sudoers:
///     name: nginx-service
///     user: nginx
///     commands:
///       - /usr/sbin/service nginx restart
///       - /usr/sbin/service nginx status
///     nopassword: true
///
/// - name: Allow developers group to run docker commands
///   sudoers:
///     name: docker-developers
///     user: "%developers"
///     commands: /usr/bin/docker
///     nopassword: true
///     setenv: true
///
/// - name: Allow specific user to run all commands
///   sudoers:
///     name: admin-user
///     user: adminuser
///     commands: ALL
///
/// - name: Remove sudoers rule
///   sudoers:
///     name: deprecated-rule
///     user: olduser
///     commands: ALL
///     state: absent
///
/// - name: Custom sudoers path
///   sudoers:
///     name: custom-rule
///     user: myuser
///     commands: /usr/local/bin/myapp
///     sudoers_path: /etc/sudoers.d
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_SUDOERS_PATH: &str = "/etc/sudoers.d";
const SUDOERS_PERMISSIONS: u32 = 0o440;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the sudoers rule. This becomes the filename in sudoers.d.
    pub name: String,
    /// User or group to grant sudo access. Groups should be prefixed with %.
    pub user: String,
    /// Commands the user/group can run. Can be a single command or list.
    pub commands: Commands,
    /// Whether to require password for sudo.
    /// **[default: `false`]**
    pub nopassword: Option<bool>,
    /// Allow user to set environment variables with sudo.
    /// **[default: `false`]**
    pub setenv: Option<bool>,
    /// Path to the sudoers.d directory.
    /// **[default: `"/etc/sudoers.d"`]**
    pub sudoers_path: Option<String>,
    /// Whether the rule should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone, Copy)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum Commands {
    Single(String),
    Multiple(Vec<String>),
}

impl Commands {
    fn to_sudoers_format(&self) -> String {
        match self {
            Commands::Single(cmd) => cmd.clone(),
            Commands::Multiple(cmds) => cmds.join(", "),
        }
    }
}

fn generate_sudoers_content(params: &Params) -> String {
    let nopasswd = if params.nopassword.unwrap_or(false) {
        "NOPASSWD: "
    } else {
        ""
    };
    let setenv = if params.setenv.unwrap_or(false) {
        "SETENV: "
    } else {
        ""
    };
    let commands = params.commands.to_sudoers_format();

    format!(
        "{} ALL=(ALL) {}{}{}\n",
        params.user, nopasswd, setenv, commands
    )
}

fn validate_sudoers_name(name: &str) -> Result<()> {
    if name.contains('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "sudoers rule name cannot contain '.' (periods are not allowed in sudoers.d filenames)",
        ));
    }
    if name.contains('~') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "sudoers rule name cannot contain '~' (tilde characters are not allowed in sudoers.d filenames)",
        ));
    }
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "sudoers rule name cannot be empty",
        ));
    }
    Ok(())
}

pub fn sudoers(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    validate_sudoers_name(&params.name)?;

    let state = params.state.unwrap_or_default();
    let sudoers_path = params
        .sudoers_path
        .as_deref()
        .unwrap_or(DEFAULT_SUDOERS_PATH);
    let rule_path = Path::new(sudoers_path).join(&params.name);

    match state {
        State::Present => {
            let desired_content = generate_sudoers_content(&params);

            if rule_path.exists() {
                let current_content = fs::read_to_string(&rule_path)?;

                if current_content == desired_content {
                    return Ok(ModuleResult::new(
                        false,
                        None,
                        Some(rule_path.display().to_string()),
                    ));
                }

                diff(&current_content, &desired_content);

                if !check_mode {
                    if let Some(parent) = rule_path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }

                    let mut file = fs::File::create(&rule_path)?;
                    file.write_all(desired_content.as_bytes())?;

                    fs::set_permissions(
                        &rule_path,
                        fs::Permissions::from_mode(SUDOERS_PERMISSIONS),
                    )?;
                }

                Ok(ModuleResult::new(
                    true,
                    None,
                    Some(rule_path.display().to_string()),
                ))
            } else {
                diff("", &desired_content);

                if !check_mode {
                    if let Some(parent) = rule_path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }

                    let mut file = fs::File::create(&rule_path)?;
                    file.write_all(desired_content.as_bytes())?;

                    fs::set_permissions(
                        &rule_path,
                        fs::Permissions::from_mode(SUDOERS_PERMISSIONS),
                    )?;
                }

                Ok(ModuleResult::new(
                    true,
                    None,
                    Some(rule_path.display().to_string()),
                ))
            }
        }
        State::Absent => {
            if rule_path.exists() {
                let current_content = fs::read_to_string(&rule_path)?;
                diff(&current_content, "");

                if !check_mode {
                    fs::remove_file(&rule_path)?;
                }

                Ok(ModuleResult::new(
                    true,
                    None,
                    Some(rule_path.display().to_string()),
                ))
            } else {
                Ok(ModuleResult::new(
                    false,
                    None,
                    Some(rule_path.display().to_string()),
                ))
            }
        }
    }
}

#[derive(Debug)]
pub struct Sudoers;

impl Module for Sudoers {
    fn get_name(&self) -> &str {
        "sudoers"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((sudoers(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_single_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nginx-service
            user: nginx
            commands: /usr/sbin/service nginx restart
            nopassword: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "nginx-service");
        assert_eq!(params.user, "nginx");
        assert_eq!(
            params.commands,
            Commands::Single("/usr/sbin/service nginx restart".to_string())
        );
        assert_eq!(params.nopassword, Some(true));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_multiple_commands() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: docker-developers
            user: "%developers"
            commands:
              - /usr/bin/docker
              - /usr/bin/docker-compose
            nopassword: true
            setenv: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "docker-developers");
        assert_eq!(params.user, "%developers");
        assert_eq!(
            params.commands,
            Commands::Multiple(vec![
                "/usr/bin/docker".to_string(),
                "/usr/bin/docker-compose".to_string()
            ])
        );
        assert_eq!(params.nopassword, Some(true));
        assert_eq!(params.setenv, Some(true));
    }

    #[test]
    fn test_commands_to_sudoers_format() {
        let single = Commands::Single("/usr/bin/docker".to_string());
        assert_eq!(single.to_sudoers_format(), "/usr/bin/docker");

        let multiple = Commands::Multiple(vec![
            "/usr/bin/docker".to_string(),
            "/usr/bin/docker-compose".to_string(),
        ]);
        assert_eq!(
            multiple.to_sudoers_format(),
            "/usr/bin/docker, /usr/bin/docker-compose"
        );
    }

    #[test]
    fn test_generate_sudoers_content_basic() {
        let params = Params {
            name: "test".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: None,
            setenv: None,
            sudoers_path: None,
            state: None,
        };
        let content = generate_sudoers_content(&params);
        assert_eq!(content, "nginx ALL=(ALL) /usr/sbin/service nginx restart\n");
    }

    #[test]
    fn test_generate_sudoers_content_nopassword() {
        let params = Params {
            name: "test".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: None,
            state: None,
        };
        let content = generate_sudoers_content(&params);
        assert_eq!(
            content,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n"
        );
    }

    #[test]
    fn test_generate_sudoers_content_setenv() {
        let params = Params {
            name: "test".to_string(),
            user: "%developers".to_string(),
            commands: Commands::Single("/usr/bin/docker".to_string()),
            nopassword: Some(true),
            setenv: Some(true),
            sudoers_path: None,
            state: None,
        };
        let content = generate_sudoers_content(&params);
        assert_eq!(
            content,
            "%developers ALL=(ALL) NOPASSWD: SETENV: /usr/bin/docker\n"
        );
    }

    #[test]
    fn test_generate_sudoers_content_all_commands() {
        let params = Params {
            name: "test".to_string(),
            user: "admin".to_string(),
            commands: Commands::Single("ALL".to_string()),
            nopassword: None,
            setenv: None,
            sudoers_path: None,
            state: None,
        };
        let content = generate_sudoers_content(&params);
        assert_eq!(content, "admin ALL=(ALL) ALL\n");
    }

    #[test]
    fn test_validate_sudoers_name_valid() {
        assert!(validate_sudoers_name("nginx-service").is_ok());
        assert!(validate_sudoers_name("docker-developers").is_ok());
        assert!(validate_sudoers_name("admin-user").is_ok());
    }

    #[test]
    fn test_validate_sudoers_name_invalid_dot() {
        let result = validate_sudoers_name("nginx.service");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("periods"));
    }

    #[test]
    fn test_validate_sudoers_name_invalid_tilde() {
        let result = validate_sudoers_name("nginx~service");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("tilde"));
    }

    #[test]
    fn test_validate_sudoers_name_empty() {
        let result = validate_sudoers_name("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_sudoers_create_rule() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, false).unwrap();
        assert!(result.changed);

        let rule_path = sudoers_path.join("nginx-service");
        assert!(rule_path.exists());

        let content = fs::read_to_string(&rule_path).unwrap();
        assert_eq!(
            content,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n"
        );

        let perms = fs::metadata(&rule_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o440);
    }

    #[test]
    fn test_sudoers_modify_rule() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");
        fs::create_dir_all(&sudoers_path).unwrap();

        let rule_path = sudoers_path.join("nginx-service");
        fs::write(
            &rule_path,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx status\n",
        )
        .unwrap();

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&rule_path).unwrap();
        assert_eq!(
            content,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n"
        );
    }

    #[test]
    fn test_sudoers_no_change() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");
        fs::create_dir_all(&sudoers_path).unwrap();

        let rule_path = sudoers_path.join("nginx-service");
        fs::write(
            &rule_path,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n",
        )
        .unwrap();
        fs::set_permissions(&rule_path, fs::Permissions::from_mode(0o440)).unwrap();

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sudoers_remove_rule() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");
        fs::create_dir_all(&sudoers_path).unwrap();

        let rule_path = sudoers_path.join("nginx-service");
        fs::write(
            &rule_path,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart\n",
        )
        .unwrap();

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Absent),
        };

        let result = sudoers(params, false).unwrap();
        assert!(result.changed);
        assert!(!rule_path.exists());
    }

    #[test]
    fn test_sudoers_remove_nonexistent_rule() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");
        fs::create_dir_all(&sudoers_path).unwrap();

        let params = Params {
            name: "nonexistent".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Absent),
        };

        let result = sudoers(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_sudoers_check_mode() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Single("/usr/sbin/service nginx restart".to_string()),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, true).unwrap();
        assert!(result.changed);

        let rule_path = sudoers_path.join("nginx-service");
        assert!(!rule_path.exists());
    }

    #[test]
    fn test_sudoers_multiple_commands() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");

        let params = Params {
            name: "nginx-service".to_string(),
            user: "nginx".to_string(),
            commands: Commands::Multiple(vec![
                "/usr/sbin/service nginx restart".to_string(),
                "/usr/sbin/service nginx status".to_string(),
            ]),
            nopassword: Some(true),
            setenv: None,
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, false).unwrap();
        assert!(result.changed);

        let rule_path = sudoers_path.join("nginx-service");
        let content = fs::read_to_string(&rule_path).unwrap();
        assert_eq!(
            content,
            "nginx ALL=(ALL) NOPASSWD: /usr/sbin/service nginx restart, /usr/sbin/service nginx status\n"
        );
    }

    #[test]
    fn test_sudoers_group_user() {
        let dir = tempdir().unwrap();
        let sudoers_path = dir.path().join("sudoers.d");

        let params = Params {
            name: "docker-developers".to_string(),
            user: "%developers".to_string(),
            commands: Commands::Single("/usr/bin/docker".to_string()),
            nopassword: Some(true),
            setenv: Some(true),
            sudoers_path: Some(sudoers_path.to_str().unwrap().to_string()),
            state: Some(State::Present),
        };

        let result = sudoers(params, false).unwrap();
        assert!(result.changed);

        let rule_path = sudoers_path.join("docker-developers");
        let content = fs::read_to_string(&rule_path).unwrap();
        assert_eq!(
            content,
            "%developers ALL=(ALL) NOPASSWD: SETENV: /usr/bin/docker\n"
        );
    }
}
