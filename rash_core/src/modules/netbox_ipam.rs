/// ANCHOR: module
/// # netbox_ipam
///
/// Manage IP addresses and prefixes in NetBox IPAM/DCIM system.
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
/// - name: Create an IP address in NetBox
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: ip_address
///     address: "192.168.1.100/24"
///     state: present
///     description: "Web server"
///
/// - name: Create a prefix
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: prefix
///     address: "192.168.1.0/24"
///     state: present
///     description: "Office network"
///
/// - name: Query an IP address
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: ip_address
///     address: "192.168.1.100/24"
///     state: query
///   register: ip_info
///
/// - name: Create a VLAN
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: vlan
///     vlan_id: 100
///     vlan_name: "office-vlan"
///     state: present
///
/// - name: Create a VRF
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: vrf
///     vrf_name: "customer-a"
///     state: present
///
/// - name: Delete an IP address
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: ip_address
///     address: "192.168.1.100/24"
///     state: absent
///
/// - name: Create IP with tenant
///   netbox_ipam:
///     url: "http://netbox:8000"
///     token: "{{ netbox_token }}"
///     type: ip_address
///     address: "10.0.0.1/24"
///     state: present
///     tenant: "engineering"
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
use serde_json::json;
use serde_norway::Value as YamlValue;
use serde_norway::value;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// NetBox instance URL.
    pub url: String,
    /// NetBox API token.
    pub token: String,
    /// Type of NetBox IPAM object to manage.
    /// **[default: `"ip_address"`]**
    pub r#type: Option<ResourceType>,
    /// IP address or prefix CIDR (required for ip_address and prefix types).
    pub address: Option<String>,
    /// Desired state of the resource.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Description for the resource.
    pub description: Option<String>,
    /// Tenant name to assign.
    pub tenant: Option<String>,
    /// VLAN ID (required for vlan type).
    pub vlan_id: Option<u32>,
    /// VLAN name (for vlan type).
    pub vlan_name: Option<String>,
    /// VRF name (required for vrf type).
    pub vrf_name: Option<String>,
    /// Route distinguisher for VRF (e.g. "65000:100").
    pub rd: Option<String>,
    /// Timeout in seconds for API requests.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    /// If false, SSL certificates will not be validated.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    #[default]
    IpAddress,
    Prefix,
    Vlan,
    Vrf,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Query,
}

fn default_timeout() -> u64 {
    30
}

fn default_validate_certs() -> bool {
    true
}

fn normalize_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/').trim_end_matches("/api");
    format!("{url}/api/")
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

fn build_auth_request(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
) -> reqwest::blocking::RequestBuilder {
    client
        .request(method, url)
        .header("Authorization", format!("Token {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
}

fn get_tenant_id(
    client: &Client,
    base_url: &str,
    token: &str,
    tenant_name: &str,
) -> Result<Option<u32>> {
    let url = format!("{base_url}tenancy/tenants/?name={tenant_name}");
    let response = build_auth_request(client, reqwest::Method::GET, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to query tenant '{tenant_name}': {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to query tenant '{tenant_name}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    let data: serde_json::Value = response.json().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse tenant response: {e}"),
        )
    })?;

    let results = data.get("results").and_then(|r| r.as_array());
    match results {
        Some(arr) if !arr.is_empty() => arr[0]
            .get("id")
            .and_then(|id| id.as_u64())
            .map(|id| Some(id as u32))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Tenant '{tenant_name}' has no valid ID"),
                )
            }),
        _ => Ok(None),
    }
}

fn query_ip_address(
    client: &Client,
    base_url: &str,
    token: &str,
    address: &str,
) -> Result<Option<serde_json::Value>> {
    let encoded = urlencoding::encode(address);
    let url = format!("{base_url}ipam/ip-addresses/?address={encoded}");
    let response = build_auth_request(client, reqwest::Method::GET, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to query IP address '{address}': {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to query IP address '{address}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    let data: serde_json::Value = response.json().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse IP address response: {e}"),
        )
    })?;

    let count = data.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return Ok(None);
    }

    let results = data.get("results").and_then(|r| r.as_array());
    match results {
        Some(arr) if !arr.is_empty() => Ok(Some(arr[0].clone())),
        _ => Ok(None),
    }
}

