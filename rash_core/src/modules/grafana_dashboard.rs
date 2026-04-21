/// ANCHOR: module
/// # grafana_dashboard
///
/// Manage Grafana dashboards (create, update, delete).
///
/// Create, update, or delete Grafana dashboards via the Grafana HTTP API.
/// Supports dashboard JSON definitions, folder assignment, and overwrite
/// behavior. Useful for monitoring infrastructure automation.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Examples
///
/// ```yaml
/// - name: Create a Grafana dashboard
///   grafana_dashboard:
///     name: app-metrics
///     folder: Applications
///     dashboard:
///       title: App Metrics
///       panels: []
///     state: present
///
/// - name: Create dashboard with UID and overwrite
///   grafana_dashboard:
///     name: system-overview
///     uid: sys-overview-01
///     dashboard:
///       title: System Overview
///       panels:
///         - title: CPU Usage
///           type: graph
///     overwrite: true
///     state: present
///
/// - name: Delete a Grafana dashboard
///   grafana_dashboard:
///     name: old-dashboard
///     state: absent
///
/// - name: Create dashboard with custom URL and token
///   grafana_dashboard:
///     name: custom-dashboard
///     url: https://grafana.example.com
///     token: "{{ grafana_api_token }}"
///     dashboard:
///       title: Custom Dashboard
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn default_url() -> String {
    "http://localhost:3000".to_string()
}

fn default_overwrite() -> bool {
    false
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Dashboard name (used for identification and display).
    pub name: String,
    /// The desired state of the dashboard.
    #[serde(default)]
    pub state: State,
    /// Folder name to place the dashboard in.
    pub folder: Option<String>,
    /// Dashboard JSON definition as a dict.
    pub dashboard: Option<JsonValue>,
    /// Whether to overwrite an existing dashboard if it exists.
    #[serde(default = "default_overwrite")]
    pub overwrite: bool,
    /// Dashboard UID (unique identifier).
    pub uid: Option<String>,
    /// Grafana server URL.
    #[serde(default = "default_url")]
    pub url: String,
    /// Grafana API token for authentication.
    pub token: Option<String>,
}

struct GrafanaClient {
    url: String,
    token: Option<String>,
}

impl GrafanaClient {
    fn new(params: &Params) -> Self {
        Self {
            url: params.url.trim_end_matches('/').to_string(),
            token: params.token.clone(),
        }
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder().build().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create HTTP client: {e}"),
            )
        })
    }

    fn add_auth_header(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(ref token) = self.token {
            request.header("Authorization", format!("Bearer {token}"))
        } else {
            request
        }
    }

    fn get_dashboard_by_uid(&self, uid: &str) -> Result<Option<JsonValue>> {
        let url = format!("{}/api/dashboards/uid/{uid}", self.url);
        let client = self.build_client()?;
        let request = self.add_auth_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana returned status {status}: {error_text}"),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Grafana response: {e}"),
            )
        })?;

        Ok(Some(json))
    }

    fn search_dashboard_by_name(&self, name: &str) -> Result<Option<JsonValue>> {
        let encoded = urlencoding::encode(name);
        let url = format!("{}/api/search?query={encoded}", self.url);
        let client = self.build_client()?;
        let request = self.add_auth_header(client.get(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana search request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana returned status {status}: {error_text}"),
            ));
        }

        let results: Vec<JsonValue> = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Grafana search response: {e}"),
            )
        })?;

        for result in results {
            if result.get("title").and_then(|v| v.as_str()) == Some(name) {
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    fn find_dashboard(&self, params: &Params) -> Result<Option<(String, JsonValue)>> {
        if let Some(ref uid) = params.uid {
            match self.get_dashboard_by_uid(uid)? {
                Some(data) => Ok(Some((uid.clone(), data))),
                None => Ok(None),
            }
        } else {
            match self.search_dashboard_by_name(&params.name)? {
                Some(search_result) => {
                    let uid = match search_result.get("uid").and_then(|v| v.as_str()) {
                        Some(uid) if !uid.is_empty() => uid.to_string(),
                        _ => return Ok(None),
                    };
                    match self.get_dashboard_by_uid(&uid)? {
                        Some(data) => Ok(Some((uid, data))),
                        None => Ok(None),
                    }
                }
                None => Ok(None),
            }
        }
    }

    fn create_or_update_dashboard(&self, params: &Params) -> Result<(bool, Option<String>)> {
        let dashboard = params.dashboard.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "dashboard parameter is required when state=present",
            )
        })?;

        let mut dashboard_json = dashboard.clone();
        if let serde_json::Value::Object(ref mut map) = dashboard_json {
            map.entry("title")
                .or_insert_with(|| serde_json::Value::String(params.name.clone()));
            if let Some(ref uid) = params.uid {
                map.entry("uid")
                    .or_insert_with(|| serde_json::Value::String(uid.clone()));
            }
        }

        let mut body = serde_json::json!({
            "dashboard": dashboard_json,
            "overwrite": params.overwrite,
        });

        if let Some(ref uid) = params.uid {
            body["dashboard"]["uid"] = serde_json::Value::String(uid.clone());
        }

        if let Some(ref folder) = params.folder {
            body["folderTitle"] = serde_json::Value::String(folder.clone());
        }

        let url = format!("{}/api/dashboards/db", self.url);
        let client = self.build_client()?;
        let request = self.add_auth_header(client.post(&url).json(&body));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana create/update request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana returned status {status}: {error_text}"),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Grafana response: {e}"),
            )
        })?;

        let new_uid = json.get("uid").and_then(|v| v.as_str()).map(String::from);
        let status_val = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let changed = status_val != "unchanged";

        Ok((changed, new_uid))
    }

    fn delete_dashboard(&self, uid: &str) -> Result<bool> {
        let url = format!("{}/api/dashboards/uid/{uid}", self.url);
        let client = self.build_client()?;
        let request = self.add_auth_header(client.delete(&url));

        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana delete request failed: {e}"),
            )
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Grafana returned status {status}: {error_text}"),
            ));
        }

        Ok(true)
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let dashboard = params.dashboard.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "dashboard parameter is required when state=present",
        )
    })?;

    let client = GrafanaClient::new(params);

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "name": params.name,
                "folder": params.folder,
                "uid": params.uid,
                "overwrite": params.overwrite,
            }))?),
            Some(format!("Would create/update dashboard '{}'", params.name)),
        ));
    }

    if let Some((existing_uid, existing_data)) = client.find_dashboard(params)? {
        let existing_dashboard = existing_data
            .get("dashboard")
            .cloned()
            .unwrap_or(JsonValue::Null);
        let existing_title = existing_dashboard
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut incoming = dashboard.clone();
        if let serde_json::Value::Object(ref mut map) = incoming {
            map.entry("title")
                .or_insert_with(|| serde_json::Value::String(params.name.clone()));
        }

        if existing_title == params.name && incoming == existing_dashboard && !params.overwrite {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "name": params.name,
                    "uid": existing_uid,
                }))?),
                Some(format!("Dashboard '{}' already up to date", params.name)),
            ));
        }
    }

    let (changed, new_uid) = client.create_or_update_dashboard(params)?;

    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(json!({
            "name": params.name,
            "uid": new_uid,
            "folder": params.folder,
        }))?),
        Some(format!(
            "Dashboard '{}' created/updated successfully",
            params.name
        )),
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if check_mode {
        return Ok(ModuleResult::new(
            true,
            Some(value::to_value(json!({
                "name": params.name,
                "uid": params.uid,
            }))?),
            Some(format!("Would delete dashboard '{}'", params.name)),
        ));
    }

    let client = GrafanaClient::new(params);

    match client.find_dashboard(params)? {
        Some((existing_uid, _)) => {
            let deleted = client.delete_dashboard(&existing_uid)?;

            Ok(ModuleResult::new(
                deleted,
                Some(value::to_value(json!({
                    "name": params.name,
                    "uid": existing_uid,
                    "deleted": deleted,
                }))?),
                if deleted {
                    Some(format!("Dashboard '{}' deleted", params.name))
                } else {
                    Some(format!(
                        "Dashboard '{}' not found for deletion",
                        params.name
                    ))
                },
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "name": params.name,
                "found": false,
            }))?),
            Some(format!("Dashboard '{}' not found", params.name)),
        )),
    }
}

