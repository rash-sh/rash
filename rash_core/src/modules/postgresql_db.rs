/// ANCHOR: module
/// # postgresql_db
///
/// Add or remove PostgreSQL databases.
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
/// - name: Create a new database with name "myapp"
///   postgresql_db:
///     name: myapp
///     state: present
///
/// - name: Create a new database with owner and encoding
///   postgresql_db:
///     name: myapp
///     state: present
///     owner: appuser
///     encoding: UTF-8
///
/// - name: Dump database to a file
///   postgresql_db:
///     name: myapp
///     state: dump
///     target: /tmp/myapp.sql
///
/// - name: Dump database in custom format
///   postgresql_db:
///     name: myapp
///     state: dump
///     target: /tmp/myapp.dump
///     target_opts: "-Fc"
///
/// - name: Restore database from a file
///   postgresql_db:
///     name: myapp
///     state: restore
///     target: /tmp/myapp.sql
///
/// - name: Drop database
///   postgresql_db:
///     name: myapp
///     state: absent
///
/// - name: Connect to remote database
///   postgresql_db:
///     name: myapp
///     state: present
///     login_host: db.example.com
///     login_user: admin
///     login_password: secret
///     port: 5432
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
    Dump,
    Restore,
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Present => write!(f, "present"),
            State::Absent => write!(f, "absent"),
            State::Dump => write!(f, "dump"),
            State::Restore => write!(f, "restore"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the database to add or remove.
    pub name: String,
    /// The database state.
    #[serde(default)]
    pub state: State,
    /// Name of the role to set as owner of the database.
    pub owner: Option<String>,
    /// Template used to create the database.
    pub template: Option<String>,
    /// Encoding of the database.
    pub encoding: Option<String>,
    /// Collation order (LC_COLLATE) to use in the database.
    pub lc_collate: Option<String>,
    /// Character classification (LC_CTYPE) to use in the database.
    pub lc_ctype: Option<String>,
    /// File to backup or restore database.
    pub target: Option<String>,
    /// Additional arguments to pass to pg_dump/psql during dump/restore.
    pub target_opts: Option<String>,
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

fn database_exists(params: &Params) -> Result<bool> {
    let mut args = build_psql_base_args(params);
    args.extend(vec![
        "-t".to_string(),
        "-A".to_string(),
        "-c".to_string(),
        format!(
            "SELECT 1 FROM pg_database WHERE datname = '{}'",
            params.name
        ),
        "postgres".to_string(),
    ]);

    trace!("Checking if database exists: psql {:?}", args);

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
            format!("Failed to check database existence: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "1")
}

fn create_database(params: &Params, check_mode: bool) -> Result<bool> {
    if database_exists(params)? {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = build_psql_base_args(params);
    args.push("postgres".to_string());

    let mut create_cmd = format!("CREATE DATABASE \"{}\"", params.name);

    if let Some(ref owner) = params.owner {
        create_cmd.push_str(&format!(" OWNER \"{}\"", owner));
    }

    if let Some(ref template) = params.template {
        create_cmd.push_str(&format!(" TEMPLATE \"{}\"", template));
    }

    if let Some(ref encoding) = params.encoding {
        create_cmd.push_str(&format!(" ENCODING '{}'", encoding));
    }

    if let Some(ref lc_collate) = params.lc_collate {
        create_cmd.push_str(&format!(" LC_COLLATE '{}'", lc_collate));
    }

    if let Some(ref lc_ctype) = params.lc_ctype {
        create_cmd.push_str(&format!(" LC_CTYPE '{}'", lc_ctype));
    }

    args.push("-c".to_string());
    args.push(create_cmd);

    trace!("Creating database: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create database: {}", stderr),
        ));
    }

    Ok(true)
}

fn drop_database(params: &Params, check_mode: bool) -> Result<bool> {
    if !database_exists(params)? {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = build_psql_base_args(params);
    args.extend(vec![
        "postgres".to_string(),
        "-c".to_string(),
        format!("DROP DATABASE \"{}\"", params.name),
    ]);

    trace!("Dropping database: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to drop database: {}", stderr),
        ));
    }

    Ok(true)
}

