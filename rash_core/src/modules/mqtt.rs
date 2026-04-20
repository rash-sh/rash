/// ANCHOR: module
/// # mqtt
///
/// MQTT publish/subscribe messaging for IoT communication.
///
/// Publish and subscribe to MQTT topics with support for Quality of Service
/// levels, message retention, and authentication. Essential for IoT device
/// scripting, sensor data publishing, home automation, and edge computing.
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
/// - name: Publish sensor reading
///   mqtt:
///     topic: sensors/temperature
///     payload: "22.5"
///     broker: localhost
///
/// - name: Publish with QoS and retain
///   mqtt:
///     topic: devices/status
///     payload: '{"online": true}'
///     broker: mqtt.example.com
///     port: 1883
///     qos: 1
///     retain: true
///
/// - name: Publish with authentication
///   mqtt:
///     topic: home/living_room/thermostat
///     payload: "21.0"
///     broker: mqtt.example.com
///     username: "{{ mqtt_user }}"
///     password: "{{ mqtt_pass }}"
///     client_id: rash-thermostat
///
/// - name: Subscribe to a topic
///   mqtt:
///     topic: sensors/#
///     broker: localhost
///     state: subscribe
///   register: mqtt_messages
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

use std::time::Duration;

use rumqttc::{Client, MqttOptions, QoS, Transport};

const DEFAULT_PORT: u16 = 1883;
const DEFAULT_QUEUE_CAP: usize = 10;
const DEFAULT_SUBSCRIBE_TIMEOUT_SECS: u64 = 5;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum MqttState {
    /// Publish a message to a topic.
    #[default]
    Publish,
    /// Subscribe to a topic and wait for messages.
    Subscribe,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_qos() -> u8 {
    0
}

fn default_retain() -> bool {
    false
}

fn default_client_id() -> String {
    format!("rash-{}", uuid::Uuid::new_v4().as_simple())
}

fn default_subscribe_timeout() -> u64 {
    DEFAULT_SUBSCRIBE_TIMEOUT_SECS
}

fn default_max_messages() -> usize {
    1
}

fn qos_from_u8(val: u8) -> Result<QoS> {
    match val {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid QoS value: {val}. Must be 0, 1, or 2"),
        )),
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// MQTT topic to publish to or subscribe from.
    pub topic: String,
    /// Message content to publish (required for state=publish).
    pub payload: Option<String>,
    /// Quality of Service level (0, 1, or 2).
    /// **[default: `0`]**
    #[serde(default = "default_qos")]
    pub qos: u8,
    /// Whether to retain the message on the broker.
    /// **[default: `false`]**
    #[serde(default = "default_retain")]
    pub retain: bool,
    /// MQTT broker hostname or IP address.
    pub broker: String,
    /// MQTT broker port.
    /// **[default: `1883`]**
    #[serde(default = "default_port")]
    pub port: u16,
    /// Username for MQTT authentication.
    pub username: Option<String>,
    /// Password for MQTT authentication.
    pub password: Option<String>,
    /// MQTT client identifier.
    /// **[default: `rash-<uuid>`]**
    #[serde(default = "default_client_id")]
    pub client_id: String,
    /// Operation state: publish or subscribe.
    /// **[default: `publish`]**
    #[serde(default)]
    pub state: MqttState,
    /// Timeout in seconds to wait for messages when subscribing.
    /// **[default: `5`]**
    #[serde(default = "default_subscribe_timeout")]
    pub subscribe_timeout: u64,
    /// Maximum number of messages to collect when subscribing.
    /// **[default: `1`]**
    #[serde(default = "default_max_messages")]
    pub max_messages: usize,
    /// Enable TLS/SSL connection (use port 8883 for secure MQTT).
    /// **[default: `false`]**
    #[serde(default)]
    pub tls: bool,
}

fn build_mqtt_options(params: &Params) -> Result<MqttOptions> {
    let mut opts = MqttOptions::new(&params.client_id, &params.broker, params.port);
    opts.set_keep_alive(Duration::from_secs(5));

    if let (Some(user), Some(pass)) = (&params.username, &params.password) {
        opts.set_credentials(user, pass);
    }

    if params.tls {
        opts.set_transport(Transport::tls_with_default_config());
    }

    Ok(opts)
}

