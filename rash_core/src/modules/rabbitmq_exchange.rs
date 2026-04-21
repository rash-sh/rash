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
///     name: my_exchange
///     type: topic
///     durable: true
///
/// - name: Create a durable direct exchange on a specific vhost
///   rabbitmq_exchange:
///     name: my_direct
///     type: direct
///     durable: true
///     vhost: /myapp
///
/// - name: Create a fanout exchange with authentication
///   rabbitmq_exchange:
///     name: my_fanout
///     type: fanout
///     durable: false
///     login_user: admin
///     login_password: secret
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
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::process::Command;

fn default_state() -> State {
    State::Present
}

fn default_durable() -> bool {
    true
}

fn default_vhost() -> String {
    "/".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Exchange name.
    pub name: String,
    /// Exchange type (direct, topic, fanout, headers).
    #[serde(default, rename = "type")]
    pub exchange_type: Option<ExchangeType>,
    /// Whether the exchange should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: State,
    /// Whether exchange survives broker restart.
    /// **[default: `true`]**
    #[serde(default = "default_durable")]
    pub durable: bool,
    /// RabbitMQ virtual host.
    /// **[default: `/`]**
    #[serde(default = "default_vhost")]
    pub vhost: String,
    /// RabbitMQ user for authentication.
    pub login_user: Option<String>,
    /// RabbitMQ password for authentication.
    pub login_password: Option<String>,
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
}

fn build_rabbitmqadmin_base_args(params: &Params) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(ref user) = params.login_user {
        args.push("--username".to_string());
        args.push(user.clone());
    }
    if let Some(ref password) = params.login_password {
        args.push("--password".to_string());
        args.push(password.clone());
    }
    args.push("--vhost".to_string());
    args.push(params.vhost.clone());
    args
}

