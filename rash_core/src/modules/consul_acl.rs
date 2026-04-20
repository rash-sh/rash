/// ANCHOR: module
/// # consul_acl
///
/// Manage Consul ACL tokens and policies.
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
/// - name: Create a client ACL token
///   consul_acl:
///     name: agent-token
///     token_type: client
///     rules: |
///       node "" {
///         policy = "read"
///       }
///     state: present
///
/// - name: Create a management ACL token
///   consul_acl:
///     name: admin-token
///     token_type: management
///     state: present
///
/// - name: Create a token with policies
///   consul_acl:
///     name: app-token
///     policies:
///       - read-only
///       - service-write
///     state: present
///
/// - name: Create a token with TTL
///   consul_acl:
///     name: temp-token
///     token_type: client
///     ttl: 1h
///     state: present
///
/// - name: Delete an ACL token
///   consul_acl:
///     name: old-token
///     state: absent
///
/// - name: Create token with custom Consul server and auth
///   consul_acl:
///     name: secure-token
///     token_type: client
///     rules: |
///       service "myapp" {
///         policy = "write"
///       }
///     host: consul-server.example.com
///     port: 8500
///     token: "{{ consul_management_token }}"
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
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    #[default]
    Client,
    Management,
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The name of the ACL token.
    pub name: String,
    /// The type of token (client or management).
    /// **[default: `"client"`]**
    #[serde(default)]
    pub token_type: TokenType,
    /// ACL rules in HCL format (legacy Consul ACL system).
    pub rules: Option<String>,
    /// The desired state of the ACL token.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Management token for authentication with Consul.
    pub token: Option<String>,
    /// Existing token accessor ID to update or delete.
    pub token_id: Option<String>,
    /// List of policy names to attach to the token.
    pub policies: Option<Vec<String>>,
    /// Time-to-live for the token (e.g. "1h", "30m").
    pub ttl: Option<String>,
    /// The Consul host.
    /// **[default: `"localhost"`]**
    #[serde(default = "default_host")]
    pub host: String,
    /// The Consul port.
    /// **[default: `8500`]**
    #[serde(default = "default_port")]
    pub port: u16,
    /// Validate SSL certificates.
    /// **[default: `true`]**
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
    /// The datacenter to use.
    pub dc: Option<String>,
    /// The namespace (Consul Enterprise).
    pub ns: Option<String>,
}

struct ConsulAclClient {
    host: String,
    port: u16,
    token: Option<String>,
    dc: Option<String>,
    ns: Option<String>,
    validate_certs: bool,
}

impl ConsulAclClient {
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

