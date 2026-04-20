/// ANCHOR: module
/// # docker_stack
///
/// Manage Docker Swarm stacks using compose files for declarative service deployment.
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
/// - name: Deploy a stack
///   docker_stack:
///     name: myapp
///     compose: /opt/myapp/docker-compose.yml
///     state: present
///
/// - name: Deploy with prune and registry auth
///   docker_stack:
///     name: myapp
///     compose: /opt/myapp/docker-compose.yml
///     state: present
///     prune: true
///     with_registry_auth: true
///
/// - name: Deploy with specific image resolution
///   docker_stack:
///     name: myapp
///     compose: /opt/myapp/docker-compose.yml
///     state: present
///     resolve_image: always
///
/// - name: Remove a stack
///   docker_stack:
///     name: oldapp
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
use std::path::Path;
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
    /// Stack name.
    name: String,
    /// Path to the compose file (required for state=present).
    #[serde(default)]
    compose: Option<String>,
    /// Desired state of the stack.
    #[serde(default)]
    state: State,
    /// Prune services that are no longer referenced.
    #[serde(default)]
    prune: bool,
    /// Image resolution mode (always, changed, never).
    #[serde(default)]
    resolve_image: Option<String>,
    /// Send registry authentication details to Swarm agents.
    #[serde(default)]
    with_registry_auth: bool,
}

#[derive(Debug)]
pub struct DockerStack;

struct DockerStackClient {
    check_mode: bool,
}

impl Module for DockerStack {
    fn get_name(&self) -> &str {
        "docker_stack"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_stack(parse_params(optional_params)?, check_mode)?,
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

impl DockerStackClient {
    fn new(check_mode: bool) -> Self {
        DockerStackClient { check_mode }
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
                    "Error executing docker stack: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn stack_exists(&self, name: &str) -> Result<bool> {
        let args = ["stack", "ls", "--format", "{{.Name}}"];
        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    fn get_stack_services(&self, name: &str) -> Result<Vec<serde_json::Value>> {
        let args = ["stack", "services", "--format", "json", name];
        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() || output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let services: Vec<serde_json::Value> = stdout
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        Ok(services)
    }

    fn deploy(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let compose = params.compose.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "compose is required when state=present",
            )
        })?;

        let mut args = vec!["stack", "deploy"];

        args.push("-c");
        args.push(compose);

        if params.prune {
            args.push("--prune");
        }

        if let Some(ref resolve_image) = params.resolve_image {
            args.push("--resolve-image");
            args.push(resolve_image);
        }

        if params.with_registry_auth {
            args.push("--with-registry-auth");
        }

        args.push(&params.name);

        let output = self.exec_cmd(&args, true)?;
        Ok(output.status.success())
    }

    fn remove(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let args = ["stack", "rm", name];
        let output = self.exec_cmd(&args, true)?;
        Ok(output.status.success())
    }

    fn get_stack_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        let exists = self.stack_exists(name)?;
        result.insert("exists".to_string(), serde_json::Value::Bool(exists));

        if exists {
            let services = self.get_stack_services(name)?;
            let services_map: serde_json::Map<String, serde_json::Value> = services
                .iter()
                .filter_map(|svc| {
                    let svc_name = svc.get("Name").and_then(|n| n.as_str()).unwrap_or("");
                    if svc_name.is_empty() {
                        return None;
                    }
                    Some((svc_name.to_string(), svc.clone()))
                })
                .collect();

            result.insert(
                "services".to_string(),
                serde_json::Value::Object(services_map),
            );
        }

        Ok(result)
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }
    Ok(())
}

fn validate_compose_for_present(params: &Params) -> Result<()> {
    if params.state == State::Present && params.compose.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "compose is required when state=present",
        ));
    }

    if let Some(ref compose) = params.compose
        && !Path::new(compose).exists()
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("compose file '{}' does not exist", compose),
        ));
    }

    Ok(())
}

fn validate_resolve_image(resolve_image: &Option<String>) -> Result<()> {
    if let Some(mode) = resolve_image {
        match mode.as_str() {
            "always" | "changed" | "never" => {}
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "resolve_image must be one of: always, changed, never (got '{}')",
                        mode
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn docker_stack(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_name(&params.name)?;
    validate_compose_for_present(&params)?;
    validate_resolve_image(&params.resolve_image)?;

    let client = DockerStackClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            trace!("state: Present");
            let exists = client.stack_exists(&params.name)?;

            if check_mode {
                if !exists {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push(format!("Stack '{}' would be deployed", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!(
                        "Stack '{}' already exists (would be updated)",
                        params.name
                    ));
                    changed = true;
                }
            } else {
                let stdout_before = client.get_stack_services(&params.name)?;
                client.deploy(&params)?;
                let stdout_after = client.get_stack_services(&params.name)?;

                if !exists {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push(format!("Stack '{}' deployed", params.name));
                    changed = true;
                } else if stdout_before.len() != stdout_after.len() {
                    diff(
                        format!("services: {}", stdout_before.len()),
                        format!("services: {}", stdout_after.len()),
                    );
                    output_messages.push(format!("Stack '{}' updated", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!("Stack '{}' up to date", params.name));
                }
            }
        }
        State::Absent => {
            trace!("state: Absent");
            let exists = client.stack_exists(&params.name)?;

            if exists {
                if check_mode {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Stack '{}' would be removed", params.name));
                    changed = true;
                } else {
                    client.remove(&params.name)?;
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Stack '{}' removed", params.name));
                    changed = true;
                }
            } else {
                output_messages.push(format!("Stack '{}' already absent", params.name));
            }
        }
    }

    let extra = client.get_stack_state(&params.name)?;

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
            name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Present);
        assert!(params.compose.is_none());
        assert!(!params.prune);
        assert!(params.resolve_image.is_none());
        assert!(!params.with_registry_auth);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            compose: /opt/myapp/docker-compose.yml
            state: present
            prune: true
            resolve_image: always
            with_registry_auth: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(
            params.compose,
            Some("/opt/myapp/docker-compose.yml".to_string())
        );
        assert_eq!(params.state, State::Present);
        assert!(params.prune);
        assert_eq!(params.resolve_image, Some("always".to_string()));
        assert!(params.with_registry_auth);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oldapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "oldapp");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_name_empty() {
        let error = validate_name("").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_name_valid() {
        let result = validate_name("myapp");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_compose_for_present_missing() {
        let params = Params {
            name: "myapp".to_string(),
            compose: None,
            state: State::Present,
            prune: false,
            resolve_image: None,
            with_registry_auth: false,
        };
        let error = validate_compose_for_present(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_compose_for_present_absent_ok() {
        let params = Params {
            name: "myapp".to_string(),
            compose: None,
            state: State::Absent,
            prune: false,
            resolve_image: None,
            with_registry_auth: false,
        };
        let result = validate_compose_for_present(&params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_resolve_image_valid() {
        for mode in &["always", "changed", "never"] {
            let result = validate_resolve_image(&Some(mode.to_string()));
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_validate_resolve_image_invalid() {
        let error = validate_resolve_image(&Some("invalid".to_string())).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_resolve_image_none() {
        let result = validate_resolve_image(&None);
        assert!(result.is_ok());
    }
}
