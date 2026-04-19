/// ANCHOR: module
/// # mongodb_user
///
/// Manage MongoDB users and permissions.
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
/// - name: Create MongoDB user
///   mongodb_user:
///     name: app_user
///     password: secret
///     database: myapp
///     roles: readWrite
///     state: present
///
/// - name: Create MongoDB user with multiple roles
///   mongodb_user:
///     name: admin_user
///     password: secret
///     database: admin
///     roles:
///       - userAdminAnyDatabase
///       - readWriteAnyDatabase
///     state: present
///
/// - name: Create user on remote MongoDB server
///   mongodb_user:
///     name: app_user
///     password: secret
///     database: myapp
///     roles: readWrite
///     login_host: mongo.example.com
///     login_port: 27017
///     login_user: admin
///     login_password: admin_secret
///
/// - name: Drop MongoDB user
///   mongodb_user:
///     name: app_user
///     database: myapp
///     state: absent
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

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    27017
}

fn default_database() -> String {
    "admin".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The username of the MongoDB user to manage.
    pub name: String,
    /// The password for the MongoDB user.
    pub password: Option<String>,
    /// The database where the user is created/managed.
    /// **[default: `"admin"`]**
    #[serde(default = "default_database")]
    pub database: String,
    /// The roles assigned to the user. Can be a single role or a list of roles.
    pub roles: Option<Roles>,
    /// The desired state of the user.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// The host running MongoDB.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// The port MongoDB is listening on.
    /// **[default: `27017`]**
    #[serde(default = "default_login_port")]
    pub login_port: u16,
    /// The MongoDB user to login with (must have userAdmin privileges).
    pub login_user: Option<String>,
    /// The password for login_user.
    pub login_password: Option<String>,
    /// Authentication database to use for login.
    pub login_database: Option<String>,
    /// Whether to update existing user password/roles.
    /// **[default: `true`]**
    #[serde(default = "default_update_on_create")]
    pub update_password: UpdatePassword,
}

fn default_update_on_create() -> UpdatePassword {
    UpdatePassword::Always
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum UpdatePassword {
    Always,
    OnCreate,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum Roles {
    Single(String),
    Multiple(Vec<String>),
}

impl Roles {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Roles::Single(role) => vec![role.clone()],
            Roles::Multiple(roles) => roles.clone(),
        }
    }
}

pub struct UserInfo {
    pub roles: Vec<String>,
}

fn build_mongo_base_args(params: &Params) -> Vec<String> {
    let mut args = vec![
        "--quiet".to_string(),
        "--host".to_string(),
        params.login_host.clone(),
        "--port".to_string(),
        params.login_port.to_string(),
    ];

    if let Some(ref user) = params.login_user {
        args.push("--username".to_string());
        args.push(user.clone());
    }

    if let Some(ref password) = params.login_password {
        args.push("--password".to_string());
        args.push(password.clone());
    }

    let auth_db = params.login_database.as_ref().unwrap_or(&params.database);
    args.push("--authenticationDatabase".to_string());
    args.push(auth_db.clone());

    args
}

