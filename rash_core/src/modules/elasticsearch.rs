/// ANCHOR: module
/// # elasticsearch
///
/// Manage Elasticsearch indices and documents.
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
/// - name: Create an index with settings
///   elasticsearch:
///     hostname: localhost
///     port: 9200
///     index: logs
///     state: present
///     body:
///       settings:
///         number_of_shards: 3
///         number_of_replicas: 1
///
/// - name: Create an index with mappings
///   elasticsearch:
///     hostname: localhost
///     port: 9200
///     index: products
///     state: present
///     body:
///       mappings:
///         properties:
///           name:
///             type: text
///           price:
///             type: float
///
/// - name: Query documents in an index
///   elasticsearch:
///     hostname: localhost
///     port: 9200
///     index: logs
///     state: query
///     body:
///       query:
///         match:
///           level: error
///   register: search_results
///
/// - name: Index a document with a specific ID
///   elasticsearch:
///     hostname: localhost
///     port: 9200
///     index: logs
///     id: "doc-001"
///     state: present
///     body:
///       message: "Application started"
///       level: info
///
/// - name: Delete an index
///   elasticsearch:
///     hostname: localhost
///     port: 9200
///     index: old-logs
///     state: absent
///
/// - name: Query with authentication
///   elasticsearch:
///     hostname: es-cluster.example.com
///     port: 9200
///     index: metrics
///     state: query
///     username: admin
///     password: '{{ es_password }}'
///     body:
///       query:
///         match_all: {}
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
    Query,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Elasticsearch server hostname.
    #[serde(default = "default_hostname")]
    pub hostname: String,
    /// Elasticsearch server port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Index name.
    pub index: String,
    /// The desired state of the index or document.
    #[serde(default)]
    pub state: State,
    /// Document body or index settings/mappings.
    pub body: Option<JsonValue>,
    /// Document ID (for document-level operations).
    pub id: Option<String>,
    /// Authentication username.
    pub username: Option<String>,
    /// Authentication password.
    pub password: Option<String>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_hostname() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    9200
}

fn default_validate_certs() -> bool {
    true
}

struct ElasticsearchClient {
    hostname: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    validate_certs: bool,
}

impl ElasticsearchClient {
    fn new(params: &Params) -> Self {
        Self {
            hostname: params.hostname.clone(),
            port: params.port,
            username: params.username.clone(),
            password: params.password.clone(),
            validate_certs: params.validate_certs,
        }
    }

    fn build_url(&self, path: &str) -> String {
        format!("http://{}:{}/{}", self.hostname, self.port, path)
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

    fn add_auth(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request.basic_auth(username, Some(password))
        } else {
            request
        }
    }

    fn check_response(
        &self,
        response: reqwest::blocking::Response,
    ) -> Result<reqwest::blocking::Response> {
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch returned status {}: {}", status, error_text),
            ));
        }
        Ok(response)
    }

    fn index_exists(&self, index: &str) -> Result<bool> {
        let url = self.build_url(index);
        let client = self.build_client()?;
        let request = self.add_auth(client.head(&url));
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch index check failed: {e}"),
            )
        })?;
        Ok(response.status().is_success())
    }

    fn create_index(&self, index: &str, body: Option<&JsonValue>) -> Result<bool> {
        let url = self.build_url(index);
        let client = self.build_client()?;

        let mut request = self.add_auth(client.put(&url));
        if let Some(json_body) = body {
            request = request.json(json_body);
        }

        let response = self.check_response(request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch create index request failed: {e}"),
            )
        })?)?;

        let acknowledged = response.json::<JsonValue>().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse response: {e}"),
            )
        })?;

        Ok(acknowledged
            .get("acknowledged")
            .and_then(|v| v.as_bool())
            .unwrap_or(true))
    }

    fn delete_index(&self, index: &str) -> Result<bool> {
        let url = self.build_url(index);
        let client = self.build_client()?;
        let request = self.add_auth(client.delete(&url));
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch delete index request failed: {e}"),
            )
        })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }

        self.check_response(response)?;

        Ok(true)
    }

    fn index_document(&self, index: &str, id: &str, body: &JsonValue) -> Result<(bool, String)> {
        let url = self.build_url(&format!("{}/_doc/{}", index, id));
        let client = self.build_client()?;

        let request = self.add_auth(client.put(&url).json(body));
        let response = self.check_response(request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch index document request failed: {e}"),
            )
        })?)?;

        let result: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse response: {e}"),
            )
        })?;

        let result_val = result
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let changed = result_val != "noop";
        Ok((changed, result_val.to_string()))
    }

    fn query_index(&self, index: &str, body: Option<&JsonValue>) -> Result<JsonValue> {
        let url = self.build_url(&format!("{}/_search", index));
        let client = self.build_client()?;

        let mut request = self.add_auth(client.get(&url));
        if let Some(query_body) = body {
            request = request.json(query_body);
        }

        let response = self.check_response(request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Elasticsearch query request failed: {e}"),
            )
        })?)?;

        response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse response: {e}"),
            )
        })
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ElasticsearchClient::new(params);

    if let Some(doc_id) = &params.id {
        let body = params.body.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "body parameter is required when indexing a document",
            )
        })?;

        if check_mode {
            return Ok(ModuleResult::new(true, None, None));
        }

        let (changed, result) = client.index_document(&params.index, doc_id, body)?;
        Ok(ModuleResult::new(
            changed,
            Some(value::to_value(json!({
                "index": params.index,
                "id": doc_id,
                "result": result
            }))?),
            Some(format!("Document {} indexed in {}", doc_id, params.index)),
        ))
    } else {
        let exists = client.index_exists(&params.index)?;

        if exists {
            if check_mode {
                return Ok(ModuleResult::new(false, None, None));
            }
            Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "index": params.index,
                    "changed": false
                }))?),
                Some(format!("Index {} already exists", params.index)),
            ))
        } else {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            client.create_index(&params.index, params.body.as_ref())?;
            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "index": params.index,
                    "changed": true
                }))?),
                Some(format!("Index {} created", params.index)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = ElasticsearchClient::new(params);

    if check_mode {
        let exists = client.index_exists(&params.index)?;
        return Ok(ModuleResult::new(exists, None, None));
    }

    let deleted = client.delete_index(&params.index)?;
    Ok(ModuleResult::new(
        deleted,
        Some(value::to_value(json!({
            "index": params.index,
            "deleted": deleted
        }))?),
        if deleted {
            Some(format!("Index {} deleted", params.index))
        } else {
            Some(format!("Index {} not found", params.index))
        },
    ))
}

