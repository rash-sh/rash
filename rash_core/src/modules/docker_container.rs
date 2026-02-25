/// ANCHOR: module
/// # docker_container
///
/// Manage Docker containers.
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
/// - name: Start a container
///   docker_container:
///     name: myapp
///     image: nginx:latest
///     state: started
///
/// - name: Stop a container
///   docker_container:
///     name: myapp
///     state: stopped
///
/// - name: Remove a container
///   docker_container:
///     name: myapp
///     state: absent
///
/// - name: Create a container with ports and environment
///   docker_container:
///     name: webapp
///     image: nginx:latest
///     state: started
///     ports:
///       - "8080:80"
///       - "443:443"
///     env:
///       NGINX_HOST: example.com
///
/// - name: Create a container with volumes
///   docker_container:
///     name: dataapp
///     image: alpine:latest
///     state: started
///     volumes:
///       - "/host/path:/container/path"
///       - "named_volume:/data"
///
/// - name: Create a container with health check
///   docker_container:
///     name: healthy_app
///     image: nginx:latest
///     state: started
///     healthcheck:
///       test: ["CMD", "curl", "-f", "http://localhost/"]
///       interval: 30s
///       timeout: 10s
///       retries: 3
///
/// - name: Create a container with resource limits
///   docker_container:
///     name: limited_app
///     image: nginx:latest
///     state: started
///     memory: "512m"
///     cpu_shares: 512
///
/// - name: Create a container connected to a network
///   docker_container:
///     name: networked_app
///     image: nginx:latest
///     state: started
///     networks:
///       - mynetwork
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Present,
    Started,
    Stopped,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct HealthCheck {
    /// Command to run to check health.
    test: Vec<String>,
    /// Time between health checks.
    #[serde(default = "default_health_interval")]
    interval: String,
    /// Maximum time to allow one check to run.
    #[serde(default = "default_health_timeout")]
    timeout: String,
    /// Consecutive failures needed to report unhealthy.
    #[serde(default = "default_health_retries")]
    retries: u32,
    /// Start period for the container to bootstrap.
    #[serde(default)]
    start_period: Option<String>,
}

fn default_health_interval() -> String {
    "30s".to_string()
}

fn default_health_timeout() -> String {
    "30s".to_string()
}

fn default_health_retries() -> u32 {
    3
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of the container.
    name: String,
    /// Image to use for the container.
    image: Option<String>,
    /// State of the container.
    #[serde(default = "default_state")]
    state: State,
    /// Environment variables.
    env: Option<Vec<String>>,
    /// Dictionary of environment variables.
    env_dict: Option<serde_json::Map<String, serde_json::Value>>,
    /// Port mappings.
    ports: Option<Vec<String>>,
    /// Volume mappings.
    volumes: Option<Vec<String>>,
    /// Networks to connect to.
    networks: Option<Vec<String>>,
    /// Health check configuration.
    healthcheck: Option<HealthCheck>,
    /// Memory limit (e.g., "512m", "1g").
    memory: Option<String>,
    /// CPU shares (relative weight).
    cpu_shares: Option<u32>,
    /// CPU quota in microseconds.
    cpu_quota: Option<i64>,
    /// CPU period in microseconds.
    cpu_period: Option<u32>,
    /// Command to run in the container.
    command: Option<Vec<String>>,
    /// Entry point for the container.
    entrypoint: Option<String>,
    /// Working directory inside the container.
    working_dir: Option<String>,
    /// User to run as inside the container.
    user: Option<String>,
    /// Restart policy (no, always, on-failure, unless-stopped).
    restart_policy: Option<String>,
    /// Container hostname.
    hostname: Option<String>,
    /// Run container in privileged mode.
    #[serde(default)]
    privileged: bool,
    /// Keep stdin open.
    #[serde(default)]
    interactive: bool,
    /// Allocate a pseudo-TTY.
    #[serde(default)]
    tty: bool,
    /// Automatically remove the container when it exits.
    #[serde(default)]
    auto_remove: bool,
    /// List of capabilities to add.
    capabilities_add: Option<Vec<String>>,
    /// List of capabilities to drop.
    capabilities_drop: Option<Vec<String>>,
    /// Pull image before running.
    #[serde(default)]
    pull: bool,
    /// Force container removal on state=absent.
    #[serde(default)]
    force: bool,
}

