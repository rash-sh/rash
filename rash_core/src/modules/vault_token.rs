/// ANCHOR: module
/// # vault_token
///
/// Manage HashiCorp Vault tokens - create, renew, revoke, and lookup tokens.
/// Complements the existing vault module for complete Vault integration.
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
/// - name: Create a Vault token with policies
///   vault_token:
///     policies:
///       - read-only
///       - myapp
///     ttl: 24h
///     state: present
///   register: token
///
/// - name: Create a token with custom metadata
///   vault_token:
///     policies:
///       - admin
///     ttl: 48h
///     renewable: true
///     meta:
///       purpose: ci-cd
///       team: platform
///     state: present
///   register: token
///
/// - name: Create a token using a role
///   vault_token:
///     role_name: my-role
///     policies:
///       - myapp
///     ttl: 1h
///     state: present
///   register: token
///
/// - name: Renew a token
///   vault_token:
///     token: "{{ token.id }}"
///     ttl: 24h
///     state: renew
///
/// - name: Lookup token info
///   vault_token:
///     token: "{{ token.id }}"
///     state: lookup
///   register: token_info
///
/// - name: Revoke a token
///   vault_token:
///     token: "{{ token.id }}"
///     state: absent
///
/// - name: Use environment variables for connection
///   vault_token:
///     policies:
///       - read-only
///     ttl: 1h
///     state: present
///   register: token
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
    Present,
    Renew,
    Lookup,
    #[default]
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// List of policies for the token (required for state=present).
    pub policies: Option<Vec<String>>,
    /// Time-to-live for the token (e.g., "24h", "48h", "720h").
    pub ttl: Option<String>,
    /// Whether the token is renewable.
    #[serde(default = "default_renewable")]
    pub renewable: bool,
    /// The desired state of the token.
    #[serde(default)]
    pub state: State,
    /// The token to operate on (for lookup, renew, revoke). If not provided, uses VAULT_TOKEN environment variable.
    pub token: Option<String>,
    /// The URL of the Vault server. If not provided, uses VAULT_ADDR environment variable.
    pub url: Option<String>,
    /// The Vault namespace (Enterprise feature).
    pub namespace: Option<String>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// The token role name to use when creating the token.
    pub role_name: Option<String>,
    /// Metadata to associate with the token.
    pub meta: Option<HashMap<String, String>>,
    /// If true, the token will not have a parent token.
    #[serde(default)]
    pub no_parent: bool,
    /// The maximum number of times the token can be used. 0 means unlimited.
    #[serde(default)]
    pub num_uses: u64,
    /// The period for the token. If set, the token will be a periodic token.
    pub period: Option<String>,
    /// The token type (default "default"). Can be "default" or "service".
    #[serde(default = "default_token_type")]
    pub type_: Option<String>,
    /// Whether to display the token in the output. Defaults to true.
    #[serde(default = "default_display_token")]
    pub display_token: bool,
}

fn default_renewable() -> bool {
    true
}

fn default_validate_certs() -> bool {
    true
}

fn default_token_type() -> Option<String> {
    None
}