fn query_prefix(
    client: &Client,
    base_url: &str,
    token: &str,
    prefix: &str,
) -> Result<Option<serde_json::Value>> {
    let encoded = urlencoding::encode(prefix);
    let url = format!("{base_url}ipam/prefixes/?prefix={encoded}");
    let response = build_auth_request(client, reqwest::Method::GET, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to query prefix '{prefix}': {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to query prefix '{prefix}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    let data: serde_json::Value = response.json().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse prefix response: {e}"),
        )
    })?;

    let count = data.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return Ok(None);
    }

    let results = data.get("results").and_then(|r| r.as_array());
    match results {
        Some(arr) if !arr.is_empty() => Ok(Some(arr[0].clone())),
        _ => Ok(None),
    }
}

fn query_vlan(
    client: &Client,
    base_url: &str,
    token: &str,
    vlan_id: u32,
) -> Result<Option<serde_json::Value>> {
    let url = format!("{base_url}ipam/vlans/?vid={vlan_id}");
    let response = build_auth_request(client, reqwest::Method::GET, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to query VLAN {vlan_id}: {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to query VLAN {vlan_id}: HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    let data: serde_json::Value = response.json().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse VLAN response: {e}"),
        )
    })?;

    let count = data.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return Ok(None);
    }

    let results = data.get("results").and_then(|r| r.as_array());
    match results {
        Some(arr) if !arr.is_empty() => Ok(Some(arr[0].clone())),
        _ => Ok(None),
    }
}

fn query_vrf(
    client: &Client,
    base_url: &str,
    token: &str,
    vrf_name: &str,
) -> Result<Option<serde_json::Value>> {
    let url = format!("{base_url}ipam/vrfs/?name={vrf_name}");
    let response = build_auth_request(client, reqwest::Method::GET, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to query VRF '{vrf_name}': {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to query VRF '{vrf_name}': HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    let data: serde_json::Value = response.json().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse VRF response: {e}"),
        )
    })?;

    let count = data.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return Ok(None);
    }

    let results = data.get("results").and_then(|r| r.as_array());
    match results {
        Some(arr) if !arr.is_empty() => Ok(Some(arr[0].clone())),
        _ => Ok(None),
    }
}

fn delete_resource(
    client: &Client,
    base_url: &str,
    token: &str,
    endpoint: &str,
    resource_id: u32,
) -> Result<()> {
    let url = format!("{base_url}{endpoint}/{resource_id}/");
    let response = build_auth_request(client, reqwest::Method::DELETE, &url, token)
        .send()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to delete resource: {e}"),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to delete resource: HTTP {} - {body}",
                status.as_u16()
            ),
        ));
    }

    Ok(())
}

fn handle_ip_address(
    params: &Params,
    client: &Client,
    base_url: &str,
    check_mode: bool,
) -> Result<ModuleResult> {
    let address = params.address.as_deref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "address is required for ip_address type",
        )
    })?;

    let state = params.state.clone().unwrap_or_default();
    let existing = query_ip_address(client, base_url, &params.token, address)?;

    match state {
        State::Query => match existing {
            Some(record) => {
                let extra = json!({
                    "type": "ip_address",
                    "address": address,
                    "data": record,
                });
                Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Found IP address '{address}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("IP address '{address}' not found")),
                extra: None,
            }),
        },
        State::Present => {
            if existing.is_some() {
                let extra = json!({
                    "type": "ip_address",
                    "address": address,
                    "exists": true,
                });
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("IP address '{address}' already exists")),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would create IP address '{address}'")),
                    extra: None,
                });
            }

            let mut body = json!({
                "address": address,
            });

            if let Some(desc) = &params.description {
                body["description"] = json!(desc);
            }

            if let Some(tenant_name) = &params.tenant {
                let tenant_id = get_tenant_id(client, base_url, &params.token, tenant_name)?;
                if let Some(tid) = tenant_id {
                    body["tenant"] = json!(tid);
                }
            }

            let url = format!("{base_url}ipam/ip-addresses/");
            let response = build_auth_request(client, reqwest::Method::POST, &url, &params.token)
                .json(&body)
                .send()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to create IP address '{address}': {e}"),
                    )
                })?;

            let status = response.status();
            if !status.is_success() {
                let resp_body = response.text().unwrap_or_default();
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create IP address '{address}': HTTP {} - {resp_body}",
                        status.as_u16()
                    ),
                ));
            }

            let created: serde_json::Value = response.json().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse created IP address response: {e}"),
                )
            })?;

            let extra = json!({
                "type": "ip_address",
                "address": address,
                "data": created,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Created IP address '{address}'")),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => match existing {
            Some(record) => {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would delete IP address '{address}'")),
                        extra: None,
                    });
                }

                let resource_id = record.get("id").and_then(|id| id.as_u64()).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "IP address record has no valid ID")
                })? as u32;

                delete_resource(
                    client,
                    base_url,
                    &params.token,
                    "ipam/ip-addresses",
                    resource_id,
                )?;

                let extra = json!({
                    "type": "ip_address",
                    "address": address,
                    "deleted": true,
                });

                Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Deleted IP address '{address}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("IP address '{address}' does not exist")),
                extra: None,
            }),
        },
    }
}

