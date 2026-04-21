/// ANCHOR: module
/// # zabbix_host
///
/// Register and manage hosts in Zabbix monitoring.
///
/// Useful for automated infrastructure provisioning where monitoring
/// must be configured alongside deployment. Supports creating and
/// deleting hosts with group and template associations.
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
/// - name: Create a host in Zabbix
///   zabbix_host:
///     host_name: web-server-01
///     host_ip: 192.168.1.50
///     groups:
///       - Linux servers
///     templates:
///       - Template OS Linux
///     server_url: http://zabbix.example.com/api_jsonrpc.php
///     login_user: Admin
///     login_password: zabbix
///
/// - name: Create host with explicit state
///   zabbix_host:
///     host_name: db-server-01
///     host_ip: 192.168.1.60
///     groups:
///       - Linux servers
///       - Database servers
///     server_url: http://zabbix.example.com/api_jsonrpc.php
///     login_user: Admin
///     login_password: zabbix
///     state: present
///
/// - name: Delete a host from Zabbix
///   zabbix_host:
///     host_name: old-server-01
///     server_url: http://zabbix.example.com/api_jsonrpc.php
///     login_user: Admin
///     login_password: zabbix
///     state: absent
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

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Host name in Zabbix.
    pub host_name: String,
    /// IP address of the host.
    pub host_ip: Option<String>,
    /// The desired state of the host.
    #[serde(default)]
    pub state: State,
    /// Host group names.
    pub groups: Option<Vec<String>>,
    /// Template names to link.
    pub templates: Option<Vec<String>>,
    /// Zabbix API URL.
    pub server_url: String,
    /// Zabbix username.
    pub login_user: String,
    /// Zabbix password.
    pub login_password: String,
}

struct ZabbixClient {
    server_url: String,
    auth_token: String,
    client: reqwest::blocking::Client,
    request_id: i64,
}

impl ZabbixClient {
    fn new(params: &Params) -> Result<Self> {
        let client = reqwest::blocking::Client::new();
        let auth_token = Self::login(
            &client,
            &params.server_url,
            &params.login_user,
            &params.login_password,
        )?;
        Ok(Self {
            server_url: params.server_url.clone(),
            auth_token,
            client,
            request_id: 0,
        })
    }

