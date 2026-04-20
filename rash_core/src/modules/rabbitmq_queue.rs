/// ANCHOR: module
/// # rabbitmq_queue
///
/// Manage RabbitMQ queues, exchanges, and bindings.
///
/// Requires the RabbitMQ management plugin and `rabbitmqadmin` CLI tool.
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
/// - name: Create a durable queue
///   rabbitmq_queue:
///     name: task_queue
///     durable: true
///     state: present
///
/// - name: Create a quorum queue
///   rabbitmq_queue:
///     name: orders
///     durable: true
///     type: quorum
///     state: present
///
/// - name: Create a queue and bind to an exchange
///   rabbitmq_queue:
///     name: task_queue
///     durable: true
///     exchange: tasks
///     routing_key: task.#
///     state: present
///
/// - name: Create a queue on a specific vhost
///   rabbitmq_queue:
///     name: my_queue
///     vhost: /myapp
///     durable: true
///     state: present
///
/// - name: Delete a queue
///   rabbitmq_queue:
///     name: old_queue
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
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_vhost() -> String {
    "/".to_string()
}

fn default_durable() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the RabbitMQ queue.
    pub name: String,
    /// Whether the queue should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Queue durability.
    /// **[default: `true`]**
    #[serde(default = "default_durable")]
    pub durable: bool,
    /// Exchange to bind the queue to.
    pub exchange: Option<String>,
    /// Routing key for the binding.
    pub routing_key: Option<String>,
    /// Queue type (classic, quorum, or stream).
    pub r#type: Option<QueueType>,
    /// RabbitMQ virtual host.
    /// **[default: `/`]**
    #[serde(default = "default_vhost")]
    pub vhost: String,
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
pub enum QueueType {
    Classic,
    Quorum,
    Stream,
}

impl QueueType {
    fn as_str(&self) -> &str {
        match self {
            QueueType::Classic => "classic",
            QueueType::Quorum => "quorum",
            QueueType::Stream => "stream",
        }
    }
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

fn queue_exists(name: &str, vhost: &str) -> Result<bool> {
    let vhost_arg = format!("--vhost={}", vhost);
    let args = vec!["list", "queues", &vhost_arg, "name"];
    let output = run_rabbitmqadmin(&args)?;

    for line in output.lines() {
        if line.trim() == name {
            return Ok(true);
        }
    }

    Ok(false)
}

fn binding_exists(exchange: &str, queue: &str, routing_key: &str, vhost: &str) -> Result<bool> {
    let output = run_rabbitmqadmin(&[
        "list",
        "bindings",
        "-p",
        vhost,
        "source",
        "destination",
        "routing_key",
    ])?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3
            && parts[0].trim() == exchange
            && parts[1].trim() == queue
            && parts[2].trim() == routing_key
        {
            return Ok(true);
        }
    }

    Ok(false)
}

