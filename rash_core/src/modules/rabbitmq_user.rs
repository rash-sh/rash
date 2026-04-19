/// ANCHOR: module
/// # rabbitmq_user
///
/// Manage RabbitMQ users and permissions.
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
/// - name: Create a RabbitMQ user
///   rabbitmq_user:
///     user: myapp
///     password: secret
///     tags: management
///     state: present
///
/// - name: Create user with administrator tag
///   rabbitmq_user:
///     user: admin
///     password: adminpass
///     tags: administrator
///     state: present
///
/// - name: Set permissions for user on a vhost
///   rabbitmq_user:
///     user: myapp
///     password: secret
///     vhost: /myapp
///     configure_priv: "^myapp-.*"
///     write_priv: "^myapp-.*"
///     read_priv: "^myapp-.*"
///     state: present
///
/// - name: Create user with multiple tags
///   rabbitmq_user:
///     user: monitoring
///     password: monpass
///     tags:
///       - monitoring
///       - management
///     state: present
///
/// - name: Delete a user
///   rabbitmq_user:
///     user: olduser
///     state: absent
///
/// - name: Clear permissions for a user
///   rabbitmq_user:
///     user: myapp
///     vhost: /
///     configure_priv: ""
///     write_priv: ""
///     read_priv: ""
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_vhost() -> String {
    "/".to_string()
}

fn default_permissions() -> String {
    "".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the RabbitMQ user to create, remove or modify.
    pub user: String,
    /// Password for the user.
    pub password: Option<String>,
    /// User tags (administrator, management, monitoring, policymaker, etc).
    /// Can be a single string or a list of tags.
    #[serde(default)]
    pub tags: Option<Tags>,
    /// RabbitMQ virtual host.
    /// **[default: `/`]**
    #[serde(default = "default_vhost")]
    pub vhost: String,
    /// Configure permissions regex pattern.
    /// **[default: `""`]**
    #[serde(default = "default_permissions")]
    pub configure_priv: String,
    /// Write permissions regex pattern.
    /// **[default: `""`]**
    #[serde(default = "default_permissions")]
    pub write_priv: String,
    /// Read permissions regex pattern.
    /// **[default: `""`]**
    #[serde(default = "default_permissions")]
    pub read_priv: String,
    /// Whether the user should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum Tags {
    Single(String),
    Multiple(Vec<String>),
}

impl Tags {
    fn to_string_list(&self) -> String {
        match self {
            Tags::Single(s) => s.clone(),
            Tags::Multiple(v) => v.join(","),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserInfo {
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionInfo {
    pub configure: String,
    pub write: String,
    pub read: String,
}

fn run_rabbitmqctl(args: &[&str]) -> Result<String> {
    let output = Command::new("rabbitmqctl")
        .args(args)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute rabbitmqctl: {}", e),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("rabbitmqctl failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn user_exists(username: &str) -> Result<Option<UserInfo>> {
    let output = run_rabbitmqctl(&["list_users"])?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 && parts[0] == username {
            let tags_str = parts[1].trim();
            let tags: Vec<String> = if tags_str.starts_with('[') && tags_str.ends_with(']') {
                let inner = &tags_str[1..tags_str.len() - 1];
                inner.split(',').map(|t| t.trim().to_string()).collect()
            } else {
                vec![]
            };
            return Ok(Some(UserInfo {
                name: username.to_string(),
                tags,
            }));
        }
    }

    Ok(None)
}

fn get_user_permissions(username: &str, vhost: &str) -> Result<Option<PermissionInfo>> {
    let output = run_rabbitmqctl(&["list_user_permissions", username])?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 4 && parts[0] == vhost {
            return Ok(Some(PermissionInfo {
                configure: parts[1].trim().to_string(),
                write: parts[2].trim().to_string(),
                read: parts[3].trim().to_string(),
            }));
        }
    }

    Ok(None)
}

fn create_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create user '{}'", params.user)),
        ));
    }

    let password = params.password.as_deref().unwrap_or("");

    run_rabbitmqctl(&["add_user", &params.user, password])?;

    if let Some(ref tags) = params.tags {
        let tags_str = tags.to_string_list();
        if !tags_str.is_empty() {
            run_rabbitmqctl(&["set_user_tags", &params.user, &tags_str])?;
        }
    }

    if !params.configure_priv.is_empty()
        || !params.write_priv.is_empty()
        || !params.read_priv.is_empty()
    {
        run_rabbitmqctl(&[
            "set_permissions",
            "-p",
            &params.vhost,
            &params.user,
            &params.configure_priv,
            &params.write_priv,
            &params.read_priv,
        ])?;
    }

    let extra = Some(value::to_value(json!({
        "user": params.user,
        "tags": params.tags.as_ref().map(|t| t.to_string_list()),
        "vhost": params.vhost,
        "configure_priv": params.configure_priv,
        "write_priv": params.write_priv,
        "read_priv": params.read_priv,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("User '{}' created", params.user)),
    ))
}

