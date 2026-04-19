/// ANCHOR: module
/// # docker_exec
///
/// Execute commands inside running Docker containers.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Run a simple command in a container
///   docker_exec:
///     container: myapp
///     command: ls /app
///
/// - name: Run script in container as specific user
///   docker_exec:
///     container: myapp
///     command: /scripts/update.sh
///     user: appuser
///     workdir: /app
///
/// - name: Run command with environment variables
///   docker_exec:
///     container: webapp
///     command: env
///     env:
///       DEBUG: "true"
///       LOG_LEVEL: info
///
/// - name: Run interactive command with TTY
///   docker_exec:
///     container: myapp
///     command: /bin/bash
///     tty: true
///     stdin: true
///
/// - name: Run command in background
///   docker_exec:
///     container: myapp
///     command: /scripts/background_task.sh
///     detach: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Container name or ID.
    container: String,
    /// Command to execute inside the container.
    command: String,
    /// User to run the command as (e.g., "user", "uid", "user:group", "uid:gid").
    #[serde(default)]
    user: Option<String>,
    /// Working directory inside the container.
    #[serde(default)]
    workdir: Option<String>,
    /// Environment variables as a dictionary.
    #[serde(default)]
    env: Option<serde_json::Map<String, serde_json::Value>>,
    /// Run command in the background (detached mode).
    #[serde(default)]
    detach: bool,
    /// Allocate a pseudo-TTY.
    #[serde(default)]
    tty: bool,
    /// Keep STDIN open even if not attached.
    #[serde(default)]
    stdin: bool,
}

#[derive(Debug)]
pub struct DockerExec;

impl Module for DockerExec {
    fn get_name(&self) -> &str {
        "docker_exec"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_exec(parse_params(optional_params)?, check_mode)?,
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

struct DockerClient;

impl DockerClient {
    fn new() -> Self {
        DockerClient
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

    fn container_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            &[
                "ps",
                "-a",
                "--filter",
                &format!("name=^{}$", name),
                "--format",
                "{{.Names}}",
            ],
            false,
        )?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }
}

fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name cannot be empty",
        ));
    }
    Ok(())
}

fn docker_exec(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_container_name(&params.container)?;

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would execute '{}' in container '{}'",
                params.command, params.container
            )),
            extra: None,
        });
    }

    let client = DockerClient::new();

    if !client.container_exists(&params.container)? {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Container '{}' does not exist", params.container),
        ));
    }

    let mut args: Vec<String> = vec!["exec".to_string()];

    if params.detach {
        args.push("-d".to_string());
    }

    if params.tty {
        args.push("-t".to_string());
    }

    if params.stdin {
        args.push("-i".to_string());
    }

    if let Some(ref user) = params.user {
        args.push("-u".to_string());
        args.push(user.clone());
    }

    if let Some(ref workdir) = params.workdir {
        args.push("-w".to_string());
        args.push(workdir.clone());
    }

    if let Some(ref env_dict) = params.env {
        for (key, value) in env_dict {
            let env_str = match value {
                serde_json::Value::String(s) => format!("{}={}", key, s),
                serde_json::Value::Number(n) => format!("{}={}", key, n),
                serde_json::Value::Bool(b) => format!("{}={}", key, b),
                _ => format!("{}={}", key, value),
            };
            args.push("-e".to_string());
            args.push(env_str);
        }
    }

    args.push(params.container.clone());

    args.push("/bin/sh".to_string());
    args.push("-c".to_string());
    args.push(params.command.clone());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = client.exec_cmd(&args_refs, false)?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let rc = output.status.code();

    let extra = Some(value::to_value(serde_json::json!({
        "stdout": stdout.to_string(),
        "stderr": stderr.to_string(),
        "rc": rc,
        "container": params.container,
    }))?);

    let output_str = if stdout.is_empty() && !stderr.is_empty() {
        Some(stderr.into_owned())
    } else {
        Some(stdout.into_owned())
    };

    Ok(ModuleResult {
        changed: true,
        output: output_str,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            command: ls /app
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.container, "myapp");
        assert_eq!(params.command, "ls /app");
        assert_eq!(params.user, None);
        assert_eq!(params.workdir, None);
        assert_eq!(params.env, None);
        assert!(!params.detach);
        assert!(!params.tty);
        assert!(!params.stdin);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            command: /scripts/update.sh
            user: appuser
            workdir: /app
            env:
              DEBUG: "true"
              LOG_LEVEL: info
            detach: true
            tty: true
            stdin: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.container, "myapp");
        assert_eq!(params.command, "/scripts/update.sh");
        assert_eq!(params.user, Some("appuser".to_string()));
        assert_eq!(params.workdir, Some("/app".to_string()));
        assert!(params.detach);
        assert!(params.tty);
        assert!(params.stdin);

        let env = params.env.unwrap();
        assert_eq!(
            env.get("DEBUG").unwrap(),
            &serde_json::Value::String("true".to_string())
        );
        assert_eq!(
            env.get("LOG_LEVEL").unwrap(),
            &serde_json::Value::String("info".to_string())
        );
    }

    #[test]
    fn test_parse_params_env_with_numbers() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            command: env
            env:
              PORT: 8080
              ENABLED: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let env = params.env.unwrap();
        assert_eq!(env.get("PORT").unwrap(), &serde_json::json!(8080));
        assert_eq!(env.get("ENABLED").unwrap(), &serde_json::Value::Bool(true));
    }

    #[test]
    fn test_parse_params_missing_container() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command: ls
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            command: ls
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_container_name() {
        assert!(validate_container_name("myapp").is_ok());
        assert!(validate_container_name("my-app").is_ok());
        assert!(validate_container_name("my_app").is_ok());
        assert!(validate_container_name("my.app").is_ok());
        assert!(validate_container_name("myapp123").is_ok());
        assert!(validate_container_name("MyApp").is_ok());
        assert!(validate_container_name("a1b2c3d4e5f6").is_ok());
        assert!(validate_container_name("").is_err());
    }

    #[test]
    fn test_check_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            container: myapp
            command: ls /app
            "#,
        )
        .unwrap();
        let result = docker_exec(parse_params(yaml).unwrap(), true);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.changed);
        assert!(result.output.unwrap().contains("Would execute"));
    }
}
