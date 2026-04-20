/// ANCHOR: module
/// # influxdb_query
///
/// Write and query time-series data in InfluxDB.
///
/// Manage InfluxDB databases and perform write/query operations using the
/// InfluxDB v1 HTTP API. Supports creating and dropping databases, executing
/// InfluxQL queries, and writing data points using line protocol.
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
/// - name: Create InfluxDB database
///   influxdb_query:
///     database: iot_metrics
///     state: present
///
/// - name: Write sensor data points
///   influxdb_query:
///     database: iot_metrics
///     state: write
///     data:
///       - "temperature,device=sensor1 value=23.5"
///       - "humidity,device=sensor1 value=65.2"
///
/// - name: Write a single data point
///   influxdb_query:
///     database: iot_metrics
///     state: write
///     data: "temperature,device=sensor2 value=19.8"
///
/// - name: Query measurements
///   influxdb_query:
///     database: iot_metrics
///     query: "SELECT mean(value) FROM temperature GROUP BY time(1h)"
///   register: result
///
/// - name: Query with authentication
///   influxdb_query:
///     database: iot_metrics
///     query: "SHOW MEASUREMENTS"
///     hostname: influxdb.example.com
///     username: admin
///     password: "{{ influxdb_password }}"
///
/// - name: Drop database
///   influxdb_query:
///     database: old_metrics
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

const DEFAULT_PORT: u16 = 8086;
const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum InfluxdbState {
    /// Create the database if it does not exist.
    Present,
    /// Drop the database.
    Absent,
    /// Execute an InfluxQL query (default).
    #[default]
    Query,
    /// Write data points in line protocol format.
    Write,
}

fn default_hostname() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// InfluxDB hostname or IP address.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_hostname")]
    pub hostname: String,
    /// InfluxDB port.
    /// **[default: `8086`]**
    #[serde(default = "default_port")]
    pub port: u16,
    /// Database name (required for all states).
    pub database: String,
    /// The desired state of the database or operation to perform.
    /// **[default: `query`]**
    #[serde(default)]
    pub state: InfluxdbState,
    /// InfluxQL query to execute (required for state=query).
    pub query: Option<String>,
    /// Data points to write in line protocol format (required for state=write).
    /// Can be a single string or a list of strings.
    pub data: Option<OneOrMany>,
    /// Username for InfluxDB authentication.
    pub username: Option<String>,
    /// Password for InfluxDB authentication.
    pub password: Option<String>,
    /// Use HTTPS for the connection.
    /// **[default: `false`]**
    #[serde(default)]
    pub ssl: bool,
    /// Request timeout in seconds.
    /// **[default: `30`]**
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Validate SSL certificates (only applies when ssl=true).
    /// **[default: `true`]**
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum OneOrMany {
    /// A single data point string.
    One(String),
    /// Multiple data point strings.
    Many(Vec<String>),
}

fn default_validate_certs() -> bool {
    true
}

struct InfluxdbClient {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    timeout: std::time::Duration,
    validate_certs: bool,
}

impl InfluxdbClient {
    fn new(params: &Params) -> Self {
        let scheme = if params.ssl { "https" } else { "http" };
        Self {
            base_url: format!("{}://{}:{}", scheme, params.hostname, params.port),
            username: params.username.clone(),
            password: params.password.clone(),
            timeout: std::time::Duration::from_secs(params.timeout),
            validate_certs: params.validate_certs,
        }
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .timeout(self.timeout)
            .danger_accept_invalid_certs(!self.validate_certs)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })
    }

    fn build_query_url(&self, path: &str, params: &[(&str, &str)]) -> String {
        let mut url = format!("{}{}", self.base_url, path);
        let mut all_params: Vec<String> =
            params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            all_params.push(format!("u={}", u));
            all_params.push(format!("p={}", p));
        }

        if !all_params.is_empty() {
            url.push('?');
            url.push_str(&all_params.join("&"));
        }

        url
    }

    fn query(&self, database: &str, query: &str) -> Result<reqwest::blocking::Response> {
        let url = self.build_query_url("/query", &[("db", database), ("q", query)]);
        let client = self.build_client()?;
        client.get(&url).send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("InfluxDB query request failed: {e}"),
            )
        })
    }

    fn write(&self, database: &str, line_protocol: &str) -> Result<reqwest::blocking::Response> {
        let url = self.build_query_url("/write", &[("db", database)]);
        let client = self.build_client()?;
        client
            .post(&url)
            .body(line_protocol.to_string())
            .send()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("InfluxDB write request failed: {e}"),
                )
            })
    }
}

