/// ANCHOR: module
/// # helm
///
/// Manage Helm charts and repositories, the Kubernetes package manager.
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
/// - name: Add a Helm repository
///   helm:
///     repository:
///       name: bitnami
///       url: https://charts.bitnami.com/bitnami
///       state: present
///
/// - name: Update repository
///   helm:
///     repository:
///       name: bitnami
///       state: updated
///
/// - name: Remove a repository
///   helm:
///     repository:
///       name: bitnami
///       state: absent
///
/// - name: Install a chart
///   helm:
///     chart:
///       name: my-nginx
///       chart_ref: bitnami/nginx
///       state: present
///
/// - name: Install a specific version
///   helm:
///     chart:
///       name: my-nginx
///       chart_ref: bitnami/nginx
///       version: "13.2.0"
///       state: present
///
/// - name: Install with custom values
///   helm:
///     chart:
///       name: my-nginx
///       chart_ref: bitnami/nginx
///       values:
///         replicaCount: 2
///         service:
///           type: LoadBalancer
///       state: present
///
/// - name: Install with values from file
///   helm:
///     chart:
///       name: my-nginx
///       chart_ref: bitnami/nginx
///       values_files:
///         - /path/to/values.yaml
///       state: present
///
/// - name: Upgrade a chart
///   helm:
///     chart:
///       name: my-nginx
///       chart_ref: bitnami/nginx
///       state: updated
///
/// - name: Uninstall a chart
///   helm:
///     chart:
///       name: my-nginx
///       state: absent
///
/// - name: List all releases
///   helm:
///     list:
///       all: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::default_false;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
use serde_norway::{Value as YamlValue, value};
use strum_macros::Display;
#[cfg(feature = "docs")]
use strum_macros::EnumString;

fn default_executable() -> Option<String> {
    Some("helm".to_owned())
}

#[derive(Debug, PartialEq, Deserialize, Clone, Display)]
#[cfg_attr(feature = "docs", derive(EnumString, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum RepositoryState {
    Present,
    Absent,
    Updated,
}

#[derive(Debug, PartialEq, Deserialize, Clone, Display)]
#[cfg_attr(feature = "docs", derive(EnumString, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum ChartState {
    Present,
    Absent,
    Updated,
}

fn default_repo_state() -> Option<RepositoryState> {
    Some(RepositoryState::Present)
}