    fn build_url(&self, path: &str) -> String {
        let mut url = format!("http://{}:{}/v1/acl/{}", self.host, self.port, path);

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

    fn list_tokens(&self) -> Result<Vec<JsonValue>> {
        let url = self.build_url("tokens");
        let client = self.build_client()?;

        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul list tokens request failed: {e}"),
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

        serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })
    }

    fn get_token_by_accessor(&self, accessor_id: &str) -> Result<Option<JsonValue>> {
        let url = self.build_url(&format!("token/{}", accessor_id));
        let client = self.build_client()?;

        let request = self.add_token_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul get token request failed: {e}"),
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

        let token: JsonValue = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })?;

        Ok(Some(token))
    }

    fn find_token_by_name(&self, name: &str) -> Result<Option<JsonValue>> {
        let tokens = self.list_tokens()?;
        for token_entry in tokens {
            if let Some(entry_name) = token_entry.get("Description").and_then(|v| v.as_str())
                && entry_name == name
                && let Some(accessor_id) = token_entry.get("AccessorID").and_then(|v| v.as_str())
            {
                return self.get_token_by_accessor(accessor_id);
            }
        }
        Ok(None)
    }

    fn find_token(&self, params: &Params) -> Result<Option<JsonValue>> {
        if let Some(ref token_id) = params.token_id {
            self.get_token_by_accessor(token_id)
        } else {
            self.find_token_by_name(&params.name)
        }
    }

    fn build_token_body(&self, params: &Params) -> serde_json::Map<String, JsonValue> {
        let mut body = serde_json::Map::new();
        body.insert(
            "Description".to_string(),
            JsonValue::String(params.name.clone()),
        );

        if let Some(ref rules) = params.rules {
            body.insert("Rules".to_string(), JsonValue::String(rules.clone()));
        }

        if let Some(ref policies) = params.policies {
            let policy_links: Vec<JsonValue> = policies
                .iter()
                .map(|p| {
                    let mut map = serde_json::Map::new();
                    map.insert("Name".to_string(), JsonValue::String(p.clone()));
                    JsonValue::Object(map)
                })
                .collect();
            body.insert("Policies".to_string(), JsonValue::Array(policy_links));
        }

        if let Some(ref ttl) = params.ttl {
            body.insert("ExpirationTTL".to_string(), JsonValue::String(ttl.clone()));
        }

        body
    }

    fn create_token(&self, params: &Params) -> Result<JsonValue> {
        let url = self.build_url("token");
        let client = self.build_client()?;

        let mut body = self.build_token_body(params);
        match params.token_type {
            TokenType::Management => {
                body.insert(
                    "Type".to_string(),
                    JsonValue::String("management".to_string()),
                );
            }
            TokenType::Client => {
                body.insert("Type".to_string(), JsonValue::String("client".to_string()));
            }
        }

        let request = self.add_token_header(client.put(&url).json(&JsonValue::Object(body)));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul create token request failed: {e}"),
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

        serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })
    }

    fn update_token(&self, accessor_id: &str, params: &Params) -> Result<JsonValue> {
        let url = self.build_url(&format!("token/{}", accessor_id));
        let client = self.build_client()?;

        let body = self.build_token_body(params);

        let request = self.add_token_header(client.put(&url).json(&JsonValue::Object(body)));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul update token request failed: {e}"),
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

        serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Consul response: {e}"),
            )
        })
    }

    fn delete_token(&self, accessor_id: &str) -> Result<bool> {
        let url = self.build_url(&format!("token/{}", accessor_id));
        let client = self.build_client()?;

        let request = self.add_token_header(client.delete(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Consul delete token request failed: {e}"),
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

        Ok(true)
    }
}

fn needs_update(existing: &JsonValue, params: &Params) -> bool {
    if let Some(description) = existing.get("Description").and_then(|v| v.as_str())
        && description != params.name
    {
        return true;
    }

    if let Some(ref rules) = params.rules {
        if let Some(existing_rules) = existing.get("Rules").and_then(|v| v.as_str())
            && existing_rules != rules
        {
            return true;
        } else if existing.get("Rules").and_then(|v| v.as_str()).is_none() {
            return true;
        }
    }

    if let Some(ref policies) = params.policies {
        let existing_policies: Vec<String> = existing
            .get("Policies")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        p.get("Name")
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        if &existing_policies != policies {
            return true;
        }
    }

    if let Some(ref ttl) = params.ttl {
        if let Some(existing_ttl) = existing.get("ExpirationTTL").and_then(|v| v.as_str())
            && existing_ttl != ttl
        {
            return true;
        } else if existing
            .get("ExpirationTTL")
            .and_then(|v| v.as_str())
            .is_none()
        {
            return true;
        }
    }

    false
}

fn get_accessor_id(token: &JsonValue, context: &str) -> Result<String> {
    token
        .get("AccessorID")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Consul response missing AccessorID{}", context),
            )
        })
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulAclClient::new(params);

    match client.find_token(params)? {
        Some(existing) => {
            let accessor_id = get_accessor_id(&existing, " in existing token")?;

            if needs_update(&existing, params) {
                if check_mode {
                    return Ok(ModuleResult::new(true, None, None));
                }

                client.update_token(&accessor_id, params)?;

                Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "accessor_id": accessor_id,
                        "name": params.name,
                        "updated": true
                    }))?),
                    Some(format!("ACL token '{}' updated", params.name)),
                ))
            } else {
                Ok(ModuleResult::new(
                    false,
                    Some(value::to_value(json!({
                        "accessor_id": accessor_id,
                        "name": params.name,
                        "changed": false
                    }))?),
                    Some(format!(
                        "ACL token '{}' already exists with correct settings",
                        params.name
                    )),
                ))
            }
        }
        None => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            let created = client.create_token(params)?;
            let accessor_id = get_accessor_id(&created, " in create response")?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "accessor_id": accessor_id,
                    "name": params.name,
                    "created": true
                }))?),
                Some(format!("ACL token '{}' created", params.name)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ConsulAclClient::new(params);

    match client.find_token(params)? {
        Some(existing) => {
            let accessor_id = get_accessor_id(&existing, " in existing token")?;

            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            let deleted = client.delete_token(&accessor_id)?;

            Ok(ModuleResult::new(
                deleted,
                Some(value::to_value(json!({
                    "accessor_id": accessor_id,
                    "name": params.name,
                    "deleted": deleted
                }))?),
                if deleted {
                    Some(format!("ACL token '{}' deleted", params.name))
                } else {
                    Some(format!(
                        "ACL token '{}' not found for deletion",
                        params.name
                    ))
                },
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "name": params.name,
                "found": false
            }))?),
            Some(format!("ACL token '{}' not found", params.name)),
        )),
    }
}