fn exec_publish(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let payload = params.payload.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "payload is required when state=publish",
        )
    })?;

    let qos = qos_from_u8(params.qos)?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "topic": params.topic,
                "payload": payload,
                "qos": params.qos,
                "retain": params.retain,
                "broker": params.broker,
                "port": params.port,
            }))?),
            Some(format!(
                "Would publish to {} on {}:{}",
                params.topic, params.broker, params.port
            )),
        ));
    }

    let mqtt_opts = build_mqtt_options(params)?;
    let (client, mut connection) = Client::new(mqtt_opts, DEFAULT_QUEUE_CAP);

    for notification in connection.iter() {
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::ConnAck(_))) => {
                client
                    .publish(&params.topic, qos, params.retain, payload.as_bytes())
                    .map_err(|e| {
                        Error::new(
                            ErrorKind::SubprocessFail,
                            format!("Failed to publish message: {e}"),
                        )
                    })?;

                let extra = value::to_value(json!({
                    "topic": params.topic,
                    "payload": payload,
                    "qos": params.qos,
                    "retain": params.retain,
                    "broker": params.broker,
                    "port": params.port,
                }))?;

                return Ok(ModuleResult::new(
                    true,
                    Some(extra),
                    Some(format!(
                        "Published to {} on {}:{}",
                        params.topic, params.broker, params.port
                    )),
                ));
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::Disconnect)) => {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    "Broker disconnected during publish",
                ));
            }
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("MQTT connection error: {e}"),
                ));
            }
            _ => {}
        }
    }

    Err(Error::new(
        ErrorKind::SubprocessFail,
        "MQTT connection closed before publish could complete",
    ))
}

fn exec_subscribe(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let qos = qos_from_u8(params.qos)?;

    if check_mode {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "topic": params.topic,
                "qos": params.qos,
                "broker": params.broker,
                "port": params.port,
                "messages": [],
            }))?),
            Some(format!(
                "Would subscribe to {} on {}:{}",
                params.topic, params.broker, params.port
            )),
        ));
    }

    let mqtt_opts = build_mqtt_options(params)?;
    let (client, mut connection) = Client::new(mqtt_opts, DEFAULT_QUEUE_CAP);

    let mut subscribed = false;
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(params.subscribe_timeout);

    for notification in connection.iter() {
        if std::time::Instant::now() > deadline && subscribed {
            break;
        }

        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::ConnAck(_))) => {
                client.subscribe(&params.topic, qos).map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to subscribe: {e}"),
                    )
                })?;
                subscribed = true;
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::SubAck(_))) => {
                trace!("Subscribed to topic: {}", params.topic);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::Publish(msg))) => {
                let payload_str = String::from_utf8_lossy(&msg.payload).to_string();
                messages.push(json!({
                    "topic": msg.topic.clone(),
                    "payload": payload_str,
                    "qos": msg.qos as u8,
                    "retain": msg.retain,
                }));

                if messages.len() >= params.max_messages {
                    break;
                }
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Incoming::Disconnect)) => {
                break;
            }
            Err(e) => {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("MQTT connection error: {e}"),
                ));
            }
            _ => {}
        }
    }

    let output = messages
        .iter()
        .filter_map(|m| m.get("payload").and_then(|p| p.as_str()).map(String::from))
        .collect::<Vec<String>>()
        .join("\n");

    let extra = value::to_value(json!({
        "topic": params.topic,
        "qos": params.qos,
        "broker": params.broker,
        "port": params.port,
        "messages": messages,
        "count": messages.len(),
    }))?;

    Ok(ModuleResult::new(
        !messages.is_empty(),
        Some(extra),
        if output.is_empty() {
            None
        } else {
            Some(output)
        },
    ))
}