fn update_user(params: &Params, current: &UserInfo, check_mode: bool) -> Result<ModuleResult> {
    let mut changes = Vec::new();
    let mut changed = false;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would update user '{}'", params.user)),
        ));
    }

    if let Some(ref password) = params.password {
        run_rabbitmqctl(&["change_password", &params.user, password])?;
        changes.push("password");
        changed = true;
    }

    if let Some(ref tags) = params.tags {
        let new_tags_str = tags.to_string_list();
        let current_tags_str = current.tags.join(",");

        if new_tags_str != current_tags_str {
            if !new_tags_str.is_empty() {
                run_rabbitmqctl(&["set_user_tags", &params.user, &new_tags_str])?;
            } else {
                run_rabbitmqctl(&["set_user_tags", &params.user])?;
            }
            changes.push("tags");
            changed = true;
        }
    }

    let current_perms = get_user_permissions(&params.user, &params.vhost)?;

    let needs_perm_update = match current_perms {
        None => true,
        Some(p) => {
            p.configure != params.configure_priv
                || p.write != params.write_priv
                || p.read != params.read_priv
        }
    };

    if needs_perm_update {
        run_rabbitmqctl(&[
            "set_permissions",
            "-p",
            &params.vhost,
            &params.user,
            &params.configure_priv,
            &params.write_priv,
            &params.read_priv,
        ])?;
        changes.push("permissions");
        changed = true;
    }

    let extra = Some(value::to_value(json!({
        "user": params.user,
        "changes": changes,
    }))?);

    if changed {
        Ok(ModuleResult::new(
            true,
            extra,
            Some(format!("User '{}' updated", params.user)),
        ))
    } else {
        Ok(ModuleResult::new(
            false,
            extra,
            Some(format!("User '{}' unchanged", params.user)),
        ))
    }
}

fn delete_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete user '{}'", params.user)),
        ));
    }

    run_rabbitmqctl(&["delete_user", &params.user])?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("User '{}' deleted", params.user)),
    ))
}

fn rabbitmq_user_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let existing = user_exists(&params.user)?;

    match params.state {
        State::Present => match existing {
            None => create_user(&params, check_mode),
            Some(info) => update_user(&params, &info, check_mode),
        },
        State::Absent => match existing {
            None => Ok(ModuleResult::new(
                false,
                None,
                Some(format!("User '{}' does not exist", params.user)),
            )),
            Some(_) => delete_user(&params, check_mode),
        },
    }
}

#[derive(Debug)]
pub struct RabbitmqUser;

impl Module for RabbitmqUser {
    fn get_name(&self) -> &str {
        "rabbitmq_user"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((rabbitmq_user_impl(params, check_mode)?, None))
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
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.vhost, "/");
        assert_eq!(params.configure_priv, "");
        assert_eq!(params.write_priv, "");
        assert_eq!(params.read_priv, "");
    }

    #[test]
    fn test_parse_params_with_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: myapp
            password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "myapp");
        assert_eq!(params.password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_with_single_tag() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: admin
            password: adminpass
            tags: administrator
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "admin");
        assert_eq!(params.tags, Some(Tags::Single("administrator".to_string())));
    }

    #[test]
    fn test_parse_params_with_multiple_tags() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: monitoring
            password: monpass
            tags:
              - monitoring
              - management
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "monitoring");
        assert_eq!(
            params.tags,
            Some(Tags::Multiple(vec![
                "monitoring".to_string(),
                "management".to_string()
            ]))
        );
    }

    #[test]
    fn test_parse_params_with_permissions() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: myapp
            password: secret
            vhost: /myapp
            configure_priv: "^myapp-.*"
            write_priv: "^myapp-.*"
            read_priv: "^myapp-.*"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "myapp");
        assert_eq!(params.vhost, "/myapp");
        assert_eq!(params.configure_priv, "^myapp-.*");
        assert_eq!(params.write_priv, "^myapp-.*");
        assert_eq!(params.read_priv, "^myapp-.*");
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: olduser
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "olduser");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_tags_to_string_list_single() {
        let tags = Tags::Single("administrator".to_string());
        assert_eq!(tags.to_string_list(), "administrator");
    }

    #[test]
    fn test_tags_to_string_list_multiple() {
        let tags = Tags::Multiple(vec!["monitoring".to_string(), "management".to_string()]);
        assert_eq!(tags.to_string_list(), "monitoring,management");
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: myapp
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
