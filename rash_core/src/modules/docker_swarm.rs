/// ANCHOR: module
/// # docker_swarm
///
/// Manage Docker Swarm clusters.
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
/// - name: Initialize a swarm cluster
///   docker_swarm:
///     state: present
///     advertise_addr: "192.168.1.10"
///
/// - name: Join a swarm as a worker
///   docker_swarm:
///     state: join
///     join_token: "SWMTKN-1-xxx"
///     remote_addrs:
///       - "192.168.1.10:2377"
///
/// - name: Join a swarm as a manager
///   docker_swarm:
///     state: join
///     join_token: "SWMTKN-1-xxx-manager"
///     remote_addrs:
///       - "192.168.1.10:2377"
///
/// - name: Leave the swarm
///   docker_swarm:
///     state: absent
///     force: true
///
/// - name: Remove a node from the swarm (manager only)
///   docker_swarm:
///     state: remove
///     node_id: "node-xxx"
///     force: true
///
/// - name: Create a swarm service
///   docker_swarm:
///     state: service
///     service:
///       name: myapp
///       image: nginx:latest
///       replicas: 3
///       ports:
///         - "8080:80"
///
/// - name: Update a swarm service
///   docker_swarm:
///     state: service
///     service:
///       name: myapp
///       image: nginx:latest
///       replicas: 5
///       env:
///         - "DEBUG=true"
///
/// - name: Remove a swarm service
///   docker_swarm:
///     state: service
///     service:
///       name: myapp
///       service_state: absent
///
/// - name: Get swarm status
///   docker_swarm:
///     state: inspect
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum SwarmState {
    Present,
    Absent,
    Join,
    Remove,
    Service,
    Inspect,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum ServiceState {
    Present,
    Absent,
    #[serde(alias = "running")]
    Started,
    #[serde(alias = "stopped")]
    Paused,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct ServiceConfig {
    /// Name of the service.
    name: String,
    /// Image to use for the service.
    image: Option<String>,
    /// State of the service.
    #[serde(default = "default_service_state")]
    service_state: ServiceState,
    /// Number of replicas.
    #[serde(default = "default_replicas")]
    replicas: u32,
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
    /// Restart policy condition.
    restart_condition: Option<String>,
    /// Restart policy delay.
    restart_delay: Option<String>,
    /// Restart policy max attempts.
    restart_max_attempts: Option<u32>,
    /// Restart policy window.
    restart_window: Option<String>,
    /// Update parallelism.
    update_parallelism: Option<u32>,
    /// Update delay.
    update_delay: Option<String>,
    /// Update failure action.
    update_failure_action: Option<String>,
    /// Update monitor.
    update_monitor: Option<String>,
    /// Update max failure ratio.
    update_max_failure_ratio: Option<f32>,
    /// Constraints for placement.
    constraints: Option<Vec<String>>,
    /// Preferences for placement.
    preferences: Option<Vec<String>>,
    /// Command to run.
    command: Option<Vec<String>>,
    /// Args to pass to the command.
    args: Option<Vec<String>>,
    /// Health check configuration.
    healthcheck: Option<HealthCheck>,
    /// Resources limits.
    limits: Option<Resources>,
    /// Resources reservations.
    reservations: Option<Resources>,
    /// Labels for the service.
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Container labels.
    container_labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Hostname for containers.
    hostname: Option<String>,
    /// Mounts for the service.
    mounts: Option<Vec<String>>,
    /// DNS servers.
    dns_servers: Option<Vec<String>>,
    /// DNS search domains.
    dns_search: Option<Vec<String>>,
    /// DNS options.
    dns_options: Option<Vec<String>>,
    /// Capabilities to add.
    capabilities_add: Option<Vec<String>>,
    /// Capabilities to drop.
    capabilities_drop: Option<Vec<String>>,
    /// Log driver.
    log_driver: Option<String>,
    /// Log driver options.
    log_options: Option<serde_json::Map<String, serde_json::Value>>,
    /// User to run as.
    user: Option<String>,
    /// Working directory.
    workdir: Option<String>,
    /// Stop grace period.
    stop_grace_period: Option<String>,
    /// Stop signal.
    stop_signal: Option<String>,
    /// TTY allocation.
    #[serde(default)]
    tty: bool,
    /// STDIN open.
    #[serde(default)]
    stdin: bool,
    /// Read-only root filesystem.
    #[serde(default)]
    read_only: bool,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct HealthCheck {
    /// Command to run to check health.
    test: Option<Vec<String>>,
    /// Time between health checks.
    #[serde(default)]
    interval: Option<String>,
    /// Maximum time to allow one check to run.
    #[serde(default)]
    timeout: Option<String>,
    /// Consecutive failures needed to report unhealthy.
    #[serde(default)]
    retries: Option<u32>,
    /// Start period for the container to bootstrap.
    #[serde(default)]
    start_period: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct Resources {
    /// CPU limit (nano CPUs).
    cpu: Option<String>,
    /// Memory limit.
    memory: Option<String>,
    /// Generic resources.
    generic_resources: Option<Vec<String>>,
}

fn default_service_state() -> ServiceState {
    ServiceState::Present
}

fn default_replicas() -> u32 {
    1
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// State of the swarm.
    #[serde(default = "default_swarm_state")]
    state: SwarmState,
    /// Address to advertise.
    advertise_addr: Option<String>,
    /// Listen address.
    listen_addr: Option<String>,
    /// Join token for joining a swarm.
    join_token: Option<String>,
    /// Remote addresses to join.
    remote_addrs: Option<Vec<String>>,
    /// Force the operation.
    #[serde(default)]
    force: bool,
    /// Node ID to remove.
    node_id: Option<String>,
    /// Service configuration.
    service: Option<ServiceConfig>,
    /// Availability of the node (active, pause, drain).
    availability: Option<String>,
}

fn default_swarm_state() -> SwarmState {
    SwarmState::Inspect
}

#[derive(Debug)]
pub struct DockerSwarm;

#[derive(Debug, Clone)]
struct SwarmInfo {
    cluster_id: String,
    node_id: String,
    is_manager: bool,
    is_leader: bool,
    availability: String,
}

#[derive(Debug, Clone)]
struct ServiceInfo {
    id: String,
    name: String,
    image: String,
    replicas: u32,
    mode: String,
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

    fn is_swarm_active(&self) -> Result<bool> {
        let output = self.exec_cmd(&["info", "--format", "{{.Swarm.LocalNodeState}}"], false)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim() == "active")
    }

    fn get_swarm_info(&self) -> Result<Option<SwarmInfo>> {
        if !self.is_swarm_active()? {
            return Ok(None);
        }

        let output = self.exec_cmd(
            &[
                "info",
                "--format",
                "{{.Swarm.ClusterID}}|{{.Swarm.NodeID}}|{{.Swarm.ControlAvailable}}|{{.Swarm.IsManager}}|{{.Swarm.Availability}}",
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 5 {
            Ok(Some(SwarmInfo {
                cluster_id: parts[0].to_string(),
                node_id: parts[1].to_string(),
                is_manager: parts[2] == "true",
                is_leader: parts[3] == "true",
                availability: parts[4].to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn get_join_token(&self, role: &str) -> Result<String> {
        let output = self.exec_cmd(&["swarm", "join-token", "-q", role], true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }

    fn init_swarm(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["swarm".to_string(), "init".to_string()];

        if let Some(ref addr) = params.advertise_addr {
            args.push("--advertise-addr".to_string());
            args.push(addr.clone());
        }

        if let Some(ref addr) = params.listen_addr {
            args.push("--listen-addr".to_string());
            args.push(addr.clone());
        }

        if let Some(ref avail) = params.availability {
            args.push("--availability".to_string());
            args.push(avail.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn join_swarm(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let token = params.join_token.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "join_token is required for joining a swarm",
            )
        })?;

        let addrs = params.remote_addrs.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "remote_addrs is required for joining a swarm",
            )
        })?;

        if addrs.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "remote_addrs must contain at least one address",
            ));
        }

        let mut args: Vec<String> = vec!["swarm".to_string(), "join".to_string()];

        args.push("--token".to_string());
        args.push(token.clone());

        if let Some(ref addr) = params.advertise_addr {
            args.push("--advertise-addr".to_string());
            args.push(addr.clone());
        }

        if let Some(ref addr) = params.listen_addr {
            args.push("--listen-addr".to_string());
            args.push(addr.clone());
        }

        if let Some(ref avail) = params.availability {
            args.push("--availability".to_string());
            args.push(avail.clone());
        }

        for addr in addrs {
            args.push(addr.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn leave_swarm(&self, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<&str> = vec!["swarm", "leave"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not part of a swarm") || stderr.contains("already left") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error leaving swarm: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn remove_node(&self, node_id: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.is_swarm_active()? {
            return Err(Error::new(ErrorKind::InvalidData, "Not in a swarm cluster"));
        }

        let mut args: Vec<&str> = vec!["node", "rm"];
        if force {
            args.push("--force");
        }
        args.push(node_id);

        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") || stderr.contains("No such node") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error removing node: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn service_exists(&self, name: &str) -> Result<bool> {
        let output = self.exec_cmd(
            &["service", "inspect", "--format", "{{.Spec.Name}}", name],
            false,
        )?;
        Ok(output.status.success())
    }

    fn get_service_info(&self, name: &str) -> Result<Option<ServiceInfo>> {
        let output = self.exec_cmd(
            &[
                "service",
                "inspect",
                "--format",
                "{{.ID}}|{{.Spec.Name}}|{{.Spec.TaskTemplate.ContainerSpec.Image}}|{{.Spec.Mode.Replicated.Replicas}}|{{.Spec.Mode.Type}}",
                name,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 4 {
            let replicas = parts[3].parse::<u32>().unwrap_or(1);
            Ok(Some(ServiceInfo {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                image: parts[2].to_string(),
                replicas,
                mode: parts.get(4).unwrap_or(&"replicated").to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    fn create_service(&self, config: &ServiceConfig) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["service".to_string(), "create".to_string()];

        args.push("--name".to_string());
        args.push(config.name.clone());

        args.push("--replicas".to_string());
        args.push(config.replicas.to_string());

        if let Some(ref env_list) = config.env {
            for env in env_list {
                args.push("-e".to_string());
                args.push(env.clone());
            }
        }

        if let Some(ref env_dict) = config.env_dict {
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

        if let Some(ref ports) = config.ports {
            for port in ports {
                args.push("-p".to_string());
                args.push(port.clone());
            }
        }

        if let Some(ref mounts) = config.mounts {
            for mount in mounts {
                args.push("--mount".to_string());
                args.push(mount.clone());
            }
        }

        if let Some(ref volumes) = config.volumes {
            for volume in volumes {
                args.push("--mount".to_string());
                args.push(format!(
                    "type=volume,source={},target={}",
                    volume.split(':').next().unwrap_or(volume),
                    volume.split(':').nth(1).unwrap_or(volume)
                ));
            }
        }

        if let Some(ref networks) = config.networks {
            for network in networks {
                args.push("--network".to_string());
                args.push(network.clone());
            }
        }

        if let Some(ref restart) = config.restart_condition {
            args.push("--restart-condition".to_string());
            args.push(restart.clone());
        }

        if let Some(ref delay) = config.restart_delay {
            args.push("--restart-delay".to_string());
            args.push(delay.clone());
        }

        if let Some(max) = config.restart_max_attempts {
            args.push("--restart-max-attempts".to_string());
            args.push(max.to_string());
        }

        if let Some(ref window) = config.restart_window {
            args.push("--restart-window".to_string());
            args.push(window.clone());
        }

        if let Some(parallelism) = config.update_parallelism {
            args.push("--update-parallelism".to_string());
            args.push(parallelism.to_string());
        }

        if let Some(ref delay) = config.update_delay {
            args.push("--update-delay".to_string());
            args.push(delay.clone());
        }

        if let Some(ref action) = config.update_failure_action {
            args.push("--update-failure-action".to_string());
            args.push(action.clone());
        }

        if let Some(ref monitor) = config.update_monitor {
            args.push("--update-monitor".to_string());
            args.push(monitor.clone());
        }

        if let Some(ratio) = config.update_max_failure_ratio {
            args.push("--update-max-failure-ratio".to_string());
            args.push(ratio.to_string());
        }

        if let Some(ref constraints) = config.constraints {
            for constraint in constraints {
                args.push("--constraint".to_string());
                args.push(constraint.clone());
            }
        }

        if let Some(ref preferences) = config.preferences {
            for preference in preferences {
                args.push("--placement-pref".to_string());
                args.push(preference.clone());
            }
        }

        if let Some(ref labels) = config.labels {
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

        if let Some(ref container_labels) = config.container_labels {
            for (key, value) in container_labels {
                let label_str = match value {
                    serde_json::Value::String(s) => format!("{}={}", key, s),
                    serde_json::Value::Number(n) => format!("{}={}", key, n),
                    serde_json::Value::Bool(b) => format!("{}={}", key, b),
                    _ => format!("{}={}", key, value),
                };
                args.push("--container-label".to_string());
                args.push(label_str);
            }
        }

        if let Some(ref hostname) = config.hostname {
            args.push("--hostname".to_string());
            args.push(hostname.clone());
        }

        if let Some(ref dns) = config.dns_servers {
            for server in dns {
                args.push("--dns".to_string());
                args.push(server.clone());
            }
        }

        if let Some(ref search) = config.dns_search {
            for domain in search {
                args.push("--dns-search".to_string());
                args.push(domain.clone());
            }
        }

        if let Some(ref opts) = config.dns_options {
            for opt in opts {
                args.push("--dns-option".to_string());
                args.push(opt.clone());
            }
        }

        if let Some(ref caps) = config.capabilities_add {
            for cap in caps {
                args.push("--cap-add".to_string());
                args.push(cap.clone());
            }
        }

        if let Some(ref caps) = config.capabilities_drop {
            for cap in caps {
                args.push("--cap-drop".to_string());
                args.push(cap.clone());
            }
        }

        if let Some(ref driver) = config.log_driver {
            args.push("--log-driver".to_string());
            args.push(driver.clone());
        }

        if let Some(ref opts) = config.log_options {
            for (key, value) in opts {
                let opt_str = match value {
                    serde_json::Value::String(s) => format!("{}={}", key, s),
                    serde_json::Value::Number(n) => format!("{}={}", key, n),
                    serde_json::Value::Bool(b) => format!("{}={}", key, b),
                    _ => format!("{}={}", key, value),
                };
                args.push("--log-opt".to_string());
                args.push(opt_str);
            }
        }

        if let Some(ref user) = config.user {
            args.push("--user".to_string());
            args.push(user.clone());
        }

        if let Some(ref workdir) = config.workdir {
            args.push("--workdir".to_string());
            args.push(workdir.clone());
        }

        if let Some(ref period) = config.stop_grace_period {
            args.push("--stop-grace-period".to_string());
            args.push(period.clone());
        }

        if let Some(ref signal) = config.stop_signal {
            args.push("--stop-signal".to_string());
            args.push(signal.clone());
        }

        if config.tty {
            args.push("--tty".to_string());
        }

        if config.stdin {
            args.push("--stdin".to_string());
        }

        if config.read_only {
            args.push("--read-only".to_string());
        }

        if let Some(ref limits) = config.limits {
            if let Some(ref cpu) = limits.cpu {
                args.push("--limit-cpu".to_string());
                args.push(cpu.clone());
            }
            if let Some(ref memory) = limits.memory {
                args.push("--limit-memory".to_string());
                args.push(memory.clone());
            }
        }

        if let Some(ref reservations) = config.reservations {
            if let Some(ref cpu) = reservations.cpu {
                args.push("--reserve-cpu".to_string());
                args.push(cpu.clone());
            }
            if let Some(ref memory) = reservations.memory {
                args.push("--reserve-memory".to_string());
                args.push(memory.clone());
            }
        }

        if let Some(ref healthcheck) = config.healthcheck {
            if let Some(ref test) = healthcheck.test
                && !test.is_empty()
            {
                let test_str = if test[0] == "NONE" {
                    "--no-healthcheck".to_string()
                } else {
                    format!("--health-cmd={}", test.join(" "))
                };
                args.push(test_str);
            }
            if let Some(ref interval) = healthcheck.interval {
                args.push("--health-interval".to_string());
                args.push(interval.clone());
            }
            if let Some(ref timeout) = healthcheck.timeout {
                args.push("--health-timeout".to_string());
                args.push(timeout.clone());
            }
            if let Some(retries) = healthcheck.retries {
                args.push("--health-retries".to_string());
                args.push(retries.to_string());
            }
            if let Some(ref start_period) = healthcheck.start_period {
                args.push("--health-start-period".to_string());
                args.push(start_period.clone());
            }
        }

        let image = config.image.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "image is required when creating a service",
            )
        })?;

        args.push(image.clone());

        if let Some(ref command) = config.command {
            args.push(command.join(" "));
        }

        if let Some(ref args_list) = config.args {
            for arg in args_list {
                args.push(arg.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn update_service(&self, config: &ServiceConfig) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args: Vec<String> = vec!["service".to_string(), "update".to_string()];

        args.push("--replicas".to_string());
        args.push(config.replicas.to_string());

        if let Some(ref image) = config.image {
            args.push("--image".to_string());
            args.push(image.clone());
        }

        if let Some(ref env_list) = config.env {
            for env in env_list {
                args.push("-e".to_string());
                args.push(env.clone());
            }
        }

        if let Some(ref env_dict) = config.env_dict {
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

        if let Some(ref ports) = config.ports {
            for port in ports {
                args.push("-p".to_string());
                args.push(port.clone());
            }
        }

        if let Some(ref restart) = config.restart_condition {
            args.push("--restart-condition".to_string());
            args.push(restart.clone());
        }

        if let Some(ref limits) = config.limits {
            if let Some(ref cpu) = limits.cpu {
                args.push("--limit-cpu".to_string());
                args.push(cpu.clone());
            }
            if let Some(ref memory) = limits.memory {
                args.push("--limit-memory".to_string());
                args.push(memory.clone());
            }
        }

        if let Some(ref reservations) = config.reservations {
            if let Some(ref cpu) = reservations.cpu {
                args.push("--reserve-cpu".to_string());
                args.push(cpu.clone());
            }
            if let Some(ref memory) = reservations.memory {
                args.push("--reserve-memory".to_string());
                args.push(memory.clone());
            }
        }

        args.push(config.name.clone());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn remove_service(&self, name: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        if !self.service_exists(name)? {
            return Ok(false);
        }

        self.exec_cmd(&["service", "rm", name], true)?;
        Ok(true)
    }

    fn scale_service(&self, name: &str, replicas: u32) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        self.exec_cmd(
            &["service", "scale", &format!("{}={}", name, replicas)],
            true,
        )?;
        Ok(true)
    }

    fn get_swarm_state(&self) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        result.insert(
            "active".to_string(),
            serde_json::Value::Bool(self.is_swarm_active()?),
        );

        if let Some(info) = self.get_swarm_info()? {
            result.insert(
                "cluster_id".to_string(),
                serde_json::Value::String(info.cluster_id),
            );
            result.insert(
                "node_id".to_string(),
                serde_json::Value::String(info.node_id),
            );
            result.insert(
                "is_manager".to_string(),
                serde_json::Value::Bool(info.is_manager),
            );
            result.insert(
                "is_leader".to_string(),
                serde_json::Value::Bool(info.is_leader),
            );
            result.insert(
                "availability".to_string(),
                serde_json::Value::String(info.availability),
            );

            if info.is_manager {
                let worker_token = self.get_join_token("worker")?;
                let manager_token = self.get_join_token("manager")?;
                result.insert(
                    "join_token_worker".to_string(),
                    serde_json::Value::String(worker_token),
                );
                result.insert(
                    "join_token_manager".to_string(),
                    serde_json::Value::String(manager_token),
                );
            }
        }

        Ok(result)
    }

    fn get_service_state(&self, name: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_service_info(name)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert("name".to_string(), serde_json::Value::String(info.name));
            result.insert("image".to_string(), serde_json::Value::String(info.image));
            result.insert(
                "replicas".to_string(),
                serde_json::Value::Number(info.replicas.into()),
            );
            result.insert("mode".to_string(), serde_json::Value::String(info.mode));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

impl Module for DockerSwarm {
    fn get_name(&self) -> &str {
        "docker_swarm"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_swarm(parse_params(optional_params)?, check_mode)?,
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

fn docker_swarm(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        SwarmState::Present => {
            let is_active = client.is_swarm_active()?;
            if !is_active {
                client.init_swarm(&params)?;
                diff("swarm: inactive".to_string(), "swarm: active".to_string());
                output_messages.push("Swarm cluster initialized".to_string());
                changed = true;
            } else {
                output_messages.push("Swarm cluster already active".to_string());
            }
        }
        SwarmState::Join => {
            if client.is_swarm_active()? {
                output_messages.push("Already part of a swarm cluster".to_string());
            } else {
                client.join_swarm(&params)?;
                diff("swarm: inactive".to_string(), "swarm: joined".to_string());
                output_messages.push("Joined swarm cluster".to_string());
                changed = true;
            }
        }
        SwarmState::Absent => {
            if client.leave_swarm(params.force)? {
                diff("swarm: active".to_string(), "swarm: inactive".to_string());
                output_messages.push("Left swarm cluster".to_string());
                changed = true;
            } else {
                output_messages.push("Not part of a swarm cluster".to_string());
            }
        }
        SwarmState::Remove => {
            let node_id = params.node_id.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "node_id is required for removing a node",
                )
            })?;

            if client.remove_node(node_id, params.force)? {
                diff(format!("node: {}", node_id), "node: removed".to_string());
                output_messages.push(format!("Node '{}' removed from swarm", node_id));
                changed = true;
            } else {
                output_messages.push(format!("Node '{}' not found", node_id));
            }
        }
        SwarmState::Service => {
            let service_config = params.service.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "service config is required for state=service",
                )
            })?;

            match service_config.service_state {
                ServiceState::Absent => {
                    if client.remove_service(&service_config.name)? {
                        diff(
                            format!("service: {}", service_config.name),
                            "service: absent".to_string(),
                        );
                        output_messages.push(format!("Service '{}' removed", service_config.name));
                        changed = true;
                    } else {
                        output_messages
                            .push(format!("Service '{}' not found", service_config.name));
                    }
                }
                ServiceState::Present | ServiceState::Started => {
                    if !client.is_swarm_active()? {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "Not in a swarm cluster. Initialize or join a swarm first.",
                        ));
                    }

                    let exists = client.service_exists(&service_config.name)?;
                    if exists {
                        if let Some(ref image) = service_config.image {
                            let current_info = client.get_service_info(&service_config.name)?;
                            let needs_update = current_info.as_ref().is_none_or(|info| {
                                info.image != *image || info.replicas != service_config.replicas
                            });

                            if needs_update {
                                client.update_service(service_config)?;
                                diff(
                                    format!("service: {} (old)", service_config.name),
                                    format!("service: {} (updated)", service_config.name),
                                );
                                output_messages
                                    .push(format!("Service '{}' updated", service_config.name));
                                changed = true;
                            } else {
                                output_messages.push(format!(
                                    "Service '{}' already up to date",
                                    service_config.name
                                ));
                            }
                        } else if service_config.replicas != 1 {
                            client.scale_service(&service_config.name, service_config.replicas)?;
                            output_messages.push(format!(
                                "Service '{}' scaled to {} replicas",
                                service_config.name, service_config.replicas
                            ));
                            changed = true;
                        } else {
                            output_messages
                                .push(format!("Service '{}' already exists", service_config.name));
                        }
                    } else {
                        client.create_service(service_config)?;
                        diff(
                            "service: absent".to_string(),
                            format!("service: {}", service_config.name),
                        );
                        output_messages.push(format!("Service '{}' created", service_config.name));
                        changed = true;
                    }
                }
                ServiceState::Paused => {
                    if client.scale_service(&service_config.name, 0)? {
                        output_messages.push(format!(
                            "Service '{}' paused (scaled to 0)",
                            service_config.name
                        ));
                        changed = true;
                    }
                }
            }
        }
        SwarmState::Inspect => {
            output_messages.push("Swarm status inspected".to_string());
        }
    }

    let extra = if params.state == SwarmState::Service {
        if let Some(ref service_config) = params.service {
            client.get_service_state(&service_config.name)?
        } else {
            client.get_swarm_state()?
        }
    } else {
        client.get_swarm_state()?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_init() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            advertise_addr: "192.168.1.10"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Present);
        assert_eq!(params.advertise_addr, Some("192.168.1.10".to_string()));
    }

    #[test]
    fn test_parse_params_join() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: join
            join_token: "SWMTKN-1-xxx"
            remote_addrs:
              - "192.168.1.10:2377"
              - "192.168.1.11:2377"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Join);
        assert_eq!(params.join_token, Some("SWMTKN-1-xxx".to_string()));
        assert_eq!(
            params.remote_addrs,
            Some(vec![
                "192.168.1.10:2377".to_string(),
                "192.168.1.11:2377".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_params_leave() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Absent);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_remove_node() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: remove
            node_id: "node-abc123"
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Remove);
        assert_eq!(params.node_id, Some("node-abc123".to_string()));
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_service_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Service);
        let service = params.service.unwrap();
        assert_eq!(service.name, "myapp");
        assert_eq!(service.image, Some("nginx:latest".to_string()));
        assert_eq!(service.service_state, ServiceState::Present);
        assert_eq!(service.replicas, 1);
    }

    #[test]
    fn test_parse_params_service_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: webapp
              image: nginx:latest
              replicas: 5
              ports:
                - "8080:80"
              env:
                - "DEBUG=true"
              networks:
                - "mynet"
              restart_condition: "on-failure"
              limits:
                cpu: "0.5"
                memory: "512M"
              labels:
                app: "webapp"
                version: "1.0"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Service);
        let service = params.service.unwrap();
        assert_eq!(service.name, "webapp");
        assert_eq!(service.replicas, 5);
        assert_eq!(service.ports, Some(vec!["8080:80".to_string()]));
        assert_eq!(service.env, Some(vec!["DEBUG=true".to_string()]));
        assert_eq!(service.networks, Some(vec!["mynet".to_string()]));
        assert_eq!(service.restart_condition, Some("on-failure".to_string()));
        let limits = service.limits.unwrap();
        assert_eq!(limits.cpu, Some("0.5".to_string()));
        assert_eq!(limits.memory, Some("512M".to_string()));
    }

    #[test]
    fn test_parse_params_service_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: oldapp
              service_state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        assert_eq!(service.name, "oldapp");
        assert_eq!(service.service_state, ServiceState::Absent);
    }

    #[test]
    fn test_parse_params_service_paused() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              service_state: paused
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        assert_eq!(service.service_state, ServiceState::Paused);
    }

    #[test]
    fn test_parse_params_inspect() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: inspect
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Inspect);
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            {}
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, SwarmState::Inspect);
    }

    #[test]
    fn test_parse_params_healthcheck() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
              healthcheck:
                test:
                  - CMD
                  - curl
                  - "-f"
                  - "http://localhost/"
                interval: "30s"
                timeout: "10s"
                retries: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        let healthcheck = service.healthcheck.unwrap();
        assert_eq!(
            healthcheck.test,
            Some(vec![
                "CMD".to_string(),
                "curl".to_string(),
                "-f".to_string(),
                "http://localhost/".to_string()
            ])
        );
        assert_eq!(healthcheck.interval, Some("30s".to_string()));
        assert_eq!(healthcheck.timeout, Some("10s".to_string()));
        assert_eq!(healthcheck.retries, Some(3));
    }

    #[test]
    fn test_parse_params_resources() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
              limits:
                cpu: "1.5"
                memory: "1G"
              reservations:
                cpu: "0.25"
                memory: "256M"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        let limits = service.limits.unwrap();
        assert_eq!(limits.cpu, Some("1.5".to_string()));
        assert_eq!(limits.memory, Some("1G".to_string()));
        let reservations = service.reservations.unwrap();
        assert_eq!(reservations.cpu, Some("0.25".to_string()));
        assert_eq!(reservations.memory, Some("256M".to_string()));
    }

    #[test]
    fn test_parse_params_env_dict() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
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
        let service = params.service.unwrap();
        let env_dict = service.env_dict.unwrap();
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
    fn test_parse_params_update_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
              update_parallelism: 2
              update_delay: "10s"
              update_failure_action: "rollback"
              update_monitor: "5s"
              update_max_failure_ratio: 0.5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        assert_eq!(service.update_parallelism, Some(2));
        assert_eq!(service.update_delay, Some("10s".to_string()));
        assert_eq!(service.update_failure_action, Some("rollback".to_string()));
        assert_eq!(service.update_monitor, Some("5s".to_string()));
        assert_eq!(service.update_max_failure_ratio, Some(0.5));
    }

    #[test]
    fn test_parse_params_placement() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
              constraints:
                - "node.role==manager"
                - "node.labels.region==us-east"
              preferences:
                - "spread=node.labels.zone"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        assert_eq!(
            service.constraints,
            Some(vec![
                "node.role==manager".to_string(),
                "node.labels.region==us-east".to_string()
            ])
        );
        assert_eq!(
            service.preferences,
            Some(vec!["spread=node.labels.zone".to_string()])
        );
    }

    #[test]
    fn test_parse_params_log_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: service
            service:
              name: myapp
              image: nginx:latest
              log_driver: "json-file"
              log_options:
                max-size: "10m"
                max-file: "3"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let service = params.service.unwrap();
        assert_eq!(service.log_driver, Some("json-file".to_string()));
        let log_opts = service.log_options.unwrap();
        assert_eq!(
            log_opts.get("max-size").unwrap(),
            &serde_json::Value::String("10m".to_string())
        );
        assert_eq!(
            log_opts.get("max-file").unwrap(),
            &serde_json::Value::String("3".to_string())
        );
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
