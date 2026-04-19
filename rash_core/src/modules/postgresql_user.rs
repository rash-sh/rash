/// ANCHOR: module
/// # postgresql_user
///
/// Add or remove PostgreSQL users (roles).
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
/// - name: Create a new user with password
///   postgresql_user:
///     name: app_user
///     password: secret
///     state: present
///
/// - name: Create a user with specific role attributes
///   postgresql_user:
///     name: app_admin
///     password: admin_password
///     role_attr_flags: CREATEDB,NOSUPERUSER
///     state: present
///
/// - name: Create a superuser
///   postgresql_user:
///     name: admin_user
///     password: admin_secret
///     role_attr_flags: SUPERUSER
///     state: present
///
/// - name: Remove a user
///   postgresql_user:
///     name: old_user
///     state: absent
///
/// - name: Connect to remote database and create user
///   postgresql_user:
///     name: remote_user
///     password: remote_pass
///     login_host: db.example.com
///     login_user: admin
///     login_password: secret
///     port: 5432
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum State {
    #[default]
    Present,
    Absent,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Present => write!(f, "present"),
            State::Absent => write!(f, "absent"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the user (role) to add or remove.
    pub name: String,
    /// The user state.
    #[serde(default)]
    pub state: State,
    /// Password for the user.
    pub password: Option<String>,
    /// Whether the password is already encrypted.
    #[serde(default = "default_encrypted")]
    pub encrypted: bool,
    /// Role attributes flags.
    pub role_attr_flags: Option<String>,
    /// Host running the database.
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// The username to authenticate with.
    #[serde(default = "default_login_user")]
    pub login_user: String,
    /// The password to authenticate with.
    pub login_password: Option<String>,
    /// Database port to connect to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Path to a Unix domain socket for local connections.
    pub login_unix_socket: Option<String>,
    /// Disable SSL certificate verification.
    #[serde(default)]
    pub ssl_mode: Option<String>,
}

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_user() -> String {
    "postgres".to_string()
}

fn default_port() -> u16 {
    5432
}

fn default_encrypted() -> bool {
    true
}

fn build_env(params: &Params) -> Vec<(String, String)> {
    let mut env = Vec::new();

    if let Some(password) = &params.login_password {
        env.push(("PGPASSWORD".to_string(), password.clone()));
    }

    env
}

fn build_psql_base_args(params: &Params) -> Vec<String> {
    let mut args = vec![
        "-h".to_string(),
        params.login_host.clone(),
        "-p".to_string(),
        params.port.to_string(),
        "-U".to_string(),
        params.login_user.clone(),
    ];

    if let Some(ref socket) = params.login_unix_socket {
        args.push("-h".to_string());
        args.push(socket.clone());
    }

    if let Some(ref ssl_mode) = params.ssl_mode {
        args.push(format!("sslmode={}", ssl_mode));
    }

    args
}

fn user_exists(params: &Params) -> Result<bool> {
    let mut args = build_psql_base_args(params);
    args.extend(vec![
        "-t".to_string(),
        "-A".to_string(),
        "-c".to_string(),
        format!("SELECT 1 FROM pg_roles WHERE rolname = '{}'", params.name),
        "postgres".to_string(),
    ]);

    trace!("Checking if user exists: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::NotFound,
                    "psql command not found. Please install PostgreSQL client.",
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
    Ok(stdout.trim() == "1")
}

fn parse_role_attr_flags(flags_str: &str) -> Result<Vec<String>> {
    let mut flags = Vec::new();
    for flag in flags_str.split(',') {
        let flag = flag.trim().to_uppercase();
        let valid = matches!(
            flag.as_str(),
            "SUPERUSER"
                | "NOSUPERUSER"
                | "CREATEDB"
                | "NOCREATEDB"
                | "CREATEROLE"
                | "NOCREATEROLE"
                | "INHERIT"
                | "NOINHERIT"
                | "LOGIN"
                | "NOLOGIN"
                | "REPLICATION"
                | "NOREPLICATION"
                | "BYPASSRLS"
                | "NOBYPASSRLS"
        );
        if !valid {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid role_attr_flags value: {}", flag),
            ));
        }
        flags.push(flag);
    }
    Ok(flags)
}

