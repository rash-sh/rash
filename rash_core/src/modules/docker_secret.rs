/// ANCHOR: module
/// # docker_secret
///
/// Manage Docker secrets for secure container orchestration.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Create a secret from inline data
///   docker_secret:
///     name: my_secret
///     data: "my_secret_value"
///
/// - name: Create a secret from file
///   docker_secret:
///     name: db_password
///     data_src: /path/to/password.txt
///
/// - name: Create a secret with labels
///   docker_secret:
///     name: api_key
///     data: "super_secret_api_key"
///     labels:
///       environment: production
///       service: api
///
/// - name: Update a secret (requires removal and recreation)
///   docker_secret:
///     name: my_secret
///     data: "new_secret_value"
///     force: true
///
/// - name: Remove a secret
///   docker_secret:
///     name: old_secret
///     state: absent
///
/// - name: Check if secret exists
///   docker_secret:
///     name: my_secret
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize, Clone, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Secret name (required).
    name: String,
    /// Secret data as a string.
    #[serde(default)]
    data: Option<String>,
    /// File path to read secret data from.
    #[serde(default)]
    data_src: Option<String>,
    /// Desired state of the secret.
    #[serde(default)]
    state: State,
    /// Key/value metadata labels.
    #[serde(default)]
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Force update by removing and recreating the secret.
    #[serde(default)]
    force: bool,
}

#[derive(Debug)]
pub struct DockerSecret;

struct DockerClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct SecretInfo {
    id: String,
    name: String,
    labels: serde_json::Map<String, serde_json::Value>,
    created_at: String,
    updated_at: String,
}

