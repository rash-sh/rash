/// ANCHOR: module
/// # mysql_replication
///
/// Manage MySQL/MariaDB replication topology.
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
/// - name: Configure server as primary
///   mysql_replication:
///     mode: primary
///     state: present
///     login_user: root
///     login_password: "{{ vault_root_password }}"
///
/// - name: Configure server as replica
///   mysql_replication:
///     mode: replica
///     state: present
///     primary_host: db-primary.internal
///     primary_user: repl
///     primary_password: "{{ vault_repl_password }}"
///     primary_port: 3306
///     login_user: root
///     login_password: "{{ vault_root_password }}"
///
/// - name: Stop replication
///   mysql_replication:
///     mode: replica
///     state: absent
///     login_user: root
///     login_password: "{{ vault_root_password }}"
///
/// - name: Get primary status
///   mysql_replication:
///     state: getprimary
///     login_user: root
///     login_password: "{{ vault_root_password }}"
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
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_mode() -> Mode {
    Mode::Primary
}

fn default_login_host() -> String {
    "localhost".to_string()
}

fn default_login_port() -> u16 {
    3306
}

fn default_primary_port() -> u16 {
    3306
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The replication state.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Whether the server is a primary or replica.
    /// **[default: `"primary"`]**
    #[serde(default = "default_mode")]
    pub mode: Mode,
    /// Primary server hostname (required when mode=replica and state=present).
    pub primary_host: Option<String>,
    /// Replication user on the primary.
    pub primary_user: Option<String>,
    /// Replication user password on the primary.
    pub primary_password: Option<String>,
    /// Primary server port.
    /// **[default: `3306`]**
    #[serde(default = "default_primary_port")]
    pub primary_port: u16,
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
    Getprimary,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Primary,
    Replica,
}

fn build_mysql_base_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(ref config_file) = params.config_file {
        args.push(format!("--defaults-file={}", config_file));
    }

    args.push(format!("--host={}", params.login_host));
    args.push(format!("--port={}", params.login_port));

    if let Some(ref user) = params.login_user {
        args.push(format!("--user={}", user));
    }

    if let Some(ref password) = params.login_password {
        args.push(format!("--password={}", password));
    }

    args
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
        .replace('\\', "\\\\")
        .replace('\0', "\\0")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\x1a', "\\Z")
}

fn execute_mysql(params: &Params, sql: &str) -> Result<std::process::Output> {
    let mut cmd = Command::new("mysql");
    cmd.args(build_mysql_base_args(params));
    cmd.arg("-e");
    cmd.arg(sql);

    cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute mysql: {}", e),
        )
    })
}

fn check_primary_status(params: &Params) -> Result<bool> {
    let output = execute_mysql(params, "SHOW MASTER STATUS")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

fn check_replica_status(params: &Params) -> Result<bool> {
    let output = execute_mysql(params, "SHOW SLAVE STATUS\\G")?;

    if !output.status.success() {
        let output = execute_mysql(params, "SHOW REPLICA STATUS\\G")?;
        if !output.status.success() {
            return Ok(false);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(!stdout.trim().is_empty());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

fn configure_primary(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some("Would configure server as primary".to_string()),
            extra: None,
        });
    }

    let sql = "STOP SLAVE; RESET SLAVE ALL; STOP REPLICA; RESET REPLICA ALL";
    let _ = execute_mysql(params, sql);

    let output = execute_mysql(
        params,
        "SET GLOBAL read_only = 0; SET GLOBAL super_read_only = 0; SET GLOBAL binlog_format = 'ROW'",
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to configure primary: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "mode": "primary",
        "state": "present",
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some("Server configured as primary".to_string()),
        extra,
    })
}

fn configure_replica(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let primary_host = params.primary_host.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "primary_host is required when mode=replica",
        )
    })?;
    let primary_user = params.primary_user.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "primary_user is required when mode=replica",
        )
    })?;
    let primary_password = params.primary_password.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "primary_password is required when mode=replica",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would configure server as replica of '{}'",
                primary_host
            )),
            extra: None,
        });
    }

    let change_master_sql = format!(
        "CHANGE MASTER TO \
         MASTER_HOST='{}', \
         MASTER_USER='{}', \
         MASTER_PASSWORD='{}', \
         MASTER_PORT={}, \
         MASTER_CONNECT_RETRY=10, \
         MASTER_USE_GTID=slave_pos",
        escape_sql_string(primary_host),
        escape_sql_string(primary_user),
        escape_sql_string(primary_password),
        params.primary_port,
    );

    let change_replica_sql = format!(
        "CHANGE REPLICATION SOURCE TO \
         SOURCE_HOST='{}', \
         SOURCE_USER='{}', \
         SOURCE_PASSWORD='{}', \
         SOURCE_PORT={}, \
         SOURCE_CONNECT_RETRY=10",
        escape_sql_string(primary_host),
        escape_sql_string(primary_user),
        escape_sql_string(primary_password),
        params.primary_port,
    );

    let master_output = execute_mysql(params, &change_master_sql);
    let used_source_syntax = match master_output {
        Ok(ref o) if o.status.success() => false,
        _ => {
            let replica_output = execute_mysql(params, &change_replica_sql)?;
            if !replica_output.status.success() {
                let stderr = String::from_utf8_lossy(&replica_output.stderr);
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to configure replica source: {}", stderr),
                ));
            }
            true
        }
    };

    let start_sql = if used_source_syntax {
        "START REPLICA"
    } else {
        "START SLAVE"
    };
    let output = execute_mysql(params, start_sql)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to start replication: {}", stderr),
        ));
    }

    let extra = Some(value::to_value(json!({
        "mode": "replica",
        "state": "present",
        "primary_host": primary_host,
        "primary_port": params.primary_port,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Server configured as replica of '{}'",
            primary_host
        )),
        extra,
    })
}

