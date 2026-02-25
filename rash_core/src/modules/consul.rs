/// ANCHOR: module
/// # consul
///
/// Add, modify & delete services within a Consul cluster.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - consul:
///     service_name: nginx
///     service_port: 80
///
/// - consul:
///     service_name: nginx
///     service_port: 80
///     script: curl http://localhost
///     interval: 60s
///
/// - consul:
///     service_name: nginx
///     service_port: 80
///     interval: 60s
///     tcp: localhost:80
///
/// - consul:
///     service_name: nginx
///     service_port: 80
///     interval: 60s
///     http: http://localhost:80/status
///
/// - consul:
///     service_name: nginx
///     service_port: 80
///     service_address: 10.1.5.23
///
/// - consul:
///     service_name: nginx
///     service_port: 80
///     tags:
///       - prod
///       - webservers
///
/// - consul:
///     service_name: nginx
///     state: absent
///
/// - consul:
///     service_name: celery-worker
///     tags:
///       - prod
///       - worker
///
/// - consul:
///     check_name: Disk usage
///     check_id: disk_usage
///     script: /opt/disk_usage.py
///     interval: 5m
///
/// - consul:
///     check_name: nginx-check2
///     check_id: nginx-check2
///     service_id: nginx
///     interval: 60s
///     http: http://localhost:80/morestatus
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::time::Duration;

use minijinja::Value;
use reqwest::blocking::Client;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The protocol scheme on which the Consul agent is running
    #[serde(default = "default_scheme")]
    pub scheme: String,
    /// Host of the Consul agent defaults to localhost
    #[serde(default = "default_host")]
    pub host: String,
    /// The port on which the Consul agent is running
    #[serde(default = "default_port")]
    pub port: u16,
    /// Whether to verify the TLS certificate of the Consul agent
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// The token key identifying an ACL rule set
    pub token: Option<String>,
    /// Register or deregister the Consul service, defaults to present
    #[serde(default = "default_state")]
    pub state: String,
    /// Unique name for the service on a node, must be unique per node
    pub service_name: Option<String>,
    /// The ID for the service, must be unique per node
    pub service_id: Option<String>,
    /// The address to advertise that the service is listening on
    pub service_address: Option<String>,
    /// The port on which the service is listening
    pub service_port: Option<u16>,
    /// Tags that are attached to the service registration
    pub tags: Option<Vec<String>>,
    /// Name for the service check. Required if standalone
    pub check_name: Option<String>,
    /// An ID for the service check
    pub check_id: Option<String>,
    /// The script/command that is run periodically to check the health of the service
    pub script: Option<String>,
    /// Checks can be registered with an HTTP endpoint
    pub http: Option<String>,
    /// Checks can be registered with a TCP port (format: host:port)
    pub tcp: Option<String>,
    /// The interval at which the service check is run (e.g., 15s, 1m)
    pub interval: Option<String>,
    /// A custom HTTP check timeout (e.g., 15s, 1m)
    pub timeout: Option<String>,
    /// Checks can be registered with a TTL instead of script/interval
    pub ttl: Option<String>,
    /// Notes to attach to check when registering it
    pub notes: Option<String>,
}

fn default_scheme() -> String {
    "http".to_string()
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    8500
}

fn default_validate_certs() -> bool {
    true
}

fn default_state() -> String {
    "present".to_string()
}

#[derive(Debug, Serialize)]
struct ServiceRegistration {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check: Option<CheckDefinition>,
}

#[derive(Debug, Serialize)]
struct CheckDefinition {
    #[serde(skip_serializing_if = "Option::is_none")]
    check_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct CheckRegistration {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

fn build_client(params: &Params) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(!params.validate_certs)
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {e}"),
            )
        })
}

fn build_base_url(params: &Params) -> String {
    format!("{}://{}:{}", params.scheme, params.host, params.port)
}

fn add_auth_header(
    request_builder: reqwest::blocking::RequestBuilder,
    token: Option<&String>,
) -> reqwest::blocking::RequestBuilder {
    if let Some(t) = token {
        request_builder.header("X-Consul-Token", t)
    } else {
        request_builder
    }
}

