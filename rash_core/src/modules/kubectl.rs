/// ANCHOR: module
/// # kubectl
///
/// Manage Kubernetes resources using kubectl.
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
/// - name: Apply a manifest
///   kubectl:
///     state: present
///     src: deployment.yaml
///
/// - name: Apply a manifest with definition
///   kubectl:
///     state: present
///     definition:
///       apiVersion: apps/v1
///       kind: Deployment
///       metadata:
///         name: nginx-deployment
///       spec:
///         replicas: 3
///         selector:
///           matchLabels:
///             app: nginx
///         template:
///           metadata:
///             labels:
///               app: nginx
///           spec:
///             containers:
///               - name: nginx
///                 image: nginx:1.14.2
///                 ports:
///                   - containerPort: 80
///
/// - name: Delete a resource
///   kubectl:
///     state: absent
///     kind: deployment
///     name: nginx-deployment
///
/// - name: Scale a deployment
///   kubectl:
///     state: present
///     kind: deployment
///     name: nginx-deployment
///     replicas: 5
///
/// - name: Delete resources from a manifest
///   kubectl:
///     state: absent
///     src: deployment.yaml
///
/// - name: Apply with namespace
///   kubectl:
///     state: present
///     src: deployment.yaml
///     namespace: mynamespace
///
/// - name: Force delete a pod
///   kubectl:
///     state: absent
///     kind: pod
///     name: stuck-pod
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::fs;
use std::io::Write;
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Desired state of the resource.
    #[serde(default = "default_state")]
    state: State,
    /// Path to a manifest file to apply/delete.
    src: Option<String>,
    /// Inline resource definition (YAML/JSON).
    #[cfg_attr(feature = "docs", schemars(skip))]
    definition: Option<YamlValue>,
    /// Resource kind (deployment, pod, service, etc.).
    kind: Option<String>,
    /// Resource name.
    name: Option<String>,
    /// Kubernetes namespace.
    namespace: Option<String>,
    /// Number of replicas (for scaling deployments).
    replicas: Option<u32>,
    /// Force deletion of resources.
    #[serde(default)]
    force: bool,
    /// Kubernetes context to use.
    context: Option<String>,
    /// Wait for the operation to complete.
    #[serde(default)]
    wait: bool,
    /// Timeout for wait operation (e.g., "60s", "5m").
    wait_timeout: Option<String>,
    /// Delete cascade policy (background, foreground, orphan).
    cascade: Option<String>,
    /// Grace period for deletion (seconds).
    grace_period: Option<u32>,
    /// Label selector to filter resources.
    selector: Option<String>,
}

fn default_state() -> State {
    State::Present
}

#[derive(Debug)]
pub struct Kubectl;

struct KubectlClient {
    check_mode: bool,
}

impl KubectlClient {
    fn new(check_mode: bool) -> Self {
        KubectlClient { check_mode }
    }

