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
/// ## Examples
///
/// ```yaml
/// - name: Create a secret from inline data
///   docker_secret:
///     name: db_password
///     data: "my_secret_password"
///     state: present
///
/// - name: Create a secret from file
///   docker_secret:
///     name: api_key
///     data_src: /path/to/api_key.txt
///     state: present
///
/// - name: Create a secret with labels
///   docker_secret:
///     name: tls_cert
///     data_src: /etc/ssl/cert.pem
///     labels:
///       environment: production
///       owner: team-ops
///     state: present
///
/// - name: Force update a secret (remove and recreate)
///   docker_secret:
///     name: db_password
///     data: "new_password"
///     force: true
///     state: present
///
/// - name: Remove a secret
///   docker_secret:
///     name: old_secret
///     state: absent
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
use std::io::Write;
use std::process::{Command, Output, Stdio};

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
    /// Secret data as inline string.
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

#[derive(Debug, Clone)]
struct SecretInfo {
    id: String,
    name: String,
    labels: serde_json::Map<String, serde_json::Value>,
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

fn build_label_args(labels: &serde_json::Map<String, serde_json::Value>, args: &mut Vec<String>) {
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

struct DockerClient {
    check_mode: bool,
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
            &["secret", "inspect", "--format", "{{json .}}", name],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        let spec = parsed
            .get("Spec")
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Missing Spec in secret inspect"))?;

        let id = parsed
            .get("ID")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let secret_name = spec
            .get("Name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let labels = spec
            .get("Labels")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        Ok(Some(SecretInfo {
            id,
            name: secret_name,
            labels: labels.into_iter().collect(),
        }))
    }

    fn create_secret(&self, params: &Params, secret_data: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["secret".to_string(), "create".to_string()];

        if let Some(ref labels) = params.labels {
            build_label_args(labels, &mut args);
        }

        args.push(params.name.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        let mut child = Command::new("docker")
            .args(&args_refs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let stdin = child.stdin.as_mut().ok_or_else(|| {
            Error::new(
                ErrorKind::SubprocessFail,
                "Failed to open stdin for docker secret create",
            )
        })?;
        stdin
            .write_all(secret_data.as_bytes())
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `docker {:?}` (with stdin data)", args_refs);
        trace!("{:?}", output);

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error creating docker secret: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(true)
    }

    fn create_secret_from_file(&self, params: &Params, file_path: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["secret".to_string(), "create".to_string()];

        if let Some(ref labels) = params.labels {
            build_label_args(labels, &mut args);
        }

        args.push(params.name.clone());
        args.push(file_path.to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_cmd(&args_refs, true)?;
        Ok(true)
    }

    fn remove_secret(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.secret_exists(name)? {
            return Ok(false);
        }

        self.exec_cmd(&["secret", "rm", name], true)?;
        Ok(true)
    }

    fn get_secret_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_secret_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            if !info.labels.is_empty() {
                result.insert("labels".to_string(), serde_json::Value::Object(info.labels));
            }
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

fn resolve_secret_data(params: &Params) -> Result<String> {
    match (&params.data, &params.data_src) {
        (Some(data), None) => Ok(data.clone()),
        (None, Some(path)) => fs::read_to_string(path).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read secret data from '{}': {}", path, e),
            )
        }),
        (Some(_), Some(_)) => Err(Error::new(
            ErrorKind::InvalidData,
            "Cannot specify both 'data' and 'data_src'",
        )),
        (None, None) => Err(Error::new(
            ErrorKind::InvalidData,
            "Either 'data' or 'data_src' is required when state is present",
        )),
    }
}

fn docker_secret(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_secret_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            let secret_data = resolve_secret_data(&params)?;
            let exists = client.secret_exists(&params.name)?;

            if exists {
                if params.force {
                    client.remove_secret(&params.name)?;
                    if let Some(ref data_src) = params.data_src {
                        client.create_secret_from_file(&params, data_src)?;
                    } else {
                        client.create_secret(&params, &secret_data)?;
                    }
                    diff(
                        format!("secret: {} (old)", params.name),
                        format!("secret: {} (updated)", params.name),
                    );
                    output_messages.push(format!("Secret '{}' force updated", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!(
                        "Secret '{}' already exists (use force=true to update)",
                        params.name
                    ));
                }
            } else {
                if let Some(ref data_src) = params.data_src {
                    client.create_secret_from_file(&params, data_src)?;
                } else {
                    client.create_secret(&params, &secret_data)?;
                }
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
            data: "secret_value"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_secret");
        assert_eq!(params.data, Some("secret_value".to_string()));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_with_data_src() {
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
            name: tls_cert
            data: "cert_data"
            labels:
              environment: production
              owner: team-ops
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
            labels.get("owner").unwrap(),
            &serde_json::Value::String("team-ops".to_string())
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
    fn test_parse_params_force_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_secret
            data: "new_value"
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "my_secret");
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_secret
            data: "value"
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
        assert!(validate_secret_name(&"a".repeat(65)).is_err());
        assert!(validate_secret_name("-mysecret").is_err());
        assert!(validate_secret_name(".mysecret").is_err());
        assert!(validate_secret_name("my secret").is_err());
        assert!(validate_secret_name("my/secret").is_err());
    }

    #[test]
    fn test_resolve_secret_data_inline() {
        let params = Params {
            name: "test".to_string(),
            data: Some("my_secret_value".to_string()),
            data_src: None,
            state: State::Present,
            labels: None,
            force: false,
        };
        assert_eq!(resolve_secret_data(&params).unwrap(), "my_secret_value");
    }

    #[test]
    fn test_resolve_secret_data_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("secret.txt");
        fs::write(&file_path, "file_secret_value").unwrap();

        let params = Params {
            name: "test".to_string(),
            data: None,
            data_src: Some(file_path.to_str().unwrap().to_string()),
            state: State::Present,
            labels: None,
            force: false,
        };
        assert_eq!(resolve_secret_data(&params).unwrap(), "file_secret_value");
    }

    #[test]
    fn test_resolve_secret_data_both() {
        let params = Params {
            name: "test".to_string(),
            data: Some("value".to_string()),
            data_src: Some("/path/to/file".to_string()),
            state: State::Present,
            labels: None,
            force: false,
        };
        assert!(resolve_secret_data(&params).is_err());
    }

    #[test]
    fn test_resolve_secret_data_neither() {
        let params = Params {
            name: "test".to_string(),
            data: None,
            data_src: None,
            state: State::Present,
            labels: None,
            force: false,
        };
        assert!(resolve_secret_data(&params).is_err());
    }

    #[test]
    fn test_resolve_secret_data_missing_file() {
        let params = Params {
            name: "test".to_string(),
            data: None,
            data_src: Some("/nonexistent/path/secret.txt".to_string()),
            state: State::Present,
            labels: None,
            force: false,
        };
        assert!(resolve_secret_data(&params).is_err());
    }
}
