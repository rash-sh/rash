/// ANCHOR: module
/// # helm_info
///
/// Get information about Helm releases in Kubernetes clusters.
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
/// - name: Get release info
///   helm_info:
///     name: myapp
///     namespace: production
///   register: release_info
///
/// - name: List all releases
///   helm_info:
///   register: all_releases
///
/// - name: Get release info with specific context
///   helm_info:
///     name: myapp
///     namespace: production
///     context: minikube
///   register: release_info
///
/// - name: Get release info with specific kubeconfig
///   helm_info:
///     name: myapp
///     kubeconfig: /path/to/kubeconfig
///   register: release_info
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::path::PathBuf;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::Value as YamlValue;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Release name. If omitted, lists all releases.
    name: Option<String>,
    /// Kubernetes namespace.
    /// **[default: `default`]**
    #[serde(default = "default_namespace")]
    namespace: String,
    /// Path to kubeconfig file.
    kubeconfig: Option<String>,
    /// Kubernetes context to use.
    context: Option<String>,
}

fn default_namespace() -> String {
    "default".to_string()
}

#[derive(Debug)]
pub struct HelmInfo;

impl Module for HelmInfo {
    fn get_name(&self) -> &str {
        "helm_info"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((helm_info(parse_params(optional_params)?)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct HelmClient;

impl HelmClient {
    fn exec_cmd_with_kubeconfig(&self, args: &[&str], params: &Params) -> Result<Output> {
        let mut cmd = Command::new("helm");
        cmd.args(args);

        if let Some(ref kubeconfig) = params.kubeconfig {
            cmd.arg("--kubeconfig").arg(kubeconfig);
        }

        if let Some(ref context) = params.context {
            cmd.arg("--kube-context").arg(context);
        }

        cmd.arg("--namespace").arg(&params.namespace);

        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `helm {:?}`", args);
        trace!("{output:?}");
        Ok(output)
    }

    fn helm_available(&self) -> bool {
        Command::new("helm")
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn list_releases(&self, params: &Params) -> Result<Vec<ReleaseInfo>> {
        let output = self.exec_cmd_with_kubeconfig(&["list", "--output", "json"], params)?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to list releases: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_release_list(&stdout)
    }

    fn get_release_status(&self, params: &Params) -> Result<Option<ReleaseInfo>> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "name is required to get release status",
            )
        })?;

        let output =
            self.exec_cmd_with_kubeconfig(&["status", name, "--output", "json"], params)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_release_status(&stdout)
    }

    fn get_release_values(
        &self,
        params: &Params,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "name is required to get release values",
            )
        })?;

        let output = self.exec_cmd_with_kubeconfig(
            &["get", "values", name, "--output", "json", "--all"],
            params,
        )?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let values: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(values.as_object().cloned())
    }

    fn get_release_history(&self, params: &Params) -> Result<Vec<RevisionInfo>> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "name is required to get release history",
            )
        })?;

        let output =
            self.exec_cmd_with_kubeconfig(&["history", name, "--output", "json"], params)?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_release_history(&stdout)
    }
}

#[derive(Debug, Clone)]
struct ReleaseInfo {
    name: String,
    namespace: String,
    revision: u32,
    status: String,
    chart: String,
    chart_version: String,
    app_version: Option<String>,
    updated: Option<String>,
}

#[derive(Debug, Clone)]
struct RevisionInfo {
    revision: u32,
    updated: String,
    status: String,
    chart: String,
    chart_version: String,
    app_version: Option<String>,
    description: Option<String>,
}

fn parse_release_list(json: &str) -> Result<Vec<ReleaseInfo>> {
    let releases: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    releases
        .into_iter()
        .map(|r| {
            let name = r["name"].as_str().unwrap_or("").to_string();
            let namespace = r["namespace"].as_str().unwrap_or("default").to_string();
            let revision = r["revision"].as_u64().unwrap_or(0) as u32;
            let status = r["status"].as_str().unwrap_or("").to_string();
            let chart = r["chart"].as_str().unwrap_or("").to_string();
            let chart_version = r["chart_version"].as_str().unwrap_or("").to_string();
            let app_version = r["app_version"].as_str().map(|s| s.to_string());
            let updated = r["updated"].as_str().map(|s| s.to_string());

            Ok(ReleaseInfo {
                name,
                namespace,
                revision,
                status,
                chart,
                chart_version,
                app_version,
                updated,
            })
        })
        .collect()
}

