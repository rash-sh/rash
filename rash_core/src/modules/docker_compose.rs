/// ANCHOR: module
/// # docker_compose
///
/// Manage Docker Compose projects for multi-container applications.
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
/// - name: Start a docker-compose project
///   docker_compose:
///     project_src: /app
///     state: started
///
/// - name: Stop a docker-compose project
///   docker_compose:
///     project_src: /app
///     state: stopped
///
/// - name: Remove a docker-compose project
///   docker_compose:
///     project_src: /app
///     state: absent
///
/// - name: Restart a docker-compose project
///   docker_compose:
///     project_src: /app
///     state: restarted
///
/// - name: Start specific services
///   docker_compose:
///     project_src: /app
///     state: started
///     services:
///       - web
///       - db
///
/// - name: Pull images before starting
///   docker_compose:
///     project_src: /app
///     state: started
///     pull: true
///
/// - name: Scale services
///   docker_compose:
///     project_src: /app
///     state: started
///     scale:
///       web: 3
///       worker: 5
///
/// - name: Use a specific compose file
///   docker_compose:
///     project_src: /app
///     files:
///       - docker-compose.yml
///       - docker-compose.prod.yml
///     state: started
///
/// - name: Build images before starting
///   docker_compose:
///     project_src: /app
///     state: started
///     build: true
///
/// - name: Start with a custom project name
///   docker_compose:
///     project_src: /app
///     project_name: myproject
///     state: started
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
    Restarted,
    Started,
    Stopped,
}

fn default_state() -> State {
    State::Started
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the docker-compose project directory.
    project_src: String,
    /// Desired state of the project.
    #[serde(default = "default_state")]
    state: State,
    /// List of specific services to manage.
    #[serde(default)]
    services: Option<Vec<String>>,
    /// Scale mapping for services (e.g., {"web": 3}).
    #[serde(default)]
    scale: Option<serde_json::Map<String, serde_json::Value>>,
    /// Pull images before starting.
    #[serde(default)]
    pull: bool,
    /// Build images before starting.
    #[serde(default)]
    build: bool,
    /// List of compose files to use.
    #[serde(default)]
    files: Option<Vec<String>>,
    /// Custom project name.
    #[serde(default)]
    project_name: Option<String>,
    /// Remove volumes when removing project (state=absent).
    #[serde(default)]
    remove_volumes: bool,
    /// Remove images when removing project (state=absent).
    #[serde(default)]
    remove_images: bool,
    /// Remove orphans (containers not defined in compose file).
    #[serde(default)]
    remove_orphans: bool,
    /// Timeout in seconds for operations.
    #[serde(default)]
    timeout: Option<u32>,
    /// Force recreation of containers.
    #[serde(default)]
    force_recreate: bool,
    /// Do not start linked services.
    #[serde(default)]
    no_deps: bool,
}

#[derive(Debug)]
pub struct DockerCompose;

struct DockerComposeClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct ServiceInfo {
    name: String,
    state: String,
    running: bool,
}

