/// ANCHOR: module
/// # uri
///
/// Interacts with HTTP and HTTPS web services.
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
/// - uri:
///     url: http://www.example.com
///     method: GET
///
/// - uri:
///     url: https://httpbin.org/post
///     method: POST
///     body: '{"key": "value"}'
///     headers:
///       Content-Type: application/json
///     status_code: [200, 201]
///
/// - uri:
///     url: https://api.example.com/data
///     method: GET
///     return_content: true
///   register: api_response
///
/// - uri:
///     url: https://httpbin.org/basic-auth/user/pass
///     method: GET
///     url_username: user
///     url_password: pass
///     force_basic_auth: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::time::Duration;

use minijinja::Value;
use reqwest::Method;
use reqwest::blocking::{Client, Response};
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_yaml::Value as YamlValue;
use serde_yaml::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// HTTP or HTTPS URL in the form (http|https)://host.domain[:port]/path
    pub url: String,
    /// The HTTP method of the request or response
    #[serde(default = "default_method")]
    pub method: String,
    /// The body of the http request/response to the web service
    pub body: Option<String>,
    /// Add custom HTTP headers to a request in the format of a hash
    pub headers: Option<HashMap<String, String>>,
    /// A list of valid, numeric, HTTP status codes that signifies success of the request
    #[serde(default = "default_status_code")]
    pub status_code: Vec<i32>,
    /// The socket level timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// Whether or not to return the body of the response as a "content" key in the dictionary result
    #[serde(default)]
    pub return_content: bool,
    /// A username for the module to use for Basic authentication
    pub url_username: Option<String>,
    /// A password for the module to use for Basic authentication
    pub url_password: Option<String>,
    /// Force the sending of the Basic authentication header upon initial request
    #[serde(default)]
    pub force_basic_auth: bool,
    /// If false, SSL certificates will not be validated
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_status_code() -> Vec<i32> {
    vec![200]
}

fn default_timeout() -> u64 {
    30
}

fn default_validate_certs() -> bool {
    true
}

fn make_request(params: &Params) -> Result<Response> {
    let client = Client::builder()
        .timeout(Duration::from_secs(params.timeout))
        .danger_accept_invalid_certs(!params.validate_certs)
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {}", e),
            )
        })?;

    let method = params.method.parse::<Method>().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid HTTP method '{}': {}", params.method, e),
        )
    })?;

    let mut request_builder = client.request(method, &params.url);

    // Add headers
    if let Some(headers) = &params.headers {
        for (key, value) in headers {
            request_builder = request_builder.header(key, value);
        }
    }

    // Add body
    if let Some(body) = &params.body {
        // Try to parse as JSON first, if it fails, use as plain text
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(body) {
            request_builder = request_builder.json(&json_value);
        } else {
            request_builder = request_builder.body(body.clone());
        }
    }

    // Add basic auth
    if let (Some(username), Some(password)) = (&params.url_username, &params.url_password) {
        request_builder = request_builder.basic_auth(username, Some(password));
    }

    let response = request_builder.send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("HTTP request failed: {}", e),
        )
    })?;

    Ok(response)
}

#[derive(Debug)]
pub struct Uri;

impl Module for Uri {
    fn get_name(&self) -> &str {
        "uri"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        let params: Params = parse_params(params)?;

        let response = make_request(&params)?;
        let status = response.status();
        let status_code = status.as_u16() as i32;

        // Check if status code is in the list of expected codes
        if !params.status_code.contains(&status_code) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Status code {} not in expected status codes: {:?}",
                    status_code, params.status_code
                ),
            ));
        }

        // Get response headers
        let mut response_headers = HashMap::new();
        for (key, value) in response.headers() {
            response_headers.insert(key.to_string(), value.to_str().unwrap_or("").to_string());
        }

        // Get response body if requested
        let content = if params.return_content {
            Some(response.text().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to read response body: {}", e),
                )
            })?)
        } else {
            None
        };

        let mut extra_data = json!({
            "status": status_code,
            "url": params.url,
            "headers": response_headers,
        });

        if let Some(content) = &content {
            extra_data["content"] = serde_json::Value::String(content.clone());

            // Try to parse JSON content
            if let Ok(json_content) = serde_json::from_str::<serde_json::Value>(content) {
                extra_data["json"] = json_content;
            }
        }

        let extra = Some(value::to_value(extra_data)?);

        let output = if params.return_content {
            content
        } else {
            Some(format!(
                "HTTP {} {}",
                status_code,
                status.canonical_reason().unwrap_or("Unknown")
            ))
        };

        Ok((
            ModuleResult {
                changed: false, // URI requests don't typically change the system state
                output,
                extra,
            },
            vars,
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
    use serde_yaml::from_str;

    #[test]
    fn test_parse_params_simple() {
        let yaml = r#"
url: "http://example.com"
method: "GET"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url, "http://example.com");
        assert_eq!(params.method, "GET");
        assert_eq!(params.status_code, vec![200]);
        assert_eq!(params.timeout, 30);
        assert!(!params.return_content);
        assert!(params.validate_certs);
    }

    #[test]
    fn test_parse_params_with_headers() {
        let yaml = r#"
url: "http://example.com"
method: "POST"
headers:
  Content-Type: "application/json"
  Authorization: "Bearer token123"
body: '{"test": "data"}'
status_code: [200, 201]
return_content: true
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url, "http://example.com");
        assert_eq!(params.method, "POST");
        assert_eq!(params.status_code, vec![200, 201]);
        assert!(params.return_content);

        let headers = params.headers.unwrap();
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer token123");

        assert_eq!(params.body.unwrap(), r#"{"test": "data"}"#);
    }

    #[test]
    fn test_parse_params_with_auth() {
        let yaml = r#"
url: "http://example.com"
url_username: "testuser"
url_password: "testpass"
force_basic_auth: true
validate_certs: false
timeout: 60
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url_username.unwrap(), "testuser");
        assert_eq!(params.url_password.unwrap(), "testpass");
        assert!(params.force_basic_auth);
        assert!(!params.validate_certs);
        assert_eq!(params.timeout, 60);
    }
}
