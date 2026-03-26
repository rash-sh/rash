/// ANCHOR: module
/// # vault
///
/// Interact with HashiCorp Vault for secrets management.
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
/// - name: Read secret from Vault
///   vault:
///     path: secret/data/myapp
///     url: https://vault.example.com
///     token: '{{ vault_token }}'
///     state: read
///   register: secret_data
///
/// - name: Write secret to Vault
///   vault:
///     path: secret/data/myapp
///     url: https://vault.example.com
///     token: '{{ vault_token }}'
///     data:
///       username: admin
///       password: '{{ db_password }}'
///     state: present
///
/// - name: Delete secret
///   vault:
///     path: secret/data/oldapp
///     url: https://vault.example.com
///     token: '{{ vault_token }}'
///     state: absent
///
/// - name: Read secret with namespace (Vault Enterprise)
///   vault:
///     path: secret/data/myapp
///     url: https://vault.example.com
///     token: '{{ vault_token }}'
///     namespace: team-a
///     state: read
///   register: secret_data
///
/// - name: Write to KV v1 engine
///   vault:
///     path: kv/myapp
///     url: https://vault.example.com
///     token: '{{ vault_token }}'
///     engine: v1
///     data:
///       key: value
///     state: present
///
/// - name: Use environment variables for connection
///   vault:
///     path: secret/data/myapp
///     state: read
///   register: secret_data
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::env;

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

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    V2,
    V1,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The path to the secret in Vault.
    pub path: String,
    /// The URL of the Vault server. If not provided, uses VAULT_ADDR environment variable.
    pub url: Option<String>,
    /// The Vault token for authentication. If not provided, uses VAULT_TOKEN environment variable.
    pub token: Option<String>,
    /// The secret data to write (required for state=present).
    pub data: Option<HashMap<String, JsonValue>>,
    /// The desired state of the secret.
    #[serde(default)]
    pub state: State,
    /// The Vault namespace (Enterprise feature).
    pub namespace: Option<String>,
    /// The KV secrets engine version.
    #[serde(default)]
    pub engine: Engine,
    /// The mount point for the secrets engine.
    #[serde(default = "default_mount")]
    pub mount: String,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_mount() -> String {
    "secret".to_string()
}

fn default_validate_certs() -> bool {
    true
}

fn get_vault_url(params: &Params) -> Result<String> {
    params
        .url
        .clone()
        .or_else(|| env::var("VAULT_ADDR").ok())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "Vault URL not provided. Set 'url' parameter or VAULT_ADDR environment variable.",
            )
        })
}

fn get_vault_token(params: &Params) -> Result<String> {
    params
        .token
        .clone()
        .or_else(|| env::var("VAULT_TOKEN").ok())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "Vault token not provided. Set 'token' parameter or VAULT_TOKEN environment variable.",
            )
        })
}

struct VaultClient {
    url: String,
    token: String,
    namespace: Option<String>,
    validate_certs: bool,
}

impl VaultClient {
    fn new(params: &Params) -> Result<Self> {
        Ok(Self {
            url: get_vault_url(params)?,
            token: get_vault_token(params)?,
            namespace: params.namespace.clone(),
            validate_certs: params.validate_certs,
        })
    }

    fn build_request(
        &self,
        method: &str,
        full_path: &str,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(!self.validate_certs)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })?;

        let url = format!("{}/v1/{}", self.url.trim_end_matches('/'), full_path);

        let mut request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "DELETE" => client.delete(&url),
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unsupported HTTP method: {method}"),
                ));
            }
        };

        request = request.header("X-Vault-Token", &self.token);

        if let Some(ref ns) = self.namespace
            && !ns.is_empty()
        {
            request = request.header("X-Vault-Namespace", ns);
        }

        Ok(request)
    }

    fn read(&self, full_path: &str) -> Result<JsonValue> {
        let request = self.build_request("GET", full_path)?;
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault read request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault returned status {}: {}", status, error_text),
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
                format!("Failed to parse Vault response: {e}"),
            )
        })?;

        Ok(json)
    }

    fn write(&self, full_path: &str, data: &HashMap<String, JsonValue>) -> Result<JsonValue> {
        let request = self.build_request("POST", full_path)?;

        let body = serde_json::to_value(data).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize data: {e}"),
            )
        })?;

        let response = request.json(&body).send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault write request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault returned status {}: {}", status, error_text),
            ));
        }

        let response_text = response.text().unwrap_or_else(|_| "{}".to_string());

        if response_text.is_empty() {
            return Ok(JsonValue::Object(serde_json::Map::new()));
        }

        let json: JsonValue = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Vault response: {e}"),
            )
        })?;

        Ok(json)
    }

    fn delete(&self, full_path: &str) -> Result<bool> {
        let request = self.build_request("DELETE", full_path)?;
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault delete request failed: {e}"),
            )
        })?;

        let status = response.status();
        Ok(status.is_success())
    }
}

fn build_full_path(mount: &str, path: &str, engine: &Engine) -> String {
    match engine {
        Engine::V2 => {
            if path.starts_with(&format!("{mount}/data/")) {
                path.to_string()
            } else if path.starts_with("data/") {
                format!("{mount}/{path}")
            } else if path.starts_with(&format!("{mount}/")) {
                format!(
                    "{mount}/data/{}",
                    path.strip_prefix(&format!("{mount}/")).unwrap_or(path)
                )
            } else {
                format!("{mount}/data/{path}")
            }
        }
        Engine::V1 => {
            if path.starts_with(&format!("{mount}/")) {
                path.to_string()
            } else {
                format!("{mount}/{path}")
            }
        }
    }
}

