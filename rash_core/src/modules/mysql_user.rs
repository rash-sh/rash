/// ANCHOR: module
/// # mysql_user
///
/// Manage MySQL/MariaDB database users.
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
/// - name: Create database user
///   mysql_user:
///     name: app_user
///     password: secret_password
///     state: present
///
/// - name: Create user with specific host and privileges
///   mysql_user:
///     name: app_user
///     password: secret_password
///     host: "%"
///     priv: "app_db.*:SELECT,INSERT,UPDATE"
///     state: present
///
/// - name: Create user with login credentials
///   mysql_user:
///     login_user: root
///     login_password: root_password
///     name: app_user
///     password: app_password
///     state: present
///
/// - name: Drop database user
///   mysql_user:
///     name: old_user
///     host: "%"
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{parse_params, Module, ModuleResult};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::value;
use serde_norway::Value as YamlValue;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    3306
}

fn default_host() -> String {
    "localhost".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the user to add or remove.
    pub name: String,
    /// Password for the user.
    pub password: Option<String>,
    /// The user state.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Host part of the user.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_host")]
    pub host: String,
    /// Privileges to grant (format: "db.table:priv1,priv2" or "db.*:ALL").
    #[serde(rename = "priv")]
    pub privileges: Option<String>,
    /// Database host to connect to.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// Database user to connect with.
    pub login_user: Option<String>,
    /// Database password to use.
    pub login_password: Option<String>,
    /// Database port to connect to.
    /// **[default: `3306`]**
    #[serde(default = "default_login_port")]
    pub login_port: u16,
    /// MySQL config file to read credentials from.
    pub config_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserInfo {
    pub name: String,
    pub host: String,
}

fn build_mysql_base_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    args.push(format!("--host={}", params.login_host));
    args.push(format!("--port={}", params.login_port));

    if let Some(ref user) = params.login_user {
        args.push(format!("--user={}", user));
    }

    if let Some(ref password) = params.login_password {
        args.push(format!("--password={}", password));
    }

    if let Some(ref config_file) = params.config_file {
        args.push(format!("--defaults-file={}", config_file));
    }

    args
}

fn user_exists(params: &Params) -> Result<Option<UserInfo>> {
    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.args([
        "--batch",
        "--skip-column-names",
        "-e",
        &format!(
            "SELECT User, Host FROM mysql.user WHERE User = '{}' AND Host = '{}'",
            escape_sql_string(&params.name),
            escape_sql_string(&params.host)
        ),
    ]);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("MySQL query failed: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();

    if line.is_empty() {
        return Ok(None);
    }

    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() >= 2 {
        Ok(Some(UserInfo {
            name: parts[0].to_string(),
            host: parts[1].to_string(),
        }))
    } else {
        Ok(None)
    }
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
        .replace('\\', "\\\\")
        .replace('\0', "\\0")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\x1a', "\\Z")
}

fn create_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would create user '{}'@'{}'",
                params.name, params.host
            )),
            extra: None,
        });
    }

    let password_clause = match &params.password {
        Some(pw) => format!(" IDENTIFIED BY '{}'", escape_sql_string(pw)),
        None => String::new(),
    };

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(format!(
        "CREATE USER '{}'@'{}'{}",
        escape_sql_string(&params.name),
        escape_sql_string(&params.host),
        password_clause
    ));

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create user: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("User '{}'@'{}' created", params.name, params.host)),
        extra: None,
    })
}

fn drop_user(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would drop user '{}'@'{}'",
                params.name, params.host
            )),
            extra: None,
        });
    }

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(format!(
        "DROP USER IF EXISTS '{}'@'{}'",
        escape_sql_string(&params.name),
        escape_sql_string(&params.host)
    ));

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to drop user: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("User '{}'@'{}' dropped", params.name, params.host)),
        extra: None,
    })
}

fn parse_priv_string(priv_str: &str) -> Result<(String, String, Vec<String>)> {
    let parts: Vec<&str> = priv_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid privilege format '{}'. Expected 'db.table:priv1,priv2'",
                priv_str
            ),
        ));
    }

    let db_table = parts[0].to_string();
    let privs: Vec<String> = parts[1].split(',').map(|s| s.trim().to_string()).collect();

    let db_table_parts: Vec<&str> = db_table.splitn(2, '.').collect();
    if db_table_parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid db.table format '{}'. Expected 'db.table' or 'db.*'",
                db_table
            ),
        ));
    }

    Ok((
        db_table_parts[0].to_string(),
        db_table_parts[1].to_string(),
        privs,
    ))
}

fn grant_privileges(params: &Params, check_mode: bool) -> Result<()> {
    let priv_str = params.privileges.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "privileges is required when granting privileges",
        )
    })?;

    let (db, table, privs) = parse_priv_string(priv_str)?;

    if check_mode {
        return Ok(());
    }

    let priv_str = privs.join(", ");
    let grant_stmt = if table == "*" {
        format!(
            "GRANT {} ON {}.* TO '{}'@'{}'",
            priv_str,
            escape_sql_string(&db),
            escape_sql_string(&params.name),
            escape_sql_string(&params.host)
        )
    } else {
        format!(
            "GRANT {} ON {}.{} TO '{}'@'{}'",
            priv_str,
            escape_sql_string(&db),
            escape_sql_string(&table),
            escape_sql_string(&params.name),
            escape_sql_string(&params.host)
        )
    };

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(&grant_stmt);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to grant privileges: {}", stderr),
        ));
    }

    Ok(())
}