fn register_service(params: &Params) -> Result<ModuleResult> {
    let client = build_client(params)?;
    let base_url = build_base_url(params);

    let service_id = params
        .service_id
        .clone()
        .or_else(|| params.service_name.clone());

    let check = if params.script.is_some()
        || params.http.is_some()
        || params.tcp.is_some()
        || params.ttl.is_some()
    {
        Some(CheckDefinition {
            check_id: None,
            name: None,
            script: params.script.clone(),
            http: params.http.clone(),
            tcp: params.tcp.clone(),
            interval: params.interval.clone(),
            timeout: params.timeout.clone(),
            ttl: params.ttl.clone(),
            notes: params.notes.clone(),
            service_id: None,
            status: None,
        })
    } else {
        None
    };

    let service = ServiceRegistration {
        id: service_id,
        name: params.service_name.clone(),
        address: params.service_address.clone(),
        port: params.service_port,
        tags: params.tags.clone(),
        check,
    };

    let url = format!("{}/v1/agent/service/register", base_url);
    let request = add_auth_header(client.put(&url), params.token.as_ref()).json(&service);

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to register service with Consul: {e}"),
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Consul API returned error {}: {}", status, body),
        ));
    }

    let mut extra_data = json!({
        "service_name": params.service_name,
        "operation": "registered",
    });

    if let Some(id) = &params.service_id {
        extra_data["service_id"] = json!(id);
    }
    if let Some(port) = &params.service_port {
        extra_data["service_port"] = json!(port);
    }
    if let Some(addr) = &params.service_address {
        extra_data["service_address"] = json!(addr);
    }
    if let Some(tags) = &params.tags {
        extra_data["tags"] = json!(tags);
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Service '{}' registered successfully",
            params.service_name.as_deref().unwrap_or("unknown")
        )),
        extra: Some(value::to_value(extra_data)?),
    })
}

fn deregister_service(params: &Params) -> Result<ModuleResult> {
    let client = build_client(params)?;
    let base_url = build_base_url(params);

    let service_id = params
        .service_id
        .clone()
        .or_else(|| params.service_name.clone());

    let service_id = match service_id {
        Some(id) => id,
        None => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "service_id or service_name is required for deregistration",
            ));
        }
    };

    let url = format!("{}/v1/agent/service/deregister/{}", base_url, service_id);
    let request = add_auth_header(client.put(&url), params.token.as_ref());

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to deregister service with Consul: {e}"),
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Consul API returned error {}: {}", status, body),
        ));
    }

    let extra_data = json!({
        "service_id": service_id,
        "operation": "deregistered",
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Service '{}' deregistered successfully",
            service_id
        )),
        extra: Some(value::to_value(extra_data)?),
    })
}

fn register_check(params: &Params) -> Result<ModuleResult> {
    let client = build_client(params)?;
    let base_url = build_base_url(params);

    let check = CheckRegistration {
        id: params.check_id.clone(),
        name: params.check_name.clone(),
        script: params.script.clone(),
        http: params.http.clone(),
        tcp: params.tcp.clone(),
        interval: params.interval.clone(),
        timeout: params.timeout.clone(),
        ttl: params.ttl.clone(),
        notes: params.notes.clone(),
    };

    let url = format!("{}/v1/agent/check/register", base_url);
    let request = add_auth_header(client.put(&url), params.token.as_ref()).json(&check);

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to register check with Consul: {e}"),
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Consul API returned error {}: {}", status, body),
        ));
    }

    let mut extra_data = json!({
        "check_name": params.check_name,
        "operation": "registered",
    });

    if let Some(id) = &params.check_id {
        extra_data["check_id"] = json!(id);
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!(
            "Check '{}' registered successfully",
            params.check_name.as_deref().unwrap_or("unknown")
        )),
        extra: Some(value::to_value(extra_data)?),
    })
}

fn deregister_check(params: &Params) -> Result<ModuleResult> {
    let client = build_client(params)?;
    let base_url = build_base_url(params);

    let check_id = params
        .check_id
        .clone()
        .or_else(|| params.check_name.clone());

    let check_id = match check_id {
        Some(id) => id,
        None => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "check_id or check_name is required for deregistration",
            ));
        }
    };

    let url = format!("{}/v1/agent/check/deregister/{}", base_url, check_id);
    let request = add_auth_header(client.put(&url), params.token.as_ref());

    let response = request.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to deregister check with Consul: {e}"),
        )
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Consul API returned error {}: {}", status, body),
        ));
    }

    let extra_data = json!({
        "check_id": check_id,
        "operation": "deregistered",
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Check '{}' deregistered successfully", check_id)),
        extra: Some(value::to_value(extra_data)?),
    })
}

