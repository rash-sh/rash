/// ANCHOR: module
/// # kubernetes
///
/// Manage Kubernetes resources declaratively using inline definitions or
/// manifest files. This module uses `kubectl` under the hood and supports
/// server-side apply, resource validation, and idempotent operations.
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
/// - name: Create namespace
///   kubernetes:
///     state: present
///     definition:
///       apiVersion: v1
///       kind: Namespace
///       metadata:
///         name: myapp
///
/// - name: Deploy application
///   kubernetes:
///     state: present
///     definition:
///       apiVersion: apps/v1
///       kind: Deployment
///       metadata:
///         name: myapp
///         namespace: myapp
///       spec:
///         replicas: 3
///         selector:
///           matchLabels:
///             app: myapp
///         template:
///           metadata:
///             labels:
///               app: myapp
///           spec:
///             containers:
///               - name: myapp
///                 image: myapp:latest
///                 ports:
///                   - containerPort: 8080
///
/// - name: Create service
///   kubernetes:
///     state: present
///     definition:
///       apiVersion: v1
///       kind: Service
///       metadata:
///         name: myapp-svc
///         namespace: myapp
///       spec:
///         selector:
///           app: myapp
///         ports:
///           - port: 80
///             targetPort: 8080
///
/// - name: Apply manifest from file
///   kubernetes:
///     state: present
///     src: manifest.yaml
///
/// - name: Delete a resource by kind and name
///   kubernetes:
///     state: absent
///     kind: Deployment
///     name: myapp
///     namespace: myapp
///
/// - name: Delete using inline definition
///   kubernetes:
///     state: absent
///     definition:
///       apiVersion: v1
///       kind: Namespace
///       metadata:
///         name: myapp
///
/// - name: Apply with explicit kubeconfig
///   kubernetes:
///     state: present
///     kubeconfig: /path/to/kubeconfig
///     definition:
///       apiVersion: v1
///       kind: ConfigMap
///       metadata:
///         name: my-config
///         namespace: default
///       data:
///         key: value
///
/// - name: Apply without validation
///   kubernetes:
///     state: present
///     validate: false
///     definition:
///       apiVersion: v1
///       kind: ConfigMap
///       metadata:
///         name: my-config
///       data:
///         key: value
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_state() -> State {
    State::Present
}

fn default_validate() -> Option<bool> {
    Some(true)
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Absent,
    Present,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Desired state of the resource.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: State,
    /// Inline resource definition (YAML/JSON map with apiVersion, kind, metadata, spec).
    #[cfg_attr(feature = "docs", schemars(skip))]
    definition: Option<YamlValue>,
    /// Path to a manifest file to apply or delete.
    src: Option<String>,
    /// Resource kind (e.g., Pod, Deployment, Service, ConfigMap).
    kind: Option<String>,
    /// Resource name (used with kind for deletions without definition).
    name: Option<String>,
    /// Kubernetes namespace.
    namespace: Option<String>,
    /// API version of the resource (e.g., v1, apps/v1).
    api_version: Option<String>,
    /// Path to kubeconfig file.
    kubeconfig: Option<String>,
    /// Kubernetes context to use.
    context: Option<String>,
    /// Kubernetes API server URL.
    host: Option<String>,
    /// Validate resource definition before applying.
    /// **[default: `true`]**
    #[serde(default = "default_validate")]
    validate: Option<bool>,
    /// Wait for the operation to complete.
    /// **[default: `false`]**
    #[serde(default)]
    wait: bool,
    /// Timeout for wait operation (e.g., "60s", "5m").
    wait_timeout: Option<String>,
    /// Force deletion of resources (implies grace-period=0).
    /// **[default: `false`]**
    #[serde(default)]
    force: bool,
    /// Grace period for deletion in seconds.
    grace_period: Option<u32>,
    /// Delete cascade policy (background, foreground, orphan).
    cascade: Option<String>,
    /// Label selector to filter resources.
    selector: Option<String>,
    /// Additional arguments passed to kubectl.
    extra_args: Option<String>,
}

#[derive(Debug)]
pub struct Kubernetes;

struct KubectlRunner {
    kubeconfig: Option<String>,
    context: Option<String>,
    host: Option<String>,
    validate: bool,
    check_mode: bool,
    extra_args: Option<String>,
}

impl KubectlRunner {
    fn new(params: &Params, check_mode: bool) -> Self {
        KubectlRunner {
            kubeconfig: params.kubeconfig.clone(),
            context: params.context.clone(),
            host: params.host.clone(),
            validate: params.validate.unwrap_or(true),
            check_mode,
            extra_args: params.extra_args.clone(),
        }
    }