fn update_password(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would update password for user '{}'@'{}'",
                params.name, params.host
            )),
            extra: None,
        });
    }

    let password = params
        .password
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "password is required when updating"))?;

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(format!(
        "ALTER USER '{}'@'{}' IDENTIFIED BY '{}'",
        escape_sql_string(&params.name),
        escape_sql_string(&params.host),
        escape_sql_string(password)
    ));

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to update password: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Password updated for user '{}'@'{}'",
            params.name, params.host
        )),
        extra: None,
    })
}

fn mysql_user_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => {
            let existing = user_exists(&params)?;

            match existing {
                None => {
                    let result = create_user(&params, check_mode)?;
                    if params.privileges.is_some() {
                        grant_privileges(&params, check_mode)?;
                    }
                    let extra = Some(value::to_value(json!({
                        "user": params.name,
                        "host": params.host,
                        "priv": params.privileges,
                    }))?);
                    Ok(ModuleResult {
                        changed: true,
                        output: result.output,
                        extra,
                    })
                }
                Some(_) => {
                    let mut changed = false;
                    let mut messages = Vec::new();

                    if params.password.is_some() {
                        let result = update_password(&params, check_mode)?;
                        if result.changed {
                            changed = true;
                            if let Some(msg) = result.output {
                                messages.push(msg);
                            }
                        }
                    }

                    if params.privileges.is_some() {
                        grant_privileges(&params, check_mode)?;
                        changed = true;
                        messages.push(format!(
                            "Privileges granted to '{}'@'{}'",
                            params.name, params.host
                        ));
                    }

                    let extra = Some(value::to_value(json!({
                        "user": params.name,
                        "host": params.host,
                        "priv": params.privileges,
                    }))?);

                    if changed {
                        Ok(ModuleResult {
                            changed: true,
                            output: Some(messages.join("; ")),
                            extra,
                        })
                    } else {
                        Ok(ModuleResult {
                            changed: false,
                            output: Some(format!(
                                "User '{}'@'{}' already exists",
                                params.name, params.host
                            )),
                            extra,
                        })
                    }
                }
            }
        }
        State::Absent => {
            let existing = user_exists(&params)?;

            match existing {
                None => Ok(ModuleResult {
                    changed: false,
                    output: Some(format!(
                        "User '{}'@'{}' does not exist",
                        params.name, params.host
                    )),
                    extra: None,
                }),
                Some(_) => drop_user(&params, check_mode),
            }
        }
    }
}

#[derive(Debug)]
pub struct MysqlUser;

impl Module for MysqlUser {
    fn get_name(&self) -> &str {
        "mysql_user"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mysql_user_impl(params, check_mode)?, None))
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
            name: app_user
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "app_user");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.host, "localhost");
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 3306);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app_user
            password: secret
            host: "%"
            priv: "app_db.*:SELECT,INSERT,UPDATE"
            state: present
            login_host: 192.168.1.100
            login_user: admin
            login_password: admin_secret
            login_port: 3307
            config_file: /etc/mysql/debian.cnf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "app_user");
        assert_eq!(params.password, Some("secret".to_string()));
        assert_eq!(params.host, "%");
        assert_eq!(
            params.privileges,
            Some("app_db.*:SELECT,INSERT,UPDATE".to_string())
        );
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "192.168.1.100");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("admin_secret".to_string()));
        assert_eq!(params.login_port, 3307);
        assert_eq!(
            params.config_file,
            Some("/etc/mysql/debian.cnf".to_string())
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_user
            host: "%"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_user");
        assert_eq!(params.host, "%");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("normal"), "normal");
        assert_eq!(escape_sql_string("with'quote"), "with''quote");
        assert_eq!(escape_sql_string("with\\backslash"), "with\\\\backslash");
        assert_eq!(escape_sql_string("with'both\\chars"), "with''both\\\\chars");
    }

    #[test]
    fn test_parse_priv_string() {
        let (db, table, privs) = parse_priv_string("app_db.*:SELECT,INSERT,UPDATE").unwrap();
        assert_eq!(db, "app_db");
        assert_eq!(table, "*");
        assert_eq!(privs, vec!["SELECT", "INSERT", "UPDATE"]);

        let (db, table, privs) = parse_priv_string("mydb.mytable:ALL").unwrap();
        assert_eq!(db, "mydb");
        assert_eq!(table, "mytable");
        assert_eq!(privs, vec!["ALL"]);
    }

    #[test]
    fn test_parse_priv_string_invalid() {
        let result = parse_priv_string("invalid_format");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_mysql_base_args() {
        let params = Params {
            name: "app_user".to_string(),
            password: Some("secret".to_string()),
            state: State::Present,
            host: "%".to_string(),
            privileges: None,
            login_host: "192.168.1.100".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("admin_secret".to_string()),
            login_port: 3307,
            config_file: None,
        };
        let args = build_mysql_base_args(&params);
        assert!(args.contains(&"--host=192.168.1.100".to_string()));
        assert!(args.contains(&"--port=3307".to_string()));
        assert!(args.contains(&"--user=admin".to_string()));
        assert!(args.contains(&"--password=admin_secret".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app_user
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
