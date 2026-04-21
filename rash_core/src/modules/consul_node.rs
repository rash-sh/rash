/// ANCHOR: module
/// # consul_node
///
/// Manage Consul node catalog registrations.
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
/// - name: Register a node in Consul catalog
///   consul_node:
///     name: edge-device-01
///     address: 192.168.1.100
///     state: present
///
/// - name: Register a node with metadata
///   consul_node:
///     name: edge-device-01
///     address: 192.168.1.100
///     meta:
///       role: gateway
///       location: factory-floor
///     state: present
///
/// - name: Register a node in a specific datacenter
///   consul_node:
///     name: edge-device-01
///     address: 192.168.1.100
///     datacenter: dc2
///     state: present
///
/// - name: Deregister a node from Consul catalog
///   consul_node:
///     name: edge-device-01
///     address: 192.168.1.100
///     state: absent
///
/// - name: Register node with custom Consul server
///   consul_node:
///     name: edge-device-01
///     address: 192.168.1.100
///     host: consul-server.example.com
///     port: 8500
///     state: present
///
/// - name: Register node with ACL token
///   consul_node:
///     name: secure-node
///     address: 10.0.0.50
///     token: '{{ consul_token }}'
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
use serde_json::Value as JsonValue;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The node name.
    pub name: String,
    /// The node IP address or hostname.
    pub address: String,
    /// The desired state of the node.
    #[serde(default)]
    pub state: State,
    /// The datacenter to use.
    pub datacenter: Option<String>,
    /// Node metadata key-value pairs.
    pub meta: Option<HashMap<String, String>>,
    /// The Consul host.
    #[serde(default = "default_host")]
    pub host: String,
    /// The Consul port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// ACL token for authentication.
    pub token: Option<String>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
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
    datacenter: Option<String>,
    validate_certs: bool,
}

impl ConsulClient {
    fn new(params: &Params) -> Self {
        Self {
            host: params.host.clone(),
            port: params.port,
            token: params.token.clone(),
            datacenter: params.datacenter.clone(),
            validate_certs: params.validate_certs,
        }
    }

    fn build_url(&self, path: &str) -> String {
        let mut url = format!("http://{}:{}/v1/{}", self.host, self.port, path);

        let mut query_params = Vec::new();

        if let Some(ref dc) = self.datacenter {
            query_params.push(format!("dc={}", dc));
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

    fn get_node(&self, node_name: &str) -> Result<Option<JsonValue>> {
        let url = self.build_url(&format!("catalog/node/{}", node_name));
        let client = self.build_client()?;

        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul get node request failed: {e}"),
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

        let json: JsonValue = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })?;

        if json.get("Node").is_some() {
            Ok(Some(json))
        } else {
            Ok(None)
        }
    }

    fn register(
        &self,
        name: &str,
        address: &str,
        meta: &Option<HashMap<String, String>>,
    ) -> Result<bool> {
        let url = self.build_url("catalog/register");
        let client = self.build_client()?;

        let mut body = json!({
            "Node": name,
            "Address": address,
        });

        if let Some(ref dc) = self.datacenter {
            body["Datacenter"] = json!(dc);
        }

        if let Some(meta) = meta {
            body["NodeMeta"] = json!(meta);
        }

        let request = self.add_token_header(client.put(&url).json(&body));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul register request failed: {e}"),
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

        Ok(true)
    }

    fn deregister(&self, name: &str) -> Result<bool> {
        let url = self.build_url("catalog/deregister");
        let client = self.build_client()?;

        let mut body = json!({
            "Node": name,
        });

        if let Some(ref dc) = self.datacenter {
            body["Datacenter"] = json!(dc);
        }

        let request = self.add_token_header(client.put(&url).json(&body));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul deregister request failed: {e}"),
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

        Ok(true)
    }
}

fn node_matches(
    existing: &JsonValue,
    address: &str,
    meta: &Option<HashMap<String, String>>,
) -> bool {
    let node = match existing.get("Node") {
        Some(n) => n,
        None => return false,
    };

    let existing_address = node.get("Address").and_then(|v| v.as_str()).unwrap_or("");
    if existing_address != address {
        return false;
    }

    if let Some(meta) = meta {
        let existing_meta = node.get("Meta").cloned().unwrap_or(json!({}));
        if let Some(existing_map) = existing_meta.as_object() {
            for (key, value) in meta {
                match existing_map.get(key).and_then(|v| v.as_str()) {
                    Some(v) if v == value => continue,
                    _ => return false,
                }
            }
        } else {
            return false;
        }
    }

    true
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);

    match client.get_node(&params.name)? {
        Some(existing) if node_matches(&existing, &params.address, &params.meta) => {
            Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "node": params.name,
                    "address": params.address,
                    "changed": false
                }))?),
                Some(format!(
                    "Node {} already registered with correct configuration",
                    params.name
                )),
            ))
        }
        _ => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            client.register(&params.name, &params.address, &params.meta)?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "node": params.name,
                    "address": params.address,
                    "changed": true
                }))?),
                Some(format!("Node {} registered successfully", params.name)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);

    if check_mode {
        match client.get_node(&params.name)? {
            Some(_) => Ok(ModuleResult::new(true, None, None)),
            None => Ok(ModuleResult::new(false, None, None)),
        }
    } else {
        let exists = client.get_node(&params.name)?.is_some();

        if !exists {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "node": params.name,
                    "deleted": false
                }))?),
                Some(format!("Node {} not found", params.name)),
            ));
        }

        client.deregister(&params.name)?;

        Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "node": params.name,
                "deleted": true
            }))?),
            Some(format!("Node {} deregistered successfully", params.name)),
        ))
    }
}