fn default_chart_state() -> Option<ChartState> {
    Some(ChartState::Present)
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct RepositoryParams {
    /// Name of the Helm repository.
    name: String,
    /// URL of the Helm repository (required for state=present).
    url: Option<String>,
    /// Whether the repository should exist, be removed, or updated.
    /// **[default: `"present"`]**
    #[serde(default = "default_repo_state")]
    state: Option<RepositoryState>,
    /// Username for authenticated repository access.
    username: Option<String>,
    /// Password for authenticated repository access.
    password: Option<String>,
    /// Skip TLS certificate verification.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    skip_tls_verify: Option<bool>,
    /// Pass custom CA certificate file.
    ca_file: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct ChartParams {
    /// Release name for the chart.
    name: String,
    /// Chart reference (repository/chart or local path).
    chart_ref: Option<String>,
    /// Chart version to install.
    version: Option<String>,
    /// Kubernetes namespace for the release.
    namespace: Option<String>,
    /// Whether the release should exist, be removed, or updated.
    /// **[default: `"present"`]**
    #[serde(default = "default_chart_state")]
    state: Option<ChartState>,
    /// Values to pass to the chart (key-value pairs).
    values: Option<serde_json::Map<String, serde_json::Value>>,
    /// Path(s) to values files.
    values_files: Option<Vec<String>>,
    /// Set values on the command line (key=value format).
    set: Option<Vec<String>>,
    /// Set string values on the command line.
    set_string: Option<Vec<String>>,
    /// Set values from file.
    set_file: Option<Vec<String>>,
    /// Wait for resources to be ready.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    wait: Option<bool>,
    /// Timeout for wait operation.
    timeout: Option<String>,
    /// Force resource updates through delete/recreate.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    force: Option<bool>,
    /// Create the namespace if it doesn't exist.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    create_namespace: Option<bool>,
    /// Skip CRDs during installation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    skip_crds: Option<bool>,
    /// Atomic installation (rollback on failure).
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    atomic: Option<bool>,
    /// Disable hooks during installation.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    disable_hooks: Option<bool>,
    /// Keep history of deleted releases.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    keep_history: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(deny_unknown_fields)]
struct ListParams {
    /// List releases in specific namespace.
    namespace: Option<String>,
    /// List releases across all namespaces.
    /// **[default: `false`]**
    #[serde(default = "default_false")]
    all: Option<bool>,
    /// Filter releases by name pattern.
    filter: Option<String>,
    /// Maximum number of releases to list.
    limit: Option<u32>,
    /// List releases in specific kubeconfig context.
    kube_context: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path of the helm binary to use.
    /// **[default: `"helm"`]**
    #[serde(default = "default_executable")]
    executable: Option<String>,
    /// Additional options to pass to helm.
    extra_args: Option<String>,
    /// Kubernetes config file path.
    kubeconfig: Option<String>,
    /// Kubernetes context to use.
    kube_context: Option<String>,
    /// Repository management parameters.
    repository: Option<RepositoryParams>,
    /// Chart management parameters.
    chart: Option<ChartParams>,
    /// List releases parameters.
    list: Option<ListParams>,
}

#[derive(Debug)]
pub struct Helm;

impl Module for Helm {
    fn get_name(&self) -> &str {
        "helm"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((helm(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct HelmClient {
    executable: PathBuf,
    extra_args: Option<String>,
    kubeconfig: Option<String>,
    kube_context: Option<String>,
    check_mode: bool,
}

impl HelmClient {
    pub fn new(params: &Params, check_mode: bool) -> Result<Self> {
        Ok(HelmClient {
            executable: PathBuf::from(params.executable.as_ref().unwrap()),
            extra_args: params.extra_args.clone(),
            kubeconfig: params.kubeconfig.clone(),
            kube_context: params.kube_context.clone(),
            check_mode,
        })
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(self.executable.clone());
        if let Some(ref kubeconfig) = self.kubeconfig {
            cmd.env("KUBECONFIG", kubeconfig);
        }
        if let Some(ref context) = self.kube_context {
            cmd.arg("--kube-context").arg(context);
        }
        cmd
    }

    #[inline]
    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        if let Some(ref extra_args) = self.extra_args {
            cmd.args(shell_words_split(extra_args)?);
        };
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable.display()
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(output)
    }

    fn get_repositories(&self) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("repo").arg("list");

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut repos = BTreeSet::new();

        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if !parts.is_empty() {
                repos.insert(parts[0].to_string());
            }
        }

        Ok(repos)
    }

    fn repo_add(&self, params: &RepositoryParams) -> Result<bool> {
        let existing = self.get_repositories()?;
        if existing.contains(&params.name) {
            return Ok(false);
        }

        if self.check_mode {
            return Ok(true);
        }

        let url = params.url.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "url is required when adding a repository",
            )
        })?;

        let mut cmd = self.get_cmd();
        cmd.arg("repo").arg("add").arg(&params.name).arg(url);

        if let Some(ref username) = params.username {
            cmd.arg("--username").arg(username);
        }
        if let Some(ref password) = params.password {
            cmd.arg("--password").arg(password);
        }
        if params.skip_tls_verify.unwrap_or(false) {
            cmd.arg("--insecure-skip-tls-verify");
        }
        if let Some(ref ca_file) = params.ca_file {
            cmd.arg("--ca-file").arg(ca_file);
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    fn repo_remove(&self, params: &RepositoryParams) -> Result<bool> {
        let existing = self.get_repositories()?;
        if !existing.contains(&params.name) {
            return Ok(false);
        }

        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("repo").arg("remove").arg(&params.name);

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    fn repo_update(&self, params: &RepositoryParams) -> Result<bool> {
        let existing = self.get_repositories()?;
        if !existing.contains(&params.name) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Repository '{}' does not exist", params.name),
            ));
        }

        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("repo").arg("update");

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    fn get_releases(&self, namespace: Option<&str>) -> Result<BTreeSet<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--short");

        if let Some(ns) = namespace {
            cmd.arg("-n").arg(ns);
        } else {
            cmd.arg("--all-namespaces");
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(BTreeSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut releases = BTreeSet::new();

        for line in stdout.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                releases.insert(trimmed.to_string());
            }
        }

        Ok(releases)
    }

    fn get_release_info(&self, name: &str, namespace: Option<&str>) -> Result<Option<ReleaseInfo>> {
        let mut cmd = self.get_cmd();
        cmd.arg("status").arg(name).arg("--output").arg("json");

        if let Some(ns) = namespace {
            cmd.arg("-n").arg(ns);
        }

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(Some(ReleaseInfo {
            name: parsed["name"].as_str().unwrap_or(name).to_string(),
            namespace: parsed["namespace"]
                .as_str()
                .unwrap_or("default")
                .to_string(),
            revision: parsed["revision"].as_u64().unwrap_or(0),
            status: parsed["info"]["status"]
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            chart: parsed["chart"]["metadata"]["name"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            chart_version: parsed["chart"]["metadata"]["version"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            app_version: parsed["chart"]["metadata"]["appVersion"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        }))
    }

    fn install_or_upgrade(&self, params: &ChartParams, upgrade: bool) -> Result<bool> {
        let chart_ref = params.chart_ref.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "chart_ref is required for chart operations",
            )
        })?;

        let existing_releases = self.get_releases(params.namespace.as_deref())?;

        if upgrade {
            if !existing_releases.contains(&params.name) {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Release '{}' does not exist", params.name),
                ));
            }
        } else {
            if existing_releases.contains(&params.name) {
                return Ok(false);
            }
        }

        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();

        if upgrade {
            cmd.arg("upgrade");
        } else {
            cmd.arg("install");
        }

        cmd.arg(&params.name).arg(chart_ref);

        if let Some(ref ns) = params.namespace {
            cmd.arg("-n").arg(ns);
        }

        if let Some(ref version) = params.version {
            cmd.arg("--version").arg(version);
        }

        if let Some(ref values) = params.values {
            let values_json =
                serde_json::to_string(values).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            cmd.arg("--set-json").arg(values_json);
        }

        if let Some(ref values_files) = params.values_files {
            for file in values_files {
                cmd.arg("-f").arg(file);
            }
        }

        if let Some(ref set) = params.set {
            for s in set {
                cmd.arg("--set").arg(s);
            }
        }

        if let Some(ref set_string) = params.set_string {
            for s in set_string {
                cmd.arg("--set-string").arg(s);
            }
        }

        if let Some(ref set_file) = params.set_file {
            for s in set_file {
                cmd.arg("--set-file").arg(s);
            }
        }

        if params.wait.unwrap_or(false) {
            cmd.arg("--wait");
        }

        if let Some(ref timeout) = params.timeout {
            cmd.arg("--timeout").arg(timeout);
        }

        if params.force.unwrap_or(false) {
            cmd.arg("--force");
        }

        if params.create_namespace.unwrap_or(false) {
            cmd.arg("--create-namespace");
        }

        if params.skip_crds.unwrap_or(false) {
            cmd.arg("--skip-crds");
        }

        if params.atomic.unwrap_or(false) {
            cmd.arg("--atomic");
        }

        if params.disable_hooks.unwrap_or(false) {
            cmd.arg("--no-hooks");
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    fn uninstall(&self, params: &ChartParams) -> Result<bool> {
        let existing = self.get_releases(params.namespace.as_deref())?;
        if !existing.contains(&params.name) {
            return Ok(false);
        }

        if self.check_mode {
            return Ok(true);
        }

        let mut cmd = self.get_cmd();
        cmd.arg("uninstall").arg(&params.name);

        if let Some(ref ns) = params.namespace {
            cmd.arg("-n").arg(ns);
        }

        if params.keep_history.unwrap_or(false) {
            cmd.arg("--keep-history");
        }

        self.exec_cmd(&mut cmd, true)?;
        Ok(true)
    }

    fn list_releases(&self, params: &ListParams) -> Result<Vec<ReleaseInfo>> {
        let mut cmd = self.get_cmd();
        cmd.arg("list").arg("--output").arg("json");

        if params.all.unwrap_or(false) {
            cmd.arg("--all-namespaces");
        } else if let Some(ref ns) = params.namespace {
            cmd.arg("-n").arg(ns);
        }

        if let Some(ref filter) = params.filter {
            cmd.arg("--filter").arg(filter);
        }

        if let Some(limit) = params.limit {
            cmd.arg("--max").arg(limit.to_string());
        }

        let output = self.exec_cmd(&mut cmd, true)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        let releases = parsed
            .iter()
            .map(|r| ReleaseInfo {
                name: r["name"].as_str().unwrap_or("").to_string(),
                namespace: r["namespace"].as_str().unwrap_or("default").to_string(),
                revision: r["revision"].as_u64().unwrap_or(0),
                status: r["status"].as_str().unwrap_or("unknown").to_string(),
                chart: r["chart"].as_str().unwrap_or("").to_string(),
                chart_version: r["chart_version"].as_str().unwrap_or("").to_string(),
                app_version: r["app_version"].as_str().unwrap_or("").to_string(),
            })
            .collect();

        Ok(releases)
    }
}

#[derive(Debug, Clone)]
struct ReleaseInfo {
    name: String,
    namespace: String,
    revision: u64,
    status: String,
    chart: String,
    chart_version: String,
    app_version: String,
}

fn shell_words_split(s: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for c in s.chars() {
        if in_quotes {
            if c == quote_char {
                in_quotes = false;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quotes = true;
            quote_char = c;
        } else if c == ' ' {
            if !current.is_empty() {
                result.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    Ok(result)
}

fn helm(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let client = HelmClient::new(&params, check_mode)?;

    if let Some(ref repo_params) = params.repository {
        return handle_repository(&client, repo_params);
    }

    if let Some(ref chart_params) = params.chart {
        return handle_chart(&client, chart_params);
    }

    if let Some(ref list_params) = params.list {
        return handle_list(&client, list_params);
    }

    Err(Error::new(
        ErrorKind::InvalidData,
        "One of 'repository', 'chart', or 'list' parameters is required",
    ))
}

fn handle_repository(client: &HelmClient, params: &RepositoryParams) -> Result<ModuleResult> {
    let state = params.state.clone().unwrap_or(RepositoryState::Present);

    let changed = match state {
        RepositoryState::Present => {
            logger::add(std::slice::from_ref(&params.name));
            client.repo_add(params)?
        }
        RepositoryState::Absent => {
            logger::remove(std::slice::from_ref(&params.name));
            client.repo_remove(params)?
        }
        RepositoryState::Updated => client.repo_update(params)?,
    };

    let extra = value::to_value(json!({
        "repository": params.name,
        "state": state.to_string(),
    }))?;

    Ok(ModuleResult {
        changed,
        output: Some(format!(
            "Repository '{}' {}",
            params.name,
            state.to_string().to_lowercase()
        )),
        extra: Some(extra),
    })
}

fn handle_chart(client: &HelmClient, params: &ChartParams) -> Result<ModuleResult> {
    let state = params.state.clone().unwrap_or(ChartState::Present);

    let changed = match state {
        ChartState::Present => {
            logger::add(std::slice::from_ref(&params.name));
            client.install_or_upgrade(params, false)?
        }
        ChartState::Updated => {
            logger::add(std::slice::from_ref(&params.name));
            client.install_or_upgrade(params, true)?
        }
        ChartState::Absent => {
            logger::remove(std::slice::from_ref(&params.name));
            client.uninstall(params)?
        }
    };

    let release_info = client.get_release_info(&params.name, params.namespace.as_deref())?;

    let extra = if let Some(info) = release_info {
        value::to_value(json!({
            "release": {
                "name": info.name,
                "namespace": info.namespace,
                "revision": info.revision,
                "status": info.status,
                "chart": info.chart,
                "chart_version": info.chart_version,
                "app_version": info.app_version,
            },
        }))?
    } else {
        value::to_value(json!({
            "release": {
                "name": params.name,
                "namespace": params.namespace,
                "status": "absent",
            },
        }))?
    };

    Ok(ModuleResult {
        changed,
        output: Some(format!(
            "Release '{}' {}",
            params.name,
            state.to_string().to_lowercase()
        )),
        extra: Some(extra),
    })
}

fn handle_list(client: &HelmClient, params: &ListParams) -> Result<ModuleResult> {
    let releases = client.list_releases(params)?;

    let releases_json: Vec<serde_json::Value> = releases
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "namespace": r.namespace,
                "revision": r.revision,
                "status": r.status,
                "chart": r.chart,
                "chart_version": r.chart_version,
                "app_version": r.app_version,
            })
        })
        .collect();

    let extra = value::to_value(json!({
        "releases": releases_json,
        "total": releases_json.len(),
    }))?;

    Ok(ModuleResult {
        changed: false,
        output: Some(format!("Found {} releases", releases_json.len())),
        extra: Some(extra),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_repo() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository:
              name: bitnami
              url: https://charts.bitnami.com/bitnami
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let repo = params.repository.unwrap();
        assert_eq!(repo.name, "bitnami");
        assert_eq!(
            repo.url,
            Some("https://charts.bitnami.com/bitnami".to_string())
        );
    }

    #[test]
    fn test_parse_params_chart() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        assert_eq!(chart.name, "my-nginx");
        assert_eq!(chart.chart_ref, Some("bitnami/nginx".to_string()));
    }

    #[test]
    fn test_parse_params_chart_with_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              values:
                replicaCount: 2
                service:
                  type: LoadBalancer
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        let values = chart.values.unwrap();
        assert_eq!(values.get("replicaCount").unwrap(), &serde_json::json!(2));
    }

    #[test]
    fn test_parse_params_chart_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              namespace: production
              create_namespace: true
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        assert_eq!(chart.namespace, Some("production".to_string()));
        assert_eq!(chart.create_namespace, Some(true));
    }

    #[test]
    fn test_parse_params_list() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            list:
              all: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.list.unwrap().all, Some(true));
    }

    #[test]
    fn test_parse_params_list_with_namespace() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            list:
              namespace: production
              limit: 10
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let list = params.list.unwrap();
        assert_eq!(list.namespace, Some("production".to_string()));
        assert_eq!(list.limit, Some(10));
    }

    #[test]
    fn test_parse_params_repo_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository:
              name: private
              url: https://charts.example.com
              username: admin
              password: secret
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let repo = params.repository.unwrap();
        assert_eq!(repo.username, Some("admin".to_string()));
        assert_eq!(repo.password, Some("secret".to_string()));
    }

    #[test]
    fn test_parse_params_executable() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: /usr/local/bin/helm
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.executable, Some("/usr/local/bin/helm".to_string()));
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_shell_words_split() {
        let args = shell_words_split("--verbose --wait --timeout 5m").unwrap();
        assert_eq!(args, vec!["--verbose", "--wait", "--timeout", "5m"]);

        let args = shell_words_split("--set \"key=value\" --set-file 'file.txt'").unwrap();
        assert_eq!(args, vec!["--set", "key=value", "--set-file", "file.txt"]);
    }

    #[test]
    fn test_helm_client_new() {
        let params = Params {
            executable: Some("helm".to_string()),
            extra_args: None,
            kubeconfig: None,
            kube_context: None,
            repository: None,
            chart: None,
            list: None,
        };
        let client = HelmClient::new(&params, false).unwrap();
        assert_eq!(client.executable, PathBuf::from("helm"));
        assert!(!client.check_mode);
    }

    #[test]
    fn test_helm_client_new_with_kubeconfig() {
        let params = Params {
            executable: Some("helm".to_string()),
            extra_args: None,
            kubeconfig: Some("/path/to/kubeconfig".to_string()),
            kube_context: Some("my-context".to_string()),
            repository: None,
            chart: None,
            list: None,
        };
        let client = HelmClient::new(&params, false).unwrap();
        assert_eq!(client.kubeconfig, Some("/path/to/kubeconfig".to_string()));
        assert_eq!(client.kube_context, Some("my-context".to_string()));
    }

    #[test]
    fn test_parse_params_repo_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository:
              name: bitnami
              state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.repository.unwrap().state,
            Some(RepositoryState::Absent)
        );
    }

    #[test]
    fn test_parse_params_chart_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chart.unwrap().state, Some(ChartState::Absent));
    }

    #[test]
    fn test_parse_params_chart_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              state: updated
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.chart.unwrap().state, Some(ChartState::Updated));
    }

    #[test]
    fn test_parse_params_chart_with_wait() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              wait: true
              timeout: 5m
              atomic: true
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        assert_eq!(chart.wait, Some(true));
        assert_eq!(chart.timeout, Some("5m".to_string()));
        assert_eq!(chart.atomic, Some(true));
    }

    #[test]
    fn test_parse_params_chart_with_set() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              set:
                - replicaCount=2
                - service.type=LoadBalancer
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        assert_eq!(
            chart.set,
            Some(vec![
                "replicaCount=2".to_string(),
                "service.type=LoadBalancer".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_params_chart_with_values_files() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            chart:
              name: my-nginx
              chart_ref: bitnami/nginx
              values_files:
                - /path/to/values.yaml
                - /path/to/overrides.yaml
              state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let chart = params.chart.unwrap();
        assert_eq!(
            chart.values_files,
            Some(vec![
                "/path/to/values.yaml".to_string(),
                "/path/to/overrides.yaml".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_params_repo_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository:
              name: bitnami
              state: updated
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.repository.unwrap().state,
            Some(RepositoryState::Updated)
        );
    }

    #[test]
    fn test_parse_params_no_operation() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            executable: helm
            "#,
        )
        .unwrap();
        let error = helm(parse_params(yaml).unwrap(), false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_multiple_operations() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            repository:
              name: bitnami
            chart:
              name: my-nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.repository.is_some());
        assert!(params.chart.is_some());
    }
}
