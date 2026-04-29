/// ANCHOR: module
/// # vault_secret
///
/// Read, write, and delete secrets from HashiCorp Vault with granular
/// secret operations supporting both KV v1 and v2 engines.
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
///   vault_secret:
///     path: secret/data/myapp/config
///     state: read
///     url: "http://vault:8200"
///     token: "{{ vault_token }}"
///   register: app_secrets
///
/// - name: Write secret to Vault
///   vault_secret:
///     path: secret/data/myapp/config
///     state: present
///     url: "http://vault:8200"
///     token: "{{ vault_token }}"
///     secret:
///       username: admin
///       password: "{{ db_password }}"
///
/// - name: Delete secret from Vault
///   vault_secret:
///     path: secret/data/myapp/config
///     state: absent
///     url: "http://vault:8200"
///     token: "{{ vault_token }}"
///
/// - name: Read secret from KV v1 engine
///   vault_secret:
///     path: kv/myapp/config
///     state: read
///     version: 1
///     url: "http://vault:8200"
///     token: "{{ vault_token }}"
///   register: kv1_secrets
///
/// - name: Write secret using environment variables
///   vault_secret:
///     path: secret/data/myapp/config
///     state: present
///     secret:
///       api_key: "{{ api_key }}"
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
    #[default]
    Read,
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The path to the secret in Vault.
    pub path: String,
    /// The secret data to write (required for state=present).
    pub secret: Option<HashMap<String, JsonValue>>,
    /// The desired state of the secret.
    #[serde(default)]
    pub state: State,
    /// The secrets engine type.
    #[serde(default = "default_engine")]
    pub engine: String,
    /// The KV secrets engine version (1 or 2).
    #[serde(default = "default_version")]
    pub version: u8,
    /// The Vault token for authentication. If not provided, uses VAULT_TOKEN environment variable.
    pub token: Option<String>,
    /// The URL of the Vault server. If not provided, uses VAULT_ADDR environment variable.
    pub url: Option<String>,
    /// The Vault namespace (Enterprise feature).
    pub namespace: Option<String>,
    /// The mount point for the secrets engine.
    #[serde(default = "default_mount")]
    pub mount: String,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_engine() -> String {
    "kv".to_string()
}

fn default_version() -> u8 {
    2
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

struct VaultSecretClient {
    url: String,
    token: String,
    namespace: Option<String>,
    validate_certs: bool,
}

impl VaultSecretClient {
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
        if status.as_u16() == 404 {
            return Ok(JsonValue::Null);
        }

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

        serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Vault response: {e}"),
            )
        })
    }

    fn write(&self, full_path: &str, body: &JsonValue) -> Result<JsonValue> {
        let request = self.build_request("POST", full_path)?;
        let response = request.json(body).send().map_err(|e| {
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

        serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Vault response: {e}"),
            )
        })
    }

    fn delete(&self, full_path: &str) -> Result<bool> {
        let request = self.build_request("DELETE", full_path)?;
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault delete request failed: {e}"),
            )
        })?;

        Ok(response.status().is_success())
    }
}

fn build_full_path(mount: &str, path: &str, version: u8) -> String {
    if version == 2 {
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
    } else if path.starts_with(&format!("{mount}/")) {
        path.to_string()
    } else {
        format!("{mount}/{path}")
    }
}

fn extract_secret_data(response: &JsonValue, version: u8) -> Option<HashMap<String, JsonValue>> {
    if version == 2 {
        response
            .get("data")
            .and_then(|d| d.get("data"))
            .and_then(|d| d.as_object())
            .map(|obj| obj.clone().into_iter().collect())
    } else {
        response
            .get("data")
            .and_then(|d| d.as_object())
            .map(|obj| obj.clone().into_iter().collect())
    }
}

fn exec_read(params: &Params) -> Result<ModuleResult> {
    let client = VaultSecretClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, params.version);

    let response = client.read(&full_path)?;

    if response.is_null() {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "data": {},
                "found": false
            }))?),
            Some("Secret not found".to_string()),
        ));
    }

    let secret_data = extract_secret_data(&response, params.version);

    match secret_data {
        Some(data) => {
            let metadata = if params.version == 2 {
                response
                    .get("data")
                    .and_then(|d| d.get("metadata"))
                    .cloned()
            } else {
                None
            };

            let mut extra = serde_json::Map::new();
            extra.insert(
                "data".to_string(),
                serde_json::to_value(&data).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to serialize data: {e}"),
                    )
                })?,
            );
            extra.insert("raw".to_string(), response.clone());
            if let Some(meta) = metadata {
                extra.insert("metadata".to_string(), meta);
            }

            Ok(ModuleResult::new(
                false,
                Some(value::to_value(JsonValue::Object(extra)).map_err(|e| {
                    Error::new(ErrorKind::InvalidData, format!("Conversion error: {e}"))
                })?),
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
    let secret = params.secret.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "secret parameter is required when state=present",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultSecretClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, params.version);

    let body = if params.version == 2 {
        let mut map = serde_json::Map::new();
        map.insert(
            "data".to_string(),
            serde_json::to_value(secret).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to serialize secret: {e}"),
                )
            })?,
        );
        JsonValue::Object(map)
    } else {
        serde_json::to_value(secret).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize secret: {e}"),
            )
        })?
    };

    let response = client.write(&full_path, &body)?;

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(json!({
            "data": secret,
            "raw": response
        }))?),
        Some("Secret written successfully".to_string()),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultSecretClient::new(params)?;
    let full_path = build_full_path(&params.mount, &params.path, params.version);

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