pub fn grafana_dashboard(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct GrafanaDashboard;

impl Module for GrafanaDashboard {
    fn get_name(&self) -> &str {
        "grafana_dashboard"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            grafana_dashboard(parse_params(optional_params)?, check_mode)?,
            None,
        ))
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
    fn test_parse_params_present_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: app-metrics
            dashboard:
              title: App Metrics
              panels: []
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "app-metrics");
        assert_eq!(params.state, State::Present);
        assert!(params.dashboard.is_some());
        assert!(!params.overwrite);
        assert!(params.uid.is_none());
        assert!(params.folder.is_none());
    }

    #[test]
    fn test_parse_params_present_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: system-overview
            folder: Applications
            dashboard:
              title: System Overview
              panels:
                - title: CPU Usage
                  type: graph
            overwrite: true
            uid: sys-overview-01
            url: https://grafana.example.com
            token: my-api-token
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "system-overview");
        assert_eq!(params.folder, Some("Applications".to_string()));
        assert!(params.overwrite);
        assert_eq!(params.uid, Some("sys-overview-01".to_string()));
        assert_eq!(params.url, "https://grafana.example.com");
        assert_eq!(params.token, Some("my-api-token".to_string()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-dashboard
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old-dashboard");
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_absent_with_url() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old-dashboard
            url: https://grafana.example.com
            token: my-token
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "old-dashboard");
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.url, "https://grafana.example.com");
        assert_eq!(params.token, Some("my-token".to_string()));
    }

    #[test]
    fn test_parse_params_missing_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dashboard:
              title: Test
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-dashboard
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.url, "http://localhost:3000");
        assert!(!params.overwrite);
        assert_eq!(params.state, State::Present);
        assert!(params.uid.is_none());
        assert!(params.folder.is_none());
        assert!(params.token.is_none());
        assert!(params.dashboard.is_none());
    }

    #[test]
    fn test_check_mode_present() {
        let module = GrafanaDashboard;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-dashboard
            dashboard:
              title: Test Dashboard
            state: present
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would create/update"));
    }

    #[test]
    fn test_check_mode_absent() {
        let module = GrafanaDashboard;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: nonexistent-dashboard
            state: absent
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();

        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would delete"));
    }

    #[test]
    fn test_exec_present_missing_dashboard() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: test-dashboard
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let error = grafana_dashboard(params, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_grafana_client_new_trims_url() {
        let params = Params {
            name: "test".to_string(),
            state: State::Present,
            folder: None,
            dashboard: None,
            overwrite: false,
            uid: None,
            url: "http://localhost:3000/".to_string(),
            token: None,
        };
        let client = GrafanaClient::new(&params);
        assert_eq!(client.url, "http://localhost:3000");
    }

    #[test]
    fn test_parse_params_with_uid_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my-dashboard
            uid: abc123
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.uid, Some("abc123".to_string()));
        assert_eq!(params.state, State::Absent);
    }
}
