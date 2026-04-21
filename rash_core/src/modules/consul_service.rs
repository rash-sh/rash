/// ANCHOR: module
/// # consul_service
///
/// Register and deregister services in HashiCorp Consul.
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
/// - name: Register a service in Consul
///   consul_service:
///     name: my-api
///     address: "{{ ansible_host }}"
///     port: 8080
///     tags:
///       - v2
///       - production
///     state: present
///
/// - name: Register service with health check
///   consul_service:
///     name: my-api
///     address: "{{ ansible_host }}"
///     port: 8080
///     tags:
///       - v2
///       - production
///     check:
///       http: "http://localhost:8080/health"
///       interval: 10s
///     state: present
///
/// - name: Register service with custom Consul server
///   consul_service:
///     name: my-api
///     address: consul-server.example.com
///     port: 8080
///     host: consul-server.example.com
///     state: present
///
/// - name: Register service with ACL token
///   consul_service:
///     name: my-api
///     address: "{{ ansible_host }}"
///     port: 8080
///     token: "{{ consul_token }}"
///     state: present
///
/// - name: Deregister a service
///   consul_service:
///     name: my-api
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
use serde_json::Value as JsonValue;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ServiceCheck {
    /// HTTP URL for health check.
    pub http: Option<String>,
    /// TCP address for health check (host:port).
    pub tcp: Option<String>,
    /// Interval for health check (e.g. "10s").
    #[serde(default)]
    pub interval: Option<String>,
    /// Timeout for health check (e.g. "5s").
    #[serde(default)]
    pub timeout: Option<String>,
    /// TTL for health check (e.g. "30s").
    #[serde(default)]
    pub ttl: Option<String>,
    /// Script to run for health check.
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// Deregister service after this duration of critical health.
    #[serde(default)]
    pub deregister_critical_service_after: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The service name.
    pub name: String,
    /// The desired state of the service.
    #[serde(default)]
    pub state: State,
    /// The service IP/hostname.
    #[serde(default)]
    pub address: Option<String>,
    /// The service port.
    #[serde(default)]
    pub port: Option<u16>,
    /// Service tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Health check configuration.
    #[serde(default)]
    pub check: Option<ServiceCheck>,
    /// ACL token for authentication.
    pub token: Option<String>,
    /// The Consul host.
    #[serde(default = "default_host")]
    pub host: String,
    /// The Consul port.
    #[serde(default = "default_port")]
    pub port_consul: u16,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// The datacenter to use.
    pub dc: Option<String>,
    /// The namespace (Consul Enterprise).
    pub ns: Option<String>,
    /// Service meta key/value pairs.
    #[serde(default)]
    pub meta: Option<std::collections::HashMap<String, String>>,
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

struct ConsulClient {
    host: String,
    port: u16,
    token: Option<String>,
    dc: Option<String>,
    ns: Option<String>,
    validate_certs: bool,
}

impl ConsulClient {
    fn new(params: &Params) -> Self {
        Self {
            host: params.host.clone(),
            port: params.port_consul,
            token: params.token.clone(),
            dc: params.dc.clone(),
            ns: params.ns.clone(),
            validate_certs: params.validate_certs,
        }
    }

    fn build_url(&self, path: &str) -> String {
        let mut url = format!("http://{}:{}/v1{}", self.host, self.port, path);

        let mut query_params = Vec::new();

        if let Some(ref dc) = self.dc {
            query_params.push(format!("dc={}", dc));
        }

        if let Some(ref ns) = self.ns {
            query_params.push(format!("ns={}", ns));
        }

        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        url
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(!self.validate_certs)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })
    }

    fn add_token_header(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(ref token) = self.token {
            request.header("X-Consul-Token", token)
        } else {
            request
        }
    }

    fn get_service(&self, service_name: &str) -> Result<Option<JsonValue>> {
        let url = self.build_url(&format!(
            "/health/service/{}",
            percent_encoding(service_name)
        ));

        let client = self.build_client()?;
        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul service lookup request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul returned status {}: {}", status, error_text),
            ));
        }

        let response_text = response.text().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read response: {e}"),
            )
        })?;

        let services: Vec<JsonValue> = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })?;

        if services.is_empty() {
            Ok(None)
        } else {
            Ok(Some(services[0].clone()))
        }
    }

    fn register_service(&self, payload: &JsonValue) -> Result<()> {
        let url = self.build_url("/agent/service/register");
        let client = self.build_client()?;

        let request = self.add_token_header(client.put(&url).json(payload));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul service registration request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Consul returned status {} when registering service: {}",
                    status, error_text
                ),
            ));
        }

        Ok(())
    }

    fn deregister_service(&self, service_id: &str) -> Result<()> {
        let url = self.build_url(&format!(
            "/agent/service/deregister/{}",
            percent_encoding(service_id)
        ));
        let client = self.build_client()?;

        let request = self.add_token_header(client.put(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul service deregistration request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Consul returned status {} when deregistering service: {}",
                    status, error_text
                ),
            ));
        }

        Ok(())
    }
}