fn parse_release_status(json: &str) -> Result<Option<ReleaseInfo>> {
    let status: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    let name = status["name"].as_str().unwrap_or("").to_string();
    if name.is_empty() {
        return Ok(None);
    }

    let namespace = status["namespace"]
        .as_str()
        .unwrap_or("default")
        .to_string();
    let revision = status["revision"].as_u64().unwrap_or(0) as u32;
    let status_str = status["info"]["status"].as_str().unwrap_or("").to_string();
    let chart = status["chart"]["metadata"]["name"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let chart_version = status["chart"]["metadata"]["version"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let app_version = status["chart"]["metadata"]["appVersion"]
        .as_str()
        .map(|s| s.to_string());
    let updated = status["info"]["last_deployed"]
        .as_str()
        .map(|s| s.to_string());

    Ok(Some(ReleaseInfo {
        name,
        namespace,
        revision,
        status: status_str,
        chart,
        chart_version,
        app_version,
        updated,
    }))
}

fn parse_release_history(json: &str) -> Result<Vec<RevisionInfo>> {
    let history: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    history
        .into_iter()
        .map(|h| {
            let revision = h["revision"].as_u64().unwrap_or(0) as u32;
            let updated = h["updated"].as_str().unwrap_or("").to_string();
            let status = h["status"].as_str().unwrap_or("").to_string();
            let chart = h["chart"].as_str().unwrap_or("").to_string();
            let chart_version = h["chart_version"].as_str().unwrap_or("").to_string();
            let app_version = h["app_version"].as_str().map(|s| s.to_string());
            let description = h["description"].as_str().map(|s| s.to_string());

            Ok(RevisionInfo {
                revision,
                updated,
                status,
                chart,
                chart_version,
                app_version,
                description,
            })
        })
        .collect()
}

fn validate_kubeconfig(path: &str) -> Result<()> {
    let kubeconfig_path = PathBuf::from(path);
    if !kubeconfig_path.exists() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("kubeconfig file '{}' does not exist", path),
        ));
    }
    Ok(())
}

