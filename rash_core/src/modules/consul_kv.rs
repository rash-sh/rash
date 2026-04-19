/// ANCHOR: module
/// # consul_kv
///
/// Manage Consul key-value store entries.
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
/// - name: Set a key in Consul KV store
///   consul_kv:
///     key: myapp/config/database_url
///     value: postgres://localhost:5432/mydb
///     state: present
///
/// - name: Get a key from Consul KV store
///   consul_kv:
///     key: myapp/config/database_url
///     state: read
///   register: result
///
/// - name: Delete a key
///   consul_kv:
///     key: myapp/config/old_setting
///     state: absent
///
/// - name: Delete keys recursively
///   consul_kv:
///     key: myapp/old_feature
///     state: absent
///     recurse: true
///
/// - name: Set key with custom Consul server
///   consul_kv:
///     key: myapp/config/api_key
///     value: secret123
///     host: consul-server.example.com
///     port: 8500
///     state: present
///
/// - name: Set key with ACL token
///   consul_kv:
///     key: secure/config/password
///     value: '{{ vault_password }}'
///     token: '{{ consul_token }}'
///     state: present
///
/// - name: Set key in specific datacenter
///   consul_kv:
///     key: myapp/config/setting
///     value: production
///     dc: dc2
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use base64::{Engine as _, engine::general_purpose};
use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Read,
    Present,
    #[default]
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The key path in Consul KV store.
    pub key: String,
    /// The value to set (required for state=present).
    pub value: Option<String>,
    /// The desired state of the key.
    #[serde(default)]
    pub state: State,
    /// The Consul host.
    #[serde(default = "default_host")]
    pub host: String,
    /// The Consul port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// ACL token for authentication.
    pub token: Option<String>,
    /// Recursively delete keys (only for state=absent).
    #[serde(default)]
    pub recurse: bool,
    /// The datacenter to use.
    pub dc: Option<String>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// The namespace (Consul Enterprise).
    pub ns: Option<String>,
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
            port: params.port,
            token: params.token.clone(),
            dc: params.dc.clone(),
            ns: params.ns.clone(),
            validate_certs: params.validate_certs,
        }
    }

    fn build_url(&self, key: &str) -> String {
        let mut url = format!("http://{}:{}/v1/kv/{}", self.host, self.port, key);

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

    fn read(&self, key: &str) -> Result<Option<(String, u64)>> {
        let url = self.build_url(key);
        let client = self.build_client()?;

        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul read request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

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

        let json: Vec<JsonValue> = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })?;

        if json.is_empty() {
            return Ok(None);
        }

        let entry = &json[0];
        let value_base64 = entry.get("Value").and_then(|v| v.as_str()).unwrap_or("");

        let value = if value_base64.is_empty() {
            String::new()
        } else {
            let decoded = general_purpose::STANDARD
                .decode(value_base64)
                .map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to decode base64 value: {e}"),
                    )
                })?;
            String::from_utf8(decoded).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to convert decoded value to string: {e}"),
                )
            })?
        };

        let modify_index = entry
            .get("ModifyIndex")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(Some((value, modify_index)))
    }

    fn write(&self, key: &str, value: &str) -> Result<bool> {
        let url = self.build_url(key);
        let client = self.build_client()?;

        let request = self.add_token_header(client.put(&url).body(value.to_string()));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul write request failed: {e}"),
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

        Ok(response_text == "true")
    }

    fn delete(&self, key: &str, recurse: bool) -> Result<bool> {
        let mut url = self.build_url(key);

        if recurse {
            if url.contains('?') {
                url.push_str("&recurse=true");
            } else {
                url.push_str("?recurse=true");
            }
        }

        let client = self.build_client()?;

        let request = self.add_token_header(client.delete(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul delete request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }

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

        Ok(response_text == "true")
    }
}