fn percent_encoding(input: &str) -> String {
    let mut result = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

fn build_registration_payload(params: &Params) -> JsonValue {
    let mut payload = json!({
        "ID": params.name,
        "Name": params.name,
    });

    if let Some(ref address) = params.address {
        payload["Address"] = json!(address);
    }

    if let Some(port) = params.port {
        payload["Port"] = json!(port);
    }

    if let Some(ref tags) = params.tags {
        payload["Tags"] = json!(tags);
    }

    if let Some(ref meta) = params.meta {
        payload["Meta"] = json!(meta);
    }

    if let Some(ref check) = params.check {
        let mut check_json = json!({});

        if let Some(ref http) = check.http {
            check_json["HTTP"] = json!(http);
        }

        if let Some(ref tcp) = check.tcp {
            check_json["TCP"] = json!(tcp);
        }

        if let Some(ref interval) = check.interval {
            check_json["Interval"] = json!(interval);
        }

        if let Some(ref timeout) = check.timeout {
            check_json["Timeout"] = json!(timeout);
        }

        if let Some(ref ttl) = check.ttl {
            check_json["TTL"] = json!(ttl);
        }

        if let Some(ref args) = check.args {
            check_json["Args"] = json!(args);
        }

        if let Some(ref deregister_after) = check.deregister_critical_service_after {
            check_json["DeregisterCriticalServiceAfter"] = json!(deregister_after);
        }

        payload["Check"] = check_json;
    }

    payload
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);
    let payload = build_registration_payload(params);

    if let Some(existing) = client.get_service(&params.name)? {
        let existing_service = existing.get("Service").cloned().unwrap_or(json!(null));

        let existing_address = existing_service
            .get("Address")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let existing_port = existing_service
            .get("Port")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let existing_tags = existing_service
            .get("Tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let new_address = params.address.as_deref().unwrap_or("");
        let new_port = params.port.unwrap_or(0) as u64;
        let new_tags = params.tags.clone().unwrap_or_default();

        if existing_address == new_address && existing_port == new_port && existing_tags == new_tags
        {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "service": params.name,
                    "changed": false
                }))?),
                Some(format!(
                    "Service {} already registered with same configuration",
                    params.name
                )),
            ));
        }
    }

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    client.register_service(&payload)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "service": params.name,
            "changed": true,
            "address": params.address,
            "port": params.port,
            "tags": params.tags,
        }))?),
        Some(format!("Service {} registered successfully", params.name)),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);

    match client.get_service(&params.name)? {
        Some(_) => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            client.deregister_service(&params.name)?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "service": params.name,
                    "changed": true,
                    "deleted": true
                }))?),
                Some(format!("Service {} deregistered", params.name)),
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "service": params.name,
                "changed": false,
                "deleted": false
            }))?),
            Some(format!("Service {} not found", params.name)),
        )),
    }
}

