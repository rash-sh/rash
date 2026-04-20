/// ANCHOR: module
/// # mysql_query
///
/// Execute SQL queries against MySQL/MariaDB databases.
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
/// - name: Create application user
///   mysql_query:
///     query: "CREATE USER 'appuser'@'localhost' IDENTIFIED BY 'secret'"
///     login_user: root
///     login_password: "{{ mysql_root_pass }}"
///
/// - name: Grant permissions
///   mysql_query:
///     database: myapp
///     query: "GRANT ALL ON myapp.* TO 'appuser'@'localhost'"
///     login_user: root
///     login_password: "{{ mysql_root_pass }}"
///
/// - name: Run migration script
///   mysql_query:
///     database: myapp
///     query: "{{ lookup('file', 'migrations/v1.sql') }}"
///     login_user: appuser
///     login_password: "{{ app_pass }}"
///     single_transaction: true
///
/// - name: Query with unix socket
///   mysql_query:
///     query: "SELECT * FROM users LIMIT 10"
///     database: myapp
///     login_user: root
///     login_unix_socket: /var/run/mysqld/mysqld.sock
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

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    3306
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// SQL query to execute.
    pub query: String,
    /// Database name to connect to.
    pub database: Option<String>,
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
    /// Unix socket path to connect to.
    pub login_unix_socket: Option<String>,
    /// Execute query in a single transaction.
    /// **[default: `false`]**
    #[serde(default)]
    pub single_transaction: bool,
    /// MySQL config file to read credentials from.
    pub config_file: Option<String>,
}

fn build_mysql_base_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ref config_file) = params.config_file {
        args.push(format!("--defaults-file={}", config_file));
    }

    if let Some(ref socket) = params.login_unix_socket {
        args.push(format!("--socket={}", socket));
    } else {
        args.push(format!("--host={}", params.login_host));
        args.push(format!("--port={}", params.login_port));
    }

    if let Some(ref user) = params.login_user {
        args.push(format!("--user={}", user));
    }

    if let Some(ref password) = params.login_password {
        args.push(format!("--password={}", password));
    }

    args
}

fn execute_query(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        let db_msg = params
            .database
            .as_ref()
            .map(|d| format!(" on database '{}'", d))
            .unwrap_or_default();
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "Would execute query{}: {}",
                db_msg,
                truncate_query(&params.query)
            )),
            extra: None,
        });
    }

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));

    if params.single_transaction {
        cmd.arg("--init-command=START TRANSACTION");
    }

    if let Some(ref database) = params.database {
        cmd.arg(database);
    }

    cmd.arg("-e");
    cmd.arg(&params.query);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("MySQL query failed: {}", stderr),
        ));
    }

    let row_count = if stdout.is_empty() {
        0
    } else {
        stdout.lines().count().saturating_sub(1)
    };

    let changed = true;

    let extra = Some(value::to_value(json!({
        "query": params.query,
        "database": params.database,
        "row_count": row_count,
        "stdout": stdout,
        "stderr": stderr,
    }))?);

    Ok(ModuleResult {
        changed,
        output: Some(if stdout.is_empty() {
            "Query executed successfully".to_string()
        } else {
            stdout
        }),
        extra,
    })
}

fn truncate_query(query: &str) -> String {
    let chars: String = query.chars().take(100).collect();
    if query.chars().count() > 100 {
        format!("{}...", chars)
    } else {
        query.to_string()
    }
}

fn mysql_query_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    execute_query(&params, check_mode)
}

#[derive(Debug)]
pub struct MysqlQuery;

impl Module for MysqlQuery {
    fn get_name(&self) -> &str {
        "mysql_query"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mysql_query_impl(params, check_mode)?, None))
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
            query: "SELECT 1"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.query, "SELECT 1");
        assert_eq!(params.database, None);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 3306);
        assert!(!params.single_transaction);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            query: "CREATE USER 'appuser'@'localhost' IDENTIFIED BY 'secret'"
            database: myapp
            login_host: 192.168.1.100
            login_user: root
            login_password: root_pass
            login_port: 3307
            login_unix_socket: /var/run/mysqld/mysqld.sock
            single_transaction: true
            config_file: /etc/mysql/debian.cnf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.query,
            "CREATE USER 'appuser'@'localhost' IDENTIFIED BY 'secret'"
        );
        assert_eq!(params.database, Some("myapp".to_string()));
        assert_eq!(params.login_host, "192.168.1.100");
        assert_eq!(params.login_user, Some("root".to_string()));
        assert_eq!(params.login_password, Some("root_pass".to_string()));
        assert_eq!(params.login_port, 3307);
        assert_eq!(
            params.login_unix_socket,
            Some("/var/run/mysqld/mysqld.sock".to_string())
        );
        assert!(params.single_transaction);
        assert_eq!(
            params.config_file,
            Some("/etc/mysql/debian.cnf".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_database() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            query: "SELECT * FROM users"
            database: myapp
            login_user: appuser
            login_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.query, "SELECT * FROM users");
        assert_eq!(params.database, Some("myapp".to_string()));
    }

    #[test]
    fn test_parse_params_single_transaction() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            query: "UPDATE users SET active = 1"
            database: myapp
            single_transaction: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.single_transaction);
    }

    #[test]
    fn test_build_mysql_base_args_host() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: None,
            login_host: "192.168.1.100".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3307,
            login_unix_socket: None,
            single_transaction: false,
            config_file: None,
        };
        let args = build_mysql_base_args(&params);
        assert!(args.contains(&"--host=192.168.1.100".to_string()));
        assert!(args.contains(&"--port=3307".to_string()));
        assert!(args.contains(&"--user=admin".to_string()));
        assert!(args.contains(&"--password=secret".to_string()));
    }

    #[test]
    fn test_build_mysql_base_args_socket() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: None,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: None,
            login_port: 3306,
            login_unix_socket: Some("/var/run/mysqld/mysqld.sock".to_string()),
            single_transaction: false,
            config_file: None,
        };
        let args = build_mysql_base_args(&params);
        assert!(args.contains(&"--socket=/var/run/mysqld/mysqld.sock".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--host=")));
        assert!(!args.iter().any(|a| a.starts_with("--port=")));
    }

    #[test]
    fn test_build_mysql_base_args_config_file_first() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: None,
            login_host: "192.168.1.100".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3307,
            login_unix_socket: None,
            single_transaction: false,
            config_file: Some("/etc/mysql/debian.cnf".to_string()),
        };
        let args = build_mysql_base_args(&params);
        assert_eq!(args[0], "--defaults-file=/etc/mysql/debian.cnf".to_string());
    }

    #[test]
    fn test_truncate_query_short() {
        assert_eq!(truncate_query("SELECT 1"), "SELECT 1");
    }

    #[test]
    fn test_truncate_query_long() {
        let long_query = "A".repeat(200);
        let result = truncate_query(&long_query);
        assert!(result.ends_with("..."));
        assert_eq!(result.len(), 103);
    }

    #[test]
    fn test_truncate_query_multibyte() {
        let long_query = "日本語".repeat(50);
        let result = truncate_query(&long_query);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().take(100).count(), 100);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            query: "SELECT 1"
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: myapp
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
