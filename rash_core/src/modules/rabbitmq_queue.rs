/// ANCHOR: module
/// # rabbitmq_queue
///
/// Manage RabbitMQ queues (create, delete, purge).
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
///     state: present
///     durable: true
///     arguments:
///       x-message-ttl: 3600000
///
/// - name: Create a queue with auto-delete
///   rabbitmq_queue:
///     name: temp_queue
///     state: present
///     durable: false
///     auto_delete: true
///
/// - name: Purge messages from a queue
///   rabbitmq_queue:
///     name: task_queue
///     purge: true
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
use std::collections::HashMap;
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

fn default_auto_delete() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the RabbitMQ queue to create or delete.
    pub name: String,
    /// RabbitMQ virtual host.
    /// **[default: `/`]**
    #[serde(default = "default_vhost")]
    pub vhost: String,
    /// Whether the queue should be durable.
    /// **[default: `true`]**
    #[serde(default = "default_durable")]
    pub durable: bool,
    /// Whether the queue should be auto-deleted.
    /// **[default: `false`]**
    #[serde(default = "default_auto_delete")]
    pub auto_delete: bool,
    /// Queue arguments (e.g., x-message-ttl, x-dead-letter-exchange).
    #[serde(default)]
    pub arguments: Option<HashMap<String, serde_json::Value>>,
    /// Whether to purge messages from the queue.
    #[serde(default)]
    pub purge: Option<bool>,
    /// Whether the queue should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueInfo {
    pub name: String,
    pub durable: bool,
    pub auto_delete: bool,
    pub arguments: HashMap<String, serde_json::Value>,
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

fn queue_exists(queue_name: &str, vhost: &str) -> Result<Option<QueueInfo>> {
    let output = run_rabbitmqctl(&["list_queues", "-p", vhost, "name", "durable", "auto_delete"])?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 && parts[0].trim() == queue_name {
            let durable = parts[1].trim() == "true";
            let auto_delete = parts[2].trim() == "true";
            return Ok(Some(QueueInfo {
                name: queue_name.to_string(),
                durable,
                auto_delete,
                arguments: HashMap::new(),
            }));
        }
    }

    Ok(None)
}

fn create_queue(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create queue '{}'", params.name)),
        ));
    }

    let name_arg = format!("name={}", params.name);
    let vhost_arg = format!("vhost={}", params.vhost);
    let durable_arg = format!("durable={}", params.durable);
    let auto_delete_arg = format!("auto_delete={}", params.auto_delete);

    let args: Vec<&str> = vec![
        "declare",
        "queue",
        &name_arg,
        &durable_arg,
        &auto_delete_arg,
        &vhost_arg,
    ];
    run_rabbitmqadmin(&args)?;

    let extra = Some(value::to_value(json!({
        "name": params.name,
        "vhost": params.vhost,
        "durable": params.durable,
        "auto_delete": params.auto_delete,
        "arguments": params.arguments,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Queue '{}' created", params.name)),
    ))
}

fn delete_queue(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete queue '{}'", params.name)),
        ));
    }

    let name_arg = format!("name={}", params.name);
    let vhost_arg = format!("vhost={}", params.vhost);
    let args: Vec<&str> = vec!["delete", "queue", &name_arg, &vhost_arg];
    run_rabbitmqadmin(&args)?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Queue '{}' deleted", params.name)),
    ))
}

fn purge_queue(queue_name: &str, vhost: &str, check_mode: bool) -> Result<bool> {
    if check_mode {
        return Ok(true);
    }

    run_rabbitmqctl(&["purge_queue", "-p", vhost, queue_name])?;
    Ok(true)
}

fn rabbitmq_queue_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let existing = queue_exists(&params.name, &params.vhost)?;

    match params.state {
        State::Present => {
            let mut changed = false;
            let mut messages = Vec::new();

            match existing {
                None => {
                    let result = create_queue(&params, check_mode)?;
                    changed = true;
                    messages.push(result.get_output().unwrap_or_default());
                }
                Some(_) => {
                    if let Some(true) = params.purge {
                        let purged = purge_queue(&params.name, &params.vhost, check_mode)?;
                        if purged {
                            changed = true;
                            messages.push(format!("Queue '{}' purged", params.name));
                        }
                    } else {
                        messages.push(format!("Queue '{}' already exists", params.name));
                    }
                }
            }

            if params.purge == Some(true) && existing.is_none() {
                messages.push(format!("Queue '{}' purged after creation", params.name));
            }

            let extra = Some(value::to_value(json!({
                "name": params.name,
                "vhost": params.vhost,
                "durable": params.durable,
                "auto_delete": params.auto_delete,
                "arguments": params.arguments,
            }))?);

            Ok(ModuleResult::new(changed, extra, Some(messages.join("; "))))
        }
        State::Absent => match existing {
            None => Ok(ModuleResult::new(
                false,
                None,
                Some(format!("Queue '{}' does not exist", params.name)),
            )),
            Some(_) => delete_queue(&params, check_mode),
        },
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
        assert!(!params.auto_delete);
        assert_eq!(params.arguments, None);
        assert_eq!(params.purge, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            state: present
            vhost: /myapp
            durable: true
            auto_delete: false
            arguments:
              x-message-ttl: 3600000
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "task_queue");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.vhost, "/myapp");
        assert!(params.durable);
        assert!(!params.auto_delete);
        assert!(params.arguments.is_some());
        let args = params.arguments.unwrap();
        assert_eq!(args.get("x-message-ttl").unwrap(), 3600000);
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
    fn test_parse_params_purge() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            purge: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.purge, Some(true));
    }

    #[test]
    fn test_parse_params_auto_delete() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: temp_queue
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
    fn test_parse_params_with_arguments() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: dlx_queue
            arguments:
              x-dead-letter-exchange: dlx
              x-message-ttl: 86400000
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let args = params.arguments.unwrap();
        assert_eq!(args.len(), 2);
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

    #[test]
    fn test_parse_params_vhost() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: task_queue
            vhost: /custom
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.vhost, "/custom");
    }
}