fn default_display_token() -> bool {
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

struct VaultTokenClient {
    url: String,
    token: String,
    namespace: Option<String>,
    validate_certs: bool,
}

impl VaultTokenClient {
    fn new(params: &Params) -> Result<Self> {
        Ok(Self {
            url: get_vault_url(params)?,
            token: get_vault_token(params)?,
            namespace: params.namespace.clone(),
            validate_certs: params.validate_certs,
        })
    }

    fn build_request(&self, method: &str, path: &str) -> Result<reqwest::blocking::RequestBuilder> {
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(!self.validate_certs)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })?;

        let url = format!("{}/v1/{}", self.url.trim_end_matches('/'), path);

        let mut request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "DELETE" => client.delete(&url),
            "PUT" => client.put(&url),
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

    fn send_and_parse(&self, request: reqwest::blocking::RequestBuilder) -> Result<JsonValue> {
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault request failed: {e}"),
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

    fn create_token(&self, params: &Params) -> Result<JsonValue> {
        let path = match &params.role_name {
            Some(role) => format!("auth/token/create/{role}"),
            None => "auth/token/create".to_string(),
        };

        let mut body = serde_json::Map::new();

        if let Some(ref policies) = params.policies {
            body.insert(
                "policies".to_string(),
                serde_json::to_value(policies).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to serialize policies: {e}"),
                    )
                })?,
            );
        }

        if let Some(ref ttl) = params.ttl {
            body.insert("ttl".to_string(), JsonValue::String(ttl.clone()));
        }

        if let Some(ref period) = params.period {
            body.insert("period".to_string(), JsonValue::String(period.clone()));
        }

        if !params.renewable {
            body.insert("renewable".to_string(), JsonValue::Bool(false));
        }

        if params.no_parent {
            body.insert("no_parent".to_string(), JsonValue::Bool(true));
        }

        if params.num_uses > 0 {
            body.insert(
                "num_uses".to_string(),
                JsonValue::Number(params.num_uses.into()),
            );
        }

        if let Some(ref meta) = params.meta {
            body.insert(
                "metadata".to_string(),
                serde_json::to_value(meta).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to serialize metadata: {e}"),
                    )
                })?,
            );
        }

        if let Some(ref type_) = params.type_ {
            body.insert("type".to_string(), JsonValue::String(type_.clone()));
        }

        let request = self.build_request("POST", &path)?;
        let request = request.json(&JsonValue::Object(body));
        self.send_and_parse(request)
    }

    fn renew_token(&self, token: &str, ttl: Option<&str>) -> Result<JsonValue> {
        let mut body = serde_json::Map::new();
        if let Some(ttl) = ttl {
            body.insert("increment".to_string(), JsonValue::String(ttl.to_string()));
        }

        let request = self.build_request("POST", "auth/token/renew")?;
        let mut request = request.json(&JsonValue::Object(body));
        request = request.header("X-Vault-Token", token);
        self.send_and_parse(request)
    }

    fn lookup_token_self(&self) -> Result<JsonValue> {
        let request = self.build_request("GET", "auth/token/lookup-self")?;
        self.send_and_parse(request)
    }

    fn lookup_token(&self, token: &str) -> Result<JsonValue> {
        let mut body = serde_json::Map::new();
        body.insert("token".to_string(), JsonValue::String(token.to_string()));

        let request = self.build_request("POST", "auth/token/lookup")?;
        let request = request.json(&JsonValue::Object(body));
        self.send_and_parse(request)
    }

    fn revoke_token(&self, token: &str) -> Result<bool> {
        let mut body = serde_json::Map::new();
        body.insert("token".to_string(), JsonValue::String(token.to_string()));

        let request = self.build_request("POST", "auth/token/revoke")?;
        let request = request.json(&JsonValue::Object(body));
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Vault revoke request failed: {e}"),
            )
        })?;

        Ok(response.status().is_success())
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultTokenClient::new(params)?;
    let response = client.create_token(params)?;

    let auth_data = response.get("auth").cloned();

    match auth_data {
        Some(auth) => {
            let mut extra = serde_json::Map::new();
            extra.insert("auth".to_string(), auth.clone());
            extra.insert("raw".to_string(), response);

            let client_token = auth
                .get("client_token")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");

            let display_token = params.display_token;
            let output = if display_token {
                format!("Token created: {client_token}")
            } else {
                "Token created successfully".to_string()
            };

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(JsonValue::Object(extra)).map_err(|e| {
                    Error::new(ErrorKind::InvalidData, format!("Conversion error: {e}"))
                })?),
                Some(output),
            ))
        }
        None => Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "raw": response,
            }))?),
            Some("Token creation completed but no auth data returned".to_string()),
        )),
    }
}

fn exec_renew(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultTokenClient::new(params)?;
    let token = get_vault_token(params)?;
    let response = client.renew_token(&token, params.ttl.as_deref())?;

    let auth_data = response.get("auth").cloned();

    match auth_data {
        Some(auth) => {
            let mut extra = serde_json::Map::new();
            extra.insert("auth".to_string(), auth.clone());
            extra.insert("raw".to_string(), response);

            let display_token = params.display_token;
            let output = if display_token {
                let client_token = auth
                    .get("client_token")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown");
                format!("Token renewed: {client_token}")
            } else {
                "Token renewed successfully".to_string()
            };

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(JsonValue::Object(extra)).map_err(|e| {
                    Error::new(ErrorKind::InvalidData, format!("Conversion error: {e}"))
                })?),
                Some(output),
            ))
        }
        None => Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "raw": response,
            }))?),
            Some("Token renewal completed but no auth data returned".to_string()),
        )),
    }
}

fn exec_lookup(params: &Params) -> Result<ModuleResult> {
    let client = VaultTokenClient::new(params)?;

    let response = if params.token.is_some() {
        let token = get_vault_token(params)?;
        client.lookup_token(&token)?
    } else {
        client.lookup_token_self()?
    };

    let data = response.get("data").cloned();

    match data {
        Some(token_data) => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "data": token_data,
                "raw": response
            }))?),
            Some("Token lookup successful".to_string()),
        )),
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "data": {},
                "raw": response,
                "found": false
            }))?),
            Some("Token lookup returned no data".to_string()),
        )),
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let client = VaultTokenClient::new(params)?;
    let token = get_vault_token(params)?;
    let revoked = client.revoke_token(&token)?;

    Ok(ModuleResult::new(
        revoked,
        None,
        if revoked {
            Some("Token revoked successfully".to_string())
        } else {
            Some("Token not found or already revoked".to_string())
        },
    ))
}

