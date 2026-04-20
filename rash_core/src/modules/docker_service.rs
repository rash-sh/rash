/// ANCHOR: module
/// # docker_service
///
/// Manage Docker Swarm services for container orchestration.
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
/// - name: Create a swarm service
///   docker_service:
///     name: web
///     image: nginx:latest
///     replicas: 3
///     networks:
///       - frontend
///     publish:
///       - published_port: 80
///         target_port: 80
///     state: present
///
/// - name: Scale a service
///   docker_service:
///     name: web
///     replicas: 5
///
/// - name: Update a service image
///   docker_service:
///     name: web
///     image: nginx:1.25
///     state: present
///
/// - name: Remove a service
///   docker_service:
///     name: web
///     state: absent
///
/// - name: Create service with environment variables
///   docker_service:
///     name: api
///     image: myapp:latest
///     replicas: 2
///     env:
///       DATABASE_URL: "postgres://db:5432/mydb"
///       LOG_LEVEL: "info"
///     limits:
///       cpus: "0.5"
///       memory: "512m"
///     state: present
///
/// - name: Create service with volume mounts
///   docker_service:
///     name: app
///     image: myapp:latest
///     mounts:
///       - source: /data
///         target: /app/data
///     state: present
///
/// - name: Create service with restart policy
///   docker_service:
///     name: worker
///     image: myapp:latest
///     restart_policy: on-failure
///     replicas: 3
///     state: present
///
/// - name: Create service with labels
///   docker_service:
///     name: monitoring
///     image: prometheus:latest
///     labels:
///       com.example.service: "monitoring"
///       com.example.env: "production"
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
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
enum RestartPolicy {
    None,
    #[serde(rename = "on-failure")]
    OnFailure,
    Any,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct PortMapping {
    published_port: u32,
    target_port: u32,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct MountConfig {
    source: String,
    target: String,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    readonly: bool,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct ResourceLimits {
    #[serde(default)]
    cpus: Option<String>,
    #[serde(default)]
    memory: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Service name.
    name: String,
    /// Image to use for the service.
    #[serde(default)]
    image: Option<String>,
    /// Desired state of the service.
    #[serde(default = "default_state")]
    state: State,
    /// Number of replicas.
    #[serde(default)]
    replicas: Option<u64>,
    /// Command to run in the service containers.
    #[serde(default)]
    command: Option<String>,
    /// Environment variables as key-value pairs.
    #[serde(default)]
    env: Option<serde_json::Map<String, serde_json::Value>>,
    /// Networks to attach the service to.
    #[serde(default)]
    networks: Option<Vec<String>>,
    /// Resource limits for the service.
    #[serde(default)]
    limits: Option<ResourceLimits>,
    /// Volume mounts for the service.
    #[serde(default)]
    mounts: Option<Vec<MountConfig>>,
    /// Published ports for the service.
    #[serde(default)]
    publish: Option<Vec<PortMapping>>,
    /// Service labels as key-value pairs.
    #[serde(default)]
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Restart policy (none, on-failure, any).
    #[serde(default)]
    restart_policy: Option<RestartPolicy>,
}

#[derive(Debug)]
pub struct DockerService;

struct DockerServiceClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct ServiceInfo {
    image: String,
    replicas: u64,
}

impl Module for DockerService {
    fn get_name(&self) -> &str {
        "docker_service"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_service(parse_params(optional_params)?, check_mode)?,
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

impl DockerServiceClient {
    fn new(check_mode: bool) -> Self {
        DockerServiceClient { check_mode }
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
                    "Error executing docker service: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn service_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(&["service", "inspect", "--format", "{{.ID}}", name], false)?;
        Ok(output.status.success() && !output.stdout.is_empty())
    }

    fn get_service_info(&self, name: &str) -> Result<Option<ServiceInfo>> {
        let output = self.exec_cmd(
            &[
                "service",
                "inspect",
                "--format",
                "{{.Spec.TaskTemplate.ContainerSpec.Image}}|{{.Spec.Mode.Replicated.Replicas}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() || output.stdout.is_empty() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();
        if parts.len() < 2 {
            return Ok(None);
        }

        let image = parts[0].to_string();
        let replicas = parts[1].trim().parse::<u64>().unwrap_or(0);

        Ok(Some(ServiceInfo { image, replicas }))
    }

    fn build_create_args(&self, params: &Params) -> Vec<String> {
        let mut args = vec!["service".to_string(), "create".to_string()];
        args.push("--name".to_string());
        args.push(params.name.clone());

        if let Some(ref restart_policy) = params.restart_policy {
            let policy_str = match restart_policy {
                RestartPolicy::None => "none",
                RestartPolicy::OnFailure => "on-failure",
                RestartPolicy::Any => "any",
            };
            args.push("--restart-condition".to_string());
            args.push(policy_str.to_string());
        }

        if let Some(replicas) = params.replicas {
            args.push("--replicas".to_string());
            args.push(replicas.to_string());
        }

        if let Some(ref env) = params.env {
            for (key, val) in env {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                args.push("--env".to_string());
                args.push(format!("{}={}", key, val_str));
            }
        }

        if let Some(ref networks) = params.networks {
            for network in networks {
                args.push("--network".to_string());
                args.push(network.clone());
            }
        }

        if let Some(ref limits) = params.limits {
            if let Some(ref cpus) = limits.cpus {
                args.push("--limit-cpu".to_string());
                args.push(cpus.clone());
            }
            if let Some(ref memory) = limits.memory {
                args.push("--limit-memory".to_string());
                args.push(memory.clone());
            }
        }

        if let Some(ref mounts) = params.mounts {
            for mount in mounts {
                let mount_type = mount.r#type.as_deref().unwrap_or("bind");
                let mut mount_str = format!(
                    "type={},source={},target={}",
                    mount_type, mount.source, mount.target
                );
                if mount.readonly {
                    mount_str.push_str(",readonly");
                }
                args.push("--mount".to_string());
                args.push(mount_str);
            }
        }

        if let Some(ref publish) = params.publish {
            for port in publish {
                let protocol = port.protocol.as_deref().unwrap_or("tcp");
                let mode = port.mode.as_deref().unwrap_or("ingress");
                let publish_str = format!(
                    "published={},target={},protocol={},mode={}",
                    port.published_port, port.target_port, protocol, mode
                );
                args.push("--publish".to_string());
                args.push(publish_str);
            }
        }

        if let Some(ref labels) = params.labels {
            for (key, val) in labels {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                args.push("--label".to_string());
                args.push(format!("{}={}", key, val_str));
            }
        }

        if let Some(ref image) = params.image {
            args.push(image.clone());
        }

        if let Some(ref command) = params.command {
            args.push(command.clone());
        }

        args
    }

    fn build_update_args(&self, params: &Params) -> Vec<String> {
        let mut args = vec!["service".to_string(), "update".to_string()];

        if let Some(ref image) = params.image {
            args.push("--image".to_string());
            args.push(image.clone());
        }

        if let Some(replicas) = params.replicas {
            args.push("--replicas".to_string());
            args.push(replicas.to_string());
        }

        if let Some(ref env) = params.env {
            for (key, val) in env {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                args.push("--env-add".to_string());
                args.push(format!("{}={}", key, val_str));
            }
        }

        if let Some(ref networks) = params.networks {
            for network in networks {
                args.push("--network-add".to_string());
                args.push(network.clone());
            }
        }

        if let Some(ref limits) = params.limits {
            if let Some(ref cpus) = limits.cpus {
                args.push("--limit-cpu".to_string());
                args.push(cpus.clone());
            }
            if let Some(ref memory) = limits.memory {
                args.push("--limit-memory".to_string());
                args.push(memory.clone());
            }
        }

        if let Some(ref mounts) = params.mounts {
            for mount in mounts {
                let mount_type = mount.r#type.as_deref().unwrap_or("bind");
                let mut mount_str = format!(
                    "type={},source={},target={}",
                    mount_type, mount.source, mount.target
                );
                if mount.readonly {
                    mount_str.push_str(",readonly");
                }
                args.push("--mount-add".to_string());
                args.push(mount_str);
            }
        }

        if let Some(ref publish) = params.publish {
            for port in publish {
                let protocol = port.protocol.as_deref().unwrap_or("tcp");
                let mode = port.mode.as_deref().unwrap_or("ingress");
                let publish_str = format!(
                    "published={},target={},protocol={},mode={}",
                    port.published_port, port.target_port, protocol, mode
                );
                args.push("--publish-add".to_string());
                args.push(publish_str);
            }
        }

        if let Some(ref labels) = params.labels {
            for (key, val) in labels {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                args.push("--label-add".to_string());
                args.push(format!("{}={}", key, val_str));
            }
        }

        if let Some(ref restart_policy) = params.restart_policy {
            let policy_str = match restart_policy {
                RestartPolicy::None => "none",
                RestartPolicy::OnFailure => "on-failure",
                RestartPolicy::Any => "any",
            };
            args.push("--restart-condition".to_string());
            args.push(policy_str.to_string());
        }

        args.push(params.name.clone());
        args
    }

    fn create_service(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let args = self.build_create_args(params);
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn update_service(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let args = self.build_update_args(params);
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn remove_service(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["service", "rm", name], true)?;
        Ok(output.status.success())
    }

    fn needs_update(&self, params: &Params, current: &ServiceInfo) -> bool {
        if let Some(ref image) = params.image {
            if !image_contains_tag(image) && !image_contains_tag(&current.image) {
                if image.trim() != current.image.trim() {
                    return true;
                }
            } else if normalize_image_name(image) != normalize_image_name(&current.image) {
                return true;
            }
        }

        if params.replicas.is_some_and(|r| r != current.replicas) {
            return true;
        }

        false
    }
}

fn normalize_image_name(image: &str) -> String {
    let image = image.trim();
    if image.contains('/') && !image.contains("://") {
        image.to_string()
    } else if !image.contains('/') {
        format!("docker.io/library/{}", image)
    } else {
        image.to_string()
    }
}

fn image_contains_tag(image: &str) -> bool {
    if let Some(part) = image.split('/').next_back() {
        part.contains(':')
    } else {
        false
    }
}

fn docker_service(params: Params, check_mode: bool) -> Result<ModuleResult> {
    if params.name.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "name cannot be empty"));
    }

    let client = DockerServiceClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let exists = client.service_exists(&params.name)?;
    trace!("service {} exists: {}", params.name, exists);

    match params.state {
        State::Present => {
            if exists {
                let current_info = client.get_service_info(&params.name)?;
                trace!("current service info: {:?}", current_info);

                if let Some(ref info) = current_info {
                    if client.needs_update(&params, info) {
                        client.update_service(&params)?;
                        diff(
                            format!("service {} (current)", serialize_service_info(info)),
                            format!("service {} (desired)", serialize_desired_state(&params)),
                        );
                        output_messages.push(format!("Service {} updated", params.name));
                        changed = true;
                    } else {
                        output_messages.push(format!("Service {} is up to date", params.name));
                    }
                }
            } else {
                if params.image.is_none() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "image is required when creating a new service",
                    ));
                }

                client.create_service(&params)?;
                diff(
                    "service absent".to_string(),
                    format!("service {} (created)", serialize_desired_state(&params)),
                );
                output_messages.push(format!("Service {} created", params.name));
                changed = true;
            }
        }
        State::Absent => {
            if exists {
                client.remove_service(&params.name)?;
                diff(
                    format!("service {} present", params.name),
                    "service absent".to_string(),
                );
                output_messages.push(format!("Service {} removed", params.name));
                changed = true;
            } else {
                output_messages.push(format!("Service {} already absent", params.name));
            }
        }
    }

    let extra = {
        let mut map = serde_json::Map::new();
        map.insert(
            "name".to_string(),
            serde_json::Value::String(params.name.clone()),
        );
        map.insert("exists".to_string(), serde_json::Value::Bool(exists));
        map.insert("changed".to_string(), serde_json::Value::Bool(changed));
        map
    };

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

fn serialize_service_info(info: &ServiceInfo) -> String {
    format!("image={}, replicas={}", info.image, info.replicas)
}

fn serialize_desired_state(params: &Params) -> String {
    let image = params.image.as_deref().unwrap_or("unchanged");
    let replicas = params
        .replicas
        .map(|r| r.to_string())
        .unwrap_or_else(|| "unchanged".to_string());
    format!("image={}, replicas={}", image, replicas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "web");
        assert_eq!(params.state, State::Present);
        assert!(params.image.is_none());
        assert!(params.replicas.is_none());
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            image: nginx:latest
            replicas: 3
            networks:
              - frontend
              - backend
            publish:
              - published_port: 80
                target_port: 80
              - published_port: 443
                target_port: 443
                protocol: tcp
                mode: host
            env:
              DATABASE_URL: "postgres://db:5432/mydb"
              LOG_LEVEL: "info"
            labels:
              com.example.service: "web"
            limits:
              cpus: "0.5"
              memory: "512m"
            restart_policy: on-failure
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "web");
        assert_eq!(params.image, Some("nginx:latest".to_string()));
        assert_eq!(params.replicas, Some(3));
        assert_eq!(
            params.networks,
            Some(vec!["frontend".to_string(), "backend".to_string(),])
        );
        assert_eq!(params.publish.as_ref().map(|p| p.len()), Some(2));
        assert_eq!(
            params.env.as_ref().and_then(|e| e.get("DATABASE_URL")),
            Some(&serde_json::Value::String(
                "postgres://db:5432/mydb".to_string()
            ))
        );
        assert_eq!(params.state, State::Present);
        assert_eq!(params.restart_policy, Some(RestartPolicy::OnFailure));
        assert!(params.limits.is_some());
        let limits = params.limits.unwrap();
        assert_eq!(limits.cpus, Some("0.5".to_string()));
        assert_eq!(limits.memory, Some("512m".to_string()));
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_with_command() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: worker
            image: myapp:latest
            command: python worker.py
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.command, Some("python worker.py".to_string()));
    }