fn consul_acl_impl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct ConsulAcl;

impl Module for ConsulAcl {
    fn get_name(&self) -> &str {
        "consul_acl"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            consul_acl_impl(parse_params(optional_params)?, check_mode)?,
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
    fn test_parse_params_present_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: agent-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "agent-token");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.token_type, TokenType::Client);
    }

    #[test]
    fn test_parse_params_present_with_rules() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: agent-token
            token_type: client
            rules: |
              node "" {
                policy = "read"
              }
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "agent-token");
        assert_eq!(params.token_type, TokenType::Client);
        assert!(params.rules.is_some());
        assert!(params.rules.as_ref().unwrap().contains("policy = \"read\""));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_management() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: admin-token
            token_type: management
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "admin-token");
        assert_eq!(params.token_type, TokenType::Management);
    }

    #[test]
    fn test_parse_params_with_policies() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app-token
            policies:
              - read-only
              - service-write
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.policies,
            Some(vec!["read-only".to_string(), "service-write".to_string(),])
        );
    }

    #[test]
    fn test_parse_params_with_ttl() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: temp-token
            token_type: client
            ttl: 1h
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.ttl, Some("1h".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-token
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old-token");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_token_id() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: existing-token
            token_id: 00000000-0000-0000-0000-000000000001
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.token_id,
            Some("00000000-0000-0000-0000-000000000001".to_string())
        );
    }

    #[test]
    fn test_parse_params_with_auth_token() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: secure-token
            token: my-management-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.token, Some("my-management-token".to_string()));
    }

    #[test]
    fn test_parse_params_with_host_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-token
            host: consul-server.example.com
            port: 8501
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "consul-server.example.com");
        assert_eq!(params.port, 8501);
    }

    #[test]
    fn test_parse_params_with_datacenter() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: dc-token
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
            name: ns-token
            ns: team-a
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.ns, Some("team-a".to_string()));
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(r#"name: test-token"#).unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port, 8500);
        assert!(params.validate_certs);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.token_type, TokenType::Client);
        assert!(params.rules.is_none());
        assert!(params.policies.is_none());
        assert!(params.ttl.is_none());
        assert!(params.token.is_none());
        assert!(params.token_id.is_none());
        assert!(params.dc.is_none());
        assert!(params.ns.is_none());
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-token
            validate_certs: false
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_consul_client_build_url_token() {
        let params = Params {
            name: "test".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        let client = ConsulAclClient::new(&params);
        assert_eq!(
            client.build_url("token"),
            "http://localhost:8500/v1/acl/token"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc() {
        let params = Params {
            name: "test".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: Some("dc2".to_string()),
            ns: None,
        };
        let client = ConsulAclClient::new(&params);
        assert_eq!(
            client.build_url("token"),
            "http://localhost:8500/v1/acl/token?dc=dc2"
        );
    }

    #[test]
    fn test_consul_client_build_url_with_dc_and_ns() {
        let params = Params {
            name: "test".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: Some("dc2".to_string()),
            ns: Some("team-a".to_string()),
        };
        let client = ConsulAclClient::new(&params);
        assert_eq!(
            client.build_url("token"),
            "http://localhost:8500/v1/acl/token?dc=dc2&ns=team-a"
        );
    }

    #[test]
    fn test_consul_client_build_url_token_by_id() {
        let params = Params {
            name: "test".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        let client = ConsulAclClient::new(&params);
        assert_eq!(
            client.build_url("token/abc-123"),
            "http://localhost:8500/v1/acl/token/abc-123"
        );
    }

    #[test]
    fn test_needs_update_no_change() {
        let existing = serde_json::json!({
            "Description": "agent-token",
            "Rules": "node \"\" { policy = \"read\" }",
            "Policies": [{"Name": "read-only"}]
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: Some("node \"\" { policy = \"read\" }".to_string()),
            state: State::Present,
            token: None,
            token_id: None,
            policies: Some(vec!["read-only".to_string()]),
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(!needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_name_changed() {
        let existing = serde_json::json!({
            "Description": "old-name",
        });
        let params = Params {
            name: "new-name".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_rules_changed() {
        let existing = serde_json::json!({
            "Description": "agent-token",
            "Rules": "node \"\" { policy = \"read\" }",
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: Some("node \"\" { policy = \"write\" }".to_string()),
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_policies_changed() {
        let existing = serde_json::json!({
            "Description": "agent-token",
            "Policies": [{"Name": "read-only"}],
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: Some(vec!["read-only".to_string(), "write".to_string()]),
            ttl: None,
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_ttl_changed() {
        let existing = serde_json::json!({
            "Description": "agent-token",
            "ExpirationTTL": "1h",
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: Some("2h".to_string()),
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_ttl_added() {
        let existing = serde_json::json!({
            "Description": "agent-token",
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: Some("1h".to_string()),
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(needs_update(&existing, &params));
    }

    #[test]
    fn test_needs_update_ttl_unchanged() {
        let existing = serde_json::json!({
            "Description": "agent-token",
            "ExpirationTTL": "1h",
        });
        let params = Params {
            name: "agent-token".to_string(),
            token_type: TokenType::Client,
            rules: None,
            state: State::Present,
            token: None,
            token_id: None,
            policies: None,
            ttl: Some("1h".to_string()),
            host: "localhost".to_string(),
            port: 8500,
            validate_certs: true,
            dc: None,
            ns: None,
        };
        assert!(!needs_update(&existing, &params));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-token
            unknown: field
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: full-token
            token_type: management
            rules: |
              node "" {
                policy = "write"
              }
            state: present
            token: management-token
            token_id: 00000000-0000-0000-0000-000000000002
            policies:
              - global-read
              - global-write
            ttl: 2h
            host: consul.example.com
            port: 8501
            validate_certs: false
            dc: dc3
            ns: team-b
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "full-token");
        assert_eq!(params.token_type, TokenType::Management);
        assert!(params.rules.is_some());
        assert_eq!(params.state, State::Present);
        assert_eq!(params.token, Some("management-token".to_string()));
        assert_eq!(
            params.token_id,
            Some("00000000-0000-0000-0000-000000000002".to_string())
        );
        assert_eq!(
            params.policies,
            Some(vec!["global-read".to_string(), "global-write".to_string(),])
        );
        assert_eq!(params.ttl, Some("2h".to_string()));
        assert_eq!(params.host, "consul.example.com");
        assert_eq!(params.port, 8501);
        assert!(!params.validate_certs);
        assert_eq!(params.dc, Some("dc3".to_string()));
        assert_eq!(params.ns, Some("team-b".to_string()));
    }
}
