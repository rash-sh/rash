/// ANCHOR: module
/// # docker_image
///
/// Manage Docker images.
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
/// - name: Pull an image
///   docker_image:
///     name: nginx:latest
///     source: pull
///
/// - name: Build and push image
///   docker_image:
///     name: myapp
///     tag: v1.0
///     source: build
///     build:
///       path: /app
///       dockerfile: Dockerfile
///     push: true
///
/// - name: Build with build args
///   docker_image:
///     name: myapp
///     tag: latest
///     source: build
///     build:
///       path: .
///       args:
///         VERSION: "1.0"
///         DEBUG: "false"
///
/// - name: Tag and push to multiple registries
///   docker_image:
///     name: myapp:v1.0
///     source: local
///     push: true
///     repository: registry.example.com/myapp:v1.0
///
/// - name: Remove an image
///   docker_image:
///     name: myapp:old
///     state: absent
///
/// - name: Load image from tar file
///   docker_image:
///     name: myapp:loaded
///     source: load
///     load_path: /tmp/myapp.tar
///
/// - name: Force rebuild
///   docker_image:
///     name: myapp:latest
///     source: build
///     force_source: true
///     build:
///       path: /app
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

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Source {
    Build,
    Load,
    Pull,
    Local,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
struct BuildOptions {
    /// Build context path.
    #[serde(default = "default_build_path")]
    path: String,
    /// Path to Dockerfile.
    #[serde(default)]
    dockerfile: Option<String>,
    /// Build arguments.
    #[serde(default)]
    args: Option<serde_json::Map<String, serde_json::Value>>,
    /// Target build stage.
    #[serde(default)]
    target: Option<String>,
    /// Always pull base images.
    #[serde(default)]
    pull: bool,
    /// Do not use cache.
    #[serde(default)]
    no_cache: bool,
    /// Labels for the image.
    #[serde(default)]
    labels: Option<serde_json::Map<String, serde_json::Value>>,
    /// Platform for the image (e.g., linux/amd64).
    #[serde(default)]
    platform: Option<String>,
    /// Build-time network mode.
    #[serde(default)]
    network: Option<String>,
}

fn default_build_path() -> String {
    ".".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Image name with optional tag (e.g., nginx:latest).
    name: String,
    /// Tag for the image (appended to name).
    #[serde(default)]
    tag: Option<String>,
    /// Desired state of the image.
    #[serde(default)]
    state: State,
    /// Source of the image (build, load, pull, local).
    #[serde(default)]
    source: Option<Source>,
    /// Build options when source=build.
    #[serde(default)]
    build: Option<BuildOptions>,
    /// Push the image to a registry.
    #[serde(default)]
    push: bool,
    /// Repository to push to (full name including registry).
    #[serde(default)]
    repository: Option<String>,
    /// Path to load image from (for source=load).
    #[serde(default)]
    load_path: Option<String>,
    /// Force rebuild/repull even if image exists.
    #[serde(default)]
    force_source: bool,
    /// Force removal of the image.
    #[serde(default)]
    force: bool,
}

#[derive(Debug)]
pub struct DockerImage;

struct DockerClient {
    check_mode: bool,
}

#[derive(Debug, Clone)]
struct ImageInfo {
    id: String,
    repository: String,
    tag: String,
    size: i64,
}

impl Module for DockerImage {
    fn get_name(&self) -> &str {
        "docker_image"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_image(parse_params(optional_params)?, check_mode)?,
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

    fn image_exists(&self, image: &str) -> Result<bool> {
        let output = self.exec_cmd(&["image", "inspect", "--format", "{{.Id}}", image], false)?;
        Ok(output.status.success())
    }

    fn get_image_info(&self, image: &str) -> Result<Option<ImageInfo>> {
        let output = self.exec_cmd(
            &[
                "image",
                "inspect",
                "--format",
                "{{.Id}}|{{index .RepoTags 0}}|{{.Size}}",
                image,
            ],
            false,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split('|').collect();

        if parts.len() >= 3 {
            let repo_tag = parts[1];
            let (repository, tag) = if let Some(idx) = repo_tag.rfind(':') {
                (&repo_tag[..idx], &repo_tag[idx + 1..])
            } else {
                (repo_tag, "latest")
            };

            let size = parts[2].parse::<i64>().unwrap_or(0);

            Ok(Some(ImageInfo {
                id: parts[0].to_string(),
                repository: repository.to_string(),
                tag: tag.to_string(),
                size,
            }))
        } else {
            Ok(None)
        }
    }

    fn pull_image(&self, image: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["pull", image], true)?;
        Ok(output.status.success())
    }

    fn build_image(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let build_opts = params.build.as_ref();
        let mut args: Vec<String> = vec!["build".to_string()];

        let full_name = get_full_image_name(&params.name, &params.tag);

        args.push("-t".to_string());
        args.push(full_name.clone());

        if let Some(opts) = build_opts {
            if let Some(ref dockerfile) = opts.dockerfile {
                args.push("-f".to_string());
                args.push(dockerfile.clone());
            }

            if let Some(ref args_map) = opts.args {
                for (key, value) in args_map {
                    let arg_str = match value {
                        serde_json::Value::String(s) => format!("{}={}", key, s),
                        serde_json::Value::Number(n) => format!("{}={}", key, n),
                        serde_json::Value::Bool(b) => format!("{}={}", key, b),
                        _ => format!("{}={}", key, value),
                    };
                    args.push("--build-arg".to_string());
                    args.push(arg_str);
                }
            }

            if let Some(ref target) = opts.target {
                args.push("--target".to_string());
                args.push(target.clone());
            }

            if opts.pull {
                args.push("--pull".to_string());
            }

            if opts.no_cache {
                args.push("--no-cache".to_string());
            }

            if let Some(ref labels) = opts.labels {
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

            if let Some(ref platform) = opts.platform {
                args.push("--platform".to_string());
                args.push(platform.clone());
            }

            if let Some(ref network) = opts.network {
                args.push("--network".to_string());
                args.push(network.clone());
            }
        }

        if params.force_source {
            args.push("--no-cache".to_string());
        }

        let path = build_opts
            .map(|o| o.path.clone())
            .unwrap_or_else(default_build_path);
        args.push(path);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;
        Ok(output.status.success())
    }

    fn load_image(&self, path: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = Command::new("docker")
            .args(["load", "-i", path])
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `docker load -i {}`", path);
        trace!("{output:?}");

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error loading image: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(output.status.success())
    }

    fn push_image(&self, image: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["push", image], true)?;
        Ok(output.status.success())
    }

    fn tag_image(&self, source: &str, target: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let output = self.exec_cmd(&["tag", source, target], true)?;
        Ok(output.status.success())
    }

    fn remove_image(&self, image: &str, force: bool) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = vec!["image", "rm"];
        if force {
            args.push("-f");
        }
        args.push(image);

        let output = self.exec_cmd(&args, false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such image") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error removing image: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn get_image_state(&self, image: &str) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        if let Some(info) = self.get_image_info(image)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));
            result.insert("id".to_string(), serde_json::Value::String(info.id));
            result.insert(
                "repository".to_string(),
                serde_json::Value::String(info.repository),
            );
            result.insert("tag".to_string(), serde_json::Value::String(info.tag));
            result.insert(
                "size".to_string(),
                serde_json::Value::Number(info.size.into()),
            );
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn get_full_image_name(name: &str, tag: &Option<String>) -> String {
    match tag {
        Some(t) => {
            if name.contains(':') {
                name.to_string()
            } else {
                format!("{}:{}", name, t)
            }
        }
        None => {
            if name.contains(':') {
                name.to_string()
            } else {
                format!("{}:latest", name)
            }
        }
    }
}

fn validate_image_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name cannot be empty",
        ));
    }

    if name.len() > 256 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Image name too long (max 256 characters)",
        ));
    }

    Ok(())
}