pub fn consul_service(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct ConsulService;

impl Module for ConsulService {
    fn get_name(&self) -> &str {
        "consul_service"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            consul_service(parse_params(optional_params)?, check_mode)?,
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
            name: my-api
            address: localhost
            port: 8080
            tags:
              - v2
              - production
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my-api");
        assert_eq!(params.address, Some("localhost".to_string()));
        assert_eq!(params.port, Some(8080));
        assert_eq!(
            params.tags,
            Some(vec!["v2".to_string(), "production".to_string()])
        );
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my-api");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_host_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: 192.168.1.1
            port: 9090
            host: consul-server.example.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "consul-server.example.com");
        assert_eq!(params.port_consul, 8500);
        assert_eq!(params.address, Some("192.168.1.1".to_string()));
        assert_eq!(params.port, Some(9090));
    }

    #[test]
    fn test_parse_params_with_token() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            token: my-consul-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.token, Some("my-consul-token".to_string()));
    }

    #[test]
    fn test_parse_params_with_datacenter() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            dc: dc2
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.dc, Some("dc2".to_string()));
    }

    #[test]
    fn test_parse_params_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            ns: team-a
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.ns, Some("team-a".to_string()));
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            validate_certs: false
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port_consul, 8500);
        assert!(params.validate_certs);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.address, None);
        assert_eq!(params.port, None);
        assert_eq!(params.tags, None);
        assert_eq!(params.token, None);
        assert_eq!(params.dc, None);
        assert_eq!(params.ns, None);
    }

    #[test]
    fn test_parse_params_with_check() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            check:
              http: "http://localhost:8080/health"
              interval: 10s
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let check = params.check.unwrap();
        assert_eq!(check.http, Some("http://localhost:8080/health".to_string()));
        assert_eq!(check.interval, Some("10s".to_string()));
        assert_eq!(check.timeout, None);
    }

    #[test]
    fn test_parse_params_with_check_tcp() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            check:
              tcp: "localhost:8080"
              interval: 5s
              timeout: 3s
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let check = params.check.unwrap();
        assert_eq!(check.tcp, Some("localhost:8080".to_string()));
        assert_eq!(check.interval, Some("5s".to_string()));
        assert_eq!(check.timeout, Some("3s".to_string()));
    }

    #[test]
    fn test_parse_params_with_meta() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-api
            address: localhost
            port: 8080
            meta:
              version: "2.0"
              environment: production
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let meta = params.meta.unwrap();
        assert_eq!(meta.get("version").unwrap(), "2.0");
        assert_eq!(meta.get("environment").unwrap(), "production");
    }

    #[test]
    fn test_consul_client_build_url_simple() {
        let params = Params {
            name: "test".to_string(),
            state: State::Present,
            address: None,
            port: None,
            tags: None,
            check: None,
            token: None,
            host: "localhost".to_string(),
            port_consul: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
            meta: None,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("/agent/service/register"),
            "http://localhost:8500/v1/agent/service/register"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc() {
        let params = Params {
            name: "test".to_string(),
            state: State::Present,
            address: None,
            port: None,
            tags: None,
            check: None,
            token: None,
            host: "localhost".to_string(),
            port_consul: 8500,
            validate_certs: true,
            dc: Some("dc2".to_string()),
            ns: None,
            meta: None,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("/agent/service/register"),
            "http://localhost:8500/v1/agent/service/register?dc=dc2"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc_and_ns() {
        let params = Params {
            name: "test".to_string(),
            state: State::Present,
            address: None,
            port: None,
            tags: None,
            check: None,
            token: None,
            host: "localhost".to_string(),
            port_consul: 8500,
            validate_certs: true,
            dc: Some("dc2".to_string()),
            ns: Some("team-a".to_string()),
            meta: None,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("/agent/service/register"),
            "http://localhost:8500/v1/agent/service/register?dc=dc2&ns=team-a"
        );
    }

    #[test]
    fn test_build_registration_payload_minimal() {
        let params = Params {
            name: "my-api".to_string(),
            state: State::Present,
            address: None,
            port: None,
            tags: None,
            check: None,
            token: None,
            host: "localhost".to_string(),
            port_consul: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
            meta: None,
        };
        let payload = build_registration_payload(&params);
        assert_eq!(payload["ID"], "my-api");
        assert_eq!(payload["Name"], "my-api");
        assert!(payload.get("Address").is_none());
        assert!(payload.get("Port").is_none());
        assert!(payload.get("Tags").is_none());
    }

    #[test]
    fn test_build_registration_payload_full() {
        let params = Params {
            name: "my-api".to_string(),
            state: State::Present,
            address: Some("192.168.1.1".to_string()),
            port: Some(8080),
            tags: Some(vec!["v2".to_string(), "production".to_string()]),
            check: Some(ServiceCheck {
                http: Some("http://localhost:8080/health".to_string()),
                tcp: None,
                interval: Some("10s".to_string()),
                timeout: Some("5s".to_string()),
                ttl: None,
                args: None,
                deregister_critical_service_after: None,
            }),
            token: None,
            host: "localhost".to_string(),
            port_consul: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
            meta: None,
        };
        let payload = build_registration_payload(&params);
        assert_eq!(payload["ID"], "my-api");
        assert_eq!(payload["Name"], "my-api");
        assert_eq!(payload["Address"], "192.168.1.1");
        assert_eq!(payload["Port"], 8080);
        assert_eq!(payload["Tags"], json!(["v2", "production"]));
        assert_eq!(payload["Check"]["HTTP"], "http://localhost:8080/health");
        assert_eq!(payload["Check"]["Interval"], "10s");
        assert_eq!(payload["Check"]["Timeout"], "5s");
    }

    #[test]
    fn test_percent_encoding() {
        assert_eq!(percent_encoding("my-service"), "my-service");
        assert_eq!(percent_encoding("my service"), "my%20service");
        assert_eq!(percent_encoding("svc/name"), "svc%2Fname");
    }
}