pub fn mqtt(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        MqttState::Publish => exec_publish(&params, check_mode),
        MqttState::Subscribe => exec_subscribe(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Mqtt;

impl Module for Mqtt {
    fn get_name(&self) -> &str {
        "mqtt"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((mqtt(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_publish_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: sensors/temperature
            payload: "22.5"
            broker: localhost
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.topic, "sensors/temperature");
        assert_eq!(params.payload, Some("22.5".to_string()));
        assert_eq!(params.broker, "localhost");
        assert_eq!(params.state, MqttState::Publish);
        assert_eq!(params.qos, 0);
        assert!(!params.retain);
        assert_eq!(params.port, DEFAULT_PORT);
    }

    #[test]
    fn test_parse_params_publish_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: devices/status
            payload: '{"online": true}'
            broker: mqtt.example.com
            port: 8883
            qos: 2
            retain: true
            username: admin
            password: secret
            client_id: my-device
            tls: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.topic, "devices/status");
        assert_eq!(params.payload, Some("{\"online\": true}".to_string()));
        assert_eq!(params.broker, "mqtt.example.com");
        assert_eq!(params.port, 8883);
        assert_eq!(params.qos, 2);
        assert!(params.retain);
        assert_eq!(params.username, Some("admin".to_string()));
        assert_eq!(params.password, Some("secret".to_string()));
        assert_eq!(params.client_id, "my-device");
        assert_eq!(params.state, MqttState::Publish);
        assert!(params.tls);
    }

    #[test]
    fn test_parse_params_subscribe() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: sensors/#
            broker: localhost
            state: subscribe
            subscribe_timeout: 10
            max_messages: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.topic, "sensors/#");
        assert_eq!(params.broker, "localhost");
        assert_eq!(params.state, MqttState::Subscribe);
        assert_eq!(params.subscribe_timeout, 10);
        assert_eq!(params.max_messages, 5);
        assert_eq!(params.payload, None);
    }

    #[test]
    fn test_parse_params_missing_topic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            payload: "test"
            broker: localhost
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_broker() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test/topic
            payload: "data"
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_qos_from_u8() {
        assert_eq!(qos_from_u8(0).unwrap(), QoS::AtMostOnce);
        assert_eq!(qos_from_u8(1).unwrap(), QoS::AtLeastOnce);
        assert_eq!(qos_from_u8(2).unwrap(), QoS::ExactlyOnce);
        assert!(qos_from_u8(3).is_err());
        assert!(qos_from_u8(99).is_err());
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test
            broker: localhost
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.port, DEFAULT_PORT);
        assert_eq!(params.qos, 0);
        assert!(!params.retain);
        assert_eq!(params.state, MqttState::Publish);
        assert_eq!(params.subscribe_timeout, DEFAULT_SUBSCRIBE_TIMEOUT_SECS);
        assert_eq!(params.max_messages, 1);
        assert!(!params.tls);
        assert!(params.username.is_none());
        assert!(params.password.is_none());
        assert!(params.client_id.starts_with("rash-"));
    }

    #[test]
    fn test_check_mode_publish() {
        let mqtt_module = Mqtt;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test/topic
            payload: "hello"
            broker: localhost
            "#,
        )
        .unwrap();
        let (result, _) = mqtt_module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would publish"));
    }

    #[test]
    fn test_check_mode_subscribe() {
        let mqtt_module = Mqtt;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test/topic
            broker: localhost
            state: subscribe
            "#,
        )
        .unwrap();
        let (result, _) = mqtt_module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(!result.get_changed());
        assert!(result.get_output().unwrap().contains("Would subscribe"));
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: secure/data
            payload: "sensitive"
            broker: broker.example.com
            username: user
            password: pass
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.username, Some("user".to_string()));
        assert_eq!(params.password, Some("pass".to_string()));
    }

    #[test]
    fn test_build_mqtt_options_basic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test
            broker: localhost
            client_id: test-client
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let opts = build_mqtt_options(&params).unwrap();
        assert_eq!(opts.client_id(), "test-client");
    }

    #[test]
    fn test_build_mqtt_options_with_credentials() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test
            broker: localhost
            username: admin
            password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let opts = build_mqtt_options(&params).unwrap();
        assert_eq!(opts.client_id(), params.client_id);
    }

    #[test]
    fn test_build_mqtt_options_with_tls() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            topic: test
            broker: secure-broker.example.com
            port: 8883
            tls: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let opts = build_mqtt_options(&params).unwrap();
        assert_eq!(opts.client_id(), params.client_id);
    }
}