#[derive(Debug)]
pub struct Consul;

impl Module for Consul {
    fn get_name(&self) -> &str {
        "consul"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;

        let is_check_op = params.check_name.is_some()
            && params.service_name.is_none()
            && params.service_id.is_none();

        match params.state.as_str() {
            "present" => {
                if is_check_op {
                    let result = register_check(&params)?;
                    Ok((result, None))
                } else {
                    let result = register_service(&params)?;
                    Ok((result, None))
                }
            }
            "absent" => {
                if is_check_op {
                    let result = deregister_check(&params)?;
                    Ok((result, None))
                } else {
                    let result = deregister_service(&params)?;
                    Ok((result, None))
                }
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid state '{}'. Must be 'present' or 'absent'",
                    params.state
                ),
            )),
        }
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
    fn test_parse_params_simple_service() {
        let yaml = r#"
service_name: nginx
service_port: 80
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.service_name, Some("nginx".to_string()));
        assert_eq!(params.service_port, Some(80));
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port, 8500);
        assert_eq!(params.scheme, "http");
        assert_eq!(params.state, "present");
    }

    #[test]
    fn test_parse_params_with_check() {
        let yaml = r#"
service_name: nginx
service_port: 80
script: curl http://localhost
interval: 60s
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.service_name, Some("nginx".to_string()));
        assert_eq!(params.script, Some("curl http://localhost".to_string()));
        assert_eq!(params.interval, Some("60s".to_string()));
    }

    #[test]
    fn test_parse_params_with_http_check() {
        let yaml = r#"
service_name: nginx
service_port: 80
http: http://localhost:80/status
interval: 60s
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.http, Some("http://localhost:80/status".to_string()));
    }

    #[test]
    fn test_parse_params_with_tcp_check() {
        let yaml = r#"
service_name: nginx
service_port: 80
tcp: localhost:80
interval: 60s
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.tcp, Some("localhost:80".to_string()));
    }

    #[test]
    fn test_parse_params_with_tags() {
        let yaml = r#"
service_name: nginx
service_port: 80
tags:
  - prod
  - webservers
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        let tags = params.tags.unwrap();
        assert_eq!(tags, vec!["prod", "webservers"]);
    }

    #[test]
    fn test_parse_params_deregister() {
        let yaml = r#"
service_name: nginx
state: absent
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.service_name, Some("nginx".to_string()));
        assert_eq!(params.state, "absent");
    }

    #[test]
    fn test_parse_params_with_token() {
        let yaml = r#"
service_name: nginx
service_port: 80
token: my-secret-token
host: consul.example.com
port: 8501
scheme: https
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.token, Some("my-secret-token".to_string()));
        assert_eq!(params.host, "consul.example.com");
        assert_eq!(params.port, 8501);
        assert_eq!(params.scheme, "https");
    }

    #[test]
    fn test_parse_params_standalone_check() {
        let yaml = r#"
check_name: Disk usage
check_id: disk_usage
script: /opt/disk_usage.py
interval: 5m
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.check_name, Some("Disk usage".to_string()));
        assert_eq!(params.check_id, Some("disk_usage".to_string()));
        assert_eq!(params.script, Some("/opt/disk_usage.py".to_string()));
        assert_eq!(params.interval, Some("5m".to_string()));
    }

    #[test]
    fn test_parse_params_with_service_address() {
        let yaml = r#"
service_name: nginx
service_port: 80
service_address: 10.1.5.23
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.service_address, Some("10.1.5.23".to_string()));
    }

    #[test]
    fn test_build_base_url() {
        let params = Params {
            scheme: "http".to_string(),
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            token: None,
            state: "present".to_string(),
            service_name: None,
            service_id: None,
            service_address: None,
            service_port: None,
            tags: None,
            check_name: None,
            check_id: None,
            script: None,
            http: None,
            tcp: None,
            interval: None,
            timeout: None,
            ttl: None,
            notes: None,
        };

        assert_eq!(build_base_url(&params), "http://localhost:8500");
    }
}