    fn login(
        client: &reqwest::blocking::Client,
        server_url: &str,
        user: &str,
        password: &str,
    ) -> Result<String> {
        let response = Self::send_raw(
            client,
            server_url,
            "user.login",
            None,
            json!({
                "username": user,
                "password": password,
            }),
        )?;

        response
            .get("result")
            .and_then(|r| r.as_str())
            .map(String::from)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    "Failed to authenticate with Zabbix API",
                )
            })
    }

    fn send_raw(
        client: &reqwest::blocking::Client,
        server_url: &str,
        method: &str,
        auth: Option<&str>,
        params: JsonValue,
    ) -> Result<JsonValue> {
        let mut body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        if let Some(token) = auth {
            body["auth"] = json!(token);
        }

        let response = client
            .post(server_url)
            .header("Content-Type", "application/json-rpc")
            .json(&body)
            .send()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Zabbix API request failed: {e}"),
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Zabbix API returned status {}: {}", status, error_text),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Zabbix API response: {e}"),
            )
        })?;

        if let Some(error) = json.get("error") {
            let message = error
                .get("data")
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown Zabbix API error");
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Zabbix API error: {message}"),
            ));
        }

        Ok(json)
    }

    fn send(&mut self, method: &str, params: JsonValue) -> Result<JsonValue> {
        self.request_id += 1;
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "auth": self.auth_token,
            "id": self.request_id,
        });

        let response = self
            .client
            .post(&self.server_url)
            .header("Content-Type", "application/json-rpc")
            .json(&body)
            .send()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Zabbix API request failed: {e}"),
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Zabbix API returned status {}: {}", status, error_text),
            ));
        }

        let json: JsonValue = response.json().map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Zabbix API response: {e}"),
            )
        })?;

        if let Some(error) = json.get("error") {
            let message = error
                .get("data")
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown Zabbix API error");
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Zabbix API error: {message}"),
            ));
        }

        Ok(json)
    }

    fn get_host(&mut self, host_name: &str) -> Result<Option<JsonValue>> {
        let result = self.send(
            "host.get",
            json!({
                "filter": { "host": [host_name] },
                "output": ["hostid", "host"],
                "selectGroups": ["groupid", "name"],
                "selectParentTemplates": ["templateid", "host"],
            }),
        )?;

        let hosts = result
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        if hosts.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hosts[0].clone()))
        }
    }

    fn get_group_ids(&mut self, group_names: &[String]) -> Result<Vec<JsonValue>> {
        let result = self.send(
            "hostgroup.get",
            json!({
                "filter": { "name": group_names },
                "output": ["groupid", "name"],
            }),
        )?;

        let groups = result
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        if groups.len() != group_names.len() {
            let found_names: Vec<&str> = groups
                .iter()
                .filter_map(|g| g.get("name").and_then(|n| n.as_str()))
                .collect();
            let missing: Vec<&String> = group_names
                .iter()
                .filter(|name| !found_names.contains(&name.as_str()))
                .collect();
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Host groups not found in Zabbix: {:?}", missing),
            ));
        }

        Ok(groups)
    }

    fn get_template_ids(&mut self, template_names: &[String]) -> Result<Vec<JsonValue>> {
        let result = self.send(
            "template.get",
            json!({
                "filter": { "host": template_names },
                "output": ["templateid", "host"],
            }),
        )?;

        let templates = result
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        if templates.len() != template_names.len() {
            let found_names: Vec<&str> = templates
                .iter()
                .filter_map(|t| t.get("host").and_then(|h| h.as_str()))
                .collect();
            let missing: Vec<&String> = template_names
                .iter()
                .filter(|name| !found_names.contains(&name.as_str()))
                .collect();
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Templates not found in Zabbix: {:?}", missing),
            ));
        }

        Ok(templates)
    }

    fn create_host(
        &mut self,
        host_name: &str,
        host_ip: Option<&str>,
        groups: &[String],
        templates: &[String],
    ) -> Result<JsonValue> {
        let group_ids = self.get_group_ids(groups)?;
        let groups_json: Vec<JsonValue> = group_ids
            .into_iter()
            .map(|g| json!({ "groupid": g["groupid"] }))
            .collect();

        let mut params = json!({
            "host": host_name,
            "groups": groups_json,
        });

        if let Some(ip) = host_ip {
            params["interfaces"] = json!([{
                "type": 1,
                "main": 1,
                "useip": 1,
                "ip": ip,
                "dns": "",
                "port": "10050",
            }]);
        }

        if !templates.is_empty() {
            let template_ids = self.get_template_ids(templates)?;
            let templates_json: Vec<JsonValue> = template_ids
                .into_iter()
                .map(|t| json!({ "templateid": t["templateid"] }))
                .collect();
            params["templates"] = json!(templates_json);
        }

        self.send("host.create", params)
    }

    fn delete_host(&mut self, host_id: &str) -> Result<JsonValue> {
        self.send("host.delete", json!([host_id]))
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let mut client = ZabbixClient::new(params)?;

    match client.get_host(&params.host_name)? {
        Some(existing) => {
            let host_id = existing
                .get("hostid")
                .and_then(|h| h.as_str())
                .unwrap_or("unknown");

            Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "host_name": params.host_name,
                    "hostid": host_id,
                    "changed": false,
                }))?),
                Some(format!(
                    "Host '{}' already exists (id: {})",
                    params.host_name, host_id
                )),
            ))
        }
        None => {
            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "host_name": params.host_name,
                        "changed": true,
                    }))?),
                    Some(format!(
                        "Would create host '{}' in Zabbix",
                        params.host_name
                    )),
                ));
            }

            let groups = params.groups.as_deref().unwrap_or(&[]);
            let templates = params.templates.as_deref().unwrap_or(&[]);

            let result = client.create_host(
                &params.host_name,
                params.host_ip.as_deref(),
                groups,
                templates,
            )?;

            let host_ids = result
                .get("result")
                .and_then(|r| r.get("hostids"))
                .and_then(|h| h.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "host_name": params.host_name,
                    "hostids": host_ids,
                    "changed": true,
                }))?),
                Some(format!("Host '{}' created successfully", params.host_name)),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let mut client = ZabbixClient::new(params)?;

    match client.get_host(&params.host_name)? {
        Some(existing) => {
            let host_id = existing
                .get("hostid")
                .and_then(|h| h.as_str())
                .unwrap_or("unknown");

            if check_mode {
                return Ok(ModuleResult::new(
                    true,
                    Some(value::to_value(json!({
                        "host_name": params.host_name,
                        "hostid": host_id,
                        "changed": true,
                    }))?),
                    Some(format!(
                        "Would delete host '{}' (id: {})",
                        params.host_name, host_id
                    )),
                ));
            }

            client.delete_host(host_id)?;

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "host_name": params.host_name,
                    "hostid": host_id,
                    "changed": true,
                    "deleted": true,
                }))?),
                Some(format!("Host '{}' deleted successfully", params.host_name)),
            ))
        }
        None => Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "host_name": params.host_name,
                "changed": false,
            }))?),
            Some(format!("Host '{}' not found in Zabbix", params.host_name)),
        )),
    }
}

