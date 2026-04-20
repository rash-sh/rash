/// ANCHOR: module
/// # grafana
///
/// Manage Grafana dashboards, datasources, folders, and organizations.
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
/// - name: Add a Prometheus datasource
///   grafana:
///     action: add
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     datasource:
///       name: Prometheus
///       type: prometheus
///       url: http://prometheus:9090
///
/// - name: Get a datasource by name
///   grafana:
///     action: get
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     datasource:
///       name: Prometheus
///   register: ds_info
///
/// - name: Update a datasource
///   grafana:
///     action: update
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     datasource:
///       name: Prometheus
///       type: prometheus
///       url: http://prometheus-new:9090
///       access: proxy
///
/// - name: Remove a datasource
///   grafana:
///     action: remove
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     datasource:
///       name: Prometheus
///
/// - name: Add a dashboard
///   grafana:
///     action: add
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     dashboard:
///       title: My Dashboard
///       uid: my-dashboard
///       panels:
///         - title: CPU Usage
///           type: graph
///           datasource: Prometheus
///
/// - name: Get a dashboard
///   grafana:
///     action: get
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     dashboard:
///       uid: my-dashboard
///   register: dashboard_info
///
/// - name: Remove a dashboard
///   grafana:
///     action: remove
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     dashboard:
///       uid: my-dashboard
///
/// - name: Add a folder
///   grafana:
///     action: add
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     folder:
///       title: My Folder
///       uid: my-folder
///
/// - name: Remove a folder
///   grafana:
///     action: remove
///     url: http://grafana:3000
///     api_key: "{{ grafana_api_key }}"
///     folder:
///       uid: my-folder
///
/// - name: Add an organization
///   grafana:
///     action: add
///     url: http://grafana:3000
///     username: admin
///     password: admin
///     org:
///       name: Engineering
///
/// - name: Get all organizations
///   grafana:
///     action: get
///     url: http://grafana:3000
///     username: admin
///     password: admin
///     org: {}
///   register: orgs
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::time::Duration;

use minijinja::Value;
use reqwest::blocking::Client;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Get,
    Add,
    Remove,
    Update,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    #[serde(default)]
    pub action: Action,
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub dashboard: Option<JsonValue>,
    pub datasource: Option<JsonValue>,
    pub folder: Option<JsonValue>,
    pub org: Option<JsonValue>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_timeout() -> u64 {
    30
}

fn default_validate_certs() -> bool {
    true
}

fn default_url() -> String {
    "http://localhost:3000".to_string()
}

fn resolve_url(params: &Params) -> String {
    params
        .url
        .clone()
        .unwrap_or_else(default_url)
        .trim()
        .trim_end_matches('/')
        .to_string()
}

fn create_client(params: &Params) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(params.timeout))
        .danger_accept_invalid_certs(!params.validate_certs)
        .build()
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to create HTTP client: {e}"),
            )
        })
}

fn add_auth(
    params: &Params,
    request: reqwest::blocking::RequestBuilder,
) -> reqwest::blocking::RequestBuilder {
    if let Some(api_key) = &params.api_key {
        request.header("Authorization", format!("Bearer {api_key}"))
    } else if let (Some(user), Some(pass)) = (&params.username, &params.password) {
        request.basic_auth(user, Some(pass))
    } else {
        request
    }
}

fn handle_response(response: reqwest::blocking::Response, context: &str) -> Result<JsonValue> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("{context}: HTTP {} - {body}", status.as_u16()),
        ));
    }
    response.json::<JsonValue>().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse {context} response: {e}"),
        )
    })
}

