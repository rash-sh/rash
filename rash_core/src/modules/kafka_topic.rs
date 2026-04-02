/// ANCHOR: module
/// # kafka_topic
///
/// Manage Apache Kafka topics for messaging systems.
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
/// - name: Create a Kafka topic
///   kafka_topic:
///     name: my-topic
///     state: present
///     partitions: 3
///     replication_factor: 1
///     bootstrap_servers: localhost:9092
///
/// - name: Create a topic with custom configuration
///   kafka_topic:
///     name: my-topic
///     state: present
///     partitions: 6
///     replication_factor: 2
///     bootstrap_servers: kafka1:9092,kafka2:9092
///     config:
///       retention.ms: 86400000
///       segment.bytes: 1073741824
///
/// - name: Delete a Kafka topic
///   kafka_topic:
///     name: my-topic
///     state: absent
///     bootstrap_servers: localhost:9092
///
/// - name: Modify topic configuration
///   kafka_topic:
///     name: my-topic
///     state: present
///     bootstrap_servers: localhost:9092
///     config:
///       retention.ms: 604800000
///
/// - name: Connect with SASL authentication
///   kafka_topic:
///     name: my-topic
///     state: present
///     bootstrap_servers: kafka.example.com:9092
///     sasl_mechanism: PLAIN
///     sasl_username: user
///     sasl_password: secret
///     security_protocol: SASL_PLAINTEXT
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
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

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum SecurityProtocol {
    Plaintext,
    SaslPlaintext,
    SaslSsl,
    Ssl,
}

impl std::fmt::Display for SecurityProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityProtocol::Plaintext => write!(f, "PLAINTEXT"),
            SecurityProtocol::SaslPlaintext => write!(f, "SASL_PLAINTEXT"),
            SecurityProtocol::SaslSsl => write!(f, "SASL_SSL"),
            SecurityProtocol::Ssl => write!(f, "SSL"),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the Kafka topic to manage.
    pub name: String,
    /// The desired state of the topic.
    #[serde(default)]
    pub state: State,
    /// Number of partitions for the topic.
    pub partitions: Option<i32>,
    /// Replication factor for the topic.
    pub replication_factor: Option<i32>,
    /// Topic configuration overrides.
    pub config: Option<HashMap<String, String>>,
    /// Comma-separated list of Kafka broker hosts and ports.
    pub bootstrap_servers: String,
    /// Zookeeper connection string (deprecated, use bootstrap_servers instead).
    pub zookeeper: Option<String>,
    /// Kafka API version to use.
    pub api_version: Option<String>,
    /// Security protocol for connection.
    pub security_protocol: Option<SecurityProtocol>,
    /// SASL mechanism for authentication.
    pub sasl_mechanism: Option<String>,
    /// SASL username for authentication.
    pub sasl_username: Option<String>,
    /// SASL password for authentication.
    pub sasl_password: Option<String>,
    /// SSL truststore location.
    pub ssl_truststore_location: Option<String>,
    /// SSL truststore password.
    pub ssl_truststore_password: Option<String>,
    /// SSL keystore location.
    pub ssl_keystore_location: Option<String>,
    /// SSL keystore password.
    pub ssl_keystore_password: Option<String>,
}

fn find_kafka_topics_command() -> Result<String> {
    let commands = ["kafka-topics.sh", "kafka-topics"];

    for cmd in commands {
        if Command::new(cmd).arg("--version").output().is_ok() {
            return Ok(cmd.to_string());
        }
        let output = Command::new("which").arg(cmd).output();
        if let Ok(output) = output
            && output.status.success()
        {
            return Ok(cmd.to_string());
        }
    }

    Err(Error::new(
        ErrorKind::NotFound,
        "kafka-topics command not found. Please install Kafka tools.",
    ))
}