    fn build_cmd(&self) -> Command {
        let mut cmd = Command::new("kubectl");
        if let Some(ref kubeconfig) = self.kubeconfig {
            cmd.env("KUBECONFIG", kubeconfig);
        }
        cmd
    }

    fn add_global_args(&self, args: &mut Vec<String>) {
        if let Some(ref context) = self.context {
            args.push("--context".to_string());
            args.push(context.clone());
        }
        if !self.validate {
            args.push("--validate=false".to_string());
        }
        if let Some(ref host) = self.host {
            args.push("--server".to_string());
            args.push(host.clone());
        }
        if let Some(ref extra) = self.extra_args {
            for arg in extra.split_whitespace() {
                args.push(arg.to_string());
            }
        }
    }

    fn add_namespace_args(&self, args: &mut Vec<String>, namespace: &Option<String>) {
        if let Some(ns) = namespace {
            args.push("--namespace".to_string());
            args.push(ns.clone());
        }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute 'kubectl': {e}. Ensure kubectl is installed and in PATH."
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn exec_cmd_with_input(&self, args: &[String], input: &str) -> Result<Output> {
        let mut cmd = self.build_cmd();
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to spawn kubectl: {e}"),
            )
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }

        child.wait_with_output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for kubectl: {e}"),
            )
        })
    }

    fn check_output_success(&self, output: &Output) -> Result<()> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("kubectl error: {}", stderr.trim()),
            ));
        }
        Ok(())
    }

    fn apply_definition(
        &self,
        definition: &YamlValue,
        namespace: &Option<String>,
        wait: bool,
        wait_timeout: &Option<String>,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let definition_str = serde_norway::to_string(definition)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        let mut args = vec!["apply".to_string(), "-f".to_string(), "-".to_string()];
        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);

        if wait {
            args.push("--wait".to_string());
        }
        if let Some(timeout) = wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let output = self.exec_cmd_with_input(&args, &definition_str)?;
        self.check_output_success(&output)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("configured")
            || stdout.contains("created")
            || stdout.contains("unchanged")
            || output.status.success())
    }

    fn apply_src(
        &self,
        src: &str,
        namespace: &Option<String>,
        wait: bool,
        wait_timeout: &Option<String>,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        validate_src_path(src)?;

        let mut args = vec!["apply".to_string(), "-f".to_string(), src.to_string()];
        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);

        if wait {
            args.push("--wait".to_string());
        }
        if let Some(timeout) = wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let mut cmd = self.build_cmd();
        cmd.args(&args);
        let output = self.exec_cmd(&mut cmd)?;
        self.check_output_success(&output)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("configured")
            || stdout.contains("created")
            || stdout.contains("unchanged")
            || output.status.success())
    }

    fn delete_definition(
        &self,
        definition: &YamlValue,
        namespace: &Option<String>,
        params: &Params,
    ) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let definition_str = serde_norway::to_string(definition)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        let mut args = vec!["delete".to_string(), "-f".to_string(), "-".to_string()];
        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);
        self.add_delete_flags(&mut args, params);

        let output = self.exec_cmd_with_input(&args, &definition_str)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("NotFound") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("kubectl error: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn delete_src(&self, src: &str, namespace: &Option<String>, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        validate_src_path(src)?;

        let mut args = vec!["delete".to_string(), "-f".to_string(), src.to_string()];
        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);
        self.add_delete_flags(&mut args, params);

        let mut cmd = self.build_cmd();
        cmd.args(&args);
        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("NotFound") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("kubectl error: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn delete_by_kind_name(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let kind = params.kind.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "kind is required when deleting by name",
            )
        })?;
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "name is required when deleting by name",
            )
        })?;

        let mut args = vec!["delete".to_string(), kind.clone(), name.clone()];
        self.add_namespace_args(&mut args, &params.namespace);
        self.add_global_args(&mut args);
        self.add_delete_flags(&mut args, params);

        if let Some(ref selector) = params.selector {
            args.push("--selector".to_string());
            args.push(selector.clone());
        }

        let mut cmd = self.build_cmd();
        cmd.args(&args);
        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("NotFound") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("kubectl error: {}", stderr.trim()),
            ));
        }

        Ok(true)
    }

    fn add_delete_flags(&self, args: &mut Vec<String>, params: &Params) {
        if params.force {
            args.push("--force".to_string());
            args.push("--grace-period=0".to_string());
        } else if let Some(gp) = params.grace_period {
            args.push("--grace-period".to_string());
            args.push(gp.to_string());
        }

        if let Some(ref cascade) = params.cascade {
            args.push("--cascade".to_string());
            args.push(cascade.clone());
        }

        if params.wait {
            args.push("--wait".to_string());
        }

        if let Some(ref timeout) = params.wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }
    }

    fn get_resource_info(
        &self,
        kind: &str,
        name: &str,
        namespace: &Option<String>,
        _api_version: &Option<String>,
    ) -> Result<Option<serde_json::Value>> {
        let mut args = vec![
            "get".to_string(),
            kind.to_string(),
            name.to_string(),
            "-o".to_string(),
            "json".to_string(),
        ];

        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);

        let mut cmd = self.build_cmd();
        cmd.args(&args);
        let output = self.exec_cmd(&mut cmd)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() || stdout.contains("No resources found") {
            return Ok(None);
        }

        let resource: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(Some(resource))
    }

    #[allow(dead_code)]
    fn resource_exists(&self, kind: &str, name: &str, namespace: &Option<String>) -> Result<bool> {
        let mut args = vec!["get".to_string(), kind.to_string(), name.to_string()];
        self.add_namespace_args(&mut args, namespace);
        self.add_global_args(&mut args);

        let mut cmd = self.build_cmd();
        cmd.args(&args);
        let output = self.exec_cmd(&mut cmd)?;

        Ok(output.status.success())
    }
}