fn stop_replication(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some("Would stop replication and reset replica configuration".to_string()),
            extra: None,
        });
    }

    let _ = execute_mysql(params, "STOP SLAVE");
    let _ = execute_mysql(params, "RESET SLAVE ALL");
    let _ = execute_mysql(params, "STOP REPLICA");
    let _ = execute_mysql(params, "RESET REPLICA ALL");

    let extra = Some(value::to_value(json!({
        "mode": "replica",
        "state": "absent",
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some("Replication stopped and replica configuration reset".to_string()),
        extra,
    })
}

fn get_primary_status(params: &Params) -> Result<ModuleResult> {
    let output = execute_mysql(params, "SHOW MASTER STATUS")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to get primary status: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let lines: Vec<&str> = stdout.trim().lines().collect();

    if lines.is_empty() || lines[0].trim().is_empty() {
        let extra = Some(value::to_value(json!({
            "is_primary": false,
        }))?);

        return Ok(ModuleResult {
            changed: false,
            output: Some("Server is not configured as primary (no binary log)".to_string()),
            extra,
        });
    }

    let fields: Vec<&str> = lines[0].split('\t').collect();
    let file = fields.first().map(|s| s.to_string()).unwrap_or_default();
    let position = fields.get(1).map(|s| s.to_string()).unwrap_or_default();
    let binlog_do_db = fields.get(2).map(|s| s.to_string()).unwrap_or_default();
    let binlog_ignore_db = fields.get(3).map(|s| s.to_string()).unwrap_or_default();

    let extra = Some(value::to_value(json!({
        "is_primary": true,
        "file": file,
        "position": position,
        "binlog_do_db": binlog_do_db,
        "binlog_ignore_db": binlog_ignore_db,
    }))?);

    Ok(ModuleResult {
        changed: false,
        output: Some(format!(
            "Primary status: File={}, Position={}",
            file, position
        )),
        extra,
    })
}

fn mysql_replication_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => match params.mode {
            Mode::Primary => {
                let already_primary = check_primary_status(&params)?;
                if already_primary {
                    Ok(ModuleResult {
                        changed: false,
                        output: Some("Server is already configured as primary".to_string()),
                        extra: Some(value::to_value(json!({
                            "mode": "primary",
                            "state": "present",
                        }))?),
                    })
                } else {
                    configure_primary(&params, check_mode)
                }
            }
            Mode::Replica => {
                let already_replica = check_replica_status(&params)?;
                if already_replica {
                    Ok(ModuleResult {
                        changed: false,
                        output: Some("Server is already configured as replica".to_string()),
                        extra: Some(value::to_value(json!({
                            "mode": "replica",
                            "state": "present",
                        }))?),
                    })
                } else {
                    configure_replica(&params, check_mode)
                }
            }
        },
        State::Absent => {
            let is_replica = check_replica_status(&params)?;
            if !is_replica {
                Ok(ModuleResult {
                    changed: false,
                    output: Some("Replication is not running".to_string()),
                    extra: Some(value::to_value(json!({
                        "mode": "replica",
                        "state": "absent",
                    }))?),
                })
            } else {
                stop_replication(&params, check_mode)
            }
        }
        State::Getprimary => get_primary_status(&params),
    }
}

#[derive(Debug)]
pub struct MysqlReplication;