fn topic_exists(params: &Params, kafka_cmd: &str) -> Result<bool> {
    let mut args = vec![
        "--bootstrap-server".to_string(),
        params.bootstrap_servers.clone(),
        "--list".to_string(),
    ];

    if let Some(ref api_version) = params.api_version {
        args.push(format!("--api-version-request={}", api_version));
    }

    trace!("Checking if topic exists: {} {:?}", kafka_cmd, args);

    let mut cmd = Command::new(kafka_cmd);
    cmd.args(&args);

    if params.sasl_password.is_some() {
        cmd.env("KAFKA_OPTS", "-Djava.security.auth.login.config=/dev/null");
    }

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::new(ErrorKind::NotFound, "kafka-topics command not found")
        } else {
            Error::new(ErrorKind::SubprocessFail, e)
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to list topics: {}", stderr),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|line| line.trim() == params.name))
}

fn get_topic_config(params: &Params, kafka_cmd: &str) -> Result<HashMap<String, String>> {
    let args = vec![
        "--bootstrap-server".to_string(),
        params.bootstrap_servers.clone(),
        "--describe".to_string(),
        "--topic".to_string(),
        params.name.clone(),
    ];

    trace!("Getting topic config: {} {:?}", kafka_cmd, args);

    let output = Command::new(kafka_cmd)
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Ok(HashMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut config = HashMap::new();

    for line in stdout.lines() {
        if line.contains("Configs:") {
            let configs_part = line.split("Configs:").nth(1).unwrap_or("");
            for config_entry in configs_part.split(',') {
                let trimmed = config_entry.trim();
                if trimmed.starts_with(|c: char| c.is_lowercase())
                    && let Some((key, value)) = trimmed.split_once('=')
                {
                    config.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
    }

    Ok(config)
}

fn create_topic(params: &Params, kafka_cmd: &str, check_mode: bool) -> Result<bool> {
    if topic_exists(params, kafka_cmd)? {
        return modify_topic_config(params, kafka_cmd, check_mode);
    }

    if check_mode {
        return Ok(true);
    }

    let partitions = params.partitions.unwrap_or(1);
    let replication_factor = params.replication_factor.unwrap_or(1);

    let mut args = vec![
        "--bootstrap-server".to_string(),
        params.bootstrap_servers.clone(),
        "--create".to_string(),
        "--topic".to_string(),
        params.name.clone(),
        "--partitions".to_string(),
        partitions.to_string(),
        "--replication-factor".to_string(),
        replication_factor.to_string(),
    ];

    if let Some(ref config) = params.config {
        let config_str = config
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",");
        if !config_str.is_empty() {
            args.push("--config".to_string());
            args.push(config_str);
        }
    }

    if let Some(ref api_version) = params.api_version {
        args.push(format!("--api-version-request={}", api_version));
    }

    trace!("Creating topic: {} {:?}", kafka_cmd, args);

    let output = Command::new(kafka_cmd)
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create topic '{}': {}", params.name, stderr),
        ));
    }

    Ok(true)
}

fn modify_topic_config(params: &Params, kafka_cmd: &str, check_mode: bool) -> Result<bool> {
    let current_config = get_topic_config(params, kafka_cmd)?;
    let desired_config = params.config.as_ref();

    if desired_config.is_none() {
        return Ok(false);
    }

    let desired = desired_config.unwrap();
    let mut changes = HashMap::new();

    for (key, value) in desired {
        if current_config.get(key) != Some(value) {
            changes.insert(key.clone(), value.clone());
        }
    }

    if changes.is_empty() {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let config_str = changes
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",");

    let args = vec![
        "--bootstrap-server".to_string(),
        params.bootstrap_servers.clone(),
        "--alter".to_string(),
        "--topic".to_string(),
        params.name.clone(),
        "--config".to_string(),
        config_str,
    ];

    trace!("Modifying topic config: {} {:?}", kafka_cmd, args);

    let output = Command::new(kafka_cmd)
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to alter topic '{}': {}", params.name, stderr),
        ));
    }

    Ok(true)
}