fn get_string_field(data: &JsonValue, field: &str) -> Option<String> {
    data.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn determine_resource(params: &Params) -> Result<ResourceType> {
    let resource_count = [
        params.dashboard.is_some(),
        params.datasource.is_some(),
        params.folder.is_some(),
        params.org.is_some(),
    ]
    .iter()
    .filter(|&&x| x)
    .count();

    if resource_count == 0 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "One of dashboard, datasource, folder, or org must be specified",
        ));
    }

    if resource_count > 1 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Only one of dashboard, datasource, folder, or org can be specified at a time",
        ));
    }

    if params.dashboard.is_some() {
        Ok(ResourceType::Dashboard)
    } else if params.datasource.is_some() {
        Ok(ResourceType::Datasource)
    } else if params.folder.is_some() {
        Ok(ResourceType::Folder)
    } else {
        Ok(ResourceType::Org)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ResourceType {
    Dashboard,
    Datasource,
    Folder,
    Org,
}

fn exec_dashboard_get(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
) -> Result<ModuleResult> {
    let uid = get_string_field(config, "uid").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "dashboard uid is required for get action",
        )
    })?;

    let url = format!("{base_url}/api/dashboards/uid/{uid}");
    let response = add_auth(params, client.get(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to get dashboard: {e}"),
        )
    })?;

    let data = handle_response(response, "dashboard get")?;
    Ok(ModuleResult::new(
        false,
        Some(value::to_value(data.clone())?),
        Some(format!("Dashboard '{uid}' retrieved successfully")),
    ))
}

fn exec_dashboard_add(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    if check_mode {
        let title = get_string_field(config, "title").unwrap_or_else(|| "unknown".to_string());
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create/update dashboard '{title}'")),
        ));
    }

    let url = format!("{base_url}/api/dashboards/db");
    let body = json!({
        "dashboard": config,
        "overwrite": true
    });

    let response = add_auth(params, client.post(&url))
        .json(&body)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create dashboard: {e}"),
            )
        })?;

    let data = handle_response(response, "dashboard create")?;
    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let slug = data
        .get("slug")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Dashboard '{slug}' {status}")),
    ))
}

fn exec_dashboard_remove(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let uid = get_string_field(config, "uid").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "dashboard uid is required for remove action",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would remove dashboard '{uid}'")),
        ));
    }

    let url = format!("{base_url}/api/dashboards/uid/{uid}");
    let response = add_auth(params, client.delete(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove dashboard: {e}"),
        )
    })?;

    let data = handle_response(response, "dashboard remove")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Dashboard '{uid}' removed")),
    ))
}

fn exec_dashboard_update(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    exec_dashboard_add(client, params, base_url, config, check_mode)
}

fn exec_datasource_get(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
) -> Result<ModuleResult> {
    let name = get_string_field(config, "name").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "datasource name is required for get action",
        )
    })?;

    let url = format!("{base_url}/api/datasources/name/{name}");
    let response = add_auth(params, client.get(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to get datasource: {e}"),
        )
    })?;

    let data = handle_response(response, "datasource get")?;
    Ok(ModuleResult::new(
        false,
        Some(value::to_value(data.clone())?),
        Some(format!("Datasource '{name}' retrieved successfully")),
    ))
}

fn exec_datasource_add(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let name = get_string_field(config, "name").unwrap_or_else(|| "unknown".to_string());

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create datasource '{name}'")),
        ));
    }

    let url = format!("{base_url}/api/datasources");
    let response = add_auth(params, client.post(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create datasource: {e}"),
            )
        })?;

    let data = handle_response(response, "datasource create")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Datasource '{name}' created")),
    ))
}

fn exec_datasource_remove(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let name = get_string_field(config, "name").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "datasource name is required for remove action",
        )
    })?;

    let id = config
        .get("id")
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string())
        .unwrap_or_else(|| name.clone());

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would remove datasource '{name}'")),
        ));
    }

    let url = if config.get("id").is_some() {
        format!("{base_url}/api/datasources/{id}")
    } else {
        format!("{base_url}/api/datasources/name/{name}")
    };

    let response = add_auth(params, client.delete(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove datasource: {e}"),
        )
    })?;

    let data = handle_response(response, "datasource remove")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Datasource '{name}' removed")),
    ))
}