fn default_state() -> State {
    State::Started
}

#[derive(Debug)]
pub struct DockerContainer;

#[derive(Debug, Clone)]
struct ContainerInfo {
    id: String,
    name: String,
    image: String,
    state: String,
    status: String,
}

impl Module for DockerContainer {
    fn get_name(&self) -> &str {
        "docker_container"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_container(parse_params(optional_params)?, check_mode)?,
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

    fn get_container_info(&self, name: &str) -> Result<Option<ContainerInfo>> {
        let output = self.exec_cmd(
            &[
                "inspect",
                "--format",
                "{{.Id}}|{{.Name}}|{{.Config.Image}}|{{.State.Status}}|{{.State.String}}",
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
            Ok(Some(ContainerInfo {
                id: parts[0].to_string(),
                name: parts[1].trim_start_matches('/').to_string(),
                image: parts[2].to_string(),
                state: parts[3].to_string(),
                status: parts[4].to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn is_running(&self, name: &str) -> Result<bool> {
        let info = self.get_container_info(name)?;
        Ok(info.is_some_and(|i| i.state == "running"))
    }

    fn pull_image(&self, image: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["pull", image], true)?;
        Ok(output.status.success())
    }

    fn create_container(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["create".to_string()];

        args.push("--name".to_string());
        args.push(params.name.clone());

        if let Some(ref hostname) = params.hostname {
            args.push("--hostname".to_string());
            args.push(hostname.clone());
        }

        if let Some(ref env_list) = params.env {
            for env in env_list {
                args.push("-e".to_string());
                args.push(env.clone());
            }
        }

        if let Some(ref env_dict) = params.env_dict {
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

        if let Some(ref ports) = params.ports {
            for port in ports {
                args.push("-p".to_string());
                args.push(port.clone());
            }
        }

        if let Some(ref volumes) = params.volumes {
            for volume in volumes {
                args.push("-v".to_string());
                args.push(volume.clone());
            }
        }

        if let Some(ref networks) = params.networks {
            for network in networks {
                args.push("--network".to_string());
                args.push(network.clone());
            }
        }

        if let Some(ref healthcheck) = params.healthcheck {
            let test_str = format!(
                "[{}]",
                healthcheck
                    .test
                    .iter()
                    .map(|t| format!("\"{}\"", t))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            args.push("--health-cmd".to_string());
            args.push(test_str);
            args.push("--health-interval".to_string());
            args.push(healthcheck.interval.clone());
            args.push("--health-timeout".to_string());
            args.push(healthcheck.timeout.clone());
            args.push("--health-retries".to_string());
            args.push(healthcheck.retries.to_string());
            if let Some(ref start_period) = healthcheck.start_period {
                args.push("--health-start-period".to_string());
                args.push(start_period.clone());
            }
        }

        if let Some(ref memory) = params.memory {
            args.push("--memory".to_string());
            args.push(memory.clone());
        }

        if let Some(cpu_shares) = params.cpu_shares {
            args.push("--cpu-shares".to_string());
            args.push(cpu_shares.to_string());
        }

        if let Some(cpu_quota) = params.cpu_quota {
            args.push("--cpu-quota".to_string());
            args.push(cpu_quota.to_string());
        }

        if let Some(cpu_period) = params.cpu_period {
            args.push("--cpu-period".to_string());
            args.push(cpu_period.to_string());
        }

        if let Some(ref command) = params.command {
            args.push("--cmd".to_string());
            args.push(command.join(" "));
        }

        if let Some(ref entrypoint) = params.entrypoint {
            args.push("--entrypoint".to_string());
            args.push(entrypoint.clone());
        }

        if let Some(ref working_dir) = params.working_dir {
            args.push("--workdir".to_string());
            args.push(working_dir.clone());
        }

        if let Some(ref user) = params.user {
            args.push("--user".to_string());
            args.push(user.clone());
        }

        if let Some(ref restart_policy) = params.restart_policy {
            args.push("--restart".to_string());
            args.push(restart_policy.clone());
        }

        if params.privileged {
            args.push("--privileged".to_string());
        }

        if params.interactive {
            args.push("-i".to_string());
        }

        if params.tty {
            args.push("-t".to_string());
        }

        if params.auto_remove {
            args.push("--rm".to_string());
        }

        if let Some(ref caps) = params.capabilities_add {
            for cap in caps {
                args.push("--cap-add".to_string());
                args.push(cap.clone());
            }
        }

        if let Some(ref caps) = params.capabilities_drop {
            for cap in caps {
                args.push("--cap-drop".to_string());
                args.push(cap.clone());
            }
        }

        let image = params.image.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "image is required when creating a container",
            )
        })?;

        args.push(image.clone());

        if let Some(ref command) = params.command {
            for cmd in command {
                args.push(cmd.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn start_container(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if self.is_running(name)? {
            return Ok(false);
        }

        self.exec_cmd(&["start", name], true)?;
        Ok(true)
    }

    fn stop_container(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_running(name)? {
            return Ok(false);
        }

        self.exec_cmd(&["stop", name], true)?;
        Ok(true)
    }

    fn remove_container(&self, name: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.container_exists(name)? {
            return Ok(false);
        }

        let mut args = vec!["rm"];
        if force {
            args.push("-f");
        }
        args.push(name);

        self.exec_cmd(&args, true)?;
        Ok(true)
    }

    fn get_container_state(
        &self,
        name: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_container_info(name)? {
            let is_running = info.state == "running";
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("image".to_string(), serde_json::Value::String(info.image));
            result.insert("state".to_string(), serde_json::Value::String(info.state));
            result.insert("status".to_string(), serde_json::Value::String(info.status));
            result.insert("running".to_string(), serde_json::Value::Bool(is_running));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn validate_container_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name cannot be empty",
        ));
    }

    if name.len() > 63 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name too long (max 63 characters)",
        ));
    }

    let valid_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid_chars {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name contains invalid characters (only [a-zA-Z0-9.-_] allowed)",
        ));
    }

    if name.starts_with('-') || name.starts_with('.') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Container name cannot start with '-' or '.'",
        ));
    }

