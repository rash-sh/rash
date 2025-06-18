/// ANCHOR: lookup
/// # vault
///
/// Retrieve secrets from HashiCorp's Vault.
///
/// ## Parameters
///
/// | Parameter      | Required | Type    | Values                              | Description                                                                         |
/// | -------------- | -------- | ------- | ----------------------------------- | ----------------------------------------------------------------------------------- |
/// | secret         | yes      | string  |                                     | Vault path to the secret being requested in the format `path[:field]`             |
/// | url            | no       | string  |                                     | URL to the Vault service. **[default: `VAULT_ADDR` env var]**                     |
/// | token          | no       | string  |                                     | Vault token. **[default: `VAULT_TOKEN` env var]**                                 |
/// | mount          | no       | string  |                                     | Vault mount point for the secret engine. **[default: `secret`]**                  |
/// | auth_method    | no       | string  | token, userpass, approle, jwt, none | Authentication method. **[default: `token`]**                                      |
/// | username       | no       | string  |                                     | Username for userpass authentication                                               |
/// | password       | no       | string  |                                     | Password for userpass authentication                                               |
/// | role_id        | no       | string  |                                     | Role ID for approle authentication                                                 |
/// | secret_id      | no       | string  |                                     | Secret ID for approle authentication                                               |
/// | jwt            | no       | string  |                                     | JWT token for jwt authentication                                                   |
/// | namespace      | no       | string  |                                     | Vault namespace (Enterprise feature)                                               |
/// | validate_certs | no       | boolean |                                     | Validate SSL certificates. **[default: `true`]**                                  |
/// | timeout        | no       | integer |                                     | Request timeout in seconds                                                         |
/// | return_format  | no       | string  | dict, values, raw                   | How to return multiple key/value pairs. **[default: `dict`]**                     |
/// | token_validate | no       | boolean |                                     | Validate token before use. **[default: `false`]**                                 |
///
/// ## Notes
///
/// - The secret path format is `path[:field]`. If no field is specified, returns all secret data as a dict.
/// - For KV v2, the path should include `data` between the mount and path (e.g., `secret/data/myapp`).
/// - Environment variables `VAULT_ADDR` and `VAULT_TOKEN` are used if URL and token are not provided.
/// - Supports multiple authentication methods: token, userpass, approle, jwt, and none.
/// - The `return_format` parameter controls how secrets are returned:
///   - `dict`: Returns key/value pairs as a dictionary (default)
///   - `values`: Returns only the values as a list
///   - `raw`: Returns the complete API response including metadata
///
/// ANCHOR_END: lookup
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// # Basic token authentication
/// - name: Get specific field from secret
///   debug:
///     msg: "Password: {{ vault('myapp/database:password') }}"
///
/// - name: Get all fields from secret as dict
///   debug:
///     msg: "Config: {{ vault('myapp/config') }}"
///
/// # Username/password authentication
/// - name: Userpass auth
///   debug:
///     msg: "Secret: {{ vault('myapp/secret:value', auth_method='userpass', username='myuser', password='mypass') }}"
///
/// # AppRole authentication
/// - name: AppRole auth
///   debug:
///     msg: "API Key: {{ vault('api/keys:token', auth_method='approle', role_id='role123', secret_id='secret456') }}"
///
/// # JWT authentication
/// - name: JWT auth
///   debug:
///     msg: "Data: {{ vault('myapp/data', auth_method='jwt', jwt='eyJ...', role_id='myrole') }}"
///
/// # Return formats
/// - name: Get only values as list
///   debug:
///     msg: "Values: {{ vault('myapp/config', return_format='values') }}"
///
/// - name: Get raw API response
///   debug:
///     msg: "Raw: {{ vault('myapp/config', return_format='raw') }}"
///
/// # Vault Enterprise namespace
/// - name: Use namespace
///   debug:
///     msg: "Secret: {{ vault('myapp/secret:value', namespace='team-a') }}"
///
/// - name: Use custom vault server
///   debug:
///     msg: "API Key: {{ vault('api/keys:token', url='https://vault.company.com', token='hvs.xxx') }}"
///
/// - name: KV v2 path example
///   debug:
///     msg: "Secret: {{ vault('secret/data/myapp:password') }}"
/// ```
/// ANCHOR_END: examples
use std::collections::HashMap;
use std::env;
use std::result::Result as StdResult;