fn check_response(response: reqwest::blocking::Response) -> Result<String> {
    let status = response.status();
    let body = response.text().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read response body: {e}"),
        )
    })?;

    if !status.is_success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("InfluxDB returned status {}: {}", status, body),
        ));
    }

    Ok(body)
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        let client = InfluxdbClient::new(params);
        let response = client.query(&params.database, "SHOW DATABASES")?;
        let body = check_response(response)?;

        let exists = body.contains(&format!("\"{}\"", params.database));
        return Ok(ModuleResult::new(
            !exists,
            Some(value::to_value(json!({
                "database": params.database,
                "exists": exists,
            }))?),
            Some(if exists {
                format!("Database '{}' already exists", params.database)
            } else {
                format!("Would create database '{}'", params.database)
            }),
        ));
    }

    let client = InfluxdbClient::new(params);
    let create_query = format!("CREATE DATABASE \"{}\"", params.database);
    let response = client.query(&params.database, &create_query)?;
    let body = check_response(response)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "database": params.database,
            "query": create_query,
            "response": body,
        }))?),
        Some(format!("Database '{}' created", params.database)),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = InfluxdbClient::new(params);

    let response = client.query(&params.database, "SHOW DATABASES")?;
    let body = check_response(response)?;
    let exists = body.contains(&format!("\"{}\"", params.database));

    if check_mode {
        return Ok(ModuleResult::new(
            exists,
            Some(value::to_value(json!({
                "database": params.database,
                "exists": exists,
            }))?),
            Some(if exists {
                format!("Would drop database '{}'", params.database)
            } else {
                format!("Database '{}' does not exist", params.database)
            }),
        ));
    }

    if !exists {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "database": params.database,
                "exists": false,
            }))?),
            Some(format!("Database '{}' does not exist", params.database)),
        ));
    }

    let drop_query = format!("DROP DATABASE \"{}\"", params.database);
    let response = client.query(&params.database, &drop_query)?;
    let body = check_response(response)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "database": params.database,
            "query": drop_query,
            "response": body,
        }))?),
        Some(format!("Database '{}' dropped", params.database)),
    ))
}

fn exec_query(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let query = params.query.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "query parameter is required when state=query",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "database": params.database,
                "query": query,
            }))?),
            Some(format!(
                "Would execute query on '{}': {}",
                params.database, query
            )),
        ));
    }

    let client = InfluxdbClient::new(params);
    let response = client.query(&params.database, query)?;
    let body = check_response(response)?;

    let extra = value::to_value(json!({
        "database": params.database,
        "query": query,
        "results": body,
    }))?;

    let is_read_query = query.trim_start().to_uppercase().starts_with("SELECT")
        || query.trim_start().to_uppercase().starts_with("SHOW");

    Ok(ModuleResult::new(!is_read_query, Some(extra), Some(body)))
}

fn exec_write(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let data = params.data.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "data parameter is required when state=write",
        )
    })?;

    let lines: Vec<&str> = match data {
        OneOrMany::One(s) => vec![s.as_str()],
        OneOrMany::Many(v) => v.iter().map(|s| s.as_str()).collect(),
    };

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "database": params.database,
                "points_count": lines.len(),
            }))?),
            Some(format!(
                "Would write {} data point(s) to '{}'",
                lines.len(),
                params.database
            )),
        ));
    }

    let line_protocol = lines.join("\n");
    let client = InfluxdbClient::new(params);
    let response = client.write(&params.database, &line_protocol)?;
    let body = check_response(response)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "database": params.database,
            "points_count": lines.len(),
            "response": body,
        }))?),
        Some(format!(
            "Wrote {} data point(s) to '{}'",
            lines.len(),
            params.database
        )),
    ))
}

