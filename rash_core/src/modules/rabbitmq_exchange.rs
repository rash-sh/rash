/// ANCHOR: module
/// # rabbitmq_exchange
///
/// Manage RabbitMQ exchanges.
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
/// - name: Create a topic exchange
///   rabbitmq_exchange:
///     name: events
///     type: topic
///     state: present
///     durable: true
///
/// - name: Create a direct exchange with arguments
///   rabbitmq_exchange:
///     name: my_exchange
///     type: direct
///     vhost: /myapp
///     durable: true
///     arguments:
///       alternate-exchange: my_dlx
///
/// - name: Create a fanout exchange
///   rabbitmq_exchange:
///     name: broadcast
///     type: fanout
///     durable: false
///     auto_delete: true
///
/// - name: Create an internal headers exchange
///   rabbitmq_exchange:
///     name: internal_events
///     type: headers
///     internal: true
///     durable: true
///
/// - name: Delete an exchange
///   rabbitmq_exchange:
///     name: old_exchange
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
use std::collections::HashMap;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_vhost() -> String {
    "/".to_string()
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the exchange to create or delete.
    pub name: String,
    /// Exchange type (direct, topic, fanout, headers).
    #[serde(default, rename = "type")]
    pub exchange_type: Option<ExchangeType>,
    /// RabbitMQ virtual host.
    /// **[default: `/`]**
    #[serde(default = "default_vhost")]
    pub vhost: String,
    /// Whether the exchange should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Durable exchange.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub durable: bool,
    /// Auto-delete exchange.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    pub auto_delete: bool,
    /// Internal exchange (cannot be published to by publishers).
    #[serde(default)]
    pub internal: Option<bool>,
    /// Exchange arguments as key-value pairs.
    #[serde(default)]
    pub arguments: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum ExchangeType {
    Direct,
    Topic,
    Fanout,
    Headers,
}

impl ExchangeType {
    fn as_str(&self) -> &'static str {
        match self {
            ExchangeType::Direct => "direct",
            ExchangeType::Topic => "topic",
            ExchangeType::Fanout => "fanout",
            ExchangeType::Headers => "headers",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExchangeInfo {
    pub name: String,
    pub exchange_type: String,
    pub durable: bool,
    pub auto_delete: bool,
    pub internal: bool,
}

fn yaml_value_to_string(val: &serde_yaml::Value) -> String {
    match val {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        _ => serde_json::to_string(val).unwrap_or_default(),
    }
}

fn run_rabbitmqctl(args: &[&str]) -> Result<String> {
    let output = Command::new("rabbitmqctl")
        .args(args)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute rabbitmqctl: {}", e),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("rabbitmqctl failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_rabbitmqadmin(args: &[&str]) -> Result<String> {
    let output = Command::new("rabbitmqadmin")
        .args(args)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute rabbitmqadmin: {}", e),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("rabbitmqadmin failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn exchange_exists(name: &str, vhost: &str) -> Result<Option<ExchangeInfo>> {
    let output = run_rabbitmqctl(&[
        "list_exchanges",
        "-p",
        vhost,
        "name",
        "type",
        "durable",
        "auto_delete",
        "internal",
    ])?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 5 && parts[0] == name {
            return Ok(Some(ExchangeInfo {
                name: name.to_string(),
                exchange_type: parts[1].trim().to_string(),
                durable: parts[2].trim() == "true",
                auto_delete: parts[3].trim() == "true",
                internal: parts[4].trim() == "true",
            }));
        }
    }

    Ok(None)
}

fn create_exchange(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create exchange '{}'", params.name)),
        ));
    }

    let exchange_type = params
        .exchange_type
        .as_ref()
        .map(|t| t.as_str())
        .unwrap_or("direct");

    let mut args = vec![
        "declare",
        "exchange",
        "--vhost",
        &params.vhost,
        "name",
        &params.name,
        "durable",
        if params.durable { "true" } else { "false" },
        "auto_delete",
        if params.auto_delete { "true" } else { "false" },
        "internal",
        if params.internal.unwrap_or(false) {
            "true"
        } else {
            "false"
        },
        "exchange_type",
        exchange_type,
    ];

    if let Some(ref arguments) = params.arguments {
        for (key, val) in arguments {
            let val_str = yaml_value_to_string(val);
            args.push("argument");
            args.push(format!("{}={}", key, val_str).leak() as &str);
        }
    }

    run_rabbitmqadmin(&args)?;

    let extra = Some(value::to_value(json!({
        "name": params.name,
        "type": exchange_type,
        "vhost": params.vhost,
        "durable": params.durable,
        "auto_delete": params.auto_delete,
        "internal": params.internal.unwrap_or(false),
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Exchange '{}' created", params.name)),
    ))
}

fn update_exchange(
    params: &Params,
    _current: &ExchangeInfo,
    check_mode: bool,
) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would update exchange '{}'", params.name)),
        ));
    }

    let exchange_type = params
        .exchange_type
        .as_ref()
        .map(|t| t.as_str())
        .unwrap_or("direct");

    let mut args = vec![
        "declare",
        "exchange",
        "--vhost",
        &params.vhost,
        "name",
        &params.name,
        "durable",
        if params.durable { "true" } else { "false" },
        "auto_delete",
        if params.auto_delete { "true" } else { "false" },
        "internal",
        if params.internal.unwrap_or(false) {
            "true"
        } else {
            "false"
        },
        "exchange_type",
        exchange_type,
    ];

    if let Some(ref arguments) = params.arguments {
        for (key, val) in arguments {
            let val_str = yaml_value_to_string(val);
            args.push("argument");
            args.push(format!("{}={}", key, val_str).leak() as &str);
        }
    }

    run_rabbitmqadmin(&args)?;

    let extra = Some(value::to_value(json!({
        "name": params.name,
        "type": exchange_type,
        "vhost": params.vhost,
        "durable": params.durable,
        "auto_delete": params.auto_delete,
        "internal": params.internal.unwrap_or(false),
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Exchange '{}' updated", params.name)),
    ))
}

fn delete_exchange(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete exchange '{}'", params.name)),
        ));
    }

    run_rabbitmqadmin(&[
        "delete",
        "exchange",
        "--vhost",
        &params.vhost,
        "name",
        &params.name,
    ])?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Exchange '{}' deleted", params.name)),
    ))
}