fn create_user(params: &Params, check_mode: bool) -> Result<bool> {
    if user_exists(params)? {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = build_psql_base_args(params);
    args.push("postgres".to_string());

    let mut create_cmd = format!("CREATE ROLE \"{}\"", params.name);

    if let Some(ref password) = params.password {
        if params.encrypted {
            create_cmd.push_str(&format!(" ENCRYPTED PASSWORD '{}'", password));
        } else {
            create_cmd.push_str(&format!(" UNENCRYPTED PASSWORD '{}'", password));
        }
    }

    if let Some(ref flags_str) = params.role_attr_flags {
        let flags = parse_role_attr_flags(flags_str)?;
        for flag in flags {
            create_cmd.push_str(&format!(" {}", flag));
        }
    }

    args.push("-c".to_string());
    args.push(create_cmd);

    trace!("Creating user: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create user: {}", stderr),
        ));
    }

    Ok(true)
}

fn drop_user(params: &Params, check_mode: bool) -> Result<bool> {
    if !user_exists(params)? {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = build_psql_base_args(params);
    args.extend(vec![
        "postgres".to_string(),
        "-c".to_string(),
        format!("DROP ROLE \"{}\"", params.name),
    ]);

    trace!("Dropping user: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to drop user: {}", stderr),
        ));
    }

    Ok(true)
}

#[derive(Debug)]
pub struct PostgresqlUser;

impl Module for PostgresqlUser {
    fn get_name(&self) -> &str {
        "postgresql_user"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let changed = match params.state {
            State::Present => create_user(&params, check_mode)?,
            State::Absent => drop_user(&params, check_mode)?,
        };

        let extra = Some(value::to_value(json!({
            "user": params.name,
            "state": params.state.to_string(),
            "role_attr_flags": params.role_attr_flags,
        }))?);

        Ok((
            ModuleResult::new(
                changed,
                extra,
                Some(format!("User '{}' is {}", params.name, params.state)),
            ),
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
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_user, "postgres");
        assert_eq!(params.port, 5432);
        assert!(params.encrypted);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml = r#"
name: app_admin
password: secret
encrypted: true
role_attr_flags: CREATEDB,NOSUPERUSER
state: present
login_host: db.example.com
login_user: admin
login_password: admin_secret
port: 5433
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "app_admin");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.password, Some("secret".to_string()));
        assert!(params.encrypted);
        assert_eq!(
            params.role_attr_flags,
            Some("CREATEDB,NOSUPERUSER".to_string())
        );
        assert_eq!(params.login_host, "db.example.com");
        assert_eq!(params.login_user, "admin");
        assert_eq!(params.login_password, Some("admin_secret".to_string()));
        assert_eq!(params.port, 5433);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = r#"
name: old_user
state: absent
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "old_user");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_unencrypted_password() {
        let yaml = r#"
name: plain_user
password: plain_password
encrypted: false
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "plain_user");
        assert_eq!(params.password, Some("plain_password".to_string()));
        assert!(!params.encrypted);
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Present.to_string(), "present");
        assert_eq!(State::Absent.to_string(), "absent");
    }

    #[test]
    fn test_parse_role_attr_flags_valid() {
        let flags = parse_role_attr_flags("SUPERUSER,CREATEDB,LOGIN").unwrap();
        assert_eq!(flags, vec!["SUPERUSER", "CREATEDB", "LOGIN"]);

        let flags = parse_role_attr_flags("NOSUPERUSER,NOCREATEDB,NOLOGIN").unwrap();
        assert_eq!(flags, vec!["NOSUPERUSER", "NOCREATEDB", "NOLOGIN"]);
    }

    #[test]
    fn test_parse_role_attr_flags_invalid() {
        let result = parse_role_attr_flags("INVALID_FLAG");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_role_attr_flags_whitespace() {
        let flags = parse_role_attr_flags("  SUPERUSER , CREATEDB  , LOGIN  ").unwrap();
        assert_eq!(flags, vec!["SUPERUSER", "CREATEDB", "LOGIN"]);
    }
}
