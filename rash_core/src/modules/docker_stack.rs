/// ANCHOR: module
/// # docker_stack
///
/// Deploy and manage Docker Swarm stacks using compose files.
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
/// - name: Deploy a stack with prune
///   docker_stack:
///     name: myapp
///     compose: /opt/myapp/docker-compose.yml
///     state: present
///     prune: true
///
/// - name: Remove a stack
///   docker_stack:
///     name: myapp
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Present,
}

fn default_state() -> State {
    State::Present
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Stack name.
    name: String,
    /// Path to docker-compose.yml file.
    #[serde(default)]
    compose: Option<String>,
    /// Desired state of the stack.
    #[serde(default = "default_state")]
    state: State,
    /// Remove services not defined in compose.
    #[serde(default)]
    prune: bool,
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
        let args = vec!["stack", "ls", "--format", "{{.Name}}"];
        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    fn deploy(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let compose = params.compose.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "compose is required for state=present",
            )
        })?;

        if !Path::new(compose).exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("compose file '{}' does not exist", compose),
            ));
        }

        let mut args = vec!["stack", "deploy"];
        args.push("-c");
        args.push(compose);

        if params.prune {
            args.push("--prune");
        }

        args.push(&params.name);

        let output = self.exec_cmd(&args, true)?;
        Ok(output.status.success())
    }

    fn remove(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let args = vec!["stack", "rm", name];
        let output = self.exec_cmd(&args, true)?;
        Ok(output.status.success())
    }

    fn get_stack_services(&self, name: &str) -> Result<Vec<serde_json::Value>> {
        let args = vec!["stack", "services", name, "--format", "json"];
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
}

fn validate_params(params: &Params) -> Result<()> {
    if params.name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    if matches!(params.state, State::Present) && params.compose.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "compose is required when state=present",
        ));
    }

    Ok(())
}

fn docker_stack(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = DockerStackClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let exists = client.stack_exists(&params.name)?;
    trace!("stack '{}' exists: {}", params.name, exists);

    match params.state {
        State::Present => {
            if check_mode {
                if !exists {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push(format!("Stack '{}' would be deployed", params.name));
                    changed = true;
                } else {
                    output_messages.push(format!("Stack '{}' would be updated", params.name));
                    changed = true;
                }
            } else if client.deploy(&params)? {
                if !exists {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push(format!("Stack '{}' deployed", params.name));
                } else {
                    output_messages.push(format!("Stack '{}' updated", params.name));
                }
                changed = true;
            }
        }
        State::Absent => {
            if exists {
                if check_mode {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Stack '{}' would be removed", params.name));
                    changed = true;
                } else if client.remove(&params.name)? {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Stack '{}' removed", params.name));
                    changed = true;
                }
            } else {
                output_messages.push(format!("Stack '{}' already absent", params.name));
            }
        }
    }

    let mut extra = serde_json::Map::new();
    extra.insert(
        "stack".to_string(),
        serde_json::Value::String(params.name.clone()),
    );
    extra.insert(
        "exists".to_string(),
        serde_json::Value::Bool(client.stack_exists(&params.name)?),
    );

    if !check_mode {
        let services = client.get_stack_services(&params.name)?;
        if !services.is_empty() {
            extra.insert("services".to_string(), serde_json::Value::Array(services));
        }
    }

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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            compose: /opt/myapp/docker-compose.yml
            state: present
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
        assert!(!params.prune);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            compose: /opt/myapp/docker-compose.yml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_with_prune() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            compose: /opt/myapp/docker-compose.yml
            prune: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.prune);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            compose: /opt/myapp/docker-compose.yml
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_empty_name() {
        let params = Params {
            name: "".to_string(),
            compose: Some("/opt/compose.yml".to_string()),
            state: State::Present,
            prune: false,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_present_no_compose() {
        let params = Params {
            name: "myapp".to_string(),
            compose: None,
            state: State::Present,
            prune: false,
        };
        let error = validate_params(&params).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_absent_no_compose_ok() {
        let params = Params {
            name: "myapp".to_string(),
            compose: None,
            state: State::Absent,
            prune: false,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_valid() {
        let params = Params {
            name: "myapp".to_string(),
            compose: Some("/opt/compose.yml".to_string()),
            state: State::Present,
            prune: false,
        };
        assert!(validate_params(&params).is_ok());
    }
}
