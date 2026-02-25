/// ANCHOR: module
/// # docker_image
///
/// Manage Docker images.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: partial
/// diff_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Pull an image
///   docker_image:
///     name: nginx:latest
///     state: present
///     source: pull
///
/// - name: Build an image
///   docker_image:
///     name: myapp:latest
///     state: present
///     source: build
///     build:
///       path: ./app
///       dockerfile: Dockerfile
///
/// - name: Tag and push image
///   docker_image:
///     name: myapp:latest
///     state: present
///     source: local
///     repository: registry.example.com/myapp:v1.0
///     push: true
///
/// - name: Remove an image
///   docker_image:
///     name: nginx:old
///     state: absent
///
/// - name: Pull with authentication
///   docker_image:
///     name: registry.example.com/private:latest
///     state: present
///     source: pull
///     username: user
///     password: pass
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BuildParams {
    /// Path to the directory containing the Dockerfile.
    pub path: String,
    /// Alternate Dockerfile name or path relative to build path.
    pub dockerfile: Option<String>,
    /// Build arguments as key-value pairs.
    pub args: Option<HashMap<String, String>>,
    /// Do not use cache when building.
    #[serde(default)]
    pub nocache: bool,
    /// Target build stage for multi-stage builds.
    pub target: Option<String>,
    /// Platform for the build (e.g., linux/amd64).
    pub platform: Option<String>,
    /// Labels to apply to the image.
    pub labels: Option<HashMap<String, String>>,
}

fn default_tag() -> String {
    "latest".to_string()
}

fn default_state() -> String {
    "present".to_string()
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Image name. Can include tag (name:tag) or registry (registry/name:tag).
    pub name: String,
    /// Image tag. Defaults to latest.
    #[serde(default = "default_tag")]
    pub tag: String,
    /// Desired state of the image.
    /// present - ensure image exists (pull, build, or load).
    /// absent - remove the image.
    #[serde(default = "default_state")]
    pub state: String,
    /// Source for the image.
    /// pull - pull from registry.
    /// build - build from Dockerfile.
    /// local - image must already exist locally.
    /// load - load from tar archive.
    pub source: Option<String>,
    /// Repository path to tag the image with. Format: [registry/]name[:tag].
    pub repository: Option<String>,
    /// Push image to registry after tagging.
    #[serde(default)]
    pub push: bool,
    /// Build parameters when source=build.
    pub build: Option<BuildParams>,
    /// Path to tar archive when source=load.
    pub load_path: Option<String>,
    /// Path to save image archive.
    pub archive_path: Option<String>,
    /// Force removal of image (remove all tags).
    #[serde(default)]
    pub force_absent: bool,
    /// Force pulling/building even if image exists.
    #[serde(default)]
    pub force_source: bool,
    /// Force tagging even if tag already exists.
    #[serde(default)]
    pub force_tag: bool,
    /// Registry username for authentication.
    pub username: Option<String>,
    /// Registry password for authentication.
    pub password: Option<String>,
}

fn run_docker_command(
    args: &[&str],
    env: Option<&[(&str, &str)]>,
) -> Result<(bool, String, String)> {
    let mut cmd = Command::new("docker");
    cmd.args(args);

    if let Some(env_vars) = env {
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
    }

    trace!("Running docker command: docker {}", args.join(" "));

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute docker command: {e}"),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    Ok((output.status.success(), stdout, stderr))
}

fn get_image_id(image_name: &str) -> Result<Option<String>> {
    let (success, stdout, _) = run_docker_command(
        &["image", "inspect", "--format", "{{.Id}}", image_name],
        None,
    )?;

    if success && !stdout.is_empty() {
        Ok(Some(stdout))
    } else {
        Ok(None)
    }
}

fn image_exists(name: &str, tag: &str) -> Result<bool> {
    let full_name = if name.contains(':') {
        name.to_string()
    } else {
        format!("{}:{}", name, tag)
    };

    let (success, _, _) = run_docker_command(&["image", "inspect", &full_name], None)?;
    Ok(success)
}

fn pull_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let full_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    if !params.force_source && image_exists(&params.name, &params.tag)? {
        let extra = json!({
            "image": full_name,
            "changed": false,
        });
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Image {} already exists", full_name)),
            extra: Some(value::to_value(extra)?),
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would pull image {}", full_name)),
            extra: None,
        });
    }

    let mut args = vec!["pull", &full_name];

    if let Some(ref build_params) = params.build
        && let Some(ref platform) = build_params.platform
    {
        args = vec!["pull", "--platform", platform, &full_name];
    }

    if let (Some(user), Some(pass)) = (&params.username, &params.password) {
        let registry = full_name.split('/').next().unwrap_or(&full_name);
        let login_args = vec!["login", "-u", user, "--password-stdin", registry];
        let mut cmd = Command::new("docker");
        cmd.args(&login_args);
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to run docker login: {e}"),
            )
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(pass.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write password: {e}"),
                )
            })?;
        }
        let status = child.wait().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for docker login: {e}"),
            )
        })?;
        if !status.success() {
            return Err(Error::new(ErrorKind::SubprocessFail, "Docker login failed"));
        }
    }

    let (success, stdout, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to pull image: {}", stderr),
        ));
    }

    let image_id = get_image_id(&full_name)?;

    let extra = json!({
        "image": full_name,
        "image_id": image_id,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Pulled image {}", full_name)),
        extra: Some(value::to_value(extra)?),
    })
}