fn exec_query(params: &Params) -> Result<ModuleResult> {
    let client = ElasticsearchClient::new(params);
    let result = client.query_index(&params.index, params.body.as_ref())?;

    let total_hits = result
        .get("hits")
        .and_then(|h| h.get("total"))
        .and_then(|t| t.get("value"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(ModuleResult::new(
        false,
        Some(value::to_value(&result)?),
        Some(format!("Query returned {} hits", total_hits)),
    ))
}

pub fn elasticsearch(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
        State::Query => exec_query(&params),
    }
}

#[derive(Debug)]
pub struct Elasticsearch;

impl Module for Elasticsearch {
    fn get_name(&self) -> &str {
        "elasticsearch"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            elasticsearch(parse_params(optional_params)?, check_mode)?,
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
    fn test_parse_params_present_index() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            hostname: localhost
            port: 9200
            index: logs
            state: present
            body:
              settings:
                number_of_shards: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.hostname, "localhost");
        assert_eq!(params.port, 9200);
        assert_eq!(params.index, "logs");
        assert_eq!(params.state, State::Present);
        assert!(params.body.is_some());
        assert_eq!(params.id, None);
    }

    #[test]
    fn test_parse_params_present_document() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            index: logs
            id: "doc-001"
            state: present
            body:
              message: "Application started"
              level: info
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.index, "logs");
        assert_eq!(params.id, Some("doc-001".to_string()));
        assert_eq!(params.state, State::Present);
        assert!(params.body.is_some());
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            index: old-logs
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.index, "old-logs");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_query() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            index: logs
            state: query
            body:
              query:
                match:
                  level: error
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.index, "logs");
        assert_eq!(params.state, State::Query);
        assert!(params.body.is_some());
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            index: metrics
            state: query
            username: admin
            password: secret123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.username, Some("admin".to_string()));
        assert_eq!(params.password, Some("secret123".to_string()));
    }

    #[test]
    fn test_parse_params_custom_host_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            hostname: es-cluster.example.com
            port: 9200
            index: metrics
            state: query
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.hostname, "es-cluster.example.com");
        assert_eq!(params.port, 9200);
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            index: logs
            state: query
            validate_certs: false
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
            index: logs
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.hostname, "localhost");
        assert_eq!(params.port, 9200);
        assert!(params.validate_certs);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.body, None);
        assert_eq!(params.id, None);
        assert_eq!(params.username, None);
        assert_eq!(params.password, None);
    }

    #[test]
    fn test_elasticsearch_client_build_url() {
        let params = Params {
            hostname: "localhost".to_string(),
            port: 9200,
            index: "test".to_string(),
            state: State::Present,
            body: None,
            id: None,
            username: None,
            password: None,
            validate_certs: true,
        };
        let client = ElasticsearchClient::new(&params);
        assert_eq!(
            client.build_url("logs/_search"),
            "http://localhost:9200/logs/_search"
        );
        assert_eq!(client.build_url("logs"), "http://localhost:9200/logs");
    }

    #[test]
    fn test_elasticsearch_client_build_url_custom_host() {
        let params = Params {
            hostname: "es.example.com".to_string(),
            port: 9200,
            index: "test".to_string(),
            state: State::Present,
            body: None,
            id: None,
            username: None,
            password: None,
            validate_certs: true,
        };
        let client = ElasticsearchClient::new(&params);
        assert_eq!(
            client.build_url("logs/_search"),
            "http://es.example.com:9200/logs/_search"
        );
    }
}