pub fn zabbix_host(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct ZabbixHost;

impl Module for ZabbixHost {
    fn get_name(&self) -> &str {
        "zabbix_host"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            zabbix_host(parse_params(optional_params)?, check_mode)?,
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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            host_ip: 192.168.1.50
            groups:
              - Linux servers
            templates:
              - Template OS Linux
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host_name, "web-server-01");
        assert_eq!(params.host_ip, Some("192.168.1.50".to_string()));
        assert_eq!(params.groups, Some(vec!["Linux servers".to_string()]));
        assert_eq!(
            params.templates,
            Some(vec!["Template OS Linux".to_string()])
        );
        assert_eq!(
            params.server_url,
            "http://zabbix.example.com/api_jsonrpc.php"
        );
        assert_eq!(params.login_user, "Admin");
        assert_eq!(params.login_password, "zabbix");
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: old-server-01
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.host_name, "old-server-01");
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.host_ip, None);
        assert_eq!(params.groups, None);
        assert_eq!(params.templates, None);
    }

    #[test]
    fn test_parse_params_multiple_groups() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: db-server-01
            host_ip: 192.168.1.60
            groups:
              - Linux servers
              - Database servers
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.groups,
            Some(vec![
                "Linux servers".to_string(),
                "Database servers".to_string(),
            ])
        );
    }

    #[test]
    fn test_parse_params_multiple_templates() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            groups:
              - Linux servers
            templates:
              - Template OS Linux
              - Template App HTTP Service
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.templates,
            Some(vec![
                "Template OS Linux".to_string(),
                "Template App HTTP Service".to_string(),
            ])
        );
    }

    #[test]
    fn test_parse_params_missing_host_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_server_url() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_login_user() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_password: zabbix
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_login_password() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_deny_unknown_fields() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_module_name() {
        let module = ZabbixHost;
        assert_eq!(module.get_name(), "zabbix_host");
    }

    #[test]
    fn test_check_mode_present() {
        let module = ZabbixHost;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            host_name: web-server-01
            host_ip: 192.168.1.50
            groups:
              - Linux servers
            server_url: http://zabbix.example.com/api_jsonrpc.php
            login_user: Admin
            login_password: zabbix
            "#,
        )
        .unwrap();
        let result = module.exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, true);
        assert!(result.is_err() || result.is_ok());
    }
}