    fn exec_cmd(&self, args: &[&str], check_success: bool) -> Result<Output> {
        let output = Command::new("kubectl")
            .args(args)
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `kubectl {:?}`", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing kubectl: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn exec_cmd_with_input(
        &self,
        args: &[&str],
        input: &str,
        check_success: bool,
    ) -> Result<Output> {
        let mut child = Command::new("kubectl")
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

        trace!("command: `kubectl {:?}` with input", args);
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Error executing kubectl: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }
        Ok(output)
    }

    fn add_context_args(&self, args: &mut Vec<String>, context: &Option<String>) {
        if let Some(ctx) = context {
            args.push("--context".to_string());
            args.push(ctx.clone());
        }
    }

    fn add_namespace_args(&self, args: &mut Vec<String>, namespace: &Option<String>) {
        if let Some(ns) = namespace {
            args.push("--namespace".to_string());
            args.push(ns.clone());
        }
    }

    fn apply_manifest(&self, params: &Params, manifest_path: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = vec![
            "apply".to_string(),
            "-f".to_string(),
            manifest_path.to_string(),
        ];
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        if params.wait {
            args.push("--wait".to_string());
        }

        if let Some(ref timeout) = params.wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, true)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("configured")
            || stdout.contains("created")
            || stdout.contains("unchanged")
            || output.status.success())
    }

    fn apply_definition(&self, params: &Params, definition: &YamlValue) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let definition_str = serde_norway::to_string(definition)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        let mut args = vec!["apply".to_string(), "-f".to_string(), "-".to_string()];
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        if params.wait {
            args.push("--wait".to_string());
        }

        if let Some(ref timeout) = params.wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd_with_input(&args_refs, &definition_str, true)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("configured")
            || stdout.contains("created")
            || stdout.contains("unchanged")
            || output.status.success())
    }

    fn delete_manifest(&self, params: &Params, manifest_path: &str) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let mut args = vec![
            "delete".to_string(),
            "-f".to_string(),
            manifest_path.to_string(),
        ];
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        if params.force {
            args.push("--force".to_string());
            args.push("--grace-period=0".to_string());
        } else if let Some(grace_period) = params.grace_period {
            args.push("--grace-period".to_string());
            args.push(grace_period.to_string());
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

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("NotFound") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error deleting resource: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn delete_resource(&self, params: &Params) -> Result<bool> {
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
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        if params.force {
            args.push("--force".to_string());
            args.push("--grace-period=0".to_string());
        } else if let Some(grace_period) = params.grace_period {
            args.push("--grace-period".to_string());
            args.push(grace_period.to_string());
        }

        if let Some(ref cascade) = params.cascade {
            args.push("--cascade".to_string());
            args.push(cascade.clone());
        }

        if let Some(ref selector) = params.selector {
            args.push("--selector".to_string());
            args.push(selector.clone());
        }

        if params.wait {
            args.push("--wait".to_string());
        }

        if let Some(ref timeout) = params.wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("NotFound") || stderr.contains("not found") {
                return Ok(false);
            }
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Error deleting resource: {}", stderr),
            ));
        }

        Ok(true)
    }

    fn scale_deployment(&self, params: &Params) -> Result<bool> {
        if self.check_mode {
            return Ok(true);
        }

        let replicas = params.replicas.ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "replicas is required for scaling")
        })?;

        let kind = params
            .kind
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "kind is required for scaling"))?;
        let name = params
            .name
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "name is required for scaling"))?;

        let mut args = vec![
            "scale".to_string(),
            kind.clone(),
            name.clone(),
            "--replicas".to_string(),
            replicas.to_string(),
        ];
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        if params.wait {
            args.push("--wait".to_string());
        }

        if let Some(ref timeout) = params.wait_timeout {
            args.push("--timeout".to_string());
            args.push(timeout.clone());
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_cmd(&args_refs, true)?;

        Ok(true)
    }

    fn get_resource(&self, params: &Params) -> Result<Option<serde_json::Value>> {
        let kind = params.kind.clone().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "kind is required to get resource")
        })?;

        let mut args = vec![
            "get".to_string(),
            kind.clone(),
            "-o".to_string(),
            "json".to_string(),
        ];

        if let Some(ref name) = params.name {
            args.push(name.clone());
        }

        if let Some(ref selector) = params.selector {
            args.push("--selector".to_string());
            args.push(selector.clone());
        }

        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim() == "" || stdout.contains("No resources found") {
            return Ok(None);
        }

        let resource: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(Some(resource))
    }

    fn get_current_replicas(&self, params: &Params) -> Result<Option<u32>> {
        let resource = self.get_resource(params)?;

        if let Some(json) = resource
            && let Some(spec) = json.get("spec")
            && let Some(replicas) = spec.get("replicas")
        {
            return Ok(Some(replicas.as_u64().unwrap_or(0) as u32));
        }

        Ok(None)
    }

    #[allow(dead_code)]
    fn resource_exists(&self, params: &Params) -> Result<bool> {
        if params.kind.is_none() || params.name.is_none() {
            return Ok(false);
        }

        let mut args = vec!["get".to_string()];
        args.push(params.kind.clone().unwrap());
        args.push(params.name.clone().unwrap());
        self.add_context_args(&mut args, &params.context);
        self.add_namespace_args(&mut args, &params.namespace);

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.exec_cmd(&args_refs, false)?;

        Ok(output.status.success())
    }

    fn get_resource_state(
        &self,
        params: &Params,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut result = serde_json::Map::new();

        result.insert(
            "kind".to_string(),
            serde_json::Value::String(params.kind.clone().unwrap_or_default()),
        );
        result.insert(
            "name".to_string(),
            serde_json::Value::String(params.name.clone().unwrap_or_default()),
        );

        if let Some(resource) = self.get_resource(params)? {
            result.insert("exists".to_string(), serde_json::Value::Bool(true));

            if let Some(status) = resource.get("status") {
                result.insert("status".to_string(), status.clone());
            }

            if let Some(spec) = resource.get("spec")
                && let Some(replicas) = spec.get("replicas")
            {
                result.insert("replicas".to_string(), replicas.clone());
            }

            if let Some(metadata) = resource.get("metadata") {
                if let Some(name) = metadata.get("name") {
                    result.insert("name".to_string(), name.clone());
                }
                if let Some(namespace) = metadata.get("namespace") {
                    result.insert("namespace".to_string(), namespace.clone());
                }
                if let Some(uid) = metadata.get("uid") {
                    result.insert("uid".to_string(), uid.clone());
                }
            }
        } else {
            result.insert("exists".to_string(), serde_json::Value::Bool(false));
        }

        Ok(result)
    }
}

fn validate_src_path(src: &str) -> Result<()> {
    if !fs::metadata(src).map(|m| m.is_file()).unwrap_or(false) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Manifest file '{}' does not exist or is not a file", src),
        ));
    }
    Ok(())
}