fn validate_src_path(src: &str) -> Result<()> {
    let path = PathBuf::from(src);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Manifest file '{}' does not exist", src),
        ));
    }
    Ok(())
}

fn extract_resource_meta(definition: &YamlValue) -> Result<(String, String, Option<String>)> {
    let kind = definition
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "definition must contain 'kind' field",
            )
        })?;

    let name = definition
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "definition metadata must contain 'name' field",
            )
        })?;

    let namespace = definition
        .get("metadata")
        .and_then(|m| m.get("namespace"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());

    Ok((kind.to_string(), name.to_string(), namespace))
}

fn kubernetes(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let runner = KubectlRunner::new(&params, check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            if let Some(ref src) = params.src {
                let applied =
                    runner.apply_src(src, &params.namespace, params.wait, &params.wait_timeout)?;
                if applied {
                    diff("state: absent".to_string(), "state: present".to_string());
                    output_messages.push(format!("Applied manifest '{}'", src));
                    changed = true;
                } else {
                    output_messages.push(format!("Manifest '{}' already applied", src));
                }
            } else if let Some(ref definition) = params.definition {
                let applied = runner.apply_definition(
                    definition,
                    &params.namespace,
                    params.wait,
                    &params.wait_timeout,
                )?;
                if applied {
                    diff("state: absent".to_string(), "state: present".to_string());
                    let (kind, name, _) = extract_resource_meta(definition).unwrap_or_default();
                    if !kind.is_empty() && !name.is_empty() {
                        output_messages.push(format!("Applied {} '{}'", kind.to_lowercase(), name));
                    } else {
                        output_messages.push("Applied inline definition".to_string());
                    }
                    changed = true;
                } else {
                    output_messages.push("Definition already applied".to_string());
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Either 'definition' or 'src' must be provided for state 'present'",
                ));
            }
        }
        State::Absent => {
            if let Some(ref src) = params.src {
                let deleted = runner.delete_src(src, &params.namespace, &params)?;
                if deleted {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Deleted resources from manifest '{}'", src));
                    changed = true;
                } else {
                    output_messages.push(format!("Resources in '{}' already absent", src));
                }
            } else if let Some(ref definition) = params.definition {
                let deleted = runner.delete_definition(definition, &params.namespace, &params)?;
                if deleted {
                    diff("state: present".to_string(), "state: absent".to_string());
                    let (kind, name, _) = extract_resource_meta(definition).unwrap_or_default();
                    if !kind.is_empty() && !name.is_empty() {
                        output_messages.push(format!("Deleted {} '{}'", kind.to_lowercase(), name));
                    } else {
                        output_messages.push("Deleted inline definition".to_string());
                    }
                    changed = true;
                } else {
                    output_messages.push("Definition resource already absent".to_string());
                }
            } else if params.kind.is_some() && params.name.is_some() {
                let deleted = runner.delete_by_kind_name(&params)?;
                if deleted {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!(
                        "Deleted {} '{}'",
                        params.kind.as_deref().unwrap_or(""),
                        params.name.as_deref().unwrap_or("")
                    ));
                    changed = true;
                } else {
                    output_messages.push(format!(
                        "{} '{}' already absent",
                        params.kind.as_deref().unwrap_or(""),
                        params.name.as_deref().unwrap_or("")
                    ));
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Either 'definition', 'src', or 'kind' and 'name' must be provided for state 'absent'",
                ));
            }
        }
    }

    let extra = {
        let kind = params.kind.as_deref().or_else(|| {
            params
                .definition
                .as_ref()
                .and_then(|d| d.get("kind"))
                .and_then(|v| v.as_str())
        });
        let name = params.name.as_deref().or_else(|| {
            params
                .definition
                .as_ref()
                .and_then(|d| d.get("metadata"))
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str())
        });

        if let (Some(k), Some(n)) = (kind, name) {
            if let Ok(Some(resource)) =
                runner.get_resource_info(k, n, &params.namespace, &params.api_version)
            {
                let mut extra_map = serde_json::Map::new();
                extra_map.insert(
                    "kind".to_string(),
                    serde_json::Value::String(k.to_lowercase()),
                );
                extra_map.insert("name".to_string(), serde_json::Value::String(n.to_string()));

                if let Some(metadata) = resource.get("metadata") {
                    if let Some(ns) = metadata.get("namespace") {
                        extra_map.insert("namespace".to_string(), ns.clone());
                    }
                    if let Some(uid) = metadata.get("uid") {
                        extra_map.insert("uid".to_string(), uid.clone());
                    }
                    if let Some(resource_version) = metadata.get("resourceVersion") {
                        extra_map.insert("resource_version".to_string(), resource_version.clone());
                    }
                }

                if let Some(status) = resource.get("status") {
                    extra_map.insert("status".to_string(), status.clone());
                }

                Some(value::to_value(&extra_map)?)
            } else {
                let mut extra_map = serde_json::Map::new();
                extra_map.insert(
                    "kind".to_string(),
                    serde_json::Value::String(k.to_lowercase()),
                );
                extra_map.insert("name".to_string(), serde_json::Value::String(n.to_string()));
                extra_map.insert("exists".to_string(), serde_json::Value::Bool(false));
                Some(value::to_value(&extra_map)?)
            }
        } else {
            None
        }
    };

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult::new(changed, extra, final_output))
}