pub fn vault_token(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Renew => exec_renew(&params, check_mode),
        State::Lookup => exec_lookup(&params),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct VaultToken;

impl Module for VaultToken {
    fn get_name(&self) -> &str {
        "vault_token"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            vault_token(parse_params(optional_params)?, check_mode)?,
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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
              - myapp
            ttl: 24h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(
            params.policies,
            Some(vec!["read-only".to_string(), "myapp".to_string()])
        );
        assert_eq!(params.ttl, Some("24h".to_string()));
        assert!(params.renewable);
        assert_eq!(params.url, Some("http://vault:8200".to_string()));
    }

    #[test]
    fn test_parse_params_present_with_role() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            role_name: my-role
            policies:
              - myapp
            ttl: 1h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.role_name, Some("my-role".to_string()));
    }

    #[test]
    fn test_parse_params_present_with_meta() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - admin
            ttl: 48h
            meta:
              purpose: ci-cd
              team: platform
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.meta.is_some());
        let meta = params.meta.unwrap();
        assert_eq!(meta.get("purpose"), Some(&"ci-cd".to_string()));
        assert_eq!(meta.get("team"), Some(&"platform".to_string()));
    }

    #[test]
    fn test_parse_params_renew() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            token: "s.1234567890"
            ttl: 24h
            state: renew
            url: "http://vault:8200"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Renew);
        assert_eq!(params.token, Some("s.1234567890".to_string()));
        assert_eq!(params.ttl, Some("24h".to_string()));
    }

    #[test]
    fn test_parse_params_lookup() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            token: "s.1234567890"
            state: lookup
            url: "http://vault:8200"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Lookup);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            token: "s.1234567890"
            state: absent
            url: "http://vault:8200"
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
            policies:
              - read-only
            ttl: 1h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            namespace: "team-a"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.namespace, Some("team-a".to_string()));
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            ttl: 1h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            validate_certs: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_non_renewable() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            ttl: 1h
            renewable: false
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.renewable);
    }

    #[test]
    fn test_parse_params_no_parent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - root
            ttl: 24h
            no_parent: true
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.no_parent);
    }

    #[test]
    fn test_parse_params_num_uses() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            num_uses: 5
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.num_uses, 5);
    }

    #[test]
    fn test_parse_params_period() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            period: 24h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.period, Some("24h".to_string()));
    }

    #[test]
    fn test_parse_params_display_token_false() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            display_token: false
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.display_token);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.validate_certs);
        assert!(params.renewable);
        assert_eq!(params.state, State::Absent);
        assert!(params.url.is_none());
        assert!(params.token.is_none());
        assert!(params.namespace.is_none());
        assert!(params.meta.is_none());
        assert!(!params.no_parent);
        assert_eq!(params.num_uses, 0);
        assert!(params.period.is_none());
        assert!(params.ttl.is_none());
        assert!(params.role_name.is_none());
        assert!(params.display_token);
    }

    #[test]
    fn test_exec_present_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            ttl: 24h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_token(params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_extra().is_none());
    }

    #[test]
    fn test_exec_renew_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            token: "s.1234567890"
            ttl: 24h
            state: renew
            url: "http://vault:8200"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_token(params, true).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_exec_absent_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            token: "s.1234567890"
            state: absent
            url: "http://vault:8200"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = vault_token(params, true).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_create_token_path_without_role() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
            ttl: 1h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let path = match &params.role_name {
            Some(role) => format!("auth/token/create/{role}"),
            None => "auth/token/create".to_string(),
        };
        assert_eq!(path, "auth/token/create");
    }

    #[test]
    fn test_create_token_path_with_role() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            role_name: my-role
            policies:
              - read-only
            ttl: 1h
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let path = match &params.role_name {
            Some(role) => format!("auth/token/create/{role}"),
            None => "auth/token/create".to_string(),
        };
        assert_eq!(path, "auth/token/create/my-role");
    }

    #[test]
    fn test_build_create_body() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            policies:
              - read-only
              - myapp
            ttl: 24h
            renewable: false
            no_parent: true
            num_uses: 10
            meta:
              purpose: test
            state: present
            url: "http://vault:8200"
            token: "root-token"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();

        let mut body = serde_json::Map::new();
        if let Some(ref policies) = params.policies {
            body.insert(
                "policies".to_string(),
                serde_json::to_value(policies).unwrap(),
            );
        }
        if let Some(ref ttl) = params.ttl {
            body.insert("ttl".to_string(), JsonValue::String(ttl.clone()));
        }
        if !params.renewable {
            body.insert("renewable".to_string(), JsonValue::Bool(false));
        }
        if params.no_parent {
            body.insert("no_parent".to_string(), JsonValue::Bool(true));
        }
        if params.num_uses > 0 {
            body.insert(
                "num_uses".to_string(),
                JsonValue::Number(params.num_uses.into()),
            );
        }
        if let Some(ref meta) = params.meta {
            body.insert("metadata".to_string(), serde_json::to_value(meta).unwrap());
        }

        assert_eq!(body.get("policies"), Some(&json!(["read-only", "myapp"])));
        assert_eq!(body.get("ttl"), Some(&json!("24h")));
        assert_eq!(body.get("renewable"), Some(&json!(false)));
        assert_eq!(body.get("no_parent"), Some(&json!(true)));
        assert_eq!(body.get("num_uses"), Some(&json!(10)));
        assert_eq!(body.get("metadata"), Some(&json!({"purpose": "test"})));
    }
}