fn exec_read(params: &Params) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);

    match client.read(&params.key)? {
        Some((value, modify_index)) => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "key": params.key,
                "value": value,
                "modify_index": modify_index
            }))?),
            Some(value),
        )),
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "key": params.key,
                "found": false
            }))?),
            None,
        )),
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let value = params.value.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        )
    })?;

    let client = ConsulClient::new(params);

    match client.read(&params.key)? {
        Some((existing_value, _)) if existing_value == *value => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "key": params.key,
                "value": value,
                "changed": false
            }))?),
            Some(format!("Key {} already has correct value", params.key)),
        )),
        _ => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            let success = client.write(&params.key, value)?;

            if success {
                Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "key": params.key,
                        "value": value,
                        "changed": true
                    }))?),
                    Some(format!("Key {} set successfully", params.key)),
                ))
            } else {
                Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to set key {}", params.key),
                ))
            }
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulClient::new(params);

    if check_mode {
        match client.read(&params.key)? {
            Some(_) => Ok(ModuleResult::new(true, None, None)),
            None => Ok(ModuleResult::new(false, None, None)),
        }
    } else {
        let deleted = client.delete(&params.key, params.recurse)?;

        Ok(ModuleResult::new(
            deleted,
            Some(value::to_value(json!({
                "key": params.key,
                "recurse": params.recurse,
                "deleted": deleted
            }))?),
            if deleted {
                if params.recurse {
                    Some(format!("Key {} and all child keys deleted", params.key))
                } else {
                    Some(format!("Key {} deleted", params.key))
                }
            } else {
                Some(format!("Key {} not found", params.key))
            },
        ))
    }
}

pub fn consul_kv(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Read => exec_read(&params),
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct ConsulKv;

impl Module for ConsulKv {
    fn get_name(&self) -> &str {
        "consul_kv"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((consul_kv(parse_params(optional_params)?, check_mode)?, None))
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
            key: myapp/config/database_url
            value: postgres://localhost:5432/mydb
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, "myapp/config/database_url");
        assert_eq!(
            params.value,
            Some("postgres://localhost:5432/mydb".to_string())
        );
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_read() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: myapp/config/database_url
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, "myapp/config/database_url");
        assert_eq!(params.state, State::Read);
        assert_eq!(params.value, None);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: myapp/config/old_setting
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, "myapp/config/old_setting");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_absent_recurse() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: myapp/old_feature
            state: absent
            recurse: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, "myapp/old_feature");
        assert_eq!(params.state, State::Absent);
        assert!(params.recurse);
    }

    #[test]
    fn test_parse_params_with_host_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: myapp/config/api_key
            value: secret123
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
            key: secure/config/password
            value: mypassword
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
            key: myapp/config/setting
            value: production
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
            key: myapp/config/setting
            value: production
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
            key: myapp/config/setting
            value: production
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
            key: myapp/config/setting
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port, 8500);
        assert!(params.validate_certs);
        assert_eq!(params.state, State::Absent);
        assert!(!params.recurse);
    }

    #[test]
    fn test_consul_client_build_url_simple() {
        let params = Params {
            key: "test".to_string(),
            value: None,
            state: State::Read,
            host: "localhost".to_string(),
            port: 8500,
            token: None,
            recurse: false,
            dc: None,
            validate_certs: true,
            ns: None,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("mykey"),
            "http://localhost:8500/v1/kv/mykey"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc() {
        let params = Params {
            key: "test".to_string(),
            value: None,
            state: State::Read,
            host: "localhost".to_string(),
            port: 8500,
            token: None,
            recurse: false,
            dc: Some("dc2".to_string()),
            validate_certs: true,
            ns: None,
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("mykey"),
            "http://localhost:8500/v1/kv/mykey?dc=dc2"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc_and_ns() {
        let params = Params {
            key: "test".to_string(),
            value: None,
            state: State::Read,
            host: "localhost".to_string(),
            port: 8500,
            token: None,
            recurse: false,
            dc: Some("dc2".to_string()),
            validate_certs: true,
            ns: Some("team-a".to_string()),
        };
        let client = ConsulClient::new(&params);
        assert_eq!(
            client.build_url("mykey"),
            "http://localhost:8500/v1/kv/mykey?dc=dc2&ns=team-a"
        );
    }
}