use minijinja::{Error as MinijinjaError, ErrorKind as MinijinjaErrorKind, Value, value::Kwargs};
use reqwest;
use vaultrs::client::VaultClientSettingsBuilder;

pub fn function(secret: String, options: Kwargs) -> StdResult<Value, MinijinjaError> {
    // Parse secret path and field
    let (path, field) = parse_secret_path(&secret)?;

    // Get configuration parameters
    let url: Option<String> = options.get("url")?;
    let token: Option<String> = options.get("token")?;
    let mount: Option<String> = options.get("mount")?;
    let auth_method: Option<String> = options.get("auth_method")?;
    let username: Option<String> = options.get("username")?;
    let password: Option<String> = options.get("password")?;
    let role_id: Option<String> = options.get("role_id")?;
    let secret_id: Option<String> = options.get("secret_id")?;
    let jwt: Option<String> = options.get("jwt")?;
    let namespace: Option<String> = options.get("namespace")?;
    let return_format: Option<String> = options.get("return_format")?;
    // TODO: Implement these parameters
    // let validate_certs: Option<bool> = options.get("validate_certs")?;
    // let timeout: Option<u64> = options.get("timeout")?;
    // let token_validate: Option<bool> = options.get("token_validate")?;

    // Use environment variables as defaults
    let vault_url = url.or_else(|| env::var("VAULT_ADDR").ok()).ok_or_else(|| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            "Vault URL not provided. Set 'url' parameter or VAULT_ADDR environment variable.",
        )
    })?;

    let mount_point = mount.unwrap_or_else(|| "secret".to_string());
    let auth_method_str = auth_method.unwrap_or_else(|| "token".to_string());
    let return_format_str = return_format.unwrap_or_else(|| "dict".to_string());

    // Create async runtime for vault operations
    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to create async runtime: {}", e),
        )
    })?;

    // Execute vault operation
    let result = rt.block_on(async {
        // Get or obtain vault token based on auth method
        let vault_token = match auth_method_str.as_str() {
            "token" => {
                // Token authentication
                token
                    .or_else(|| env::var("VAULT_TOKEN").ok())
                    .ok_or_else(|| MinijinjaError::new(
                        MinijinjaErrorKind::InvalidOperation,
                        "Vault token not provided. Set 'token' parameter or VAULT_TOKEN environment variable.",
                    ))?
            },
            "userpass" => {
                // Username/password authentication
                let user = username.ok_or_else(|| MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    "Username required for userpass authentication",
                ))?;
                let pass = password.ok_or_else(|| MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    "Password required for userpass authentication",
                ))?;
                authenticate_userpass(&vault_url, &user, &pass).await?
            },
            "approle" => {
                // AppRole authentication
                let role = role_id.ok_or_else(|| MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    "role_id required for approle authentication",
                ))?;
                let secret = secret_id.ok_or_else(|| MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    "secret_id required for approle authentication",
                ))?;
                authenticate_approle(&vault_url, &role, &secret).await?
            },
            "jwt" => {
                // JWT authentication
                let jwt_token = jwt.ok_or_else(|| MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    "jwt required for jwt authentication",
                ))?;
                let role = role_id.unwrap_or_else(|| "default".to_string());
                authenticate_jwt(&vault_url, &jwt_token, &role).await?
            },
            "none" => {
                // No authentication - useful for Vault agent setups
                "".to_string()
            },
            _ => {
                return Err(MinijinjaError::new(
                    MinijinjaErrorKind::InvalidOperation,
                    format!("Unsupported auth method: {}", auth_method_str),
                ));
            }
        };

        fetch_secret(
            &vault_url,
            &vault_token,
            &mount_point,
            &path,
            field.as_deref(),
            &namespace,
            &return_format_str,
        )
        .await
    })?;

    options.assert_all_used()?;
    Ok(result)
}

fn parse_secret_path(secret: &str) -> StdResult<(String, Option<String>), MinijinjaError> {
    if let Some(colon_pos) = secret.find(':') {
        let path = secret[..colon_pos].to_string();
        let field = secret[colon_pos + 1..].to_string();
        Ok((path, Some(field)))
    } else {
        Ok((secret.to_string(), None))
    }
}