fn extract_secret_data(
    response: &JsonValue,
    engine: &Engine,
) -> Option<HashMap<String, JsonValue>> {
    match engine {
        Engine::V2 => response
            .get("data")
            .and_then(|d| d.get("data"))
            .and_then(|d| d.as_object())
            .map(|obj| obj.clone().into_iter().collect()),
        Engine::V1 => response
            .get("data")
            .and_then(|d| d.as_object())
            .map(|obj| obj.clone().into_iter().collect()),
    }
}

fn exec_read(params: &Params) -> Result<ModuleResult> {
    let client = VaultClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, &params.engine);

    let response = client.read(&full_path)?;
    let secret_data = extract_secret_data(&response, &params.engine);

    match secret_data {
        Some(data) => {
            let extra = Some(value::to_value(json!({
                "data": data,
                "raw": response
            }))?);
            Ok(ModuleResult::new(
                false,
                extra,
                Some("Secret read successfully".to_string()),
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "data": {},
                "raw": response,
                "found": false
            }))?),
            Some("Secret not found or empty".to_string()),
        )),
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let data = params.data.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "data parameter is required when state=present",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, &params.engine);

    let write_data = match params.engine {
        Engine::V2 => {
            let mut map = serde_json::Map::new();
            map.insert(
                "data".to_string(),
                serde_json::to_value(data).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to serialize data: {e}"),
                    )
                })?,
            );
            serde_json::Value::Object(map)
        }
        Engine::V1 => serde_json::to_value(data).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize data: {e}"),
            )
        })?,
    };

    let data_map: HashMap<String, JsonValue> = if let JsonValue::Object(m) = write_data {
        m.into_iter().collect()
    } else {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Failed to convert data to map",
        ));
    };

    let response = client.write(&full_path, &data_map)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "data": data,
            "raw": response
        }))?),
        Some("Secret written successfully".to_string()),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, &params.engine);

    let deleted = client.delete(&full_path)?;

    Ok(ModuleResult::new(
        deleted,
        None,
        if deleted {
            Some("Secret deleted successfully".to_string())
        } else {
            Some("Secret not found or already deleted".to_string())
        },
    ))
}

pub fn vault(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Read => exec_read(&params),
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Vault;

impl Module for Vault {
    fn get_name(&self) -> &str {
        "vault"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((vault(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_params_read() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "secret/data/myapp");
        assert_eq!(params.url, Some("https://vault.example.com".to_string()));
        assert_eq!(params.token, Some("test-token".to_string()));
        assert_eq!(params.state, State::Read);
        assert_eq!(params.engine, Engine::V2);
    }

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            data:
              username: admin
              password: secret123
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert!(params.data.is_some());
        let data = params.data.unwrap();
        assert_eq!(data.get("username"), Some(&json!("admin")));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            namespace: "team-a"
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.namespace, Some("team-a".to_string()));
    }

    #[test]
    fn test_parse_params_kv_v1() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "kv/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            engine: v1
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.engine, Engine::V1);
    }

    #[test]
    fn test_parse_params_custom_mount() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "myapp"
            url: "https://vault.example.com"
            token: "test-token"
            mount: "custom-secret"
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mount, "custom-secret");
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "https://vault.example.com"
            token: "test-token"
            validate_certs: false
            state: read
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
            path: "secret/data/myapp"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mount, "secret");
        assert!(params.validate_certs);
        assert_eq!(params.engine, Engine::V2);
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_build_full_path_v2() {
        assert_eq!(
            build_full_path("secret", "myapp", &Engine::V2),
            "secret/data/myapp"
        );
        assert_eq!(
            build_full_path("secret", "data/myapp", &Engine::V2),
            "secret/data/myapp"
        );
        assert_eq!(
            build_full_path("secret", "secret/myapp", &Engine::V2),
            "secret/data/myapp"
        );
        assert_eq!(
            build_full_path("secret", "secret/data/myapp", &Engine::V2),
            "secret/data/myapp"
        );
    }

    #[test]
    fn test_build_full_path_v1() {
        assert_eq!(build_full_path("kv", "myapp", &Engine::V1), "kv/myapp");
        assert_eq!(build_full_path("kv", "kv/myapp", &Engine::V1), "kv/myapp");
    }

    #[test]
    fn test_extract_secret_data_v2() {
        let response = json!({
            "data": {
                "data": {
                    "username": "admin",
                    "password": "secret"
                },
                "metadata": {
                    "version": 1
                }
            }
        });
        let data = extract_secret_data(&response, &Engine::V2);
        assert!(data.is_some());
        let data = data.unwrap();
        assert_eq!(data.get("username"), Some(&json!("admin")));
        assert_eq!(data.get("password"), Some(&json!("secret")));
    }

    #[test]
    fn test_extract_secret_data_v1() {
        let response = json!({
            "data": {
                "username": "admin",
                "password": "secret"
            }
        });
        let data = extract_secret_data(&response, &Engine::V1);
        assert!(data.is_some());
        let data = data.unwrap();
        assert_eq!(data.get("username"), Some(&json!("admin")));
        assert_eq!(data.get("password"), Some(&json!("secret")));
    }
}