    #[test]
    fn test_parse_params_with_mounts() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app
            image: myapp:latest
            mounts:
              - source: /data
                target: /app/data
              - source: config_vol
                target: /etc/config
                type: volume
                readonly: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let mounts = params.mounts.unwrap();
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].source, "/data");
        assert_eq!(mounts[0].target, "/app/data");
        assert!(mounts[1].readonly);
        assert_eq!(mounts[1].r#type, Some("volume".to_string()));
    }

    #[test]
    fn test_parse_params_restart_policies() {
        for policy in &["none", "on-failure", "any"] {
            let yaml: YamlValue = serde_norway::from_str(&format!(
                r#"
                name: test
                image: nginx
                restart_policy: {}
                "#,
                policy
            ))
            .unwrap();
            let params: Params = parse_params(yaml).unwrap();
            assert!(params.restart_policy.is_some());
        }
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_normalize_image_name() {
        assert_eq!(normalize_image_name("nginx"), "docker.io/library/nginx");
        assert_eq!(
            normalize_image_name("nginx:latest"),
            "docker.io/library/nginx:latest"
        );
        assert_eq!(
            normalize_image_name("myregistry.com/myimage"),
            "myregistry.com/myimage"
        );
    }

    #[test]
    fn test_image_contains_tag() {
        assert!(image_contains_tag("nginx:latest"));
        assert!(image_contains_tag("myregistry.com/myimage:v1"));
        assert!(!image_contains_tag("nginx"));
    }

    #[test]
    fn test_build_create_args() {
        let client = DockerServiceClient::new(false);
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            image: nginx:latest
            replicas: 3
            networks:
              - frontend
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let args = client.build_create_args(&params);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        assert!(args_str.contains(&"create"));
        assert!(args_str.contains(&"--name"));
        assert!(args_str.contains(&"web"));
        assert!(args_str.contains(&"--replicas"));
        assert!(args_str.contains(&"3"));
        assert!(args_str.contains(&"--network"));
        assert!(args_str.contains(&"frontend"));
        assert!(args_str.contains(&"nginx:latest"));
    }

    #[test]
    fn test_build_update_args() {
        let client = DockerServiceClient::new(false);
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            image: nginx:1.25
            replicas: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let args = client.build_update_args(&params);
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        assert!(args_str.contains(&"update"));
        assert!(args_str.contains(&"--image"));
        assert!(args_str.contains(&"nginx:1.25"));
        assert!(args_str.contains(&"--replicas"));
        assert!(args_str.contains(&"5"));
        assert!(args_str.contains(&"web"));
    }

    #[test]
    fn test_needs_update_image_change() {
        let client = DockerServiceClient::new(false);
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            image: nginx:1.25
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let info = ServiceInfo {
            image: "nginx:latest".to_string(),
            replicas: 3,
        };
        assert!(client.needs_update(&params, &info));
    }

    #[test]
    fn test_needs_update_replicas_change() {
        let client = DockerServiceClient::new(false);
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            replicas: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let info = ServiceInfo {
            image: "nginx:latest".to_string(),
            replicas: 3,
        };
        assert!(client.needs_update(&params, &info));
    }

    #[test]
    fn test_needs_update_no_change() {
        let client = DockerServiceClient::new(false);
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web
            image: nginx:latest
            replicas: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let info = ServiceInfo {
            image: "nginx:latest".to_string(),
            replicas: 3,
        };
        assert!(!client.needs_update(&params, &info));
    }

    #[test]
    fn test_serialize_service_info() {
        let info = ServiceInfo {
            image: "nginx:latest".to_string(),
            replicas: 3,
        };
        let serialized = serialize_service_info(&info);
        assert!(serialized.contains("nginx:latest"));
        assert!(serialized.contains("3"));
    }
}