// Authentication functions
async fn authenticate_userpass(
    url: &str,
    username: &str,
    password: &str,
) -> StdResult<String, MinijinjaError> {
    let client = reqwest::Client::new();
    let auth_url = format!(
        "{}/v1/auth/userpass/login/{}",
        url.trim_end_matches('/'),
        username
    );

    let mut payload = HashMap::new();
    payload.insert("password", password);

    let response = client
        .post(&auth_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Userpass authentication failed: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Userpass authentication failed: {}", error_text),
        ));
    }

    let response_text = response.text().await.map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to read userpass response: {}", e),
        )
    })?;

    let json_response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to parse userpass response: {}", e),
        )
    })?;

    let token = json_response
        .get("auth")
        .and_then(|auth| auth.get("client_token"))
        .and_then(|token| token.as_str())
        .ok_or_else(|| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                "No token in userpass response".to_string(),
            )
        })?;

    Ok(token.to_string())
}

async fn authenticate_approle(
    url: &str,
    role_id: &str,
    secret_id: &str,
) -> StdResult<String, MinijinjaError> {
    let client = reqwest::Client::new();
    let auth_url = format!("{}/v1/auth/approle/login", url.trim_end_matches('/'));

    let mut payload = HashMap::new();
    payload.insert("role_id", role_id);
    payload.insert("secret_id", secret_id);

    let response = client
        .post(&auth_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("AppRole authentication failed: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("AppRole authentication failed: {}", error_text),
        ));
    }

    let response_text = response.text().await.map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to read approle response: {}", e),
        )
    })?;

    let json_response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to parse approle response: {}", e),
        )
    })?;

    let token = json_response
        .get("auth")
        .and_then(|auth| auth.get("client_token"))
        .and_then(|token| token.as_str())
        .ok_or_else(|| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                "No token in approle response".to_string(),
            )
        })?;

    Ok(token.to_string())
}

async fn authenticate_jwt(url: &str, jwt: &str, role: &str) -> StdResult<String, MinijinjaError> {
    let client = reqwest::Client::new();
    let auth_url = format!("{}/v1/auth/jwt/login", url.trim_end_matches('/'));

    let mut payload = HashMap::new();
    payload.insert("jwt", jwt);
    payload.insert("role", role);

    let response = client
        .post(&auth_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("JWT authentication failed: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("JWT authentication failed: {}", error_text),
        ));
    }

    let response_text = response.text().await.map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to read jwt response: {}", e),
        )
    })?;

    let json_response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to parse jwt response: {}", e),
        )
    })?;

    let token = json_response
        .get("auth")
        .and_then(|auth| auth.get("client_token"))
        .and_then(|token| token.as_str())
        .ok_or_else(|| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                "No token in jwt response".to_string(),
            )
        })?;

    Ok(token.to_string())
}