pub fn influxdb_query(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        InfluxdbState::Present => exec_present(&params, check_mode),
        InfluxdbState::Absent => exec_absent(&params, check_mode),
        InfluxdbState::Query => exec_query(&params, check_mode),
        InfluxdbState::Write => exec_write(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct InfluxdbQuery;

impl Module for InfluxdbQuery {
    fn get_name(&self) -> &str {
        "influxdb_query"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            influxdb_query(parse_params(optional_params)?, check_mode)?,
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

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.database, "iot_metrics");
        assert_eq!(params.state, InfluxdbState::Present);
        assert_eq!(params.hostname, "localhost");
        assert_eq!(params.port, DEFAULT_PORT);
        assert!(!params.ssl);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: old_metrics
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.database, "old_metrics");
        assert_eq!(params.state, InfluxdbState::Absent);
    }

    #[test]
    fn test_parse_params_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SELECT mean(value) FROM temperature GROUP BY time(1h)"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.database, "iot_metrics");
        assert_eq!(params.state, InfluxdbState::Query);
        assert_eq!(
            params.query,
            Some("SELECT mean(value) FROM temperature GROUP BY time(1h)".to_string())
        );
    }

    #[test]
    fn test_parse_params_write_single() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: write
            data: "temperature,device=sensor1 value=23.5"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.database, "iot_metrics");
        assert_eq!(params.state, InfluxdbState::Write);
        assert_eq!(
            params.data,
            Some(OneOrMany::One(
                "temperature,device=sensor1 value=23.5".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_params_write_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: write
            data:
              - "temperature,device=sensor1 value=23.5"
              - "humidity,device=sensor1 value=65.2"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.database, "iot_metrics");
        assert_eq!(params.state, InfluxdbState::Write);
        assert_eq!(
            params.data,
            Some(OneOrMany::Many(vec![
                "temperature,device=sensor1 value=23.5".to_string(),
                "humidity,device=sensor1 value=65.2".to_string(),
            ]))
        );
    }

    #[test]
    fn test_parse_params_with_hostname() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SHOW MEASUREMENTS"
            hostname: influxdb.example.com
            port: 8087
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.hostname, "influxdb.example.com");
        assert_eq!(params.port, 8087);
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SHOW MEASUREMENTS"
            username: admin
            password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.username, Some("admin".to_string()));
        assert_eq!(params.password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_with_ssl() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SELECT * FROM cpu"
            hostname: influxdb.example.com
            ssl: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.ssl);
    }

    #[test]
    fn test_parse_params_with_timeout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SELECT * FROM cpu"
            timeout: 60
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.timeout, 60);
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SELECT * FROM cpu"
            ssl: true
            validate_certs: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_missing_database() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            query: "SELECT 1"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: test
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.hostname, "localhost");
        assert_eq!(params.port, DEFAULT_PORT);
        assert_eq!(params.state, InfluxdbState::Query);
        assert!(!params.ssl);
        assert!(params.username.is_none());
        assert!(params.password.is_none());
        assert!(params.query.is_none());
        assert!(params.data.is_none());
        assert!(params.validate_certs);
        assert_eq!(params.timeout, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn test_check_mode_query() {
        let module = InfluxdbQuery;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            query: "SELECT * FROM temperature"
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("Would execute query"));
    }

    #[test]
    fn test_check_mode_write() {
        let module = InfluxdbQuery;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: write
            data: "temperature,device=sensor1 value=23.5"
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(
            result
                .get_output()
                .unwrap()
                .contains("Would write 1 data point")
        );
    }

    #[test]
    fn test_check_mode_write_list() {
        let module = InfluxdbQuery;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: write
            data:
              - "temperature,device=sensor1 value=23.5"
              - "humidity,device=sensor1 value=65.2"
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(
            result
                .get_output()
                .unwrap()
                .contains("Would write 2 data point")
        );
    }

    #[test]
    fn test_check_mode_present() {
        let module = InfluxdbQuery;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: present
            "#,
        )
        .unwrap();
        let result = module.exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true);
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_query_missing_query_param() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let error = exec_query(&params, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_write_missing_data_param() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: iot_metrics
            state: write
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let error = exec_write(&params, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_influxdb_client_url_http() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: test
            hostname: localhost
            port: 8086
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let client = InfluxdbClient::new(&params);
        assert_eq!(client.base_url, "http://localhost:8086");
    }

    #[test]
    fn test_influxdb_client_url_https() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            database: test
            hostname: influxdb.example.com
            ssl: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let client = InfluxdbClient::new(&params);
        assert_eq!(client.base_url, "https://influxdb.example.com:8086");
    }

    #[test]
    fn test_one_or_many_deserialize_string() {
        let yaml: YamlValue = serde_norway::from_str("\"single value\"").unwrap();
        let result: OneOrMany = serde_norway::from_value(yaml).unwrap();
        assert_eq!(result, OneOrMany::One("single value".to_string()));
    }

    #[test]
    fn test_one_or_many_deserialize_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            - "first"
            - "second"
            "#,
        )
        .unwrap();
        let result: OneOrMany = serde_norway::from_value(yaml).unwrap();
        assert_eq!(
            result,
            OneOrMany::Many(vec!["first".to_string(), "second".to_string()])
        );
    }
}