pub fn vault_secret(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    if params.version != 1 && params.version != 2 {
        return Err(Error::new(ErrorKind::InvalidData, "version must be 1 or 2"));
    }

    match params.state {
        State::Read => exec_read(&params),
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct VaultSecret;

impl Module for VaultSecret {
    fn get_name(&self) -> &str {
        "vault_secret"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            vault_secret(parse_params(optional_params)?, check_mode)?,
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
    use serde_json::json;

    #[test]
    fn test_parse_params_read() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp/config"
            url: "http://vault:8200"
            token: "test-token"
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "secret/data/myapp/config");
        assert_eq!(params.url, Some("http://vault:8200".to_string()));
        assert_eq!(params.token, Some("test-token".to_string()));
        assert_eq!(params.state, State::Read);
        assert_eq!(params.version, 2);
        assert_eq!(params.engine, "kv");
    }

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp/config"
            url: "http://vault:8200"
            token: "test-token"
            secret:
              username: admin
              password: secret123
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert!(params.secret.is_some());
        let secret = params.secret.unwrap();
        assert_eq!(secret.get("username"), Some(&json!("admin")));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp/config"
            url: "http://vault:8200"
            token: "test-token"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_kv_v1() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "kv/myapp"
            url: "http://vault:8200"
            token: "test-token"
            version: 1
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.version, 1);
    }

    #[test]
    fn test_parse_params_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp/config"
            url: "http://vault:8200"
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
    fn test_parse_params_custom_mount() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "myapp"
            url: "http://vault:8200"
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
            path: "secret/data/myapp/config"
            url: "http://vault:8200"
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
            path: "secret/data/myapp/config"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mount, "secret");
        assert!(params.validate_certs);
        assert_eq!(params.version, 2);
        assert_eq!(params.state, State::Read);
        assert_eq!(params.engine, "kv");
        assert!(params.secret.is_none());
        assert!(params.url.is_none());
        assert!(params.token.is_none());
        assert!(params.namespace.is_none());
    }

    #[test]
    fn test_build_full_path_v2() {
        assert_eq!(build_full_path("secret", "myapp", 2), "secret/data/myapp");
        assert_eq!(
            build_full_path("secret", "data/myapp", 2),
            "secret/data/myapp"
        );
        assert_eq!(
            build_full_path("secret", "secret/myapp", 2),
            "secret/data/myapp"
        );
        assert_eq!(
            build_full_path("secret", "secret/data/myapp", 2),
            "secret/data/myapp"
        );
    }

    #[test]
    fn test_build_full_path_v1() {
        assert_eq!(build_full_path("kv", "myapp", 1), "kv/myapp");
        assert_eq!(build_full_path("kv", "kv/myapp", 1), "kv/myapp");
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
        let data = extract_secret_data(&response, 2);
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
        let data = extract_secret_data(&response, 1);
        assert!(data.is_some());
        let data = data.unwrap();
        assert_eq!(data.get("username"), Some(&json!("admin")));
        assert_eq!(data.get("password"), Some(&json!("secret")));
    }

    #[test]
    fn test_extract_secret_data_null() {
        let response = JsonValue::Null;
        let data = extract_secret_data(&response, 2);
        assert!(data.is_none());
    }

    #[test]
    fn test_invalid_version() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "http://vault:8200"
            token: "test-token"
            version: 3
            state: read
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_secret(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_present_without_secret() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "http://vault:8200"
            token: "test-token"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_secret(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_present_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "http://vault:8200"
            token: "test-token"
            secret:
              key: value
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_secret(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_extra().is_none());
    }

    #[test]
    fn test_exec_absent_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: "secret/data/myapp"
            url: "http://vault:8200"
            token: "test-token"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_secret(params, true).unwrap();
        assert!(result.get_changed());
    }
}