impl Module for DockerSecret {
    fn get_name(&self) -> &str {
        "docker_secret"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_secret(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

impl DockerClient {
    fn new(check_mode: bool) -> Self {
        DockerClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `docker {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing docker: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn secret_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(&["secret", "inspect", "--format", "{{.ID}}", name], false)?;
        Ok(output.status.success())
    }

    fn get_secret_info(&self, name: &str) -> Result<Option<SecretInfo>> {
        let output = self.exec_cmd(
            &[
                "secret",
                "inspect",
                "--format",
                "{{.ID}}|{{.Spec.Name}}|{{.Spec.Labels}}|{{.CreatedAt}}|{{.UpdatedAt}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 5 {
            let labels = parse_labels(parts[2]);
            Ok(Some(SecretInfo {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                labels,
                created_at: parts[3].to_string(),
                updated_at: parts[4].to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn create_secret(&self, params: &Params, data: &[u8]) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["secret".to_string(), "create".to_string()];

        if let Some(ref labels) = params.labels {
            for (key, value) in labels {
                let label_str = match value {
                    serde_json::Value::String(s) => format!("{}={}", key, s),
                    serde_json::Value::Number(n) => format!("{}={}", key, n),
                    serde_json::Value::Bool(b) => format!("{}={}", key, b),
                    _ => format!("{}={}", key, value),
                };
                args.push("--label".to_string());
                args.push(label_str);
            }
        }

        args.push(params.name.clone());
        args.push("-".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let mut child = Command::new("docker")
            .args(&args_refs)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `docker {:?}`", args_refs);

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(data).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write secret data: {}", e),
                )
            })?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error creating secret: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(true)
    }

    fn remove_secret(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["secret", "rm", name], false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no such secret") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error removing secret: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn get_secret_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_secret_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("labels".to_string(), serde_json::Value::Object(info.labels));
            result.insert(
                "created_at".to_string(),
                serde_json::Value::String(info.created_at),
            );
            result.insert(
                "updated_at".to_string(),
                serde_json::Value::String(info.updated_at),
            );
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn parse_labels(labels_str: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut labels = serde_json::Map::new();

    if labels_str == "map[]" || labels_str.is_empty() {
        return labels;
    }

    let inner = labels_str.trim_start_matches("map[").trim_end_matches("]");

    if inner.is_empty() {
        return labels;
    }

    for pair in inner.split_whitespace() {
        if let Some((key, value)) = pair.split_once(':') {
            labels.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    labels
}

fn validate_secret_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Secret name cannot be empty",
        ));
    }

    if name.len() > 64 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Secret name too long (max 64 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Secret name contains invalid characters (only [a-z0-9.-_] allowed, lowercase)",
        ));
    }

    if name.starts_with('-') || name.starts_with('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Secret name cannot start with '-' or '.'",
        ));
    }

    Ok(())
}

fn get_secret_data(params: &Params) -> Result<Vec<u8>> {
    if let Some(ref data_src) = params.data_src {
        let mut file = fs::File::open(data_src).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read secret data from file '{}': {}", data_src, e),
            )
        })?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read secret data from file '{}': {}", data_src, e),
            )
        })?;
        Ok(data)
    } else if let Some(ref data) = params.data {
        Ok(data.as_bytes().to_vec())
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            "Either 'data' or 'data_src' is required when state=present",
        ))
    }
}

fn docker_secret(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_secret_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            let exists = client.secret_exists(&params.name)?;

            if exists {
                if params.force {
                    client.remove_secret(&params.name)?;
                    let data = get_secret_data(&params)?;
                    client.create_secret(&params, &data)?;
                    diff(
                        format!("secret: {} (old)", params.name),
                        format!("secret: {} (updated)", params.name),
                    );
                    output_messages.push(format!("Secret '{}' updated", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!("Secret '{}' already exists", params.name));
                }
            } else {
                let data = get_secret_data(&params)?;
                client.create_secret(&params, &data)?;
                diff(
                    format!("secret: {} (absent)", params.name),
                    format!("secret: {} (present)", params.name),
                );
                output_messages.push(format!("Secret '{}' created", params.name));
                changed = true;
            }
        }
        State::Absent => {
            if client.remove_secret(&params.name)? {
                diff(
                    format!("secret: {} (present)", params.name),
                    format!("secret: {} (absent)", params.name),
                );
                output_messages.push(format!("Secret '{}' removed", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Secret '{}' not found", params.name));
            }
        }
    }

    let extra = client.get_secret_state(&params.name)?;

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output: final_output,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_secret
            data: secret_value
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_secret");
        assert_eq!(params.data, Some("secret_value".to_string()));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_from_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: db_password
            data_src: /path/to/password.txt
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "db_password");
        assert_eq!(params.data_src, Some("/path/to/password.txt".to_string()));
    }

    #[test]
    fn test_parse_params_with_labels() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: api_key
            data: my_api_key
            labels:
              environment: production
              service: api
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let labels = params.labels.unwrap();
        assert_eq!(
            labels.get("environment").unwrap(),
            &serde_json::Value::String("production".to_string())
        );
        assert_eq!(
            labels.get("service").unwrap(),
            &serde_json::Value::String("api".to_string())
        );
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_secret
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old_secret");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_secret
            data: new_value
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_secret
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_secret_name() {
        assert!(validate_secret_name("mysecret").is_ok());
        assert!(validate_secret_name("my-secret").is_ok());
        assert!(validate_secret_name("my_secret").is_ok());
        assert!(validate_secret_name("my.secret").is_ok());
        assert!(validate_secret_name("mysecret123").is_ok());

        assert!(validate_secret_name("").is_err());
        assert!(validate_secret_name("a".repeat(65).as_str()).is_err());
        assert!(validate_secret_name("-mysecret").is_err());
        assert!(validate_secret_name(".mysecret").is_err());
        assert!(validate_secret_name("MySecret").is_err());
        assert!(validate_secret_name("my secret").is_err());
        assert!(validate_secret_name("my/secret").is_err());
    }

    #[test]
    fn test_parse_labels_empty() {
        let labels = parse_labels("map[]");
        assert!(labels.is_empty());

        let labels = parse_labels("");
        assert!(labels.is_empty());
    }

    #[test]
    fn test_parse_labels_single() {
        let labels = parse_labels("map[env:prod]");
        assert_eq!(
            labels.get("env").unwrap(),
            &serde_json::Value::String("prod".to_string())
        );
    }

    #[test]
    fn test_parse_labels_multiple() {
        let labels = parse_labels("map[env:prod service:api]");
        assert_eq!(
            labels.get("env").unwrap(),
            &serde_json::Value::String("prod".to_string())
        );
        assert_eq!(
            labels.get("service").unwrap(),
            &serde_json::Value::String("api".to_string())
        );
    }

    #[test]
    fn test_get_secret_data_from_inline() {
        let params = Params {
            name: "test".to_string(),
            data: Some("my_secret_value".to_string()),
            data_src: None,
            state: State::Present,
            labels: None,
            force: false,
        };
        let data = get_secret_data(&params).unwrap();
        assert_eq!(data, b"my_secret_value");
    }

    #[test]
    fn test_get_secret_data_no_data() {
        let params = Params {
            name: "test".to_string(),
            data: None,
            data_src: None,
            state: State::Present,
            labels: None,
            force: false,
        };
        let result = get_secret_data(&params);
        assert!(result.is_err());
    }
}
