/// ANCHOR: module
/// # kafka_topic
///
/// Manage Kafka topics.
///
/// Create and delete Kafka topics with configurable partitions, replication
/// factor, and topic-level configuration. Useful for streaming infrastructure
/// management, event-driven architectures, and data pipeline automation.
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
/// - name: Create a topic
///   kafka_topic:
///     name: events
///     partitions: 3
///     replication_factor: 2
///     config:
///       retention.ms: "604800000"
///     state: present
///
/// - name: Delete a topic
///   kafka_topic:
///     name: old_topic
///     state: absent
///
/// - name: Create topic with custom bootstrap servers
///   kafka_topic:
///     name: my-topic
///     partitions: 6
///     replication_factor: 3
///     bootstrap_servers: kafka1:9092,kafka2:9092
///     state: present
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

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const DEFAULT_PARTITIONS: i32 = 1;
const DEFAULT_REPLICATION_FACTOR: i16 = 1;
const DEFAULT_TIMEOUT_MS: i32 = 30000;
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 5;

const KAFKA_ERROR_UNKNOWN_TOPIC: i16 = 17;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn default_partitions() -> i32 {
    DEFAULT_PARTITIONS
}

fn default_replication_factor() -> i16 {
    DEFAULT_REPLICATION_FACTOR
}

fn default_bootstrap_servers() -> String {
    "localhost:9092".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub name: String,
    #[serde(default)]
    pub state: State,
    #[serde(default = "default_partitions")]
    pub partitions: i32,
    #[serde(default = "default_replication_factor")]
    pub replication_factor: i16,
    #[serde(default, deserialize_with = "deserialize_string_map")]
    pub config: HashMap<String, String>,
    #[serde(default = "default_bootstrap_servers")]
    pub bootstrap_servers: String,
}

fn deserialize_string_map<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<serde_norway::Value> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(HashMap::new()),
        Some(serde_norway::Value::Mapping(map)) => {
            let mut result = HashMap::new();
            for (k, v) in map {
                let key = k
                    .as_str()
                    .ok_or_else(|| {
                        <D::Error as serde::de::Error>::custom("Config key must be a string")
                    })?
                    .to_string();
                let val = match &v {
                    serde_norway::Value::String(s) => s.clone(),
                    serde_norway::Value::Number(n) => n.to_string(),
                    serde_norway::Value::Bool(b) => b.to_string(),
                    _ => {
                        return Err(<D::Error as serde::de::Error>::custom(
                            "Config value must be a string, number, or boolean",
                        ));
                    }
                };
                result.insert(key, val);
            }
            Ok(result)
        }
        Some(_) => Err(<D::Error as serde::de::Error>::custom(
            "Config must be a mapping",
        )),
    }
}

fn write_i16(buf: &mut Vec<u8>, val: i16) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_i32(buf: &mut Vec<u8>, val: i32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_i16(buf, s.len() as i16);
    buf.extend_from_slice(s.as_bytes());
}

fn write_nullable_string(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(val) => write_string(buf, val),
        None => write_i16(buf, -1),
    }
}

fn read_i16(data: &[u8], offset: &mut usize) -> Result<i16> {
    if *offset + 2 > data.len() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected end of data reading i16",
        ));
    }
    let val = i16::from_be_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    Ok(val)
}

fn read_i32(data: &[u8], offset: &mut usize) -> Result<i32> {
    if *offset + 4 > data.len() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected end of data reading i32",
        ));
    }
    let val = i32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(val)
}

fn read_string(data: &[u8], offset: &mut usize) -> Result<String> {
    let len = read_i16(data, offset)?;
    if len < 0 {
        return Ok(String::new());
    }
    let len = len as usize;
    if *offset + len > data.len() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected end of data reading string",
        ));
    }
    let s = String::from_utf8(data[*offset..*offset + len].to_vec())
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    *offset += len;
    Ok(s)
}

fn read_nullable_string(data: &[u8], offset: &mut usize) -> Result<Option<String>> {
    let len = read_i16(data, offset)?;
    if len < 0 {
        return Ok(None);
    }
    let len = len as usize;
    if *offset + len > data.len() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Unexpected end of data reading nullable string",
        ));
    }
    let s = String::from_utf8(data[*offset..*offset + len].to_vec())
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
    *offset += len;
    Ok(Some(s))
}

struct TopicInfo {
    partition_count: i32,
}

