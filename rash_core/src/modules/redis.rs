/// ANCHOR: module
/// # redis
///
/// Unified utility to interact with Redis instances.
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
/// - name: Set a key
///   redis:
///     command: set
///     key: mykey
///     value: myvalue
///
/// - name: Get a key
///   redis:
///     command: get
///     key: mykey
///   register: result
///
/// - name: Delete a key
///   redis:
///     command: delete
///     key: mykey
///
/// - name: Flush all databases
///   redis:
///     command: flush
///     flush_mode: all
///
/// - name: Flush a specific database
///   redis:
///     command: flush
///     flush_mode: db
///     db: 1
///
/// - name: Configure Redis maxmemory
///   redis:
///     command: config
///     name: maxmemory
///     value: 4GB
///
/// - name: Set instance as replica
///   redis:
///     command: replica
///     master_host: 192.168.1.100
///     master_port: 6379
///
/// - name: Set instance as master
///   redis:
///     command: replica
///     replica_mode: master
///
/// - name: Connect with authentication
///   redis:
///     command: get
///     key: mykey
///     login_host: localhost
///     login_port: 6379
///     login_password: secret
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

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Command {
    /// Set a key-value pair.
    Set,
    /// Get a value by key.
    Get,
    /// Delete a key.
    Delete,
    /// Flush all or a specific database.
    Flush,
    /// Configure a Redis setting.
    Config,
    /// Configure replication.
    Replica,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum FlushMode {
    /// Flush all databases.
    #[default]
    All,
    /// Flush a specific database.
    Db,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ReplicaMode {
    /// Set instance as master.
    Master,
    /// Set instance as replica.
    #[default]
    Replica,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The Redis command to execute.
    pub command: Command,
    /// The host running Redis.
    #[serde(default = "default_host")]
    pub login_host: String,
    /// The port to connect to.
    #[serde(default = "default_port")]
    pub login_port: u16,
    /// The password to authenticate with.
    pub login_password: Option<String>,
    /// The user to authenticate with.
    pub login_user: Option<String>,
    /// The database number to use.
    pub db: Option<i64>,
    /// The key to operate on (for set/get/delete commands).
    pub key: Option<String>,
    /// The value to set (for set command) or configure (for config command).
    pub value: Option<String>,
    /// Whether the key should have an expiry time in seconds.
    pub ttl: Option<i64>,
    /// Type of flush (for flush command).
    #[serde(default)]
    pub flush_mode: FlushMode,
    /// Configuration setting name (for config command).
    pub name: Option<String>,
    /// The mode for replica command.
    #[serde(default)]
    pub replica_mode: ReplicaMode,
    /// The master host (for replica command).
    pub master_host: Option<String>,
    /// The master port (for replica command).
    pub master_port: Option<u16>,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    6379
}

fn get_connection_url(params: &Params) -> String {
    let auth = match (&params.login_user, &params.login_password) {
        (Some(user), Some(pass)) => format!("{user}:{pass}@"),
        (None, Some(pass)) => format!(":{pass}@"),
        _ => String::new(),
    };
    let db = params.db.unwrap_or(0);
    format!(
        "redis://{}{}:{}/{db}",
        auth, params.login_host, params.login_port
    )
}

fn connect(params: &Params) -> Result<redis::Connection> {
    let url = get_connection_url(params);
    let client =
        redis::Client::open(url.as_str()).map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    client
        .get_connection()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))
}

fn exec_set(
    conn: &mut redis::Connection,
    params: &Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    let key = params
        .key
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "key is required for set command"))?;
    let value = params
        .value
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "value is required for set command"))?;

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let result: String = match params.ttl {
        Some(ttl) => redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl)
            .query(conn)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?,
        None => redis::cmd("SET")
            .arg(key)
            .arg(value)
            .query(conn)
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?,
    };

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({ "key": key, "value": value }))?),
        Some(result),
    ))
}

fn exec_get(conn: &mut redis::Connection, params: &Params) -> Result<ModuleResult> {
    let key = params
        .key
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "key is required for get command"))?;

    let result: Option<String> = redis::cmd("GET")
        .arg(key)
        .query(conn)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    match result {
        Some(val) => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({ "key": key, "value": &val }))?),
            Some(val),
        )),
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({ "key": key, "found": false }))?),
            None,
        )),
    }
}

fn exec_delete(
    conn: &mut redis::Connection,
    params: &Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    let key = params
        .key
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "key is required for delete command"))?;

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let result: i32 = redis::cmd("DEL")
        .arg(key)
        .query(conn)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let changed = result > 0;
    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(json!({ "key": key, "deleted": result }))?),
        Some(format!("Deleted {} key(s)", result)),
    ))
}

