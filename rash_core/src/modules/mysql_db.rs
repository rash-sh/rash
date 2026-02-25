/// ANCHOR: module
/// # mysql_db
///
/// Manage MySQL/MariaDB databases.
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
/// - name: Create database
///   mysql_db:
///     name: myapp
///     state: present
///     encoding: utf8mb4
///     collation: utf8mb4_unicode_ci
///
/// - name: Create database with specific credentials
///   mysql_db:
///     name: myapp
///     state: present
///     login_user: root
///     login_password: secret
///     login_host: localhost
///     login_port: 3306
///
/// - name: Dump database to file
///   mysql_db:
///     name: myapp
///     state: dump
///     target: /backup/myapp.sql
///
/// - name: Import database from file
///   mysql_db:
///     name: myapp
///     state: import
///     target: /backup/myapp.sql
///
/// - name: Drop database
///   mysql_db:
///     name: oldapp
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
use std::path::Path;
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the database to manage.
    pub name: String,
    /// The database state.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// The database encoding.
    /// **[default: `"utf8"`]**
    pub encoding: Option<String>,
    /// The database collation.
    pub collation: Option<String>,
    /// File to dump/import database to/from (required for dump/import states).
    pub target: Option<String>,
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
    /// Use single transaction for dump (no table locking).
    /// **[default: `true` for dump]**
    #[serde(default)]
    pub single_transaction: Option<bool>,
    /// Use quick option for dump (retrieve rows one at a time).
    /// **[default: `true` for dump]**
    #[serde(default)]
    pub quick: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
    Dump,
    Import,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseInfo {
    pub name: String,
    pub encoding: Option<String>,
    pub collation: Option<String>,
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

fn database_exists(params: &Params) -> Result<Option<DatabaseInfo>> {
    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.args([
        "--batch",
        "--skip-column-names",
        "-e",
        &format!(
            "SELECT SCHEMA_NAME, DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME \
             FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = '{}'",
            params.name
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
    if parts.len() >= 3 {
        Ok(Some(DatabaseInfo {
            name: parts[0].to_string(),
            encoding: Some(parts[1].to_string()),
            collation: Some(parts[2].to_string()),
        }))
    } else {
        Ok(None)
    }
}

fn create_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would create database '{}'", params.name)),
            extra: None,
        });
    }

    let mut sql = format!("CREATE DATABASE `{}`", params.name);

    if let Some(ref encoding) = params.encoding {
        sql.push_str(&format!(" CHARACTER SET {}", encoding));
    }

    if let Some(ref collation) = params.collation {
        sql.push_str(&format!(" COLLATE {}", collation));
    }

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(&sql);

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
            format!("Failed to create database: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "encoding": params.encoding,
        "collation": params.collation,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Database '{}' created", params.name)),
        extra,
    })
}

fn drop_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would drop database '{}'", params.name)),
            extra: None,
        });
    }

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(format!("DROP DATABASE `{}`", params.name));

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
            format!("Failed to drop database: {}", stderr),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Database '{}' dropped", params.name)),
        extra: None,
    })
}

fn dump_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let target = params
        .target
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "target is required for dump state"))?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would dump database '{}' to '{}'",
                params.name, target
            )),
            extra: None,
        });
    }

    let mut cmd = Command::new("mysqldump");
    cmd.args(build_mysql_base_args(params));

    let single_transaction = params.single_transaction.unwrap_or(true);
    if single_transaction {
        cmd.arg("--single-transaction");
    }

    let quick = params.quick.unwrap_or(true);
    if quick {
        cmd.arg("--quick");
    }

    cmd.arg("--result-file");
    cmd.arg(target);
    cmd.arg(&params.name);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysqldump: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to dump database: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "target": target,
        "single_transaction": single_transaction,
        "quick": quick,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Database '{}' dumped to '{}'", params.name, target)),
        extra,
    })
}

fn import_database(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let target = params.target.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "target is required for import state",
        )
    })?;

    if !Path::new(target).exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Import file '{}' does not exist", target),
        ));
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would import database '{}' from '{}'",
                params.name, target
            )),
            extra: None,
        });
    }

    let file_content = std::fs::read_to_string(target).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to read import file '{}': {}", target, e),
        )
    })?;

    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg(&params.name);

    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute mysql: {}", e),
            )
        })?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(file_content.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write to mysql stdin: {}", e),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to wait for mysql: {}", e),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to import database: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "db": params.name,
        "target": target,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Database '{}' imported from '{}'",
            params.name, target
        )),
        extra,
    })
}

