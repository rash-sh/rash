/// ANCHOR: module
/// # postgresql_query
///
/// Execute SQL queries against PostgreSQL databases.
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
/// - name: Create application schema
///   postgresql_query:
///     database: myapp
///     query: "CREATE SCHEMA app_schema"
///     login_user: postgres
///     login_password: "{{ pg_pass }}"
///
/// - name: Run migration script
///   postgresql_query:
///     database: myapp
///     query: "{{ lookup('file', 'migrations/v1.sql') }}"
///     login_user: appuser
///     login_password: "{{ app_pass }}"
///
/// - name: Create extension
///   postgresql_query:
///     database: myapp
///     query: "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\""
///     login_user: postgres
///
/// - name: Query with SSL
///   postgresql_query:
///     database: myapp
///     query: "SELECT * FROM users LIMIT 10"
///     login_host: db.example.com
///     login_user: admin
///     login_password: secret
///     ssl_mode: require
///
/// - name: Query using unix socket
///   postgresql_query:
///     database: myapp
///     query: "SELECT version()"
///     login_user: postgres
///     login_unix_socket: /var/run/postgresql
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

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    5432
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// SQL query to execute.
    pub query: String,
    /// Database name to connect to.
    /// **[default: `"postgres"`]**
    #[serde(default = "default_database")]
    pub database: String,
    /// Host running the database.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_login_host")]
    pub login_host: String,
    /// The username to authenticate with.
    pub login_user: Option<String>,
    /// The password to authenticate with.
    pub login_password: Option<String>,
    /// Database port to connect to.
    /// **[default: `5432`]**
    #[serde(default = "default_login_port")]
    pub login_port: u16,
    /// Path to a Unix domain socket for local connections.
    pub login_unix_socket: Option<String>,
    /// SSL mode for the connection.
    pub ssl_mode: Option<String>,
    /// Path to SSL client certificate.
    pub ssl_cert: Option<String>,
    /// Path to SSL client key.
    pub ssl_key: Option<String>,
    /// Execute query in a single transaction.
    /// **[default: `false`]**
    #[serde(default)]
    pub single_transaction: bool,
}

fn default_database() -> String {
    "postgres".to_string()
}

fn build_env(params: &Params) -> Vec<(String, String)> {
    let mut env = Vec::new();

    if let Some(password) = &params.login_password {
        env.push(("PGPASSWORD".to_string(), password.clone()));
    }

    env
}

fn build_psql_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ref socket) = params.login_unix_socket {
        args.push("-h".to_string());
        args.push(socket.clone());
    } else {
        args.push("-h".to_string());
        args.push(params.login_host.clone());
    }

    args.push("-p".to_string());
    args.push(params.login_port.to_string());

    if let Some(ref user) = params.login_user {
        args.push("-U".to_string());
        args.push(user.clone());
    }

    if let Some(ref ssl_mode) = params.ssl_mode {
        args.push(format!("sslmode={}", ssl_mode));
    }

    if let Some(ref ssl_cert) = params.ssl_cert {
        args.push(format!("sslcert={}", ssl_cert));
    }

    if let Some(ref ssl_key) = params.ssl_key {
        args.push(format!("sslkey={}", ssl_key));
    }

    args.push(params.database.clone());

    args
}