fn handle_prefix(
    params: &Params,
    client: &Client,
    base_url: &str,
    check_mode: bool,
) -> Result<ModuleResult> {
    let prefix = params.address.as_deref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "address is required for prefix type",
        )
    })?;

    let state = params.state.clone().unwrap_or_default();
    let existing = query_prefix(client, base_url, &params.token, prefix)?;

    match state {
        State::Query => match existing {
            Some(record) => {
                let extra = json!({
                    "type": "prefix",
                    "prefix": prefix,
                    "data": record,
                });
                Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Found prefix '{prefix}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("Prefix '{prefix}' not found")),
                extra: None,
            }),
        },
        State::Present => {
            if existing.is_some() {
                let extra = json!({
                    "type": "prefix",
                    "prefix": prefix,
                    "exists": true,
                });
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Prefix '{prefix}' already exists")),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would create prefix '{prefix}'")),
                    extra: None,
                });
            }

            let mut body = json!({
                "prefix": prefix,
            });

            if let Some(desc) = &params.description {
                body["description"] = json!(desc);
            }

            if let Some(tenant_name) = &params.tenant {
                let tenant_id = get_tenant_id(client, base_url, &params.token, tenant_name)?;
                if let Some(tid) = tenant_id {
                    body["tenant"] = json!(tid);
                }
            }

            let url = format!("{base_url}ipam/prefixes/");
            let response = build_auth_request(client, reqwest::Method::POST, &url, &params.token)
                .json(&body)
                .send()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to create prefix '{prefix}': {e}"),
                    )
                })?;

            let status = response.status();
            if !status.is_success() {
                let resp_body = response.text().unwrap_or_default();
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create prefix '{prefix}': HTTP {} - {resp_body}",
                        status.as_u16()
                    ),
                ));
            }

            let created: serde_json::Value = response.json().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse created prefix response: {e}"),
                )
            })?;

            let extra = json!({
                "type": "prefix",
                "prefix": prefix,
                "data": created,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Created prefix '{prefix}'")),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => match existing {
            Some(record) => {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would delete prefix '{prefix}'")),
                        extra: None,
                    });
                }

                let resource_id = record.get("id").and_then(|id| id.as_u64()).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Prefix record has no valid ID")
                })? as u32;

                delete_resource(
                    client,
                    base_url,
                    &params.token,
                    "ipam/prefixes",
                    resource_id,
                )?;

                let extra = json!({
                    "type": "prefix",
                    "prefix": prefix,
                    "deleted": true,
                });

                Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Deleted prefix '{prefix}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("Prefix '{prefix}' does not exist")),
                extra: None,
            }),
        },
    }
}