struct KafkaAdminClient {
    stream: TcpStream,
    correlation_id: i32,
}

impl KafkaAdminClient {
    fn connect(bootstrap_servers: &str) -> Result<Self> {
        let servers: Vec<&str> = bootstrap_servers.split(',').map(|s| s.trim()).collect();
        if servers.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "No bootstrap servers provided",
            ));
        }

        let mut last_error = None;
        for server in &servers {
            match Self::connect_to_server(server) {
                Ok(stream) => {
                    return Ok(Self {
                        stream,
                        correlation_id: 0,
                    });
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| Error::new(ErrorKind::InvalidData, "No bootstrap servers provided")))
    }

    fn connect_to_server(server: &str) -> Result<TcpStream> {
        let (host, port) = if server.contains(':') {
            let parts: Vec<&str> = server.rsplitn(2, ':').collect();
            let port: u16 = parts[0].parse().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid port in bootstrap server '{}': {}", server, e),
                )
            })?;
            (parts[1].to_string(), port)
        } else {
            (server.to_string(), 9092)
        };

        let addr = format!("{}:{}", host, port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to resolve '{}': {}", addr, e),
                )
            })?
            .next()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to resolve '{}'", addr),
                )
            })?;

        let stream = TcpStream::connect_timeout(
            &socket_addr,
            Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
        )
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to connect to Kafka broker '{}': {}", addr, e),
            )
        })?;

        stream
            .set_read_timeout(Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS)))
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS)))
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        Ok(stream)
    }

    fn send_request(&mut self, api_key: i16, api_version: i16, body: &[u8]) -> Result<Vec<u8>> {
        let correlation_id = self.correlation_id;
        self.correlation_id += 1;

        let mut header = Vec::new();
        write_i16(&mut header, api_key);
        write_i16(&mut header, api_version);
        write_i32(&mut header, correlation_id);
        write_string(&mut header, "rash");

        let mut request = Vec::new();
        request.extend_from_slice(&header);
        request.extend_from_slice(body);

        let size = request.len() as i32;
        self.stream.write_all(&size.to_be_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to send request: {e}"),
            )
        })?;
        self.stream.write_all(&request).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to send request: {e}"),
            )
        })?;

        let mut size_buf = [0u8; 4];
        self.stream.read_exact(&mut size_buf).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read response size: {e}"),
            )
        })?;
        let response_size = i32::from_be_bytes(size_buf) as usize;
        if response_size > 16 * 1024 * 1024 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Kafka response too large: {response_size} bytes"),
            ));
        }

        let mut response_buf = vec![0u8; response_size];
        self.stream.read_exact(&mut response_buf).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read response: {e}"),
            )
        })?;

        Ok(response_buf)
    }

    fn topic_metadata(&mut self, topic_name: &str) -> Result<Option<TopicInfo>> {
        let mut body = Vec::new();
        write_i32(&mut body, 1);
        write_string(&mut body, topic_name);

        let response = self.send_request(3, 0, &body)?;
        let mut offset = 0;

        read_i32(&response, &mut offset)?;

        let num_brokers = read_i32(&response, &mut offset)?;
        for _ in 0..num_brokers {
            read_i32(&response, &mut offset)?;
            read_string(&response, &mut offset)?;
            read_i32(&response, &mut offset)?;
        }

        let num_topics = read_i32(&response, &mut offset)?;
        for _ in 0..num_topics {
            let error_code = read_i16(&response, &mut offset)?;
            let name = read_string(&response, &mut offset)?;

            let num_partitions = read_i32(&response, &mut offset)?;
            for _ in 0..num_partitions {
                read_i16(&response, &mut offset)?;
                read_i32(&response, &mut offset)?;
                read_i32(&response, &mut offset)?;
                let num_replicas = read_i32(&response, &mut offset)?;
                for _ in 0..num_replicas {
                    read_i32(&response, &mut offset)?;
                }
                let num_isr = read_i32(&response, &mut offset)?;
                for _ in 0..num_isr {
                    read_i32(&response, &mut offset)?;
                }
            }

            if name == topic_name {
                if error_code == 0 {
                    return Ok(Some(TopicInfo {
                        partition_count: num_partitions,
                    }));
                } else if error_code == KAFKA_ERROR_UNKNOWN_TOPIC {
                    return Ok(None);
                } else {
                    return Err(Error::new(
                        ErrorKind::SubprocessFail,
                        format!(
                            "Kafka metadata error for topic '{}': error code {}",
                            topic_name, error_code
                        ),
                    ));
                }
            }
        }

        Ok(None)
    }

    fn create_topic(
        &mut self,
        name: &str,
        partitions: i32,
        replication_factor: i16,
        config: &HashMap<String, String>,
    ) -> Result<()> {
        let mut body = Vec::new();

        write_i32(&mut body, 1);
        write_string(&mut body, name);
        write_i32(&mut body, partitions);
        write_i16(&mut body, replication_factor);
        write_i32(&mut body, -1);
        write_i32(&mut body, config.len() as i32);
        for (k, v) in config {
            write_string(&mut body, k);
            write_nullable_string(&mut body, Some(v));
        }
        write_i32(&mut body, DEFAULT_TIMEOUT_MS);

        let response = self.send_request(19, 0, &body)?;
        let mut offset = 0;

        read_i32(&response, &mut offset)?;

        let num_topics = read_i32(&response, &mut offset)?;
        for _ in 0..num_topics {
            let _topic_name = read_string(&response, &mut offset)?;
            let error_code = read_i16(&response, &mut offset)?;
            let _error_message = read_nullable_string(&response, &mut offset)?;

            if error_code != 0 {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create topic '{}': Kafka error code {}",
                        name, error_code
                    ),
                ));
            }
        }

        Ok(())
    }

    fn delete_topic(&mut self, name: &str) -> Result<()> {
        let mut body = Vec::new();

        write_i32(&mut body, 1);
        write_string(&mut body, name);
        write_i32(&mut body, DEFAULT_TIMEOUT_MS);

        let response = self.send_request(20, 0, &body)?;
        let mut offset = 0;

        read_i32(&response, &mut offset)?;

        let num_responses = read_i32(&response, &mut offset)?;
        for _ in 0..num_responses {
            let _topic_name = read_string(&response, &mut offset)?;
            let error_code = read_i16(&response, &mut offset)?;

            if error_code != 0 {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to delete topic '{}': Kafka error code {}",
                        name, error_code
                    ),
                ));
            }
        }

        Ok(())
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let mut client = KafkaAdminClient::connect(&params.bootstrap_servers)?;

    match client.topic_metadata(&params.name)? {
        Some(info) => {
            let extra = value::to_value(json!({
                "name": params.name,
                "partitions": info.partition_count,
                "exists": true,
            }))?;

            Ok(ModuleResult::new(
                false,
                Some(extra),
                Some(format!("Topic '{}' already exists", params.name)),
            ))
        }
        None => {
            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "name": params.name,
                        "partitions": params.partitions,
                        "replication_factor": params.replication_factor,
                        "config": params.config,
                    }))?),
                    Some(format!("Would create topic '{}'", params.name)),
                ));
            }

            client.create_topic(
                &params.name,
                params.partitions,
                params.replication_factor,
                &params.config,
            )?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "name": params.name,
                    "partitions": params.partitions,
                    "replication_factor": params.replication_factor,
                    "config": params.config,
                }))?),
                Some(format!("Topic '{}' created", params.name)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let mut client = KafkaAdminClient::connect(&params.bootstrap_servers)?;

    match client.topic_metadata(&params.name)? {
        Some(_) => {
            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "name": params.name,
                    }))?),
                    Some(format!("Would delete topic '{}'", params.name)),
                ));
            }

            client.delete_topic(&params.name)?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "name": params.name,
                    "deleted": true,
                }))?),
                Some(format!("Topic '{}' deleted", params.name)),
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "name": params.name,
                "exists": false,
            }))?),
            Some(format!("Topic '{}' does not exist", params.name)),
        )),
    }
}