fn kubectl(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = KubectlClient::new(check_mode);
    let mut changed = false;
    let mut output_messages = Vec::new();

    match params.state {
        State::Present => {
            if let Some(replicas_val) = params.replicas {
                if let Some(current_replicas) = client.get_current_replicas(&params)? {
                    if current_replicas != replicas_val {
                        diff(
                            format!("replicas: {}", current_replicas),
                            format!("replicas: {}", replicas_val),
                        );
                        client.scale_deployment(&params)?;
                        output_messages.push(format!(
                            "Scaled {} '{}' from {} to {} replicas",
                            params.kind.as_deref().unwrap_or(""),
                            params.name.as_deref().unwrap_or(""),
                            current_replicas,
                            replicas_val
                        ));
                        changed = true;
                    } else {
                        output_messages.push(format!(
                            "{} '{}' already has {} replicas",
                            params.kind.as_deref().unwrap_or(""),
                            params.name.as_deref().unwrap_or(""),
                            current_replicas
                        ));
                    }
                } else {
                    client.scale_deployment(&params)?;
                    diff(
                        "replicas: unknown".to_string(),
                        format!("replicas: {}", replicas_val),
                    );
                    output_messages.push(format!(
                        "Scaled {} '{}' to {} replicas",
                        params.kind.as_deref().unwrap_or(""),
                        params.name.as_deref().unwrap_or(""),
                        replicas_val
                    ));
                    changed = true;
                }
            } else if let Some(ref src) = params.src {
                validate_src_path(src)?;
                client.apply_manifest(&params, src)?;
                diff("state: absent".to_string(), "state: present".to_string());
                output_messages.push(format!("Applied manifest '{}'", src));
                changed = true;
            } else if let Some(ref definition) = params.definition {
                client.apply_definition(&params, definition)?;
                diff("state: absent".to_string(), "state: present".to_string());
                output_messages.push("Applied inline definition".to_string());
                changed = true;
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Either src, definition, or replicas must be provided for state 'present'",
                ));
            }
        }
        State::Absent => {
            if let Some(ref src) = params.src {
                validate_src_path(src)?;
                if client.delete_manifest(&params, src)? {
                    diff("state: present".to_string(), "state: absent".to_string());
                    output_messages.push(format!("Deleted resources from manifest '{}'", src));
                    changed = true;
                } else {
                    output_messages.push(format!("Resources in manifest '{}' already absent", src));
                }
            } else if params.kind.is_some() && params.name.is_some() {
                if client.delete_resource(&params)? {
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
                    "Either src, or kind and name must be provided for state 'absent'",
                ));
            }
        }
    }

    let extra = if params.kind.is_some() && params.name.is_some() {
        Some(client.get_resource_state(&params)?)
    } else {
        None
    };

    let final_output = if output_messages.is_empty() {
        None
    } else {
        Some(output_messages.join("\n"))
    };

    Ok(ModuleResult::new(
        changed,
        extra.map(|e| value::to_value(e).unwrap()),
        final_output,
    ))
}

impl Module for Kubectl {
    fn get_name(&self) -> &str {
        "kubectl"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((kubectl(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: deployment.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.src, Some("deployment.yaml".to_string()));
    }

    #[test]
    fn test_parse_params_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: deployment.yaml
            namespace: mynamespace
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.namespace, Some("mynamespace".to_string()));
    }

    #[test]
    fn test_parse_params_scale() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            kind: deployment
            name: nginx-deployment
            replicas: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.kind, Some("deployment".to_string()));
        assert_eq!(params.name, Some("nginx-deployment".to_string()));
        assert_eq!(params.replicas, Some(5));
    }

    #[test]
    fn test_parse_params_delete() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: deployment
            name: nginx-deployment
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.kind, Some("deployment".to_string()));
        assert_eq!(params.name, Some("nginx-deployment".to_string()));
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_definition() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            definition:
              apiVersion: v1
              kind: Pod
              metadata:
                name: test-pod
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.definition.is_some());
        let def = params.definition.unwrap();
        assert_eq!(def.get("kind").unwrap().as_str().unwrap(), "Pod");
    }

    #[test]
    fn test_parse_params_wait() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: deployment.yaml
            wait: true
            wait_timeout: 60s
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.wait);
        assert_eq!(params.wait_timeout, Some("60s".to_string()));
    }

    #[test]
    fn test_parse_params_cascade() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: deployment
            name: nginx-deployment
            cascade: foreground
            grace_period: 30
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cascade, Some("foreground".to_string()));
        assert_eq!(params.grace_period, Some(30));
    }

    #[test]
    fn test_parse_params_selector() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            kind: pod
            selector: app=nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.selector, Some("app=nginx".to_string()));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: deployment.yaml
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_required_for_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = kubectl(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_missing_required_for_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = kubectl(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_src_path_nonexistent() {
        let error = validate_src_path("/nonexistent/file.yaml").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            src: deployment.yaml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_context() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            src: deployment.yaml
            context: my-context
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.context, Some("my-context".to_string()));
    }
}