fn build_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let full_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    let build_params = params.build.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "build parameters required when source=build",
        )
    })?;

    if !params.force_source && image_exists(&params.name, &params.tag)? {
        let extra = json!({
            "image": full_name,
            "changed": false,
        });
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Image {} already exists", full_name)),
            extra: Some(value::to_value(extra)?),
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would build image {} from {}",
                full_name, build_params.path
            )),
            extra: None,
        });
    }

    let path = Path::new(&build_params.path);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Build path {} does not exist", build_params.path),
        ));
    }

    let mut args: Vec<String> = vec!["build".to_string(), "-t".to_string(), full_name.clone()];

    if let Some(ref dockerfile) = build_params.dockerfile {
        args.push("-f".to_string());
        args.push(dockerfile.clone());
    }

    if build_params.nocache {
        args.push("--no-cache".to_string());
    }

    if let Some(ref target) = build_params.target {
        args.push("--target".to_string());
        args.push(target.clone());
    }

    if let Some(ref platform) = build_params.platform {
        args.push("--platform".to_string());
        args.push(platform.clone());
    }

    if let Some(ref args_map) = build_params.args {
        for (key, value) in args_map {
            args.push("--build-arg".to_string());
            args.push(format!("{}={}", key, value));
        }
    }

    if let Some(ref labels) = build_params.labels {
        for (key, value) in labels {
            args.push("--label".to_string());
            args.push(format!("{}={}", key, value));
        }
    }

    args.push(build_params.path.clone());

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (success, stdout, stderr) = run_docker_command(&args_refs, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to build image: {}", stderr),
        ));
    }

    let image_id = get_image_id(&full_name)?;

    let extra = json!({
        "image": full_name,
        "image_id": image_id,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Built image {}", full_name)),
        extra: Some(value::to_value(extra)?),
    })
}

fn load_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let load_path = params.load_path.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "load_path required when source=load",
        )
    })?;

    let full_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    if !params.force_source && image_exists(&params.name, &params.tag)? {
        let extra = json!({
            "image": full_name,
            "changed": false,
        });
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Image {} already exists", full_name)),
            extra: Some(value::to_value(extra)?),
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would load image from {}", load_path)),
            extra: None,
        });
    }

    let path = Path::new(load_path);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Load path {} does not exist", load_path),
        ));
    }

    let args = vec!["load", "-i", load_path];
    let (success, stdout, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to load image: {}", stderr),
        ));
    }

    let extra = json!({
        "image": full_name,
        "load_path": load_path,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Loaded image from {}", load_path)),
        extra: Some(value::to_value(extra)?),
    })
}

fn tag_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let source_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    let target_name = params
        .repository
        .as_ref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "repository required for tagging"))?;

    let target_full = if target_name.contains(':') {
        target_name.clone()
    } else {
        format!("{}:{}", target_name, params.tag)
    };

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would tag {} as {}", source_name, target_full)),
            extra: None,
        });
    }

    let args = vec!["tag", &source_name, &target_full];
    let (success, _, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to tag image: {}", stderr),
        ));
    }

    let extra = json!({
        "source": source_name,
        "target": target_full,
        "changed": true,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Tagged {} as {}", source_name, target_full)),
        extra: Some(value::to_value(extra)?),
    })
}

fn push_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let image_name = if let Some(ref repo) = params.repository {
        if repo.contains(':') {
            repo.clone()
        } else {
            format!("{}:{}", repo, params.tag)
        }
    } else if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would push image {}", image_name)),
            extra: None,
        });
    }

    let args = vec!["push", &image_name];

    if let (Some(user), Some(pass)) = (&params.username, &params.password) {
        let registry = image_name.split('/').next().unwrap_or(&image_name);
        let login_args = vec!["login", "-u", user, "--password-stdin", registry];
        let mut cmd = Command::new("docker");
        cmd.args(&login_args);
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to run docker login: {e}"),
            )
        })?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(pass.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write password: {e}"),
                )
            })?;
        }
        let status = child.wait().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for docker login: {e}"),
            )
        })?;
        if !status.success() {
            return Err(Error::new(ErrorKind::SubprocessFail, "Docker login failed"));
        }
    }

    let (success, stdout, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to push image: {}", stderr),
        ));
    }

    let extra = json!({
        "image": image_name,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Pushed image {}", image_name)),
        extra: Some(value::to_value(extra)?),
    })
}