pub fn kafka_topic(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct KafkaTopic;

impl Module for KafkaTopic {
    fn get_name(&self) -> &str {
        "kafka_topic"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            kafka_topic(parse_params(optional_params)?, check_mode)?,
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
            name: events
            partitions: 3
            replication_factor: 2
            config:
              retention.ms: "604800000"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "events");
        assert_eq!(params.partitions, 3);
        assert_eq!(params.replication_factor, 2);
        assert_eq!(params.state, State::Present);
        assert_eq!(
            params.config.get("retention.ms"),
            Some(&"604800000".to_string())
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_topic
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_topic");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-topic
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my-topic");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.partitions, DEFAULT_PARTITIONS);
        assert_eq!(params.replication_factor, DEFAULT_REPLICATION_FACTOR);
        assert_eq!(params.bootstrap_servers, "localhost:9092");
        assert!(params.config.is_empty());
    }

    #[test]
    fn test_parse_params_missing_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_custom_bootstrap_servers() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-topic
            bootstrap_servers: kafka1:9092,kafka2:9092
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.bootstrap_servers, "kafka1:9092,kafka2:9092");
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            partitions: 6
            replication_factor: 3
            config:
              retention.ms: "604800000"
              cleanup.policy: compact
            bootstrap_servers: kafka1:9092,kafka2:9092
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "events");
        assert_eq!(params.partitions, 6);
        assert_eq!(params.replication_factor, 3);
        assert_eq!(params.bootstrap_servers, "kafka1:9092,kafka2:9092");
        assert_eq!(
            params.config.get("retention.ms"),
            Some(&"604800000".to_string())
        );
        assert_eq!(
            params.config.get("cleanup.policy"),
            Some(&"compact".to_string())
        );
    }

    #[test]
    fn test_parse_params_config_numeric_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            config:
              retention.ms: 604800000
              segment.bytes: 1073741824
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.config.get("retention.ms"),
            Some(&"604800000".to_string())
        );
        assert_eq!(
            params.config.get("segment.bytes"),
            Some(&"1073741824".to_string())
        );
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_write_read_i16() {
        let mut buf = Vec::new();
        write_i16(&mut buf, 42);
        let mut offset = 0;
        assert_eq!(read_i16(&buf, &mut offset).unwrap(), 42);
        assert_eq!(offset, 2);
    }

    #[test]
    fn test_write_read_i16_negative() {
        let mut buf = Vec::new();
        write_i16(&mut buf, -1);
        let mut offset = 0;
        assert_eq!(read_i16(&buf, &mut offset).unwrap(), -1);
    }

    #[test]
    fn test_write_read_i32() {
        let mut buf = Vec::new();
        write_i32(&mut buf, 12345);
        let mut offset = 0;
        assert_eq!(read_i32(&buf, &mut offset).unwrap(), 12345);
        assert_eq!(offset, 4);
    }

    #[test]
    fn test_write_read_i32_negative() {
        let mut buf = Vec::new();
        write_i32(&mut buf, -1);
        let mut offset = 0;
        assert_eq!(read_i32(&buf, &mut offset).unwrap(), -1);
    }

    #[test]
    fn test_write_read_string() {
        let mut buf = Vec::new();
        write_string(&mut buf, "hello");
        let mut offset = 0;
        assert_eq!(read_string(&buf, &mut offset).unwrap(), "hello");
        assert_eq!(offset, 2 + 5);
    }

    #[test]
    fn test_write_read_nullable_string_some() {
        let mut buf = Vec::new();
        write_nullable_string(&mut buf, Some("world"));
        let mut offset = 0;
        assert_eq!(
            read_nullable_string(&buf, &mut offset).unwrap(),
            Some("world".to_string())
        );
    }

    #[test]
    fn test_write_read_nullable_string_none() {
        let mut buf = Vec::new();
        write_nullable_string(&mut buf, None);
        let mut offset = 0;
        assert_eq!(read_nullable_string(&buf, &mut offset).unwrap(), None);
    }

    #[test]
    fn test_read_insufficient_data_i16() {
        let data = [0u8; 1];
        let mut offset = 0;
        assert!(read_i16(&data, &mut offset).is_err());
    }

    #[test]
    fn test_read_insufficient_data_i32() {
        let data = [0u8; 2];
        let mut offset = 0;
        assert!(read_i32(&data, &mut offset).is_err());
    }

    #[test]
    fn test_read_insufficient_data_string() {
        let data = [0u8, 5];
        let mut offset = 0;
        assert!(read_string(&data, &mut offset).is_err());
    }

    #[test]
    fn test_roundtrip_multiple_fields() {
        let mut buf = Vec::new();
        write_i16(&mut buf, 19);
        write_i32(&mut buf, 100);
        write_string(&mut buf, "test-topic");
        write_i16(&mut buf, -1);

        let mut offset = 0;
        assert_eq!(read_i16(&buf, &mut offset).unwrap(), 19);
        assert_eq!(read_i32(&buf, &mut offset).unwrap(), 100);
        assert_eq!(read_string(&buf, &mut offset).unwrap(), "test-topic");
        assert_eq!(read_i16(&buf, &mut offset).unwrap(), -1);
    }
}