fn docker_image(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_image_name(&params.name)?;

    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    let full_name = get_full_image_name(&params.name, &params.tag);

    match params.state {
        State::Present => {
            let source = params.source.clone().unwrap_or(Source::Pull);
            let exists = client.image_exists(&full_name)?;

            match source {
                Source::Pull => {
                    if !exists || params.force_source {
                        client.pull_image(&full_name)?;
                        diff(
                            format!("image: {} (absent)", full_name),
                            format!("image: {} (present)", full_name),
                        );
                        output_messages.push(format!("Image '{}' pulled", full_name));
                        changed = true;
                    } else {
                        output_messages.push(format!("Image '{}' already exists", full_name));
                    }
                }
                Source::Build => {
                    if !exists || params.force_source {
                        client.build_image(&params)?;
                        diff(
                            format!("image: {} (absent)", full_name),
                            format!("image: {} (built)", full_name),
                        );
                        output_messages.push(format!("Image '{}' built", full_name));
                        changed = true;
                    } else {
                        output_messages.push(format!("Image '{}' already exists", full_name));
                    }
                }
                Source::Load => {
                    let load_path = params.load_path.as_ref().ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidData,
                            "load_path is required when source=load",
                        )
                    })?;

                    if !exists || params.force_source {
                        client.load_image(load_path)?;
                        diff(
                            format!("image: {} (absent)", full_name),
                            format!("image: {} (loaded)", full_name),
                        );
                        output_messages.push(format!("Image loaded from '{}'", load_path));
                        changed = true;
                    } else {
                        output_messages.push(format!("Image '{}' already exists", full_name));
                    }
                }
                Source::Local => {
                    if !exists {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Image '{}' not found locally", full_name),
                        ));
                    }
                    output_messages.push(format!("Image '{}' exists locally", full_name));
                }
            }

            if params.push {
                let push_target = if let Some(ref repo) = params.repository {
                    if !exists || params.force_source {
                        client.tag_image(&full_name, repo)?;
                    }
                    repo.clone()
                } else {
                    full_name.clone()
                };

                diff(
                    format!("image: {} (local)", push_target),
                    format!("image: {} (pushed)", push_target),
                );
                client.push_image(&push_target)?;
                output_messages.push(format!("Image '{}' pushed", push_target));
                changed = true;
            }
        }
        State::Absent => {
            if client.remove_image(&full_name, params.force)? {
                diff(
                    format!("image: {} (present)", full_name),
                    format!("image: {} (absent)", full_name),
                );
                output_messages.push(format!("Image '{}' removed", full_name));
                changed = true;
            } else {
                output_messages.push(format!("Image '{}' not found", full_name));
            }
        }
    }

    let extra = client.get_image_state(&full_name)?;

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
            name: nginx:latest
            source: pull
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "nginx:latest");
        assert_eq!(params.source, Some(Source::Pull));
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_build() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            tag: v1.0
            source: build
            build:
              path: /app
              dockerfile: Dockerfile.prod
              args:
                VERSION: "1.0"
              pull: true
            push: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp");
        assert_eq!(params.tag, Some("v1.0".to_string()));
        assert_eq!(params.source, Some(Source::Build));
        assert!(params.push);

        let build = params.build.unwrap();
        assert_eq!(build.path, "/app");
        assert_eq!(build.dockerfile, Some("Dockerfile.prod".to_string()));
        assert!(build.pull);
        assert_eq!(
            build.args.unwrap().get("VERSION").unwrap(),
            &serde_json::Value::String("1.0".to_string())
        );
    }

    #[test]
    fn test_parse_params_load() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp:loaded
            source: load
            load_path: /tmp/image.tar
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp:loaded");
        assert_eq!(params.source, Some(Source::Load));
        assert_eq!(params.load_path, Some("/tmp/image.tar".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: oldimage:v1
            state: absent
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "oldimage:v1");
        assert_eq!(params.state, State::Absent);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_repository() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp:v1.0
            source: local
            push: true
            repository: registry.example.com/namespace/myapp:v1.0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "myapp:v1.0");
        assert_eq!(params.source, Some(Source::Local));
        assert!(params.push);
        assert_eq!(
            params.repository,
            Some("registry.example.com/namespace/myapp:v1.0".to_string())
        );
    }

    #[test]
    fn test_parse_params_force_source() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp:latest
            source: build
            force_source: true
            build:
              path: /app
              no_cache: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force_source);
        assert!(params.build.as_ref().unwrap().no_cache);
    }

    #[test]
    fn test_parse_params_build_labels() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            source: build
            build:
              path: .
              labels:
                maintainer: "dev@example.com"
                version: "1.0"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let build = params.build.unwrap();
        let labels = build.labels.unwrap();
        assert_eq!(
            labels.get("maintainer").unwrap(),
            &serde_json::Value::String("dev@example.com".to_string())
        );
        assert_eq!(
            labels.get("version").unwrap(),
            &serde_json::Value::String("1.0".to_string())
        );
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
    fn test_get_full_image_name() {
        assert_eq!(
            get_full_image_name("nginx", &Some("latest".to_string())),
            "nginx:latest"
        );
        assert_eq!(
            get_full_image_name("nginx:alpine", &Some("latest".to_string())),
            "nginx:alpine"
        );
        assert_eq!(get_full_image_name("nginx", &None), "nginx:latest");
        assert_eq!(get_full_image_name("nginx:alpine", &None), "nginx:alpine");
        assert_eq!(
            get_full_image_name("registry.io/myapp", &Some("v1".to_string())),
            "registry.io/myapp:v1"
        );
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
