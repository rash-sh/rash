/// ANCHOR: module
/// # docker_prune
///
/// Prune unused Docker resources (containers, images, volumes, networks, build cache).
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
/// - name: Clean up Docker
///   docker_prune:
///     containers: true
///     images: true
///     volumes: true
///     force: true
///
/// - name: Prune all Docker resources
///   docker_prune:
///     all: true
///
/// - name: Clean stopped containers only
///   docker_prune:
///     containers: true
///
/// - name: Clean build cache
///   docker_prune:
///     builder_cache: true
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

fn default_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Prune stopped containers.
    #[serde(default)]
    containers: bool,
    /// Prune unused images.
    #[serde(default)]
    images: bool,
    /// Prune unused volumes.
    #[serde(default)]
    volumes: bool,
    /// Prune unused networks.
    #[serde(default)]
    networks: bool,
    /// Prune build cache.
    #[serde(default)]
    builder_cache: bool,
    /// Prune all types.
    #[serde(default)]
    all: bool,
    /// Do not prompt for confirmation.
    #[serde(default = "default_true")]
    force: bool,
}

#[derive(Debug)]
pub struct DockerPrune;

impl Module for DockerPrune {
    fn get_name(&self) -> &str {
        "docker_prune"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_prune(parse_params(optional_params)?, check_mode)?,
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

#[derive(Debug, Clone, Default)]
struct PruneResult {
    containers_deleted: u64,
    images_deleted: u64,
    volumes_deleted: u64,
    networks_deleted: u64,
    builder_cache_deleted: u64,
    space_reclaimed: u64,
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

    fn prune_containers(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["container", "prune"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let deleted_count = stdout.lines().count() as u64;
        Ok(PruneResult {
            containers_deleted: deleted_count,
            ..Default::default()
        })
    }

    fn prune_images(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["image", "prune", "--all"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let deleted_count = stdout.lines().filter(|l| l.starts_with("Deleted:")).count() as u64;
        Ok(PruneResult {
            images_deleted: deleted_count,
            ..Default::default()
        })
    }

    fn prune_volumes(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["volume", "prune"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let deleted_count = stdout
            .lines()
            .filter(|l| l.starts_with("Deleted Volume:") || l.contains("deleted"))
            .count() as u64;
        Ok(PruneResult {
            volumes_deleted: deleted_count,
            ..Default::default()
        })
    }

    fn prune_networks(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["network", "prune"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let deleted_count = stdout.lines().count() as u64;
        Ok(PruneResult {
            networks_deleted: deleted_count,
            ..Default::default()
        })
    }

    fn prune_builder_cache(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["builder", "prune"];
        if force {
            args.push("--force");
        }

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let deleted_count = stdout.lines().count() as u64;
        Ok(PruneResult {
            builder_cache_deleted: deleted_count,
            ..Default::default()
        })
    }

    fn prune_all(&self, force: bool) -> Result<PruneResult> {
        if self.check_mode {
            return Ok(PruneResult::default());
        }

        let mut args = vec!["system", "prune", "--all"];
        if force {
            args.push("--force");
        }
        args.push("--volumes");

        let output = self.exec_cmd(&args, true)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut result = PruneResult::default();

        for line in stdout.lines() {
            if line.contains("Total reclaimed space:") {
                let space_str = line
                    .replace("Total reclaimed space:", "")
                    .trim()
                    .to_string();
                result.space_reclaimed = parse_size_to_bytes(&space_str);
            }
        }

        result.containers_deleted = stdout
            .lines()
            .filter(|l| l.contains("deleted") && l.contains("container"))
            .count() as u64;
        result.images_deleted = stdout
            .lines()
            .filter(|l| l.starts_with("Deleted Image:"))
            .count() as u64;
        result.volumes_deleted = stdout
            .lines()
            .filter(|l| l.starts_with("Deleted Volume:"))
            .count() as u64;
        result.networks_deleted = stdout
            .lines()
            .filter(|l| l.starts_with("Deleted Network:"))
            .count() as u64;

        Ok(result)
    }
}

fn parse_size_to_bytes(size_str: &str) -> u64 {
    let size_str = size_str.trim();
    if size_str.is_empty() {
        return 0;
    }

    let (num_part, unit_part) = if size_str.chars().last().map(|c| c.is_ascii_digit()) == Some(true)
    {
        (size_str, "")
    } else {
        let num_end = size_str
            .chars()
            .position(|c| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(size_str.len());
        (&size_str[..num_end], &size_str[num_end..])
    };

    let num: f64 = num_part.parse().unwrap_or(0.0);

    let multiplier: u64 = match unit_part.to_lowercase().as_str() {
        "b" => 1,
        "kb" | "k" => 1024,
        "mb" | "m" => 1024 * 1024,
        "gb" | "g" => 1024 * 1024 * 1024,
        "tb" | "t" => 1024_u64 * 1024 * 1024 * 1024,
        "" => 1,
        _ => 1,
    };

    (num * multiplier as f64) as u64
}

fn docker_prune(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = DockerClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();
    let mut total_result = PruneResult::default();

    if params.all {
        diff(
            "docker: resources present".to_string(),
            "docker: resources pruned".to_string(),
        );
        let result = client.prune_all(params.force)?;
        total_result = result;
        changed = true;
        output_messages.push("All Docker resources pruned".to_string());
    } else {
        let mut any_pruned = false;

        if params.containers {
            diff(
                "containers: stopped".to_string(),
                "containers: pruned".to_string(),
            );
            let result = client.prune_containers(params.force)?;
            total_result.containers_deleted = result.containers_deleted;
            if result.containers_deleted > 0 {
                output_messages.push(format!("Pruned {} containers", result.containers_deleted));
                changed = true;
            }
            any_pruned = true;
        }

        if params.images {
            diff("images: unused".to_string(), "images: pruned".to_string());
            let result = client.prune_images(params.force)?;
            total_result.images_deleted = result.images_deleted;
            if result.images_deleted > 0 {
                output_messages.push(format!("Pruned {} images", result.images_deleted));
                changed = true;
            }
            any_pruned = true;
        }

        if params.volumes {
            diff("volumes: unused".to_string(), "volumes: pruned".to_string());
            let result = client.prune_volumes(params.force)?;
            total_result.volumes_deleted = result.volumes_deleted;
            if result.volumes_deleted > 0 {
                output_messages.push(format!("Pruned {} volumes", result.volumes_deleted));
                changed = true;
            }
            any_pruned = true;
        }

        if params.networks {
            diff(
                "networks: unused".to_string(),
                "networks: pruned".to_string(),
            );
            let result = client.prune_networks(params.force)?;
            total_result.networks_deleted = result.networks_deleted;
            if result.networks_deleted > 0 {
                output_messages.push(format!("Pruned {} networks", result.networks_deleted));
                changed = true;
            }
            any_pruned = true;
        }

        if params.builder_cache {
            diff(
                "builder_cache: unused".to_string(),
                "builder_cache: pruned".to_string(),
            );
            let result = client.prune_builder_cache(params.force)?;
            total_result.builder_cache_deleted = result.builder_cache_deleted;
            if result.builder_cache_deleted > 0 {
                output_messages.push(format!(
                    "Pruned {} builder cache entries",
                    result.builder_cache_deleted
                ));
                changed = true;
            }
            any_pruned = true;
        }

        if !any_pruned {
            output_messages.push("No prune options specified, nothing to do".to_string());
        }
    }

    let extra = serde_json::Map::from_iter([
        (
            "containers_deleted".to_string(),
            serde_json::Value::Number(total_result.containers_deleted.into()),
        ),
        (
            "images_deleted".to_string(),
            serde_json::Value::Number(total_result.images_deleted.into()),
        ),
        (
            "volumes_deleted".to_string(),
            serde_json::Value::Number(total_result.volumes_deleted.into()),
        ),
        (
            "networks_deleted".to_string(),
            serde_json::Value::Number(total_result.networks_deleted.into()),
        ),
        (
            "builder_cache_deleted".to_string(),
            serde_json::Value::Number(total_result.builder_cache_deleted.into()),
        ),
        (
            "space_reclaimed".to_string(),
            serde_json::Value::Number(total_result.space_reclaimed.into()),
        ),
    ]);

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
            containers: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.containers);
        assert!(!params.images);
        assert!(!params.volumes);
        assert!(!params.networks);
        assert!(!params.builder_cache);
        assert!(!params.all);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            all: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.all);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_multiple() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            containers: true
            images: true
            volumes: true
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.containers);
        assert!(params.images);
        assert!(params.volumes);
        assert!(!params.networks);
        assert!(!params.builder_cache);
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_force_false() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            containers: true
            force: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.containers);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            containers: true
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_size_to_bytes() {
        assert_eq!(parse_size_to_bytes("1B"), 1);
        assert_eq!(parse_size_to_bytes("1KB"), 1024);
        assert_eq!(parse_size_to_bytes("1K"), 1024);
        assert_eq!(parse_size_to_bytes("1MB"), 1024 * 1024);
        assert_eq!(parse_size_to_bytes("1M"), 1024 * 1024);
        assert_eq!(parse_size_to_bytes("1GB"), 1024 * 1024 * 1024);
        assert_eq!(parse_size_to_bytes("1G"), 1024 * 1024 * 1024);
        assert_eq!(
            parse_size_to_bytes("2.5GB"),
            (2.5 * 1024.0 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(parse_size_to_bytes("100"), 100);
        assert_eq!(parse_size_to_bytes(""), 0);
        assert_eq!(parse_size_to_bytes("1TB"), 1024 * 1024 * 1024 * 1024);
    }
}