fn mysql_db_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => {
            let existing = database_exists(&params)?;

            match existing {
                None => create_database(&params, check_mode),
                Some(db_info) => {
                    let encoding_match = params
                        .encoding
                        .as_ref()
                        .map(|e| db_info.encoding.as_ref() == Some(e))
                        .unwrap_or(true);

                    let collation_match = params
                        .collation
                        .as_ref()
                        .map(|c| db_info.collation.as_ref() == Some(c))
                        .unwrap_or(true);

                    if encoding_match && collation_match {
                        let extra = Some(value::to_value(json!({
                            "db": db_info.name,
                            "encoding": db_info.encoding,
                            "collation": db_info.collation,
                        }))?);

                        Ok(ModuleResult {
                            changed: false,
                            output: Some(format!("Database '{}' already exists", params.name)),
                            extra,
                        })
                    } else {
                        if check_mode {
                            return Ok(ModuleResult {
                                changed: true,
                                output: Some(format!(
                                    "Would modify database '{}' encoding/collation",
                                    params.name
                                )),
                                extra: None,
                            });
                        }

                        let mut alter_sql = format!("ALTER DATABASE `{}`", params.name);
                        let mut modifications = Vec::new();

                        if let Some(ref encoding) = params.encoding {
                            modifications.push(format!("CHARACTER SET {}", encoding));
                        }

                        if let Some(ref collation) = params.collation {
                            modifications.push(format!("COLLATE {}", collation));
                        }

                        if !modifications.is_empty() {
                            alter_sql.push(' ');
                            alter_sql.push_str(&modifications.join(" "));
                        }

                        let mut cmd = Command::new("mysql");
                        cmd.args(build_mysql_base_args(&params));
                        cmd.arg("-e");
                        cmd.arg(&alter_sql);

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
                                format!("Failed to alter database: {}", stderr),
                            ));
                        }

                        let extra = Some(value::to_value(json!({
                            "db": params.name,
                            "encoding": params.encoding,
                            "collation": params.collation,
                        }))?);

                        Ok(ModuleResult {
                            changed: true,
                            output: Some(format!("Database '{}' modified", params.name)),
                            extra,
                        })
                    }
                }
            }
        }
        State::Absent => {
            let existing = database_exists(&params)?;

            match existing {
                None => Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Database '{}' does not exist", params.name)),
                    extra: None,
                }),
                Some(_) => drop_database(&params, check_mode),
            }
        }
        State::Dump => dump_database(&params, check_mode),
        State::Import => import_database(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct MysqlDb;

impl Module for MysqlDb {
    fn get_name(&self) -> &str {
        "mysql_db"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mysql_db_impl(params, check_mode)?, None))
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
            name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 3306);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: present
            encoding: utf8mb4
            collation: utf8mb4_unicode_ci
            login_host: 192.168.1.100
            login_user: admin
            login_password: secret
            login_port: 3307
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.encoding, Some("utf8mb4".to_string()));
        assert_eq!(params.collation, Some("utf8mb4_unicode_ci".to_string()));
        assert_eq!(params.login_host, "192.168.1.100");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
        assert_eq!(params.login_port, 3307);
    }

    #[test]
    fn test_parse_params_dump() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: dump
            target: /backup/myapp.sql
            single_transaction: true
            quick: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Dump);
        assert_eq!(params.target, Some("/backup/myapp.sql".to_string()));
        assert_eq!(params.single_transaction, Some(true));
        assert_eq!(params.quick, Some(true));
    }

    #[test]
    fn test_parse_params_import() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: import
            target: /backup/myapp.sql
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Import);
        assert_eq!(params.target, Some("/backup/myapp.sql".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oldapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "oldapp");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_config_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            config_file: /etc/mysql/debian.cnf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.config_file,
            Some("/etc/mysql/debian.cnf".to_string())
        );
    }

    #[test]
    fn test_build_mysql_base_args() {
        let params = Params {
            name: "myapp".to_string(),
            state: State::Present,
            encoding: None,
            collation: None,
            target: None,
            login_host: "192.168.1.100".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3307,
            config_file: None,
            single_transaction: None,
            quick: None,
        };
        let args = build_mysql_base_args(&params);
        assert!(args.contains(&"--host=192.168.1.100".to_string()));
        assert!(args.contains(&"--port=3307".to_string()));
        assert!(args.contains(&"--user=admin".to_string()));
        assert!(args.contains(&"--password=secret".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