pub fn consul_node(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct ConsulNode;

impl Module for ConsulNode {
    fn get_name(&self) -> &str {
        "consul_node"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            consul_node(parse_params(optional_params)?, check_mode)?,
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
            name: edge-device-01
            address: 192.168.1.100
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "edge-device-01");
        assert_eq!(params.address, "192.168.1.100");
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: edge-device-01
            address: 192.168.1.100
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "edge-device-01");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_meta() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: edge-device-01
            address: 192.168.1.100
            meta:
              role: gateway
              location: factory-floor
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let meta = params.meta.unwrap();
        assert_eq!(meta.get("role").unwrap(), "gateway");
        assert_eq!(meta.get("location").unwrap(), "factory-floor");
    }

    #[test]
    fn test_parse_params_with_datacenter() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: edge-device-01
            address: 192.168.1.100
            datacenter: dc2
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.datacenter, Some("dc2".to_string()));
    }

    #[test]
    fn test_parse_params_with_host_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: edge-device-01
            address: 192.168.1.100
            host: consul-server.example.com
            port: 8500
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "consul-server.example.com");
        assert_eq!(params.port, 8500);
    }

    #[test]
    fn test_parse_params_with_token() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: secure-node
            address: 10.0.0.50
            token: my-consul-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.token, Some("my-consul-token".to_string()));
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: edge-device-01
            address: 192.168.1.100
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
            name: edge-device-01
            address: 192.168.1.100
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port, 8500);
        assert!(params.validate_certs);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.meta, None);
        assert_eq!(params.datacenter, None);
        assert_eq!(params.token, None);
    }

    #[test]
    fn test_consul_client_build_url_simple() {
        let params = Params {
            name: "test".to_string(),
            address: "1.2.3.4".to_string(),
            state: State::Present,
            datacenter: None,
            meta: None,
            host: "localhost".to_string(),
            port: 8500,
            token: None,
            validate_certs: true,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("catalog/register"),
            "http://localhost:8500/v1/catalog/register"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc() {
        let params = Params {
            name: "test".to_string(),
            address: "1.2.3.4".to_string(),
            state: State::Present,
            datacenter: Some("dc2".to_string()),
            meta: None,
            host: "localhost".to_string(),
            port: 8500,
            token: None,
            validate_certs: true,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("catalog/node/test"),
            "http://localhost:8500/v1/catalog/node/test?dc=dc2"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_host_port() {
        let params = Params {
            name: "test".to_string(),
            address: "1.2.3.4".to_string(),
            state: State::Present,
            datacenter: None,
            meta: None,
            host: "consul.example.com".to_string(),
            port: 9500,
            token: None,
            validate_certs: true,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("catalog/register"),
            "http://consul.example.com:9500/v1/catalog/register"
        );
    }

    #[test]
    fn test_node_matches_same() {
        let existing = json!({
            "Node": {
                "Node": "test",
                "Address": "192.168.1.100"
            }
        });
        assert!(node_matches(&existing, "192.168.1.100", &None));
    }

    #[test]
    fn test_node_matches_different_address() {
        let existing = json!({
            "Node": {
                "Node": "test",
                "Address": "192.168.1.100"
            }
        });
        assert!(!node_matches(&existing, "192.168.1.200", &None));
    }

    #[test]
    fn test_node_matches_with_meta() {
        let existing = json!({
            "Node": {
                "Node": "test",
                "Address": "192.168.1.100",
                "Meta": {
                    "role": "gateway",
                    "location": "factory-floor"
                }
            }
        });
        let mut meta = HashMap::new();
        meta.insert("role".to_string(), "gateway".to_string());
        meta.insert("location".to_string(), "factory-floor".to_string());
        assert!(node_matches(&existing, "192.168.1.100", &Some(meta)));
    }

    #[test]
    fn test_node_matches_meta_mismatch() {
        let existing = json!({
            "Node": {
                "Node": "test",
                "Address": "192.168.1.100",
                "Meta": {
                    "role": "gateway"
                }
            }
        });
        let mut meta = HashMap::new();
        meta.insert("role".to_string(), "worker".to_string());
        assert!(!node_matches(&existing, "192.168.1.100", &Some(meta)));
    }

    #[test]
    fn test_node_matches_missing_meta_key() {
        let existing = json!({
            "Node": {
                "Node": "test",
                "Address": "192.168.1.100",
                "Meta": {
                    "role": "gateway"
                }
            }
        });
        let mut meta = HashMap::new();
        meta.insert("role".to_string(), "gateway".to_string());
        meta.insert("env".to_string(), "prod".to_string());
        assert!(!node_matches(&existing, "192.168.1.100", &Some(meta)));
    }
}