fn create_queue(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create queue '{}'", params.name)),
        ));
    }

    let durable_arg = format!("durable={}", if params.durable { "true" } else { "false" });
    let vhost_arg = format!("--vhost={}", params.vhost);
    let mut final_args: Vec<String> = vec![
        "declare".to_string(),
        "queue".to_string(),
        format!("name={}", params.name),
        durable_arg,
        vhost_arg,
    ];

    if let Some(ref queue_type) = params.r#type {
        final_args.push(format!(
            "arguments={{\"x-queue-type\":\"{}\"}}",
            queue_type.as_str()
        ));
    }

    let owned_args: Vec<&str> = final_args.iter().map(|s| s.as_str()).collect();
    run_rabbitmqadmin(&owned_args)?;

    if let Some(ref exchange) = params.exchange {
        let routing_key = params.routing_key.as_deref().unwrap_or(&params.name);

        let vhost_arg = format!("--vhost={}", params.vhost);
        let bind_args: Vec<&str> = vec![
            "declare",
            "binding",
            "source",
            exchange,
            "destination",
            &params.name,
            "routing_key",
            routing_key,
            &vhost_arg,
        ];
        run_rabbitmqadmin(&bind_args)?;
    }

    let extra = Some(value::to_value(json!({
        "name": params.name,
        "durable": params.durable,
        "queue_type": params.r#type.as_ref().map(|t| t.as_str()),
        "vhost": params.vhost,
        "exchange": params.exchange,
        "routing_key": params.routing_key,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Queue '{}' created", params.name)),
    ))
}

fn ensure_queue(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let exists = queue_exists(&params.name, &params.vhost)?;

    if exists {
        let mut changed = false;
        let mut changes: Vec<&str> = vec![];

        if let Some(ref exchange) = params.exchange {
            let routing_key = params.routing_key.as_deref().unwrap_or(&params.name);

            let has_binding = binding_exists(exchange, &params.name, routing_key, &params.vhost)?;

            if !has_binding {
                if !check_mode {
                    let vhost_arg = format!("--vhost={}", params.vhost);
                    let bind_args: Vec<&str> = vec![
                        "declare",
                        "binding",
                        "source",
                        exchange,
                        "destination",
                        &params.name,
                        "routing_key",
                        routing_key,
                        &vhost_arg,
                    ];
                    run_rabbitmqadmin(&bind_args)?;
                }
                changed = true;
                changes.push("binding");
            }
        }

        let extra = Some(value::to_value(json!({
            "name": params.name,
            "changed": changed,
            "changes": changes,
        }))?);

        if changed {
            Ok(ModuleResult::new(
                true,
                extra,
                Some(format!("Queue '{}' updated", params.name)),
            ))
        } else {
            Ok(ModuleResult::new(
                false,
                extra,
                Some(format!("Queue '{}' already exists", params.name)),
            ))
        }
    } else {
        create_queue(params, check_mode)
    }
}

fn delete_queue(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let exists = queue_exists(&params.name, &params.vhost)?;

    if !exists {
        return Ok(ModuleResult::new(
            false,
            None,
            Some(format!("Queue '{}' does not exist", params.name)),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete queue '{}'", params.name)),
        ));
    }

    let vhost_arg = format!("--vhost={}", params.vhost);
    run_rabbitmqadmin(&["delete", "queue", "name", &params.name, &vhost_arg])?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Queue '{}' deleted", params.name)),
    ))
}

fn rabbitmq_queue_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state {
        State::Present => ensure_queue(&params, check_mode),
        State::Absent => delete_queue(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct RabbitmqQueue;

impl Module for RabbitmqQueue {
    fn get_name(&self) -> &str {
        "rabbitmq_queue"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((rabbitmq_queue_impl(params, check_mode)?, None))
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
            name: task_queue
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "task_queue");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.vhost, "/");
        assert!(params.durable);
        assert_eq!(params.exchange, None);
        assert_eq!(params.routing_key, None);
        assert_eq!(params.r#type, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            durable: true
            state: present
            exchange: tasks
            routing_key: "task.#"
            type: quorum
            vhost: /myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "task_queue");
        assert!(params.durable);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.exchange, Some("tasks".to_string()));
        assert_eq!(params.routing_key, Some("task.#".to_string()));
        assert_eq!(params.r#type, Some(QueueType::Quorum));
        assert_eq!(params.vhost, "/myapp");
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_queue
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_queue");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_stream_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: event_stream
            type: stream
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.r#type, Some(QueueType::Stream));
    }

    #[test]
    fn test_parse_params_classic_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: classic_queue
            type: classic
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.r#type, Some(QueueType::Classic));
    }

    #[test]
    fn test_parse_params_non_durable() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: temp_queue
            durable: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.durable);
    }

    #[test]
    fn test_parse_params_with_binding() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            exchange: tasks
            routing_key: "task.#"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.exchange, Some("tasks".to_string()));
        assert_eq!(params.routing_key, Some("task.#".to_string()));
    }

    #[test]
    fn test_queue_type_as_str() {
        assert_eq!(QueueType::Classic.as_str(), "classic");
        assert_eq!(QueueType::Quorum.as_str(), "quorum");
        assert_eq!(QueueType::Stream.as_str(), "stream");
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