fn remove_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let full_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    if !image_exists(&params.name, &params.tag)? {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Image {} does not exist", full_name)),
            extra: None,
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would remove image {}", full_name)),
            extra: None,
        });
    }

    let mut args = vec!["rmi"];
    if params.force_absent {
        args.push("-f");
    }
    args.push(&full_name);

    let (success, stdout, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove image: {}", stderr),
        ));
    }

    let extra = json!({
        "image": full_name,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Removed image {}", full_name)),
        extra: Some(value::to_value(extra)?),
    })
}

fn archive_image(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let archive_path = params.archive_path.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "archive_path required for archiving",
        )
    })?;

    let full_name = if params.name.contains(':') {
        params.name.clone()
    } else {
        format!("{}:{}", params.name, params.tag)
    };

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would archive image {} to {}",
                full_name, archive_path
            )),
            extra: None,
        });
    }

    let args = vec!["save", "-o", archive_path, &full_name];
    let (success, stdout, stderr) = run_docker_command(&args, None)?;

    if !success {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to archive image: {}", stderr),
        ));
    }

    let extra = json!({
        "image": full_name,
        "archive_path": archive_path,
        "changed": true,
        "stdout": stdout,
    });

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Archived image {} to {}", full_name, archive_path)),
        extra: Some(value::to_value(extra)?),
    })
}

fn manage_image(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.state.as_str() {
        "present" => {
            let source = params.source.as_deref().unwrap_or("pull");

            let mut changed = false;
            let mut outputs: Vec<String> = Vec::new();

            match source {
                "pull" => {
                    let result = pull_image(&params, check_mode)?;
                    changed = changed || result.changed;
                    if let Some(o) = result.output {
                        outputs.push(o);
                    }
                }
                "build" => {
                    let result = build_image(&params, check_mode)?;
                    changed = changed || result.changed;
                    if let Some(o) = result.output {
                        outputs.push(o);
                    }
                }
                "load" => {
                    let result = load_image(&params, check_mode)?;
                    changed = changed || result.changed;
                    if let Some(o) = result.output {
                        outputs.push(o);
                    }
                }
                "local" => {
                    if !image_exists(&params.name, &params.tag)? {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Image {} not found locally", params.name),
                        ));
                    }
                    outputs.push(format!("Image {} exists locally", params.name));
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Unknown source: {}", source),
                    ));
                }
            }

            if params.repository.is_some() {
                let result = tag_image(&params, check_mode)?;
                changed = changed || result.changed;
                if let Some(o) = result.output {
                    outputs.push(o);
                }
            }

            if params.push {
                let result = push_image(&params, check_mode)?;
                changed = changed || result.changed;
                if let Some(o) = result.output {
                    outputs.push(o);
                }
            }

            if params.archive_path.is_some() {
                let result = archive_image(&params, check_mode)?;
                changed = changed || result.changed;
                if let Some(o) = result.output {
                    outputs.push(o);
                }
            }

            Ok(ModuleResult {
                changed,
                output: Some(outputs.join("\n")),
                extra: None,
            })
        }
        "absent" => remove_image(&params, check_mode),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unknown state: {}", params.state),
        )),
    }
}

#[derive(Debug)]
pub struct DockerImage;

impl Module for DockerImage {
    fn get_name(&self) -> &str {
        "docker_image"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(params)?;
        Ok((manage_image(params, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_simple() {
        let yaml = r#"
name: "nginx:latest"
state: "present"
source: "pull"
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "nginx:latest");
        assert_eq!(params.state, "present");
        assert_eq!(params.source, Some("pull".to_string()));
    }

    #[test]
    fn test_parse_params_with_build() {
        let yaml = r#"
name: "myapp"
tag: "v1.0"
state: "present"
source: "build"
build:
  path: "./app"
  dockerfile: "Dockerfile.prod"
  args:
    VERSION: "1.0"
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.name, "myapp");
        assert_eq!(params.tag, "v1.0");
        assert!(params.build.is_some());
        let build = params.build.unwrap();
        assert_eq!(build.path, "./app");
        assert_eq!(build.dockerfile, Some("Dockerfile.prod".to_string()));
        assert!(build.args.is_some());
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml = r#"
name: "nginx"
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.tag, "latest");
        assert_eq!(params.state, "present");
        assert!(!params.push);
        assert!(!params.force_absent);
    }

    #[test]
    fn test_parse_params_remove() {
        let yaml = r#"
name: "nginx"
tag: "old"
state: "absent"
force_absent: true
"#;
        let value: YamlValue = serde_norway::from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.state, "absent");
        assert!(params.force_absent);
    }
}