fn exec_datasource_update(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let name = get_string_field(config, "name").unwrap_or_else(|| "unknown".to_string());

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would update datasource '{name}'")),
        ));
    }

    let id = config
        .get("id")
        .and_then(|v| v.as_u64())
        .map(|v| v.to_string());

    let url = if let Some(ds_id) = id {
        format!("{base_url}/api/datasources/{ds_id}")
    } else {
        let lookup_url = format!("{base_url}/api/datasources/name/{name}");
        let lookup_response = add_auth(params, client.get(&lookup_url))
            .send()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to lookup datasource: {e}"),
                )
            })?;
        let lookup_data = handle_response(lookup_response, "datasource lookup")?;
        let ds_id = lookup_data
            .get("id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Datasource '{name}' not found or missing id"),
                )
            })?;
        format!("{base_url}/api/datasources/{ds_id}")
    };

    let response = add_auth(params, client.put(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to update datasource: {e}"),
            )
        })?;

    let data = handle_response(response, "datasource update")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Datasource '{name}' updated")),
    ))
}

fn exec_folder_get(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
) -> Result<ModuleResult> {
    let uid = get_string_field(config, "uid").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "folder uid is required for get action",
        )
    })?;

    let url = format!("{base_url}/api/folders/{uid}");
    let response = add_auth(params, client.get(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to get folder: {e}"),
        )
    })?;

    let data = handle_response(response, "folder get")?;
    Ok(ModuleResult::new(
        false,
        Some(value::to_value(data.clone())?),
        Some(format!("Folder '{uid}' retrieved successfully")),
    ))
}

fn exec_folder_add(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let title = get_string_field(config, "title").unwrap_or_else(|| "unknown".to_string());

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create folder '{title}'")),
        ));
    }

    let url = format!("{base_url}/api/folders");
    let response = add_auth(params, client.post(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create folder: {e}"),
            )
        })?;

    let data = handle_response(response, "folder create")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Folder '{title}' created")),
    ))
}

fn exec_folder_remove(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let uid = get_string_field(config, "uid").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "folder uid is required for remove action",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would remove folder '{uid}'")),
        ));
    }

    let url = format!("{base_url}/api/folders/{uid}");
    let response = add_auth(params, client.delete(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove folder: {e}"),
        )
    })?;

    let data = handle_response(response, "folder remove")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Folder '{uid}' removed")),
    ))
}

fn exec_folder_update(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let uid = get_string_field(config, "uid").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "folder uid is required for update action",
        )
    })?;

    let title = get_string_field(config, "title").unwrap_or_else(|| uid.clone());

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would update folder '{uid}'")),
        ));
    }

    let url = format!("{base_url}/api/folders/{uid}");
    let response = add_auth(params, client.put(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to update folder: {e}"),
            )
        })?;

    let data = handle_response(response, "folder update")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Folder '{title}' updated")),
    ))
}

fn exec_org_get(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
) -> Result<ModuleResult> {
    if let Some(org_id) = get_string_field(config, "id") {
        let url = format!("{base_url}/api/orgs/{org_id}");
        let response = add_auth(params, client.get(&url)).send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to get organization: {e}"),
            )
        })?;
        let data = handle_response(response, "org get")?;
        Ok(ModuleResult::new(
            false,
            Some(value::to_value(data.clone())?),
            Some(format!("Organization '{org_id}' retrieved successfully")),
        ))
    } else {
        let url = format!("{base_url}/api/orgs");
        let response = add_auth(params, client.get(&url)).send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to list organizations: {e}"),
            )
        })?;
        let data = handle_response(response, "orgs list")?;
        Ok(ModuleResult::new(
            false,
            Some(value::to_value(data.clone())?),
            Some("Organizations retrieved successfully".to_string()),
        ))
    }
}

fn exec_org_add(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let name = get_string_field(config, "name").ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "org name is required for add action",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would create organization '{name}'")),
        ));
    }

    let url = format!("{base_url}/api/orgs");
    let response = add_auth(params, client.post(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create organization: {e}"),
            )
        })?;

    let data = handle_response(response, "org create")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Organization '{name}' created")),
    ))
}

fn exec_org_remove(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let org_id = config.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "org id is required for remove action",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would remove organization '{org_id}'")),
        ));
    }

    let url = format!("{base_url}/api/orgs/{org_id}");
    let response = add_auth(params, client.delete(&url)).send().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to remove organization: {e}"),
        )
    })?;

    let data = handle_response(response, "org remove")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Organization '{org_id}' removed")),
    ))
}