fn rabbitmq_exchange_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let existing = exchange_exists(&params.name, &params.vhost)?;

    match params.state {
        State::Present => match existing {
            None => create_exchange(&params, check_mode),
            Some(info) => update_exchange(&params, &info, check_mode),
        },
        State::Absent => match existing {
            None => Ok(ModuleResult::new(
                false,
                None,
                Some(format!("Exchange '{}' does not exist", params.name)),
            )),
            Some(_) => delete_exchange(&params, check_mode),
        },
    }
}

#[derive(Debug)]
pub struct RabbitmqExchange;

impl Module for RabbitmqExchange {
    fn get_name(&self) -> &str {
        "rabbitmq_exchange"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((rabbitmq_exchange_impl(params, check_mode)?, None))
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
            name: events
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "events");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.vhost, "/");
        assert!(params.durable);
        assert!(!params.auto_delete);
        assert_eq!(params.internal, None);
        assert_eq!(params.exchange_type, None);
        assert_eq!(params.arguments, None);
    }

    #[test]
    fn test_parse_params_with_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            type: topic
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "events");
        assert_eq!(params.exchange_type, Some(ExchangeType::Topic));
    }

    #[test]
    fn test_parse_params_all_types() {
        for (type_str, expected) in [
            ("direct", ExchangeType::Direct),
            ("topic", ExchangeType::Topic),
            ("fanout", ExchangeType::Fanout),
            ("headers", ExchangeType::Headers),
        ] {
            let yaml_str = format!(
                r#"
                name: test_exchange
                type: {}
                "#,
                type_str
            );
            let yaml: YamlValue = serde_norway::from_str(&yaml_str).unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert_eq!(params.exchange_type, Some(expected));
        }
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            type: topic
            state: present
            vhost: /myapp
            durable: true
            auto_delete: false
            internal: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "events");
        assert_eq!(params.exchange_type, Some(ExchangeType::Topic));
        assert_eq!(params.state, State::Present);
        assert_eq!(params.vhost, "/myapp");
        assert!(params.durable);
        assert!(!params.auto_delete);
        assert_eq!(params.internal, Some(false));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_exchange
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_exchange");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_arguments() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_exchange
            type: direct
            arguments:
              alternate-exchange: my_dlx
              x-ha-policy: all
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_exchange");
        let args = params.arguments.unwrap();
        assert_eq!(
            args.get("alternate-exchange").unwrap().as_str(),
            Some("my_dlx")
        );
    }

    #[test]
    fn test_parse_params_non_durable() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: temp_exchange
            type: fanout
            durable: false
            auto_delete: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.durable);
        assert!(params.auto_delete);
    }

    #[test]
    fn test_parse_params_internal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: internal_events
            type: headers
            internal: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.internal, Some(true));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: events
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_exchange_type_as_str() {
        assert_eq!(ExchangeType::Direct.as_str(), "direct");
        assert_eq!(ExchangeType::Topic.as_str(), "topic");
        assert_eq!(ExchangeType::Fanout.as_str(), "fanout");
        assert_eq!(ExchangeType::Headers.as_str(), "headers");
    }
}