async fn fetch_secret(
    url: &str,
    token: &str,
    mount: &str,
    path: &str,
    field: Option<&str>,
    namespace: &Option<String>,
    return_format: &str,
) -> StdResult<Value, MinijinjaError> {
    // Create vault client
    let _client_settings = VaultClientSettingsBuilder::default()
        .address(url)
        .token(token)
        .build()
        .map_err(|e| {
            MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Failed to configure Vault client: {}", e),
            )
        })?;

    // Try making a direct HTTP request to Vault API
    let vault_path = format!("{}/data/{}", mount, path);
    let full_url = format!("{}/v1/{}", url.trim_end_matches('/'), vault_path);

    let http_client = reqwest::Client::new();
    let mut request = http_client.get(&full_url);

    // Add authentication token (if provided)
    if !token.is_empty() {
        request = request.header("X-Vault-Token", token);
    }

    // Add namespace header (Vault Enterprise feature)
    if let Some(ns) = namespace {
        if !ns.is_empty() {
            request = request.header("X-Vault-Namespace", ns);
        }
    }

    let response = request.send().await.map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("HTTP request failed: {}", e),
        )
    })?;

    let status = response.status();
    let response_text = response.text().await.map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to read response text: {}", e),
        )
    })?;

    if !status.is_success() {
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Vault returned status {}: {}", status, response_text),
        ));
    }

    // Parse the JSON response
    let json_response: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            format!("Failed to parse JSON response: {}", e),
        )
    })?;

    // Extract the data from the response
    let data = json_response.get("data").ok_or_else(|| {
        MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            "No 'data' field in Vault response".to_string(),
        )
    })?;

    // Debug: Check if data is null
    if data.is_null() {
        return Err(MinijinjaError::new(
            MinijinjaErrorKind::InvalidOperation,
            "Vault response data is null".to_string(),
        ));
    }

    // For KV v2, the actual secret data is nested under the "data" field
    let secret_data = if let Some(nested_data) = data.get("data") {
        nested_data // KV v2: secret is at response.data.data
    } else {
        data // KV v1 or other: secret is at response.data
    };

    // Return specific field or entire secret based on return_format
    if let Some(field_name) = field {
        if let Some(field_value) = secret_data.get(field_name) {
            // Convert JSON value to string
            let value_str = match field_value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Ok(Value::from(value_str))
        } else {
            Err(MinijinjaError::new(
                MinijinjaErrorKind::InvalidOperation,
                format!("Field '{}' not found in secret", field_name),
            ))
        }
    } else {
        // Return entire secret based on return_format
        match return_format {
            "dict" => {
                // Return as dictionary (default)
                convert_json_to_minijinja_value(secret_data)
            }
            "values" => {
                // Return only values as a list
                if let Some(obj) = secret_data.as_object() {
                    let values: Vec<Value> = obj
                        .values()
                        .map(|v| {
                            convert_json_to_minijinja_value(v)
                                .unwrap_or_else(|_| Value::from(v.to_string()))
                        })
                        .collect();
                    Ok(Value::from(values))
                } else {
                    convert_json_to_minijinja_value(secret_data)
                }
            }
            "raw" => {
                // Return raw API response including metadata
                convert_json_to_minijinja_value(&json_response)
            }
            _ => {
                // Default to dict if unknown format
                convert_json_to_minijinja_value(secret_data)
            }
        }
    }
}

fn convert_json_to_minijinja_value(
    json_value: &serde_json::Value,
) -> StdResult<Value, MinijinjaError> {
    match json_value {
        serde_json::Value::Null => Ok(Value::from(())),
        serde_json::Value::Bool(b) => Ok(Value::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::from(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::from(f))
            } else {
                Ok(Value::from(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(Value::from(s.clone())),
        serde_json::Value::Array(arr) => {
            let converted: Result<Vec<Value>, MinijinjaError> =
                arr.iter().map(convert_json_to_minijinja_value).collect();
            match converted {
                Ok(vec) => Ok(Value::from(vec)),
                Err(e) => Err(e),
            }
        }
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::BTreeMap::new();
            for (key, value) in obj.iter() {
                match convert_json_to_minijinja_value(value) {
                    Ok(v) => {
                        map.insert(key.clone(), v);
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(Value::from_serialize(&map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_secret_path_with_field() {
        let (path, field) = parse_secret_path("myapp/database:password").unwrap();
        assert_eq!(path, "myapp/database");
        assert_eq!(field, Some("password".to_string()));
    }

    #[test]
    fn test_parse_secret_path_without_field() {
        let (path, field) = parse_secret_path("myapp/config").unwrap();
        assert_eq!(path, "myapp/config");
        assert_eq!(field, None);
    }

    #[test]
    fn test_parse_secret_path_with_kv2() {
        let (path, field) = parse_secret_path("secret/data/myapp:password").unwrap();
        assert_eq!(path, "secret/data/myapp");
        assert_eq!(field, Some("password".to_string()));
    }

    #[test]
    fn test_parse_secret_path_empty() {
        let (path, field) = parse_secret_path("").unwrap();
        assert_eq!(path, "");
        assert_eq!(field, None);
    }

    #[test]
    fn test_parse_secret_path_multiple_colons() {
        let (path, field) = parse_secret_path("app:config:database:password").unwrap();
        assert_eq!(path, "app");
        assert_eq!(field, Some("config:database:password".to_string()));
    }
}