fn exec_org_update(
    client: &Client,
    params: &Params,
    base_url: &str,
    config: &JsonValue,
    check_mode: bool,
) -> Result<ModuleResult> {
    let org_id = config.get("id").and_then(|v| v.as_u64()).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "org id is required for update action",
        )
    })?;

    if check_mode {
        return Ok(ModuleResult::new(
            true,
            None,
            Some(format!("Would update organization '{org_id}'")),
        ));
    }

    let url = format!("{base_url}/api/orgs/{org_id}");
    let response = add_auth(params, client.put(&url))
        .json(config)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to update organization: {e}"),
            )
        })?;

    let data = handle_response(response, "org update")?;
    Ok(ModuleResult::new(
        true,
        Some(value::to_value(data)?),
        Some(format!("Organization '{org_id}' updated")),
    ))
}

pub fn grafana(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let base_url = resolve_url(&params);
    let client = create_client(&params)?;
    let resource = determine_resource(&params)?;

    match resource {
        ResourceType::Dashboard => {
            let config = params.dashboard.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "dashboard configuration is required",
                )
            })?;

            match params.action {
                Action::Get => exec_dashboard_get(&client, &params, &base_url, config),
                Action::Add => exec_dashboard_add(&client, &params, &base_url, config, check_mode),
                Action::Remove => {
                    exec_dashboard_remove(&client, &params, &base_url, config, check_mode)
                }
                Action::Update => {
                    exec_dashboard_update(&client, &params, &base_url, config, check_mode)
                }
            }
        }
        ResourceType::Datasource => {
            let config = params.datasource.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "datasource configuration is required",
                )
            })?;

            match params.action {
                Action::Get => exec_datasource_get(&client, &params, &base_url, config),
                Action::Add => exec_datasource_add(&client, &params, &base_url, config, check_mode),
                Action::Remove => {
                    exec_datasource_remove(&client, &params, &base_url, config, check_mode)
                }
                Action::Update => {
                    exec_datasource_update(&client, &params, &base_url, config, check_mode)
                }
            }
        }
        ResourceType::Folder => {
            let config = params.folder.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "folder configuration is required")
            })?;

            match params.action {
                Action::Get => exec_folder_get(&client, &params, &base_url, config),
                Action::Add => exec_folder_add(&client, &params, &base_url, config, check_mode),
                Action::Remove => {
                    exec_folder_remove(&client, &params, &base_url, config, check_mode)
                }
                Action::Update => {
                    exec_folder_update(&client, &params, &base_url, config, check_mode)
                }
            }
        }
        ResourceType::Org => {
            let config = params.org.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "org configuration is required")
            })?;

            match params.action {
                Action::Get => exec_org_get(&client, &params, &base_url, config),
                Action::Add => exec_org_add(&client, &params, &base_url, config, check_mode),
                Action::Remove => exec_org_remove(&client, &params, &base_url, config, check_mode),
                Action::Update => exec_org_update(&client, &params, &base_url, config, check_mode),
            }
        }
    }
}

#[derive(Debug)]
pub struct Grafana;