fn handle_vlan(
    params: &Params,
    client: &Client,
    base_url: &str,
    check_mode: bool,
) -> Result<ModuleResult> {
    let vlan_id = params
        .vlan_id
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "vlan_id is required for vlan type"))?;

    let state = params.state.clone().unwrap_or_default();
    let existing = query_vlan(client, base_url, &params.token, vlan_id)?;

    match state {
        State::Query => match existing {
            Some(record) => {
                let extra = json!({
                    "type": "vlan",
                    "vlan_id": vlan_id,
                    "data": record,
                });
                Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Found VLAN {vlan_id}")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("VLAN {vlan_id} not found")),
                extra: None,
            }),
        },
        State::Present => {
            if existing.is_some() {
                let extra = json!({
                    "type": "vlan",
                    "vlan_id": vlan_id,
                    "exists": true,
                });
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("VLAN {vlan_id} already exists")),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would create VLAN {vlan_id}")),
                    extra: None,
                });
            }

            let vlan_name = params
                .vlan_name
                .clone()
                .unwrap_or_else(|| format!("vlan-{vlan_id}"));

            let mut body = json!({
                "vid": vlan_id,
                "name": vlan_name,
            });

            if let Some(desc) = &params.description {
                body["description"] = json!(desc);
            }

            if let Some(tenant_name) = &params.tenant {
                let tenant_id = get_tenant_id(client, base_url, &params.token, tenant_name)?;
                if let Some(tid) = tenant_id {
                    body["tenant"] = json!(tid);
                }
            }

            let url = format!("{base_url}ipam/vlans/");
            let response = build_auth_request(client, reqwest::Method::POST, &url, &params.token)
                .json(&body)
                .send()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to create VLAN {vlan_id}: {e}"),
                    )
                })?;

            let status = response.status();
            if !status.is_success() {
                let resp_body = response.text().unwrap_or_default();
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create VLAN {vlan_id}: HTTP {} - {resp_body}",
                        status.as_u16()
                    ),
                ));
            }

            let created: serde_json::Value = response.json().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse created VLAN response: {e}"),
                )
            })?;

            let extra = json!({
                "type": "vlan",
                "vlan_id": vlan_id,
                "data": created,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Created VLAN {vlan_id}")),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => match existing {
            Some(record) => {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would delete VLAN {vlan_id}")),
                        extra: None,
                    });
                }

                let resource_id = record.get("id").and_then(|id| id.as_u64()).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "VLAN record has no valid ID")
                })? as u32;

                delete_resource(client, base_url, &params.token, "ipam/vlans", resource_id)?;

                let extra = json!({
                    "type": "vlan",
                    "vlan_id": vlan_id,
                    "deleted": true,
                });

                Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Deleted VLAN {vlan_id}")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("VLAN {vlan_id} does not exist")),
                extra: None,
            }),
        },
    }
}

fn handle_vrf(
    params: &Params,
    client: &Client,
    base_url: &str,
    check_mode: bool,
) -> Result<ModuleResult> {
    let vrf_name = params
        .vrf_name
        .as_deref()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "vrf_name is required for vrf type"))?;

    let state = params.state.clone().unwrap_or_default();
    let existing = query_vrf(client, base_url, &params.token, vrf_name)?;

    match state {
        State::Query => match existing {
            Some(record) => {
                let extra = json!({
                    "type": "vrf",
                    "vrf_name": vrf_name,
                    "data": record,
                });
                Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Found VRF '{vrf_name}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("VRF '{vrf_name}' not found")),
                extra: None,
            }),
        },
        State::Present => {
            if existing.is_some() {
                let extra = json!({
                    "type": "vrf",
                    "vrf_name": vrf_name,
                    "exists": true,
                });
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("VRF '{vrf_name}' already exists")),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would create VRF '{vrf_name}'")),
                    extra: None,
                });
            }

            let mut body = json!({
                "name": vrf_name,
            });

            if let Some(desc) = &params.description {
                body["description"] = json!(desc);
            }

            if let Some(rd) = &params.rd {
                body["rd"] = json!(rd);
            }

            if let Some(tenant_name) = &params.tenant {
                let tenant_id = get_tenant_id(client, base_url, &params.token, tenant_name)?;
                if let Some(tid) = tenant_id {
                    body["tenant"] = json!(tid);
                }
            }

            let url = format!("{base_url}ipam/vrfs/");
            let response = build_auth_request(client, reqwest::Method::POST, &url, &params.token)
                .json(&body)
                .send()
                .map_err(|e| {
                    Error::new(
                        ErrorKind::SubprocessFail,
                        format!("Failed to create VRF '{vrf_name}': {e}"),
                    )
                })?;

            let status = response.status();
            if !status.is_success() {
                let resp_body = response.text().unwrap_or_default();
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create VRF '{vrf_name}': HTTP {} - {resp_body}",
                        status.as_u16()
                    ),
                ));
            }

            let created: serde_json::Value = response.json().map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse created VRF response: {e}"),
                )
            })?;

            let extra = json!({
                "type": "vrf",
                "vrf_name": vrf_name,
                "data": created,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Created VRF '{vrf_name}'")),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => match existing {
            Some(record) => {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would delete VRF '{vrf_name}'")),
                        extra: None,
                    });
                }

                let resource_id = record.get("id").and_then(|id| id.as_u64()).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "VRF record has no valid ID")
                })? as u32;

                delete_resource(client, base_url, &params.token, "ipam/vrfs", resource_id)?;

                let extra = json!({
                    "type": "vrf",
                    "vrf_name": vrf_name,
                    "deleted": true,
                });

                Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Deleted VRF '{vrf_name}'")),
                    extra: Some(value::to_value(extra)?),
                })
            }
            None => Ok(ModuleResult {
                changed: false,
                output: Some(format!("VRF '{vrf_name}' does not exist")),
                extra: None,
            }),
        },
    }
}