fn delete_topic(params: &Params, kafka_cmd: &str, check_mode: bool) -> Result<bool> {
    if !topic_exists(params, kafka_cmd)? {
        return Ok(false);
    }

    if check_mode {
        return Ok(true);
    }

    let mut args = vec![
        "--bootstrap-server".to_string(),
        params.bootstrap_servers.clone(),
        "--delete".to_string(),
        "--topic".to_string(),
        params.name.clone(),
    ];

    if let Some(ref api_version) = params.api_version {
        args.push(format!("--api-version-request={}", api_version));
    }

    trace!("Deleting topic: {} {:?}", kafka_cmd, args);

    let output = Command::new(kafka_cmd)
        .args(&args)
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to delete topic '{}': {}", params.name, stderr),
        ));
    }

    Ok(true)
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
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let kafka_cmd = find_kafka_topics_command()?;

        let changed = match params.state {
            State::Present => create_topic(&params, &kafka_cmd, check_mode)?,
            State::Absent => delete_topic(&params, &kafka_cmd, check_mode)?,
        };

        let extra = Some(value::to_value(json!({
            "topic": params.name,
            "state": params.state.to_string(),
            "partitions": params.partitions,
            "replication_factor": params.replication_factor,
        }))?);

        Ok((
            ModuleResult::new(
                changed,
                extra,
                Some(format!("Topic '{}' is {}", params.name, params.state)),
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
name: my-topic
bootstrap_servers: localhost:9092
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "my-topic");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.bootstrap_servers, "localhost:9092");
    }

    #[test]
    fn test_parse_params_full() {
        let yaml = r#"
name: my-topic
state: present
partitions: 6
replication_factor: 3
bootstrap_servers: kafka1:9092,kafka2:9092
config:
  retention.ms: "86400000"
  segment.bytes: "1073741824"
api_version: "3"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "my-topic");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.partitions, Some(6));
        assert_eq!(params.replication_factor, Some(3));
        assert_eq!(params.bootstrap_servers, "kafka1:9092,kafka2:9092");

        let config = params.config.unwrap();
        assert_eq!(config.get("retention.ms"), Some(&"86400000".to_string()));
        assert_eq!(config.get("segment.bytes"), Some(&"1073741824".to_string()));
        assert_eq!(params.api_version, Some("3".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml = r#"
name: old-topic
state: absent
bootstrap_servers: localhost:9092
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "old-topic");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_sasl() {
        let yaml = r#"
name: my-topic
bootstrap_servers: kafka.example.com:9092
security_protocol: saslplaintext
sasl_mechanism: PLAIN
sasl_username: admin
sasl_password: secret
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(
            params.security_protocol,
            Some(SecurityProtocol::SaslPlaintext)
        );
        assert_eq!(params.sasl_mechanism, Some("PLAIN".to_string()));
        assert_eq!(params.sasl_username, Some("admin".to_string()));
        assert_eq!(params.sasl_password, Some("secret".to_string()));
    }

    #[test]
    fn test_state_display() {
        assert_eq!(State::Present.to_string(), "present");
        assert_eq!(State::Absent.to_string(), "absent");
    }

    #[test]
    fn test_security_protocol_display() {
        assert_eq!(SecurityProtocol::Plaintext.to_string(), "PLAINTEXT");
        assert_eq!(
            SecurityProtocol::SaslPlaintext.to_string(),
            "SASL_PLAINTEXT"
        );
        assert_eq!(SecurityProtocol::SaslSsl.to_string(), "SASL_SSL");
        assert_eq!(SecurityProtocol::Ssl.to_string(), "SSL");
    }

    #[test]
    fn test_parse_params_with_ssl() {
        let yaml = r#"
name: my-topic
bootstrap_servers: kafka.example.com:9093
security_protocol: ssl
ssl_truststore_location: /path/to/truststore.jks
ssl_truststore_password: trustpass
ssl_keystore_location: /path/to/keystore.jks
ssl_keystore_password: keypass
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.security_protocol, Some(SecurityProtocol::Ssl));
        assert_eq!(
            params.ssl_truststore_location,
            Some("/path/to/truststore.jks".to_string())
        );
        assert_eq!(
            params.ssl_truststore_password,
            Some("trustpass".to_string())
        );
        assert_eq!(
            params.ssl_keystore_location,
            Some("/path/to/keystore.jks".to_string())
        );
        assert_eq!(params.ssl_keystore_password, Some("keypass".to_string()));
    }

    #[test]
    fn test_parse_params_with_zookeeper() {
        let yaml = r#"
name: my-topic
bootstrap_servers: localhost:9092
zookeeper: localhost:2181
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.zookeeper, Some("localhost:2181".to_string()));
    }
}