fn helm_info(params: Params) -> Result<ModuleResult> {
    if let Some(ref kubeconfig) = params.kubeconfig {
        validate_kubeconfig(kubeconfig)?;
    }

    let client = HelmClient;

    if !client.helm_available() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            "helm command not found. Please ensure Helm is installed.",
        ));
    }

    if let Some(ref name) = params.name {
        let release_info = client.get_release_status(&params)?.ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Release '{}' not found in namespace '{}'",
                    name, params.namespace
                ),
            )
        })?;

        let values = client.get_release_values(&params)?;
        let history = client.get_release_history(&params)?;

        let mut extra = serde_json::Map::new();
        extra.insert(
            "name".to_string(),
            serde_json::Value::String(release_info.name),
        );
        extra.insert(
            "namespace".to_string(),
            serde_json::Value::String(release_info.namespace),
        );
        extra.insert(
            "revision".to_string(),
            serde_json::json!(release_info.revision),
        );
        extra.insert(
            "status".to_string(),
            serde_json::Value::String(release_info.status),
        );
        extra.insert(
            "chart".to_string(),
            serde_json::Value::String(release_info.chart),
        );
        extra.insert(
            "chart_version".to_string(),
            serde_json::Value::String(release_info.chart_version),
        );

        if let Some(ref app_version) = release_info.app_version {
            extra.insert(
                "app_version".to_string(),
                serde_json::Value::String(app_version.clone()),
            );
        }

        if let Some(ref updated) = release_info.updated {
            extra.insert(
                "updated".to_string(),
                serde_json::Value::String(updated.clone()),
            );
        }

        if let Some(values_map) = values {
            extra.insert("values".to_string(), serde_json::Value::Object(values_map));
        }

        if !history.is_empty() {
            let history_json: Vec<serde_json::Value> = history
                .iter()
                .map(|h| {
                    let mut map = serde_json::Map::new();
                    map.insert("revision".to_string(), serde_json::json!(h.revision));
                    map.insert(
                        "updated".to_string(),
                        serde_json::Value::String(h.updated.clone()),
                    );
                    map.insert(
                        "status".to_string(),
                        serde_json::Value::String(h.status.clone()),
                    );
                    map.insert(
                        "chart".to_string(),
                        serde_json::Value::String(h.chart.clone()),
                    );
                    map.insert(
                        "chart_version".to_string(),
                        serde_json::Value::String(h.chart_version.clone()),
                    );
                    if let Some(ref app_version) = h.app_version {
                        map.insert(
                            "app_version".to_string(),
                            serde_json::Value::String(app_version.clone()),
                        );
                    }
                    if let Some(ref description) = h.description {
                        map.insert(
                            "description".to_string(),
                            serde_json::Value::String(description.clone()),
                        );
                    }
                    serde_json::Value::Object(map)
                })
                .collect();
            extra.insert(
                "history".to_string(),
                serde_json::Value::Array(history_json),
            );
        }

        let extra_str =
            serde_json::to_string(&extra).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let extra_yaml: YamlValue = serde_norway::from_str(&extra_str)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(ModuleResult::new(false, Some(extra_yaml), None))
    } else {
        let releases = client.list_releases(&params)?;

        let releases_json: Vec<serde_json::Value> = releases
            .iter()
            .map(|r| {
                let mut map = serde_json::Map::new();
                map.insert(
                    "name".to_string(),
                    serde_json::Value::String(r.name.clone()),
                );
                map.insert(
                    "namespace".to_string(),
                    serde_json::Value::String(r.namespace.clone()),
                );
                map.insert("revision".to_string(), serde_json::json!(r.revision));
                map.insert(
                    "status".to_string(),
                    serde_json::Value::String(r.status.clone()),
                );
                map.insert(
                    "chart".to_string(),
                    serde_json::Value::String(r.chart.clone()),
                );
                map.insert(
                    "chart_version".to_string(),
                    serde_json::Value::String(r.chart_version.clone()),
                );
                if let Some(ref app_version) = r.app_version {
                    map.insert(
                        "app_version".to_string(),
                        serde_json::Value::String(app_version.clone()),
                    );
                }
                if let Some(ref updated) = r.updated {
                    map.insert(
                        "updated".to_string(),
                        serde_json::Value::String(updated.clone()),
                    );
                }
                serde_json::Value::Object(map)
            })
            .collect();

        let mut extra = serde_json::Map::new();
        extra.insert(
            "releases".to_string(),
            serde_json::Value::Array(releases_json),
        );

        let extra_str =
            serde_json::to_string(&extra).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        let extra_yaml: YamlValue = serde_norway::from_str(&extra_str)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(ModuleResult::new(false, Some(extra_yaml), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("myapp".to_string()));
        assert_eq!(params.namespace, "default");
        assert_eq!(params.kubeconfig, None);
        assert_eq!(params.context, None);
    }

    #[test]
    fn test_parse_params_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            namespace: production
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, Some("myapp".to_string()));
        assert_eq!(params.namespace, "production");
    }

    #[test]
    fn test_parse_params_with_kubeconfig() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            kubeconfig: /path/to/kubeconfig
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.kubeconfig, Some("/path/to/kubeconfig".to_string()));
    }

    #[test]
    fn test_parse_params_with_context() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: myapp
            context: minikube
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.context, Some("minikube".to_string()));
    }

    #[test]
    fn test_parse_params_list_all() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            namespace: production
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, None);
        assert_eq!(params.namespace, "production");
    }

    #[test]
    fn test_parse_params_empty() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, None);
        assert_eq!(params.namespace, "default");
    }

    #[test]
    fn test_parse_params_invalid_field() {
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
    fn test_parse_release_list() {
        let json = r#"[
            {
                "name": "myapp",
                "namespace": "production",
                "revision": 3,
                "status": "deployed",
                "chart": "nginx",
                "chart_version": "1.0.0",
                "app_version": "1.19.0",
                "updated": "2024-01-01T00:00:00Z"
            }
        ]"#;
        let releases = parse_release_list(json).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].name, "myapp");
        assert_eq!(releases[0].namespace, "production");
        assert_eq!(releases[0].revision, 3);
        assert_eq!(releases[0].status, "deployed");
        assert_eq!(releases[0].chart, "nginx");
        assert_eq!(releases[0].chart_version, "1.0.0");
        assert_eq!(releases[0].app_version, Some("1.19.0".to_string()));
    }

    #[test]
    fn test_parse_release_list_empty() {
        let json = "[]";
        let releases = parse_release_list(json).unwrap();
        assert!(releases.is_empty());
    }

    #[test]
    fn test_parse_release_status() {
        let json = r#"{
            "name": "myapp",
            "namespace": "production",
            "revision": 3,
            "info": {
                "status": "deployed",
                "last_deployed": "2024-01-01T00:00:00Z"
            },
            "chart": {
                "metadata": {
                    "name": "nginx",
                    "version": "1.0.0",
                    "appVersion": "1.19.0"
                }
            }
        }"#;
        let release = parse_release_status(json).unwrap().unwrap();
        assert_eq!(release.name, "myapp");
        assert_eq!(release.namespace, "production");
        assert_eq!(release.revision, 3);
        assert_eq!(release.status, "deployed");
        assert_eq!(release.chart, "nginx");
        assert_eq!(release.chart_version, "1.0.0");
        assert_eq!(release.app_version, Some("1.19.0".to_string()));
    }

    #[test]
    fn test_parse_release_status_empty() {
        let json = r#"{"name": ""}"#;
        let release = parse_release_status(json).unwrap();
        assert!(release.is_none());
    }

    #[test]
    fn test_parse_release_history() {
        let json = r#"[
            {
                "revision": 1,
                "updated": "2024-01-01T00:00:00Z",
                "status": "superseded",
                "chart": "nginx",
                "chart_version": "1.0.0",
                "app_version": "1.19.0",
                "description": "Install complete"
            },
            {
                "revision": 2,
                "updated": "2024-01-02T00:00:00Z",
                "status": "deployed",
                "chart": "nginx",
                "chart_version": "1.1.0",
                "app_version": "1.20.0",
                "description": "Upgrade complete"
            }
        ]"#;
        let history = parse_release_history(json).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].revision, 1);
        assert_eq!(history[0].status, "superseded");
        assert_eq!(history[1].revision, 2);
        assert_eq!(history[1].status, "deployed");
    }

    #[test]
    fn test_parse_release_history_empty() {
        let json = "[]";
        let history = parse_release_history(json).unwrap();
        assert!(history.is_empty());
    }
}
