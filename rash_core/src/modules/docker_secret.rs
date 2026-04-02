/// ANCHOR: module
/// # docker_secret
///
/// Manage Docker secrets in Swarm mode.
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
/// - name: Create database password secret
///   docker_secret:
///     name: db_password
///     data: "{{ lookup('env', 'DB_PASSWORD') }}"
///     state: present
///
/// - name: Create secret from file
///   docker_secret:
///     name: tls_cert
///     data: "{{ lookup('file', '/etc/certs/tls.pem') }}"
///     data_is_b64: true
///     state: present
///
/// - name: Create secret with labels
///   docker_secret:
///     name: api_key
///     data: "my-secret-key"
///     labels:
///       environment: production
///       service: api
///     state: present
///
/// - name: Remove secret
///   docker_secret:
///     name: db_password
///     state: absent
///
/// - name: Force recreate secret
///   docker_secret:
///     name: api_token
///     data: "new-token-value"
///     state: present
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
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
    name: String,
    #[serde(default)]
    state: State,
    data: Option<String>,
    #[serde(default)]
    data_is_b64: bool,
    #[serde(default)]
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    force: bool,
}

#[derive(Debug)]
pub struct DockerSecret;

#[derive(Debug, Clone)]
struct SecretInfo {
    id: String,
    name: String,
    labels: serde_json::Map<String, serde_json::Value>,
}

struct DockerClient {
    check_mode: bool,
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
        let output = self.exec_cmd(
            &[
                "secret",
                "ls",
                "--filter",
                &format!("name={}", name),
                "--format",
                "{{.Name}}",
            ],
            false,
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    fn get_secret_info(&self, name: &str) -> Result<Option<SecretInfo>> {
        let output = self.exec_cmd(
            &[
                "secret",
                "inspect",
                "--format",
                "{{.ID}}|{{.Spec.Name}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 2 {
            let labels_output = self.exec_cmd(
                &[
                    "secret",
                    "inspect",
                    "--format",
                    "{{json .Spec.Labels}}",
                    name,
                ],
                false,
            )?;

            let labels: serde_json::Map<String, serde_json::Value> =
                if labels_output.status.success() {
                    let labels_str = String::from_utf8_lossy(&labels_output.stdout);
                    serde_json::from_str(labels_str.trim()).unwrap_or_default()
                } else {
                    serde_json::Map::new()
                };

            Ok(Some(SecretInfo {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                labels,
            }))
        } else {
            Ok(None)
        }
    }

    fn create_secret(&self, params: &Params) -> Result<String> {
        if self.check_mode {
            return Ok("check-mode-id".to_string());
        }

        let data = params.data.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "data is required when state=present",
            )
        })?;

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

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let secret_data = if params.data_is_b64 {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            STANDARD.decode(data).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid base64 data: {}", e),
                )
            })?
        } else {
            data.as_bytes().to_vec()
        };

        let output = Command::new("docker")
            .args(&args_refs)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let mut child = output;
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&secret_data)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }

        let result = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `docker {:?}`", args_refs);
        trace!("{result:?}");

        if !result.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error creating secret: {}",
                    String::from_utf8_lossy(&result.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&result.stdout);
        Ok(stdout.trim().to_string())
    }

    fn remove_secret(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.secret_exists(name)? {
            return Ok(false);
        }

        let output = self.exec_cmd(&["secret", "rm", name], true)?;
        Ok(output.status.success())
    }

    fn get_secret_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_secret_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("labels".to_string(), serde_json::Value::Object(info.labels));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
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
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Secret name contains invalid characters (only [a-zA-Z0-9.-_] allowed)",
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

fn docker_secret(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_secret_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            let data = params.data.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "data is required when state=present",
                )
            })?;

            if data.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Secret data cannot be empty",
                ));
            }

            let exists = client.secret_exists(&params.name)?;

            if exists && !params.force {
                output_messages.push(format!("Secret '{}' already exists", params.name));
            } else {
                if exists && params.force {
                    client.remove_secret(&params.name)?;
                    diff(
                        format!("secret: {} (present)", params.name),
                        format!("secret: {} (removed for recreation)", params.name),
                    );
                }
                let id = client.create_secret(&params)?;
                diff(
                    format!("secret: {} (absent)", params.name),
                    format!("secret: {} (present)", params.name),
                );
                output_messages.push(format!("Secret '{}' created with ID: {}", params.name, id));
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
            name: db_password
            data: mysecret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "db_password");
        assert_eq!(params.data, Some("mysecret".to_string()));
        assert_eq!(params.state, State::Present);
        assert!(!params.data_is_b64);
    }

    #[test]
    fn test_parse_params_with_labels() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: api_key
            data: secret-value
            labels:
              environment: production
              service: api
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "api_key");
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
    fn test_parse_params_base64() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: tls_cert
            data: c2VjcmV0LWRhdGE=
            data_is_b64: true
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "tls_cert");
        assert_eq!(params.data, Some("c2VjcmV0LWRhdGE=".to_string()));
        assert!(params.data_is_b64);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: db_password
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "db_password");
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.data, None);
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: api_token
            data: new-token
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
            name: mysecret
            data: value
            invalid_field: test
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_secret_name() {
        assert!(validate_secret_name("db_password").is_ok());
        assert!(validate_secret_name("db-password").is_ok());
        assert!(validate_secret_name("db_password_123").is_ok());
        assert!(validate_secret_name("db.password").is_ok());
        assert!(validate_secret_name("DB_SECRET").is_ok());

        assert!(validate_secret_name("").is_err());
        assert!(validate_secret_name(&"a".repeat(65)).is_err());
        assert!(validate_secret_name("-secret").is_err());
        assert!(validate_secret_name(".secret").is_err());
        assert!(validate_secret_name("my secret").is_err());
        assert!(validate_secret_name("my/secret").is_err());
    }
}