fn exec_flush(
    conn: &mut redis::Connection,
    params: &Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    match params.flush_mode {
        FlushMode::All => {
            let _: String = redis::cmd("FLUSHALL")
                .query(conn)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({ "flush_mode": "all" }))?),
                Some("Flushed all databases".to_string()),
            ))
        }
        FlushMode::Db => {
            let _: String = redis::cmd("FLUSHDB")
                .query(conn)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
            let db = params.db.unwrap_or(0);
            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({ "flush_mode": "db", "db": db }))?),
                Some(format!("Flushed database {}", db)),
            ))
        }
    }
}

fn exec_config(
    conn: &mut redis::Connection,
    params: &Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    let name = params.name.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "name is required for config command",
        )
    })?;
    let value = params.value.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "value is required for config command",
        )
    })?;

    let current: String = redis::cmd("CONFIG")
        .arg("GET")
        .arg(name)
        .query(conn)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    let changed = current != *value;

    if check_mode {
        return Ok(ModuleResult::new(changed, None, None));
    }

    if !changed {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(
                json!({ "name": name, "value": value, "changed": false }),
            )?),
            Some(format!("{} already set to {}", name, value)),
        ));
    }

    let _: String = redis::cmd("CONFIG")
        .arg("SET")
        .arg(name)
        .arg(value)
        .query(conn)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(
            json!({ "name": name, "value": value, "changed": true }),
        )?),
        Some(format!("Set {} to {}", name, value)),
    ))
}

fn exec_replica(
    conn: &mut redis::Connection,
    params: &Params,
    check_mode: bool,
) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    match params.replica_mode {
        ReplicaMode::Master => {
            let _: String = redis::cmd("REPLICAOF")
                .arg("NO")
                .arg("ONE")
                .query(conn)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({ "replica_mode": "master" }))?),
                Some("Set instance as master".to_string()),
            ))
        }
        ReplicaMode::Replica => {
            let master_host = params.master_host.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "master_host is required for replica mode",
                )
            })?;
            let master_port = params.master_port.unwrap_or(6379);

            let _: String = redis::cmd("REPLICAOF")
                .arg(master_host)
                .arg(master_port)
                .query(conn)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "replica_mode": "replica",
                    "master_host": master_host,
                    "master_port": master_port
                }))?),
                Some(format!(
                    "Set instance as replica of {}:{}",
                    master_host, master_port
                )),
            ))
        }
    }
}

#[derive(Debug)]
pub struct Redis;

impl Module for Redis {
    fn get_name(&self) -> &str {
        "redis"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let mut conn = connect(&params)?;

        let result = match params.command {
            Command::Set => exec_set(&mut conn, &params, check_mode)?,
            Command::Get => exec_get(&mut conn, &params)?,
            Command::Delete => exec_delete(&mut conn, &params, check_mode)?,
            Command::Flush => exec_flush(&mut conn, &params, check_mode)?,
            Command::Config => exec_config(&mut conn, &params, check_mode)?,
            Command::Replica => exec_replica(&mut conn, &params, check_mode)?,
        };

        Ok((result, None))
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
    fn test_parse_params_set() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: set
            key: mykey
            value: myvalue
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Set);
        assert_eq!(params.key, Some("mykey".to_string()));
        assert_eq!(params.value, Some("myvalue".to_string()));
    }

    #[test]
    fn test_parse_params_get() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: get
            key: mykey
            login_host: 192.168.1.1
            login_port: 6380
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Get);
        assert_eq!(params.key, Some("mykey".to_string()));
        assert_eq!(params.login_host, "192.168.1.1");
        assert_eq!(params.login_port, 6380);
    }

    #[test]
    fn test_parse_params_flush() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: flush
            flush_mode: db
            db: 1
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Flush);
        assert_eq!(params.flush_mode, FlushMode::Db);
        assert_eq!(params.db, Some(1));
    }

    #[test]
    fn test_parse_params_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: config
            name: maxmemory
            value: 4GB
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Config);
        assert_eq!(params.name, Some("maxmemory".to_string()));
        assert_eq!(params.value, Some("4GB".to_string()));
    }

    #[test]
    fn test_parse_params_replica() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: replica
            master_host: 192.168.1.100
            master_port: 6379
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Replica);
        assert_eq!(params.master_host, Some("192.168.1.100".to_string()));
        assert_eq!(params.master_port, Some(6379));
    }

    #[test]
    fn test_parse_params_replica_master() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: replica
            replica_mode: master
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Command::Replica);
        assert_eq!(params.replica_mode, ReplicaMode::Master);
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: get
            key: mykey
            login_user: admin
            login_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: get
            key: mykey
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.login_host, "localhost");
        assert_eq!(params.login_port, 6379);
        assert_eq!(params.flush_mode, FlushMode::All);
        assert_eq!(params.replica_mode, ReplicaMode::Replica);
    }
}