fn user_exists(params: &Params) -> Result<Option<UserInfo>> {
    let mut args = build_mongo_base_args(params);
    args.push(params.database.clone());
    args.push("--eval".to_string());

    let query = format!(
        "db.getUsers().users.filter(u => u.user == '{}').map(u => ({{
            name: u.user,
            roles: u.roles.map(r => r.role)
        }}))",
        params.name
    );
    args.push(query);

    trace!("Checking user existence: mongosh {:?}", args);

    let output = Command::new("mongosh").args(&args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::NotFound,
                "mongosh command not found. Please install MongoDB Shell.",
            )
        } else {
            Error::new(ErrorKind::SubprocessFail, e)
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to check user existence: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();

    if trimmed.is_empty() || trimmed == "[]" || trimmed == "false" {
        return Ok(None);
    }

    let roles: Vec<String> =
        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(arr) = json_val.as_array() {
                if let Some(first) = arr.first() {
                    if let Some(obj) = first.as_object() {
                        if let Some(roles_arr) = obj.get("roles") {
                            roles_arr
                                .as_array()
                                .map(|r| {
                                    r.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };

    Ok(Some(UserInfo { roles }))
}

fn create_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would create user '{}' in database '{}'",
                params.name, params.database
            )),
            extra: None,
        });
    }

    let password = params.password.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "password is required for creating user",
        )
    })?;

    let roles = params
        .roles
        .as_ref()
        .map(|r| r.to_vec())
        .unwrap_or_default();

    if roles.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "roles are required for creating user",
        ));
    }

    let mut args = build_mongo_base_args(params);
    args.push(params.database.clone());
    args.push("--eval".to_string());

    let roles_json = roles
        .iter()
        .map(|r| format!("{{ role: \"{}\", db: \"{}\" }}", r, params.database))
        .collect::<Vec<_>>()
        .join(", ");

    let create_cmd = format!(
        "db.createUser({{
            user: \"{}\",
            pwd: \"{}\",
            roles: [{}]
        }})",
        params.name, password, roles_json
    );
    args.push(create_cmd);

    trace!("Creating user: mongosh {:?}", args);

    let output = Command::new("mongosh")
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create user: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "user": params.name,
        "database": params.database,
        "roles": roles,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "User '{}' created in database '{}'",
            params.name, params.database
        )),
        extra,
    })
}

fn update_user(
    params: &Params,
    existing_roles: Vec<String>,
    check_mode: bool,
) -> Result<ModuleResult> {
    let password = params.password.as_ref();
    let new_roles = params
        .roles
        .as_ref()
        .map(|r| r.to_vec())
        .unwrap_or_default();

    let password_changed = match params.update_password {
        UpdatePassword::Always => password.is_some(),
        UpdatePassword::OnCreate => false,
    };

    let roles_changed = !new_roles.is_empty() && new_roles != existing_roles;

    if !password_changed && !roles_changed {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "User '{}' already exists with correct settings",
                params.name
            )),
            extra: Some(value::to_value(json!({
                "user": params.name,
                "database": params.database,
                "roles": existing_roles,
            }))?),
        });
    }

    if check_mode {
        let changes: Vec<&str> = [
            if password_changed {
                Some("password")
            } else {
                None
            },
            if roles_changed { Some("roles") } else { None },
        ]
        .iter()
        .filter_map(|x| *x)
        .collect();

        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would update user '{}' ({})",
                params.name,
                changes.join(", ")
            )),
            extra: None,
        });
    }

    let mut args = build_mongo_base_args(params);
    args.push(params.database.clone());
    args.push("--eval".to_string());

    let mut cmd = String::new();

    if password_changed && let Some(pwd) = password {
        cmd = format!("db.updateUser(\"{}\", {{ pwd: \"{}\" }})", params.name, pwd);
    }

    if roles_changed && !new_roles.is_empty() {
        let roles_json = new_roles
            .iter()
            .map(|r| format!("{{ role: \"{}\", db: \"{}\" }}", r, params.database))
            .collect::<Vec<_>>()
            .join(", ");

        if cmd.is_empty() {
            cmd = format!(
                "db.updateUser(\"{}\", {{ roles: [{}] }})",
                params.name, roles_json
            );
        } else {
            cmd = format!(
                "db.updateUser(\"{}\", {{ pwd: \"{}\", roles: [{}] }})",
                params.name,
                password.unwrap(),
                roles_json
            );
        }
    }

    args.push(cmd);

    trace!("Updating user: mongosh {:?}", args);

    let output = Command::new("mongosh")
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to update user: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "user": params.name,
        "database": params.database,
        "roles": if roles_changed { new_roles } else { existing_roles },
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("User '{}' updated", params.name)),
        extra,
    })
}

fn drop_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would drop user '{}' from database '{}'",
                params.name, params.database
            )),
            extra: None,
        });
    }

    let mut args = build_mongo_base_args(params);
    args.push(params.database.clone());
    args.push("--eval".to_string());
    args.push(format!("db.dropUser(\"{}\")", params.name));

    trace!("Dropping user: mongosh {:?}", args);

    let output = Command::new("mongosh")
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to drop user: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "User '{}' dropped from database '{}'",
            params.name, params.database
        )),
        extra: None,
    })
}