    Ok(())
}

fn validate_image_name(image: &str) -> Result<()> {
    if image.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name cannot be empty",
        ));
    }

    if image.len() > 256 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name too long (max 256 characters)",
        ));
    }

    Ok(())
}

fn docker_container(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_container_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Absent => {
            let was_running = client.is_running(&params.name)?;
            if client.remove_container(&params.name, params.force)? {
                diff("state: present".to_string(), "state: absent".to_string());
                output_messages.push(format!("Container '{}' removed", params.name));
                changed = true;
            } else if was_running {
                output_messages.push(format!("Container '{}' already absent", params.name));
            }
        }
        State::Present | State::Started => {
            let image = params.image.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "image is required for state 'present' or 'started'",
                )
            })?;
            validate_image_name(image)?;

            let exists = client.container_exists(&params.name)?;
            let was_running = client.is_running(&params.name)?;

            if !exists {
                if params.pull {
                    client.pull_image(image)?;
                }
                client.create_container(&params)?;
                diff("state: absent".to_string(), "state: present".to_string());
                output_messages.push(format!(
                    "Container '{}' created from image '{}'",
                    params.name, image
                ));
                changed = true;
            }

            if params.state == State::Started {
                if client.start_container(&params.name)? {
                    diff("state: stopped".to_string(), "state: started".to_string());
                    output_messages.push(format!("Container '{}' started", params.name));
                    changed = true;
                } else if !was_running && !check_mode {
                    output_messages.push(format!("Container '{}' already running", params.name));
                }
            } else if params.state == State::Present
                && was_running
                && client.stop_container(&params.name)?
            {
                diff("state: started".to_string(), "state: present".to_string());
                output_messages.push(format!("Container '{}' stopped", params.name));
                changed = true;
            }
        }
        State::Stopped => {
            if client.container_exists(&params.name)? {
                if client.stop_container(&params.name)? {
                    diff("state: started".to_string(), "state: stopped".to_string());
                    output_messages.push(format!("Container '{}' stopped", params.name));
                    changed = true;
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Container '{}' does not exist", params.name),
                ));
            }
        }
    }

    let extra = client.get_container_state(&params.name)?;

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
            image: nginx:latest
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.image, Some("nginx:latest".to_string()));
        assert_eq!(params.state, State::Started);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            image: nginx:latest
            state: started
            env:
              - FOO=bar
              - BAZ=qux
            ports:
              - "8080:80"
            volumes:
              - "/host/path:/container/path"
            memory: "512m"
            cpu_shares: 512
            privileged: true
            restart_policy: always
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.image, Some("nginx:latest".to_string()));
        assert_eq!(params.state, State::Started);
        assert_eq!(
            params.env,
            Some(vec!["FOO=bar".to_string(), "BAZ=qux".to_string()])
        );
        assert_eq!(params.ports, Some(vec!["8080:80".to_string()]));
        assert_eq!(
            params.volumes,
            Some(vec!["/host/path:/container/path".to_string()])
        );
        assert_eq!(params.memory, Some("512m".to_string()));
        assert_eq!(params.cpu_shares, Some(512));
        assert!(params.privileged);
        assert_eq!(params.restart_policy, Some("always".to_string()));
    }

    #[test]
    fn test_parse_params_env_dict() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            image: nginx:latest
            env_dict:
              FOO: bar
              NUM: 42
              BOOL: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let env_dict = params.env_dict.unwrap();
        assert_eq!(
            env_dict.get("FOO").unwrap(),
            &serde_json::Value::String("bar".to_string())
        );
        assert_eq!(env_dict.get("NUM").unwrap(), &serde_json::json!(42));
        assert_eq!(
            env_dict.get("BOOL").unwrap(),
            &serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn test_parse_params_healthcheck() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            image: nginx:latest
            healthcheck:
              test:
                - CMD
                - curl
                - "-f"
                - "http://localhost/"
              interval: 30s
              timeout: 10s
              retries: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let healthcheck = params.healthcheck.unwrap();
        assert_eq!(
            healthcheck.test,
            vec!["CMD", "curl", "-f", "http://localhost/"]
        );
        assert_eq!(healthcheck.interval, "30s");
        assert_eq!(healthcheck.timeout, "10s");
        assert_eq!(healthcheck.retries, 3);
    }

    #[test]
    fn test_parse_params_state_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: stopped
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Stopped);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            image: nginx:latest
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

        assert!(validate_container_name("").is_err());
        assert!(validate_container_name("a".repeat(64).as_str()).is_err());
        assert!(validate_container_name("-myapp").is_err());
        assert!(validate_container_name(".myapp").is_err());
        assert!(validate_container_name("my app").is_err());
        assert!(validate_container_name("my/app").is_err());
    }

    #[test]
    fn test_validate_image_name() {
        assert!(validate_image_name("nginx").is_ok());
        assert!(validate_image_name("nginx:latest").is_ok());
        assert!(validate_image_name("library/nginx:latest").is_ok());
        assert!(validate_image_name("registry.example.com/namespace/image:tag").is_ok());

        assert!(validate_image_name("").is_err());
        assert!(validate_image_name(&"a".repeat(257)).is_err());
    }
}