fn run_rabbitmqadmin(args: &[String]) -> Result<String> {
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

fn exchange_exists(params: &Params) -> Result<Option<ExchangeInfo>> {
    let mut args = build_rabbitmqadmin_base_args(params);
    args.push("list".to_string());
    args.push("exchanges".to_string());
    args.push("name".to_string());
    args.push("type".to_string());
    args.push("durable".to_string());

    let output = run_rabbitmqadmin(&args)?;

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if parts.len() >= 3 && parts[0] == params.name {
            let durable = parts[2].to_lowercase() == "true";
            return Ok(Some(ExchangeInfo {
                name: params.name.clone(),
                exchange_type: parts[1].to_string(),
                durable,
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

    let mut args = build_rabbitmqadmin_base_args(params);
    args.push("declare".to_string());
    args.push("exchange".to_string());
    args.push("name".to_string());
    args.push(params.name.clone());
    args.push("type".to_string());
    args.push(exchange_type.to_string());
    args.push("durable".to_string());
    args.push(params.durable.to_string());

    run_rabbitmqadmin(&args)?;

    let extra = Some(value::to_value(json!({
        "name": params.name,
        "type": exchange_type,
        "durable": params.durable,
        "vhost": params.vhost,
    }))?);

    Ok(ModuleResult::new(
        true,
        extra,
        Some(format!("Exchange '{}' created", params.name)),
    ))
}

fn needs_update(params: &Params, current: &ExchangeInfo) -> bool {
    let exchange_type = params
        .exchange_type
        .as_ref()
        .map(|t| t.as_str())
        .unwrap_or("direct");

    current.exchange_type != exchange_type || current.durable != params.durable
}

fn delete_exchange(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would delete exchange '{}'", params.name)),
        ));
    }

    let mut args = build_rabbitmqadmin_base_args(params);
    args.push("delete".to_string());
    args.push("exchange".to_string());
    args.push("name".to_string());
    args.push(params.name.clone());

    run_rabbitmqadmin(&args)?;

    Ok(ModuleResult::new(
        true,
        None,
        Some(format!("Exchange '{}' deleted", params.name)),
    ))
}

fn rabbitmq_exchange_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let existing = exchange_exists(&params)?;

    match params.state {
        State::Present => match existing {
            None => create_exchange(&params, check_mode),
            Some(ref info) if needs_update(&params, info) => {
                if check_mode {
                    return Ok(ModuleResult::new(
                        true,
                        None,
                        Some(format!("Would update exchange '{}'", params.name)),
                    ));
                }
                let mut args = build_rabbitmqadmin_base_args(&params);
                args.push("delete".to_string());
                args.push("exchange".to_string());
                args.push("name".to_string());
                args.push(params.name.clone());
                run_rabbitmqadmin(&args)?;

                create_exchange(&params, false)
            }
            Some(_) => Ok(ModuleResult::new(
                false,
                None,
                Some(format!("Exchange '{}' already exists with correct settings", params.name)),
            )),
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
            name: my_exchange
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_exchange");
        assert_eq!(params.state, State::Present);
        assert!(params.durable);
        assert_eq!(params.vhost, "/");
        assert!(params.exchange_type.is_none());
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_exchange
            type: topic
            durable: false
            vhost: /myapp
            login_user: admin
            login_password: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_exchange");
        assert_eq!(params.exchange_type, Some(ExchangeType::Topic));
        assert!(!params.durable);
        assert_eq!(params.vhost, "/myapp");
        assert_eq!(params.login_user, Some("admin".to_string()));
        assert_eq!(params.login_password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_direct() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_direct
            type: direct
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.exchange_type, Some(ExchangeType::Direct));
    }

    #[test]
    fn test_parse_params_fanout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_fanout
            type: fanout
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.exchange_type, Some(ExchangeType::Fanout));
    }

    #[test]
    fn test_parse_params_headers() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_headers
            type: headers
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.exchange_type, Some(ExchangeType::Headers));
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
    fn test_exchange_type_as_str() {
        assert_eq!(ExchangeType::Direct.as_str(), "direct");
        assert_eq!(ExchangeType::Topic.as_str(), "topic");
        assert_eq!(ExchangeType::Fanout.as_str(), "fanout");
        assert_eq!(ExchangeType::Headers.as_str(), "headers");
    }

    #[test]
    fn test_needs_update_no_change() {
        let params = Params {
            name: "test".to_string(),
            exchange_type: Some(ExchangeType::Topic),
            state: State::Present,
            durable: true,
            vhost: "/".to_string(),
            login_user: None,
            login_password: None,
        };
        let info = ExchangeInfo {
            name: "test".to_string(),
            exchange_type: "topic".to_string(),
            durable: true,
        };
        assert!(!needs_update(&params, &info));
    }

    #[test]
    fn test_needs_update_type_changed() {
        let params = Params {
            name: "test".to_string(),
            exchange_type: Some(ExchangeType::Fanout),
            state: State::Present,
            durable: true,
            vhost: "/".to_string(),
            login_user: None,
            login_password: None,
        };
        let info = ExchangeInfo {
            name: "test".to_string(),
            exchange_type: "topic".to_string(),
            durable: true,
        };
        assert!(needs_update(&params, &info));
    }

    #[test]
    fn test_needs_update_durable_changed() {
        let params = Params {
            name: "test".to_string(),
            exchange_type: Some(ExchangeType::Topic),
            state: State::Present,
            durable: false,
            vhost: "/".to_string(),
            login_user: None,
            login_password: None,
        };
        let info = ExchangeInfo {
            name: "test".to_string(),
            exchange_type: "topic".to_string(),
            durable: true,
        };
        assert!(needs_update(&params, &info));
    }

    #[test]
    fn test_build_rabbitmqadmin_base_args_no_auth() {
        let params = Params {
            name: "test".to_string(),
            exchange_type: None,
            state: State::Present,
            durable: true,
            vhost: "/myapp".to_string(),
            login_user: None,
            login_password: None,
        };
        let args = build_rabbitmqadmin_base_args(&params);
        assert_eq!(args, vec!["--vhost", "/myapp"]);
    }

    #[test]
    fn test_build_rabbitmqadmin_base_args_with_auth() {
        let params = Params {
            name: "test".to_string(),
            exchange_type: None,
            state: State::Present,
            durable: true,
            vhost: "/".to_string(),
            login_user: Some("admin".to_string()),
            login_password: Some("secret".to_string()),
        };
        let args = build_rabbitmqadmin_base_args(&params);
        assert!(args.contains(&"--username".to_string()));
        assert!(args.contains(&"admin".to_string()));
        assert!(args.contains(&"--password".to_string()));
        assert!(args.contains(&"secret".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_exchange
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_check_mode_create() {
        let params = Params {
            name: "test_exchange".to_string(),
            exchange_type: Some(ExchangeType::Topic),
            state: State::Present,
            durable: true,
            vhost: "/".to_string(),
            login_user: None,
            login_password: None,
        };
        let result = create_exchange(&params, true).unwrap();
        assert!(result.get_changed());
        assert_eq!(
            result.get_output(),
            Some("Would create exchange 'test_exchange'".to_string())
        );
    }

    #[test]
    fn test_check_mode_delete() {
        let params = Params {
            name: "test_exchange".to_string(),
            exchange_type: Some(ExchangeType::Topic),
            state: State::Absent,
            durable: true,
            vhost: "/".to_string(),
            login_user: None,
            login_password: None,
        };
        let result = delete_exchange(&params, true).unwrap();
        assert!(result.get_changed());
        assert_eq!(
            result.get_output(),
            Some("Would delete exchange 'test_exchange'".to_string())
        );
    }
}