impl Module for DockerCompose {
    fn get_name(&self) -> &str {
        "docker_compose"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_compose(parse_params(optional_params)?, check_mode)?,
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

impl DockerComposeClient {
    fn new(check_mode: bool) -> Self {
        DockerComposeClient { check_mode }
    }

    fn get_base_args(&self, params: &Params) -> Vec<String> {
        let mut args: Vec<String> = vec!["compose".to_string()];

        if let Some(ref files) = params.files {
            for file in files {
                args.push("-f".to_string());
                args.push(file.clone());
            }
        } else {
            let compose_file = Path::new(&params.project_src).join("docker-compose.yml");
            if compose_file.exists() {
                args.push("-f".to_string());
                args.push(compose_file.to_string_lossy().to_string());
            }
        }

        if let Some(ref project_name) = params.project_name {
            args.push("-p".to_string());
            args.push(project_name.clone());
        }

        args.push("--project-directory".to_string());
        args.push(params.project_src.clone());

        args
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
                    "Error executing docker compose: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn project_exists(&self, params: &Params) -> Result<bool> {
        let mut args = self.get_base_args(params);
        args.push("ps".to_string());
        args.push("-q".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        Ok(!output.stdout.is_empty())
    }

    fn get_services_info(&self, params: &Params) -> Result<Vec<ServiceInfo>> {
        let mut args = self.get_base_args(params);
        args.push("ps".to_string());
        args.push("--format".to_string());
        args.push("json".to_string());

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        if !output.status.success() || output.stdout.is_empty() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let services: Vec<ServiceInfo> = stdout
            .lines()
            .filter_map(|line| {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    Some(ServiceInfo {
                        name: json
                            .get("Service")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        state: json
                            .get("State")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        running: json.get("State").and_then(|s| s.as_str()) == Some("running"),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(services)
    }

    fn pull_images(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("pull".to_string());

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn build_images(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("build".to_string());

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn up(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("up".to_string());
        args.push("-d".to_string());

        if params.remove_orphans {
            args.push("--remove-orphans".to_string());
        }

        if params.force_recreate {
            args.push("--force-recreate".to_string());
        }

        if params.no_deps {
            args.push("--no-deps".to_string());
        }

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        if let Some(ref scale) = params.scale {
            for (service, count) in scale {
                let scale_str = match count {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => count.to_string(),
                };
                args.push("--scale".to_string());
                args.push(format!("{}={}", service, scale_str));
            }
        }

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn down(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("down".to_string());

        if params.remove_volumes {
            args.push("--volumes".to_string());
        }

        if params.remove_images {
            args.push("--rmi".to_string());
            args.push("all".to_string());
        }

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn stop(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("stop".to_string());

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn restart(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = self.get_base_args(params);
        args.push("restart".to_string());

        if let Some(timeout) = params.timeout {
            args.push("--timeout".to_string());
            args.push(timeout.to_string());
        }

        if params.no_deps {
            args.push("--no-deps".to_string());
        }

        if let Some(ref services) = params.services {
            for service in services {
                args.push(service.clone());
            }
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn get_project_state(
        &self,
        params: &Params,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        let services = self.get_services_info(params)?;
        if services.is_empty() {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
            result.insert("running".to_string(), serde_json::Value::Bool(false));
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            let all_running = services.iter().all(|s| s.running);
            result.insert("running".to_string(), serde_json::Value::Bool(all_running));

            let services_map: serde_json::Map<String, serde_json::Value> = services
                .iter()
                .map(|s| {
                    let mut service_info = serde_json::Map::new();
                    service_info.insert(
                        "state".to_string(),
                        serde_json::Value::String(s.state.clone()),
                    );
                    service_info.insert("running".to_string(), serde_json::Value::Bool(s.running));
                    (s.name.clone(), serde_json::Value::Object(service_info))
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

fn validate_project_src(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "project_src cannot be empty",
        ));
    }

    if !Path::new(path).exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("project_src '{}' does not exist", path),
        ));
    }

    Ok(())
}

fn docker_compose(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_project_src(&params.project_src)?;

    let client = DockerComposeClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let services_before = client.get_services_info(&params)?;
    let any_running_before = services_before.iter().any(|s| s.running);

    match params.state {
        State::Absent => {
            if client.project_exists(&params)? {
                if client.down(&params)? {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push("Project removed".to_string());
                    changed = true;
                }
            } else {
                output_messages.push("Project already absent".to_string());
            }
        }
        State::Present | State::Started => {
            if params.pull {
                client.pull_images(&params)?;
                output_messages.push("Images pulled".to_string());
            }

            if params.build {
                client.build_images(&params)?;
                output_messages.push("Images built".to_string());
            }

            if client.up(&params)? {
                let services_after = client.get_services_info(&params)?;
                let any_running_after = services_after.iter().any(|s| s.running);

                if !any_running_before && any_running_after {
                    diff("state: stopped".to_string(), "state: started".to_string());
                    output_messages.push("Project started".to_string());
                    changed = true;
                } else if any_running_before && any_running_after {
                    output_messages.push("Project already running".to_string());
                } else if services_before.is_empty() && !services_after.is_empty() {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push("Project created".to_string());
                    changed = true;
                }
            }
        }
        State::Stopped => {
            if client.project_exists(&params)? {
                if any_running_before {
                    client.stop(&params)?;
                    diff("state: started".to_string(), "state: stopped".to_string());
                    output_messages.push("Project stopped".to_string());
                    changed = true;
                } else {
                    output_messages.push("Project already stopped".to_string());
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Project does not exist - cannot stop",
                ));
            }
        }
        State::Restarted => {
            if !client.project_exists(&params)? {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Project does not exist - cannot restart",
                ));
            }
            client.restart(&params)?;
            diff("state: running".to_string(), "state: restarted".to_string());
            output_messages.push("Project restarted".to_string());
            changed = true;
        }
    }

    let extra = client.get_project_state(&params)?;

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
            project_src: /app
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.project_src, "/app");
        assert_eq!(params.state, State::Started);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            project_src: /app
            state: started
            services:
              - web
              - db
            pull: true
            build: true
            scale:
              web: 3
              worker: 5
            files:
              - docker-compose.yml
              - docker-compose.prod.yml
            project_name: myproject
            remove_orphans: true
            timeout: 60
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.project_src, "/app");
        assert_eq!(params.state, State::Started);
        assert_eq!(
            params.services,
            Some(vec!["web".to_string(), "db".to_string()])
        );
        assert!(params.pull);
        assert!(params.build);
        assert!(params.remove_orphans);
        assert_eq!(params.timeout, Some(60));
        assert_eq!(
            params.files,
            Some(vec![
                "docker-compose.yml".to_string(),
                "docker-compose.prod.yml".to_string()
            ])
        );
        assert_eq!(params.project_name, Some("myproject".to_string()));

        let scale = params.scale.unwrap();
        assert_eq!(scale.get("web").unwrap(), &serde_json::json!(3));
        assert_eq!(scale.get("worker").unwrap(), &serde_json::json!(5));
    }

    #[test]
    fn test_parse_params_state_stopped() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            project_src: /app
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
            project_src: /app
            state: absent
            remove_volumes: true
            remove_images: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert!(params.remove_volumes);
        assert!(params.remove_images);
    }

    #[test]
    fn test_parse_params_state_restarted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            project_src: /app
            state: restarted
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Restarted);
    }

    #[test]
    fn test_parse_params_force_recreate() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            project_src: /app
            state: started
            force_recreate: true
            no_deps: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force_recreate);
        assert!(params.no_deps);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            project_src: /app
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_project_src_empty() {
        let error = validate_project_src("").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_project_src_valid() {
        let result = validate_project_src("/tmp");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_project_src_nonexistent() {
        let error = validate_project_src("/nonexistent/path").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