impl Module for Grafana {
    fn get_name(&self) -> &str {
        "grafana"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((grafana(parse_params(params)?, check_mode)?, None))
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
    fn test_parse_params_datasource_add() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
              type: prometheus
              url: http://prometheus:9090
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Add);
        assert_eq!(params.url, Some("http://grafana:3000".to_string()));
        assert_eq!(params.api_key, Some("test-key".to_string()));
        assert!(params.datasource.is_some());
        let ds = params.datasource.unwrap();
        assert_eq!(ds.get("name").and_then(|v| v.as_str()), Some("Prometheus"));
    }

    #[test]
    fn test_parse_params_dashboard_get() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            url: http://grafana:3000
            api_key: test-key
            dashboard:
              uid: my-dashboard
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Get);
        assert!(params.dashboard.is_some());
        let db = params.dashboard.unwrap();
        assert_eq!(db.get("uid").and_then(|v| v.as_str()), Some("my-dashboard"));
    }

    #[test]
    fn test_parse_params_folder_add() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            api_key: test-key
            folder:
              title: My Folder
              uid: my-folder
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Add);
        assert!(params.folder.is_some());
        let folder = params.folder.unwrap();
        assert_eq!(
            folder.get("title").and_then(|v| v.as_str()),
            Some("My Folder")
        );
    }

    #[test]
    fn test_parse_params_org_with_basic_auth() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            username: admin
            password: secret
            org:
              name: Engineering
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.username, Some("admin".to_string()));
        assert_eq!(params.password, Some("secret".to_string()));
        assert!(params.org.is_some());
        let org = params.org.unwrap();
        assert_eq!(
            org.get("name").and_then(|v| v.as_str()),
            Some("Engineering")
        );
    }

    #[test]
    fn test_parse_params_default_action() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Get);
    }

    #[test]
    fn test_parse_params_default_timeout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            datasource:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.timeout, 30);
        assert!(params.validate_certs);
    }

    #[test]
    fn test_parse_params_custom_timeout() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            url: http://grafana:3000
            api_key: test-key
            timeout: 60
            validate_certs: false
            datasource:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.timeout, 60);
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_action_remove() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: remove
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Remove);
    }

    #[test]
    fn test_parse_params_action_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: update
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
              type: prometheus
              url: http://prometheus-new:9090
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Update);
    }

    #[test]
    fn test_parse_params_no_resource() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            url: http://grafana:3000
            api_key: test-key
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = determine_resource(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_multiple_resources() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
            dashboard:
              uid: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let result = determine_resource(&params);
        assert!(result.is_err());
    }

    #[test]
    fn test_determine_resource_dashboard() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            dashboard:
              uid: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            determine_resource(&params).unwrap(),
            ResourceType::Dashboard
        );
    }

    #[test]
    fn test_determine_resource_datasource() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            datasource:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            determine_resource(&params).unwrap(),
            ResourceType::Datasource
        );
    }

    #[test]
    fn test_determine_resource_folder() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            folder:
              uid: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(determine_resource(&params).unwrap(), ResourceType::Folder);
    }

    #[test]
    fn test_determine_resource_org() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            org:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(determine_resource(&params).unwrap(), ResourceType::Org);
    }

    #[test]
    fn test_resolve_url_default() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            datasource:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(resolve_url(&params), "http://localhost:3000");
    }

    #[test]
    fn test_resolve_url_custom() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            url: http://grafana.example.com:3000/
            datasource:
              name: test
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(resolve_url(&params), "http://grafana.example.com:3000");
    }

    #[test]
    fn test_default_action() {
        let action: Action = Default::default();
        assert_eq!(action, Action::Get);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_check_mode_dashboard_add() {
        let module = Grafana;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            api_key: test-key
            dashboard:
              title: Test Dashboard
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_datasource_add() {
        let module = Grafana;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            api_key: test-key
            datasource:
              name: Prometheus
              type: prometheus
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_folder_remove() {
        let module = Grafana;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: remove
            url: http://grafana:3000
            api_key: test-key
            folder:
              uid: my-folder
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_check_mode_org_add() {
        let module = Grafana;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            username: admin
            password: admin
            org:
              name: Engineering
            "#,
        )
        .unwrap();
        let (result, _) = module
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true)
            .unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("Would"));
    }

    #[test]
    fn test_get_string_field() {
        let json: JsonValue =
            serde_json::from_str(r#"{"name": "test", "id": 5, "nested": {"key": "value"}}"#)
                .unwrap();
        assert_eq!(get_string_field(&json, "name"), Some("test".to_string()));
        assert_eq!(get_string_field(&json, "id"), None);
        assert_eq!(get_string_field(&json, "missing"), None);
    }

    #[test]
    fn test_parse_params_dashboard_with_array() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            url: http://grafana:3000
            api_key: test-key
            dashboard:
              title: My Dashboard
              panels:
                - title: CPU
                  type: graph
                - title: Memory
                  type: graph
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let db = params.dashboard.unwrap();
        let panels = db.get("panels").unwrap().as_array().unwrap();
        assert_eq!(panels.len(), 2);
    }
}