fn mongodb_user_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => {
            let existing = user_exists(&params)?;

            match existing {
                None => create_user(&params, check_mode),
                Some(user_info) => update_user(&params, user_info.roles, check_mode),
            }
        }
        State::Absent => {
            let existing = user_exists(&params)?;

            match existing {
                None => Ok(ModuleResult {
                    changed: false,
                    output: Some(format!(
                        "User '{}' does not exist in database '{}'",
                        params.name, params.database
                    )),
                    extra: None,
                }),
                Some(_) => drop_user(&params, check_mode),
            }
        }
    }
}

#[derive(Debug)]
pub struct MongodbUser;

impl Module for MongodbUser {
    fn get_name(&self) -> &str {
        "mongodb_user"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mongodb_user_impl(params, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::from_str;

    #[test]
    fn test_parse_params_minimal() {
        let yaml = r#"
name: app_user
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "app_user");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.database, "admin");
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 27017);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml = r#"
name: app_user
password: secret
database: myapp
roles: readWrite
state: present
login_host: mongo.example.com
login_port: 27017
login_user: admin
login_password: admin_secret
login_database: admin
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "app_user");
        assert_eq!(params.password, Some("secret".to_string()));
        assert_eq!(params.database, "myapp");
        assert_eq!(params.roles, Some(Roles::Single("readWrite".to_string())));
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "mongo.example.com");
        assert_eq!(params.login_port, 27017);
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("admin_secret".to_string()));
        assert_eq!(params.login_database, Some("admin".to_string()));
    }

    #[test]
    fn test_parse_params_multiple_roles() {
        let yaml = r#"
name: admin_user
password: secret
database: admin
roles:
  - userAdminAnyDatabase
  - readWriteAnyDatabase
state: present
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "admin_user");
        assert_eq!(
            params.roles,
            Some(Roles::Multiple(vec![
                "userAdminAnyDatabase".to_string(),
                "readWriteAnyDatabase".to_string()
            ]))
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = r#"
name: app_user
database: myapp
state: absent
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "app_user");
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.database, "myapp");
    }

    #[test]
    fn test_roles_to_vec_single() {
        let roles = Roles::Single("readWrite".to_string());
        assert_eq!(roles.to_vec(), vec!["readWrite"]);
    }

    #[test]
    fn test_roles_to_vec_multiple() {
        let roles = Roles::Multiple(vec!["read".to_string(), "write".to_string()]);
        assert_eq!(roles.to_vec(), vec!["read", "write"]);
    }

    #[test]
    fn test_parse_params_update_password() {
        let yaml = r#"
name: app_user
password: secret
database: myapp
roles: readWrite
update_password: always
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.update_password, UpdatePassword::Always);
    }

    #[test]
    fn test_parse_params_update_password_on_create() {
        let yaml = r#"
name: app_user
password: secret
database: myapp
roles: readWrite
update_password: oncreate
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.update_password, UpdatePassword::OnCreate);
    }

    #[test]
    fn test_build_mongo_base_args() {
        let params = Params {
            name: "app_user".to_string(),
            password: Some("secret".to_string()),
            database: "myapp".to_string(),
            roles: Some(Roles::Single("readWrite".to_string())),
            state: State::Present,
            login_host: "mongo.example.com".to_string(),
            login_port: 27018,
            login_user: Some("admin".to_string()),
            login_password: Some("admin_secret".to_string()),
            login_database: Some("admin".to_string()),
            update_password: UpdatePassword::Always,
        };
        let args = build_mongo_base_args(&params);

        assert!(args.contains(&"--quiet".to_string()));
        assert!(args.contains(&"--host".to_string()));
        assert!(args.contains(&"mongo.example.com".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"27018".to_string()));
        assert!(args.contains(&"--username".to_string()));
        assert!(args.contains(&"admin".to_string()));
        assert!(args.contains(&"--password".to_string()));
        assert!(args.contains(&"admin_secret".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml = r#"
name: app_user
unknown: field
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let error = parse_params::<Params>(value).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