pub fn netbox_ipam(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let resource_type = params.r#type.clone().unwrap_or_default();
    let base_url = normalize_url(&params.url);
    let client = create_client(&params)?;

    match resource_type {
        ResourceType::IpAddress => handle_ip_address(&params, &client, &base_url, check_mode),
        ResourceType::Prefix => handle_prefix(&params, &client, &base_url, check_mode),
        ResourceType::Vlan => handle_vlan(&params, &client, &base_url, check_mode),
        ResourceType::Vrf => handle_vrf(&params, &client, &base_url, check_mode),
    }
}

#[derive(Debug)]
pub struct NetboxIpam;

impl Module for NetboxIpam {
    fn get_name(&self) -> &str {
        "netbox_ipam"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((netbox_ipam(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::from_str;

    #[test]
    fn test_parse_params_ip_address() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
type: ip_address
address: "192.168.1.100/24"
state: present
description: "Web server"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.url, "http://netbox:8000");
        assert_eq!(params.token, "abc123");
        assert_eq!(params.r#type, Some(ResourceType::IpAddress));
        assert_eq!(params.address, Some("192.168.1.100/24".to_string()));
        assert_eq!(params.state, Some(State::Present));
        assert_eq!(params.description, Some("Web server".to_string()));
        assert_eq!(params.timeout, 30);
        assert!(params.validate_certs);
    }

    #[test]
    fn test_parse_params_prefix() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
type: prefix
address: "10.0.0.0/8"
state: absent
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.r#type, Some(ResourceType::Prefix));
        assert_eq!(params.address, Some("10.0.0.0/8".to_string()));
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_vlan() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
type: vlan
vlan_id: 100
vlan_name: "office-vlan"
state: present
tenant: "engineering"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.r#type, Some(ResourceType::Vlan));
        assert_eq!(params.vlan_id, Some(100));
        assert_eq!(params.vlan_name, Some("office-vlan".to_string()));
        assert_eq!(params.tenant, Some("engineering".to_string()));
    }

    #[test]
    fn test_parse_params_vrf() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
type: vrf
vrf_name: "customer-a"
rd: "65000:100"
state: present
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.r#type, Some(ResourceType::Vrf));
        assert_eq!(params.vrf_name, Some("customer-a".to_string()));
        assert_eq!(params.rd, Some("65000:100".to_string()));
    }

    #[test]
    fn test_parse_params_query_state() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
type: ip_address
address: "192.168.1.100/24"
state: query
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.state, Some(State::Query));
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.r#type, None);
        assert_eq!(params.state, None);
        assert_eq!(params.timeout, 30);
        assert!(params.validate_certs);
        assert!(params.address.is_none());
        assert!(params.description.is_none());
        assert!(params.tenant.is_none());
    }

    #[test]
    fn test_parse_params_timeout_and_certs() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
timeout: 60
validate_certs: false
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let params: Params = parse_params(value).unwrap();

        assert_eq!(params.timeout, 60);
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml = r#"
url: "http://netbox:8000"
token: "abc123"
unknown_field: value
"#;
        let value: YamlValue = from_str(yaml).unwrap();
        let error = parse_params::<Params>(value).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("http://netbox:8000"),
            "http://netbox:8000/api/"
        );
        assert_eq!(
            normalize_url("http://netbox:8000/"),
            "http://netbox:8000/api/"
        );
        assert_eq!(
            normalize_url("http://netbox:8000/api/"),
            "http://netbox:8000/api/"
        );
        assert_eq!(
            normalize_url(" http://netbox:8000 "),
            "http://netbox:8000/api/"
        );
    }

    #[test]
    fn test_default_resource_type() {
        let resource_type: ResourceType = Default::default();
        assert_eq!(resource_type, ResourceType::IpAddress);
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Present);
    }
}