fn execute_query(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!(
                "Would execute query on database '{}': {}",
                params.database,
                truncate_query(&params.query)
            )),
            extra: None,
        });
    }

    let mut cmd = Command::new("psql");
    cmd.args(build_psql_args(params));
    cmd.envs(build_env(params));

    if params.single_transaction {
        cmd.arg("--single-transaction");
    }

    cmd.arg("-c");
    cmd.arg(&params.query);

    trace!("Executing query: psql {:?}", cmd);

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::new(
                ErrorKind::NotFound,
                "psql command not found. Please install PostgreSQL client.",
            )
        } else {
            Error::new(ErrorKind::SubprocessFail, e)
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("PostgreSQL query failed: {}", stderr),
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

#[derive(Debug)]
pub struct PostgresqlQuery;

impl Module for PostgresqlQuery {
    fn get_name(&self) -> &str {
        "postgresql_query"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((execute_query(&params, check_mode)?, None))
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
query: "SELECT 1"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();
        assert_eq!(params.query, "SELECT 1");
        assert_eq!(params.database, "postgres");
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 5432);
        assert!(!params.single_transaction);
        assert!(params.login_user.is_none());
        assert!(params.login_password.is_none());
        assert!(params.ssl_mode.is_none());
    }

    #[test]
    fn test_parse_params_full() {
        let yaml = r#"
query: "CREATE SCHEMA app_schema"
database: myapp
login_host: db.example.com
login_user: admin
login_password: secret
login_port: 5433
ssl_mode: require
ssl_cert: /etc/ssl/certs/client.crt
ssl_key: /etc/ssl/private/client.key
single_transaction: true
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();
        assert_eq!(params.query, "CREATE SCHEMA app_schema");
        assert_eq!(params.database, "myapp");
        assert_eq!(params.login_host, "db.example.com");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
        assert_eq!(params.login_port, 5433);
        assert_eq!(params.ssl_mode, Some("require".to_string()));
        assert_eq!(
            params.ssl_cert,
            Some("/etc/ssl/certs/client.crt".to_string())
        );
        assert_eq!(
            params.ssl_key,
            Some("/etc/ssl/private/client.key".to_string())
        );
        assert!(params.single_transaction);
    }

    #[test]
    fn test_parse_params_with_database() {
        let yaml = r#"
query: "SELECT * FROM users"
database: myapp
login_user: appuser
login_password: secret
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();
        assert_eq!(params.query, "SELECT * FROM users");
        assert_eq!(params.database, "myapp");
        assert_eq!(params.login_user, Some("appuser".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_unix_socket() {
        let yaml = r#"
query: "SELECT version()"
database: myapp
login_user: postgres
login_unix_socket: /var/run/postgresql
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();
        assert_eq!(
            params.login_unix_socket,
            Some("/var/run/postgresql".to_string())
        );
    }

    #[test]
    fn test_parse_params_ssl_modes() {
        for mode in &[
            "disable",
            "allow",
            "prefer",
            "require",
            "verify-ca",
            "verify-full",
        ] {
            let yaml = format!("query: \"SELECT 1\"\nssl_mode: {}", mode);
            let value: YamlValue = from_str(&yaml).unwrap();
            let params: Params = parse_params(value).unwrap();
            assert_eq!(params.ssl_mode, Some(mode.to_string()));
        }
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml = r#"
query: "SELECT 1"
unknown: field
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let error = parse_params::<Params>(value).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_query() {
        let yaml = r#"
database: myapp
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let error = parse_params::<Params>(value).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_build_psql_args_host() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: "myapp".to_string(),
            login_host: "db.example.com".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 5433,
            login_unix_socket: None,
            ssl_mode: Some("require".to_string()),
            ssl_cert: None,
            ssl_key: None,
            single_transaction: false,
        };
        let args = build_psql_args(&params);
        assert!(args.contains(&"-h".to_string()));
        assert!(args.contains(&"db.example.com".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"5433".to_string()));
        assert!(args.contains(&"-U".to_string()));
        assert!(args.contains(&"admin".to_string()));
        assert!(args.contains(&"sslmode=require".to_string()));
        assert!(args.contains(&"myapp".to_string()));
    }

    #[test]
    fn test_build_psql_args_socket() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: "myapp".to_string(),
            login_host: "localhost".to_string(),
            login_user: Some("postgres".to_string()),
            login_password: None,
            login_port: 5432,
            login_unix_socket: Some("/var/run/postgresql".to_string()),
            ssl_mode: None,
            ssl_cert: None,
            ssl_key: None,
            single_transaction: false,
        };
        let args = build_psql_args(&params);
        assert!(args.contains(&"-h".to_string()));
        assert!(args.contains(&"/var/run/postgresql".to_string()));
        assert!(!args.contains(&"localhost".to_string()));
    }

    #[test]
    fn test_build_psql_args_ssl_cert_key() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: "myapp".to_string(),
            login_host: "db.example.com".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 5432,
            login_unix_socket: None,
            ssl_mode: Some("verify-full".to_string()),
            ssl_cert: Some("/etc/ssl/certs/client.crt".to_string()),
            ssl_key: Some("/etc/ssl/private/client.key".to_string()),
            single_transaction: false,
        };
        let args = build_psql_args(&params);
        assert!(args.contains(&"sslcert=/etc/ssl/certs/client.crt".to_string()));
        assert!(args.contains(&"sslkey=/etc/ssl/private/client.key".to_string()));
    }

    #[test]
    fn test_build_env_with_password() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: "myapp".to_string(),
            login_host: "localhost".to_string(),
            login_user: Some("postgres".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 5432,
            login_unix_socket: None,
            ssl_mode: None,
            ssl_cert: None,
            ssl_key: None,
            single_transaction: false,
        };
        let env = build_env(&params);
        assert!(env.contains(&("PGPASSWORD".to_string(), "secret".to_string())));
    }

    #[test]
    fn test_build_env_without_password() {
        let params = Params {
            query: "SELECT 1".to_string(),
            database: "myapp".to_string(),
            login_host: "localhost".to_string(),
            login_user: Some("postgres".to_string()),
            login_password: None,
            login_port: 5432,
            login_unix_socket: None,
            ssl_mode: None,
            ssl_cert: None,
            ssl_key: None,
            single_transaction: false,
        };
        let env = build_env(&params);
        assert!(env.is_empty());
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
}