impl Module for Kubernetes {
    fn get_name(&self) -> &str {
        "kubernetes"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            kubernetes(parse_params(optional_params)?, check_mode)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_definition() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            definition:
              apiVersion: v1
              kind: Namespace
              metadata:
                name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert!(params.definition.is_some());
        let def = params.definition.unwrap();
        assert_eq!(def.get("kind").unwrap().as_str().unwrap(), "Namespace");
    }

    #[test]
    fn test_parse_params_deployment() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            definition:
              apiVersion: apps/v1
              kind: Deployment
              metadata:
                name: myapp
                namespace: myapp
              spec:
                replicas: 3
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        let def = params.definition.unwrap();
        assert_eq!(def.get("kind").unwrap().as_str().unwrap(), "Deployment");
        assert_eq!(
            def.get("spec")
                .unwrap()
                .get("replicas")
                .unwrap()
                .as_i64()
                .unwrap(),
            3
        );
    }

    #[test]
    fn test_parse_params_with_kubeconfig() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            kubeconfig: /path/to/kubeconfig
            context: my-context
            definition:
              apiVersion: v1
              kind: ConfigMap
              metadata:
                name: my-config
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.kubeconfig, Some("/path/to/kubeconfig".to_string()));
        assert_eq!(params.context, Some("my-context".to_string()));
    }

    #[test]
    fn test_parse_params_with_host() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            host: https://k8s-api.example.com:6443
            definition:
              apiVersion: v1
              kind: Namespace
              metadata:
                name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.host,
            Some("https://k8s-api.example.com:6443".to_string())
        );
    }

    #[test]
    fn test_parse_params_validate() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            validate: false
            definition:
              apiVersion: v1
              kind: ConfigMap
              metadata:
                name: my-config
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.validate, Some(false));
    }

    #[test]
    fn test_parse_params_validate_default() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            definition:
              apiVersion: v1
              kind: Namespace
              metadata:
                name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.validate, Some(true));
    }

    #[test]
    fn test_parse_params_src() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: manifest.yaml
            namespace: mynamespace
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.src, Some("manifest.yaml".to_string()));
        assert_eq!(params.namespace, Some("mynamespace".to_string()));
    }

    #[test]
    fn test_parse_params_delete_by_kind_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: Deployment
            name: myapp
            namespace: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.kind, Some("Deployment".to_string()));
        assert_eq!(params.name, Some("myapp".to_string()));
        assert_eq!(params.namespace, Some("myapp".to_string()));
    }

    #[test]
    fn test_parse_params_delete_with_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: Pod
            name: stuck-pod
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
        assert_eq!(params.grace_period, None);
    }

    #[test]
    fn test_parse_params_delete_with_grace_period() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: Pod
            name: my-pod
            grace_period: 30
            cascade: foreground
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.grace_period, Some(30));
        assert_eq!(params.cascade, Some("foreground".to_string()));
    }

    #[test]
    fn test_parse_params_wait() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: manifest.yaml
            wait: true
            wait_timeout: 120s
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.wait);
        assert_eq!(params.wait_timeout, Some("120s".to_string()));
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            definition:
              apiVersion: v1
              kind: Namespace
              metadata:
                name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            invalid_field: value
            src: manifest.yaml
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_selector() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: pod
            name: test
            selector: app=nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selector, Some("app=nginx".to_string()));
    }

    #[test]
    fn test_parse_params_api_version() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            api_version: apps/v1
            definition:
              kind: Deployment
              metadata:
                name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.api_version, Some("apps/v1".to_string()));
    }

    #[test]
    fn test_parse_params_extra_args() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            extra_args: "--dry-run=client --request-timeout=30s"
            definition:
              apiVersion: v1
              kind: ConfigMap
              metadata:
                name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.extra_args,
            Some("--dry-run=client --request-timeout=30s".to_string())
        );
    }

    #[test]
    fn test_parse_params_delete_definition() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            definition:
              apiVersion: v1
              kind: Namespace
              metadata:
                name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert!(params.definition.is_some());
    }

    #[test]
    fn test_parse_params_delete_src() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            src: manifest.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.src, Some("manifest.yaml".to_string()));
    }

    #[test]
    fn test_extract_resource_meta() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: myapp
              namespace: production
            "#,
        )
        .unwrap();
        let (kind, name, ns) = extract_resource_meta(&yaml).unwrap();
        assert_eq!(kind, "Deployment");
        assert_eq!(name, "myapp");
        assert_eq!(ns, Some("production".to_string()));
    }

    #[test]
    fn test_extract_resource_meta_no_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            apiVersion: v1
            kind: Namespace
            metadata:
              name: myapp
            "#,
        )
        .unwrap();
        let (kind, name, ns) = extract_resource_meta(&yaml).unwrap();
        assert_eq!(kind, "Namespace");
        assert_eq!(name, "myapp");
        assert_eq!(ns, None);
    }

    #[test]
    fn test_extract_resource_meta_missing_kind() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            apiVersion: v1
            metadata:
              name: myapp
            "#,
        )
        .unwrap();
        let result = extract_resource_meta(&yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_resource_meta_missing_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            apiVersion: v1
            kind: Pod
            metadata:
              labels:
                app: test
            "#,
        )
        .unwrap();
        let result = extract_resource_meta(&yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_src_path_nonexistent() {
        let error = validate_src_path("/nonexistent/manifest.yaml").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_kubectl_runner_new() {
        let params = Params {
            state: State::Present,
            definition: None,
            src: None,
            kind: None,
            name: None,
            namespace: Some("default".to_string()),
            api_version: None,
            kubeconfig: Some("/path/to/kubeconfig".to_string()),
            context: Some("my-context".to_string()),
            host: None,
            validate: Some(true),
            wait: false,
            wait_timeout: None,
            force: false,
            grace_period: None,
            cascade: None,
            selector: None,
            extra_args: None,
        };
        let runner = KubectlRunner::new(&params, false);
        assert_eq!(runner.kubeconfig, Some("/path/to/kubeconfig".to_string()));
        assert_eq!(runner.context, Some("my-context".to_string()));
        assert!(runner.validate);
        assert!(!runner.check_mode);
    }

    #[test]
    fn test_kubernetes_missing_present_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = kubernetes(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_kubernetes_missing_absent_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = kubernetes(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_complex_definition() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            definition:
              apiVersion: apps/v1
              kind: Deployment
              metadata:
                name: myapp
                namespace: myapp
                labels:
                  app: myapp
                  version: v1
              spec:
                replicas: 3
                selector:
                  matchLabels:
                    app: myapp
                template:
                  metadata:
                    labels:
                      app: myapp
                  spec:
                    containers:
                      - name: myapp
                        image: myapp:latest
                        ports:
                          - containerPort: 8080
                        resources:
                          requests:
                            memory: "64Mi"
                            cpu: "250m"
                          limits:
                            memory: "128Mi"
                            cpu: "500m"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.definition.is_some());
        let def = params.definition.unwrap();
        let containers = def
            .get("spec")
            .unwrap()
            .get("template")
            .unwrap()
            .get("spec")
            .unwrap()
            .get("containers")
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(containers.len(), 1);
        assert_eq!(
            containers[0].get("name").unwrap().as_str().unwrap(),
            "myapp"
        );
    }
}