impl Module for MysqlReplication {
    fn get_name(&self) -> &str {
        "mysql_replication"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((mysql_replication_impl(params, check_mode)?, None))
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
            mode: primary
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.mode, Mode::Primary);
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 3306);
        assert_eq!(params.primary_port, 3306);
    }

    #[test]
    fn test_parse_params_replica() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mode: replica
            state: present
            primary_host: db-primary.internal
            primary_user: repl
            primary_password: secret
            primary_port: 3307
            login_user: root
            login_password: root_secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Replica);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.primary_host, Some("db-primary.internal".to_string()));
        assert_eq!(params.primary_user, Some("repl".to_string()));
        assert_eq!(params.primary_password, Some("secret".to_string()));
        assert_eq!(params.primary_port, 3307);
        assert_eq!(params.login_user, Some("root".to_string()));
        assert_eq!(params.login_password, Some("root_secret".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mode: replica
            state: absent
            login_user: root
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Replica);
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.login_user, Some("root".to_string()));
    }

    #[test]
    fn test_parse_params_getprimary() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: getprimary
            login_user: root
            login_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Getprimary);
        assert_eq!(params.mode, Mode::Primary);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mode: replica
            state: present
            primary_host: db-primary.internal
            primary_user: repl
            primary_password: repl_secret
            primary_port: 3307
            login_host: 192.168.1.100
            login_user: admin
            login_password: admin_secret
            login_port: 3308
            config_file: /etc/mysql/debian.cnf
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Mode::Replica);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.primary_host, Some("db-primary.internal".to_string()));
        assert_eq!(params.primary_user, Some("repl".to_string()));
        assert_eq!(params.primary_password, Some("repl_secret".to_string()));
        assert_eq!(params.primary_port, 3307);
        assert_eq!(params.login_host, "192.168.1.100");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("admin_secret".to_string()));
        assert_eq!(params.login_port, 3308);
        assert_eq!(
            params.config_file,
            Some("/etc/mysql/debian.cnf".to_string())
        );
    }

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("normal"), "normal");
        assert_eq!(escape_sql_string("with'quote"), "with''quote");
        assert_eq!(escape_sql_string("with\\backslash"), "with\\\\backslash");
        assert_eq!(escape_sql_string("with'both\\chars"), "with''both\\\\chars");
    }

    #[test]
    fn test_build_mysql_base_args() {
        let params = Params {
            state: State::Present,
            mode: Mode::Replica,
            primary_host: Some("db-primary.internal".to_string()),
            primary_user: Some("repl".to_string()),
            primary_password: Some("secret".to_string()),
            primary_port: 3306,
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
    fn test_build_mysql_base_args_with_config_file() {
        let params = Params {
            state: State::Present,
            mode: Mode::Primary,
            primary_host: None,
            primary_user: None,
            primary_password: None,
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: None,
            login_password: None,
            login_port: 3306,
            config_file: Some("/etc/mysql/debian.cnf".to_string()),
        };
        let args = build_mysql_base_args(&params);
        assert!(args.contains(&"--defaults-file=/etc/mysql/debian.cnf".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            mode: primary
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_check_mode_primary() {
        let params = Params {
            state: State::Present,
            mode: Mode::Primary,
            primary_host: None,
            primary_user: None,
            primary_password: None,
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = configure_primary(&params, true).unwrap();
        assert!(result.get_changed());
        assert_eq!(
            result.get_output(),
            Some("Would configure server as primary".to_string())
        );
    }

    #[test]
    fn test_check_mode_replica() {
        let params = Params {
            state: State::Present,
            mode: Mode::Replica,
            primary_host: Some("db-primary.internal".to_string()),
            primary_user: Some("repl".to_string()),
            primary_password: Some("secret".to_string()),
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = configure_replica(&params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("db-primary.internal"));
    }

    #[test]
    fn test_check_mode_stop_replication() {
        let params = Params {
            state: State::Absent,
            mode: Mode::Replica,
            primary_host: None,
            primary_user: None,
            primary_password: None,
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = stop_replication(&params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would stop"));
    }

    #[test]
    fn test_replica_missing_primary_host() {
        let params = Params {
            state: State::Present,
            mode: Mode::Replica,
            primary_host: None,
            primary_user: Some("repl".to_string()),
            primary_password: Some("secret".to_string()),
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = configure_replica(&params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_replica_missing_primary_user() {
        let params = Params {
            state: State::Present,
            mode: Mode::Replica,
            primary_host: Some("db-primary.internal".to_string()),
            primary_user: None,
            primary_password: Some("secret".to_string()),
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = configure_replica(&params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_replica_missing_primary_password() {
        let params = Params {
            state: State::Present,
            mode: Mode::Replica,
            primary_host: Some("db-primary.internal".to_string()),
            primary_user: Some("repl".to_string()),
            primary_password: None,
            primary_port: 3306,
            login_host: "localhost".to_string(),
            login_user: Some("root".to_string()),
            login_password: Some("secret".to_string()),
            login_port: 3306,
            config_file: None,
        };
        let result = configure_replica(&params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_state() {
        assert_eq!(default_state(), State::Present);
    }

    #[test]
    fn test_default_mode() {
        assert_eq!(default_mode(), Mode::Primary);
    }
}