fn dump_database(params: &Params, check_mode: bool) -> Result<bool> {
    let target = params
        .target
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "target is required for dump state"))?;

    if !database_exists(params)? {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Database '{}' does not exist", params.name),
        ));
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = vec![
        "-h".to_string(),
        params.login_host.clone(),
        "-p".to_string(),
        params.port.to_string(),
        "-U".to_string(),
        params.login_user.clone(),
    ];

    if let Some(ref target_opts) = params.target_opts {
        for opt in target_opts.split_whitespace() {
            args.push(opt.to_string());
        }
    }

    args.push("-f".to_string());
    args.push(target.clone());

    args.push(params.name.clone());

    trace!("Dumping database: pg_dump {:?}", args);

    let output = Command::new("pg_dump")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::new(
                    ErrorKind::NotFound,
                    "pg_dump command not found. Please install PostgreSQL client.",
                )
            } else {
                Error::new(ErrorKind::SubprocessFail, e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to dump database: {}", stderr),
        ));
    }

    Ok(true)
}

fn restore_database(params: &Params, check_mode: bool) -> Result<bool> {
    let target = params
        .target
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::OmitParam, "target is required for restore state"))?;

    if !std::path::Path::new(target).exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Target file '{}' does not exist", target),
        ));
    }

    if check_mode {
        return Ok(true);
    }

    let exists = database_exists(params)?;

    let mut args = build_psql_base_args(params);

    if let Some(ref target_opts) = params.target_opts {
        for opt in target_opts.split_whitespace() {
            args.push(opt.to_string());
        }
    }

    args.push("-f".to_string());
    args.push(target.clone());

    if !exists {
        args.push("-c".to_string());
        args.push(format!("CREATE DATABASE \"{}\"", params.name));
    }

    args.push(params.name.clone());

    trace!("Restoring database: psql {:?}", args);

    let output = Command::new("psql")
        .args(&args)
        .envs(build_env(params))
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to restore database: {}", stderr),
        ));
    }

    Ok(true)
}

#[derive(Debug)]
pub struct PostgresqlDb;

impl Module for PostgresqlDb {
    fn get_name(&self) -> &str {
        "postgresql_db"
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
            State::Present => create_database(&params, check_mode)?,
            State::Absent => drop_database(&params, check_mode)?,
            State::Dump => dump_database(&params, check_mode)?,
            State::Restore => restore_database(&params, check_mode)?,
        };

        let extra = Some(value::to_value(json!({
            "db": params.name,
            "state": params.state.to_string(),
            "owner": params.owner,
        }))?);

        Ok((
            ModuleResult::new(
                changed,
                extra,
                Some(format!("Database '{}' is {}", params.name, params.state)),
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
name: myapp
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_user, "postgres");
        assert_eq!(params.port, 5432);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml = r#"
name: myapp
state: present
owner: appuser
template: template0
encoding: UTF-8
lc_collate: en_US.UTF-8
lc_ctype: en_US.UTF-8
login_host: db.example.com
login_user: admin
login_password: secret
port: 5433
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.owner, Some("appuser".to_string()));
        assert_eq!(params.template, Some("template0".to_string()));
        assert_eq!(params.encoding, Some("UTF-8".to_string()));
        assert_eq!(params.lc_collate, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.lc_ctype, Some("en_US.UTF-8".to_string()));
        assert_eq!(params.login_host, "db.example.com");
        assert_eq!(params.login_user, "admin");
        assert_eq!(params.login_password, Some("secret".to_string()));
        assert_eq!(params.port, 5433);
    }

    #[test]
    fn test_parse_params_dump() {
        let yaml = r#"
name: myapp
state: dump
target: /tmp/myapp.sql
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Dump);
        assert_eq!(params.target, Some("/tmp/myapp.sql".to_string()));
    }

    #[test]
    fn test_parse_params_restore_with_opts() {
        let yaml = r#"
name: myapp
state: restore
target: /tmp/myapp.dump
target_opts: "-v --single-transaction"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Restore);
        assert_eq!(params.target, Some("/tmp/myapp.dump".to_string()));
        assert_eq!(
            params.target_opts,
            Some("-v --single-transaction".to_string())
        );
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Present.to_string(), "present");
        assert_eq!(State::Absent.to_string(), "absent");
        assert_eq!(State::Dump.to_string(), "dump");
        assert_eq!(State::Restore.to_string(), "restore");
    }
}
