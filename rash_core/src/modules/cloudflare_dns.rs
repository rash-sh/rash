/// ANCHOR: module
/// # cloudflare_dns
///
/// Manage DNS records on Cloudflare.
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
/// - name: Create A record
///   cloudflare_dns:
///     zone: example.com
///     record: www
///     type: A
///     value: 192.168.1.1
///     ttl: 300
///     proxied: true
///     state: present
///
/// - name: Create AAAA record
///   cloudflare_dns:
///     zone: example.com
///     record: www
///     type: AAAA
///     value: 2001:db8::1
///     state: present
///
/// - name: Create CNAME record
///   cloudflare_dns:
///     zone: example.com
///     record: blog
///     type: CNAME
///     value: www.example.com
///     state: present
///
/// - name: Create MX record
///   cloudflare_dns:
///     zone: example.com
///     record: "@"
///     type: MX
///     value: mail.example.com
///     priority: 10
///     state: present
///
/// - name: Create TXT record
///   cloudflare_dns:
///     zone: example.com
///     record: "@"
///     type: TXT
///     value: "v=spf1 include:_spf.example.com ~all"
///     state: present
///
/// - name: Create SRV record
///   cloudflare_dns:
///     zone: example.com
///     record: "_sip._tcp"
///     type: SRV
///     value: "sip.example.com"
///     priority: 10
///     weight: 60
///     port: 5060
///     state: present
///
/// - name: Delete a DNS record
///   cloudflare_dns:
///     zone: example.com
///     record: old
///     type: A
///     state: absent
///
/// - name: Create record using API token from environment
///   cloudflare_dns:
///     zone: example.com
///     record: test
///     type: A
///     value: 10.0.0.1
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::env;

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
    Present,
    #[default]
    Absent,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[allow(clippy::upper_case_acronyms)]
pub enum RecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
    SRV,
}

impl std::fmt::Display for RecordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordType::A => write!(f, "A"),
            RecordType::AAAA => write!(f, "AAAA"),
            RecordType::CNAME => write!(f, "CNAME"),
            RecordType::MX => write!(f, "MX"),
            RecordType::TXT => write!(f, "TXT"),
            RecordType::SRV => write!(f, "SRV"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The DNS zone to manage (e.g. example.com).
    pub zone: String,
    /// The record name (e.g. www). Use "@" for the zone root.
    #[serde(default = "default_record")]
    pub record: String,
    /// The DNS record type.
    #[serde(rename = "type", default = "default_record_type")]
    pub record_type: RecordType,
    /// The record value (required for state=present).
    pub value: Option<String>,
    /// The TTL in seconds. 1 means auto when proxied.
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    /// Whether the record is proxied through Cloudflare.
    #[serde(default)]
    pub proxied: bool,
    /// The desired state of the record.
    #[serde(default)]
    pub state: State,
    /// Cloudflare API token. Falls back to CLOUDFLARE_API_TOKEN env var.
    pub api_token: Option<String>,
    /// Priority for MX and SRV records.
    pub priority: Option<u32>,
    /// Weight for SRV records.
    pub weight: Option<u32>,
    /// Port for SRV records.
    pub port: Option<u32>,
    /// Validate SSL certificates.
    #[serde(default = "default_validate_certs")]
    pub validate_certs: bool,
}

fn default_record() -> String {
    "@".to_string()
}

fn default_record_type() -> RecordType {
    RecordType::A
}

fn default_ttl() -> u32 {
    1
}

fn default_validate_certs() -> bool {
    true
}

fn get_api_token(params: &Params) -> Result<String> {
    params
        .api_token
        .clone()
        .or_else(|| env::var("CLOUDFLARE_API_TOKEN").ok())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "Cloudflare API token not provided. Set 'api_token' parameter or CLOUDFLARE_API_TOKEN environment variable.",
            )
        })
}

fn build_fqdn(zone: &str, record: &str) -> String {
    if record == "@" {
        zone.to_string()
    } else {
        format!("{record}.{zone}")
    }
}

struct CloudflareClient {
    api_token: String,
    validate_certs: bool,
}

use reqwest::blocking::RequestBuilder;

impl CloudflareClient {
    fn new(params: &Params) -> Result<Self> {
        Ok(Self {
            api_token: get_api_token(params)?,
            validate_certs: params.validate_certs,
        })
    }

    fn build_client(&self) -> Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(!self.validate_certs)
            .build()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to create HTTP client: {e}"),
                )
            })
    }

    fn send_and_parse(&self, request: RequestBuilder) -> Result<JsonValue> {
        let response = request.send().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Cloudflare API request failed: {e}"),
            )
        })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Cloudflare returned status {}: {}", status, error_text),
            ));
        }

        let response_text = response.text().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to read response: {e}"),
            )
        })?;

        let json: JsonValue = serde_json::from_str(&response_text).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse Cloudflare response: {e}"),
            )
        })?;

        let success = json
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !success {
            let errors = json
                .get("errors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Cloudflare API error: {errors}"),
            ));
        }

        Ok(json)
    }

    fn authed_get(&self, url: &str) -> Result<RequestBuilder> {
        let client = self.build_client()?;
        Ok(client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_token)))
    }

    fn get_zone_id(&self, zone: &str) -> Result<String> {
        let url = format!("https://api.cloudflare.com/client/v4/zones?name={zone}");
        let json = self.send_and_parse(self.authed_get(&url)?)?;

        let zones = json
            .get("result")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    "Failed to parse zones from Cloudflare response",
                )
            })?;

        if zones.is_empty() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Zone '{zone}' not found in Cloudflare"),
            ));
        }

        zones[0]
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Failed to get zone ID from Cloudflare response",
                )
            })
    }

    fn get_records(
        &self,
        zone_id: &str,
        name: &str,
        record_type: &RecordType,
    ) -> Result<Vec<JsonValue>> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?name={name}&type={record_type}"
        );
        let json = self.send_and_parse(self.authed_get(&url)?)?;

        json.get("result")
            .and_then(|v| v.as_array())
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Failed to parse records from Cloudflare response",
                )
            })
    }

    fn build_record_body(
        &self,
        params: &Params,
        fqdn: &str,
    ) -> Result<serde_json::Map<String, JsonValue>> {
        let value = params.value.as_ref().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "value parameter is required when state=present",
            )
        })?;

        let mut body = serde_json::Map::new();
        body.insert(
            "type".to_string(),
            JsonValue::String(params.record_type.to_string()),
        );
        body.insert("name".to_string(), JsonValue::String(fqdn.to_string()));
        body.insert("content".to_string(), JsonValue::String(value.clone()));
        body.insert("ttl".to_string(), JsonValue::Number(params.ttl.into()));
        body.insert("proxied".to_string(), JsonValue::Bool(params.proxied));

        if let Some(priority) = params.priority {
            body.insert("priority".to_string(), JsonValue::Number(priority.into()));
        }
        if let Some(weight) = params.weight {
            body.insert("weight".to_string(), JsonValue::Number(weight.into()));
        }
        if let Some(port) = params.port {
            body.insert("port".to_string(), JsonValue::Number(port.into()));
        }

        Ok(body)
    }

    fn create_record(&self, zone_id: &str, params: &Params, fqdn: &str) -> Result<JsonValue> {
        let body = self.build_record_body(params, fqdn)?;
        let url = format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records");

        let client = self.build_client()?;
        let request = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .json(&body);

        let json = self.send_and_parse(request)?;
        Ok(json
            .get("result")
            .cloned()
            .unwrap_or(JsonValue::Object(serde_json::Map::new())))
    }

    fn update_record(
        &self,
        zone_id: &str,
        record_id: &str,
        params: &Params,
        fqdn: &str,
    ) -> Result<JsonValue> {
        let body = self.build_record_body(params, fqdn)?;
        let url =
            format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{record_id}");

        let client = self.build_client()?;
        let request = client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .json(&body);

        let json = self.send_and_parse(request)?;
        Ok(json
            .get("result")
            .cloned()
            .unwrap_or(JsonValue::Object(serde_json::Map::new())))
    }

    fn delete_record(&self, zone_id: &str, record_id: &str) -> Result<bool> {
        let url =
            format!("https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{record_id}");

        let client = self.build_client()?;
        let request = client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_token));

        let json = self.send_and_parse(request)?;
        Ok(json
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }
}

fn record_matches(existing: &JsonValue, params: &Params) -> bool {
    let content_matches = existing
        .get("content")
        .and_then(|v| v.as_str())
        .map(|c| {
            if let Some(ref value) = params.value {
                c == value
            } else {
                true
            }
        })
        .unwrap_or(false);

    let ttl_matches = existing
        .get("ttl")
        .and_then(|v| v.as_u64())
        .map(|t| t as u32 == params.ttl)
        .unwrap_or(false);

    let proxied_matches = existing
        .get("proxied")
        .and_then(|v| v.as_bool())
        .map(|p| p == params.proxied)
        .unwrap_or(false);

    let priority_matches = match params.priority {
        Some(priority) => existing
            .get("priority")
            .and_then(|v| v.as_u64())
            .map(|p| p as u32 == priority)
            .unwrap_or(true),
        None => true,
    };

    let weight_matches = match params.weight {
        Some(weight) => existing
            .get("weight")
            .and_then(|v| v.as_u64())
            .map(|w| w as u32 == weight)
            .unwrap_or(true),
        None => true,
    };

    let port_matches = match params.port {
        Some(port) => existing
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u32 == port)
            .unwrap_or(true),
        None => true,
    };

    content_matches
        && ttl_matches
        && proxied_matches
        && priority_matches
        && weight_matches
        && port_matches
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let _ = params.value.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "value parameter is required when state=present",
        )
    })?;

    let client = CloudflareClient::new(params)?;
    let fqdn = build_fqdn(&params.zone, &params.record);
    let zone_id = client.get_zone_id(&params.zone)?;
    let records = client.get_records(&zone_id, &fqdn, &params.record_type)?;

    let matching_record = records.iter().find(|r| record_matches(r, params));

    match matching_record {
        Some(existing) => {
            let record_id = existing.get("id").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "record_id": record_id,
                    "fqdn": fqdn,
                    "type": params.record_type.to_string(),
                    "changed": false
                }))?),
                Some(format!(
                    "DNS record {} (type {}) already up to date",
                    fqdn, params.record_type
                )),
            ))
        }
        None => {
            if check_mode {
                return Ok(ModuleResult::new(true, None, None));
            }

            let existing_same_name_type = records.iter().find(|r| {
                r.get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| n == fqdn)
                    .unwrap_or(false)
            });

            let result = if let Some(existing) = existing_same_name_type {
                let record_id = existing.get("id").and_then(|v| v.as_str()).unwrap_or("");
                client.update_record(&zone_id, record_id, params, &fqdn)?
            } else {
                client.create_record(&zone_id, params, &fqdn)?
            };

            let record_id = result.get("id").and_then(|v| v.as_str()).unwrap_or("");

            Ok(ModuleResult::new(
                true,
                Some(value::to_value(json!({
                    "record_id": record_id,
                    "fqdn": fqdn,
                    "type": params.record_type.to_string(),
                    "changed": true
                }))?),
                Some(format!(
                    "DNS record {} (type {}) created/updated",
                    fqdn, params.record_type
                )),
            ))
        }
    }
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let client = CloudflareClient::new(params)?;
    let fqdn = build_fqdn(&params.zone, &params.record);
    let zone_id = client.get_zone_id(&params.zone)?;
    let records = client.get_records(&zone_id, &fqdn, &params.record_type)?;

    if records.is_empty() {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "fqdn": fqdn,
                "type": params.record_type.to_string(),
                "deleted": false
            }))?),
            Some(format!(
                "DNS record {} (type {}) not found",
                fqdn, params.record_type
            )),
        ));
    }

    if check_mode {
        return Ok(ModuleResult::new(true, None, None));
    }

    let mut deleted_ids = Vec::new();
    for record in &records {
        let record_id = record.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if client.delete_record(&zone_id, record_id)? {
            deleted_ids.push(record_id.to_string());
        }
    }

    Ok(ModuleResult::new(
        !deleted_ids.is_empty(),
        Some(value::to_value(json!({
            "fqdn": fqdn,
            "type": params.record_type.to_string(),
            "deleted_records": deleted_ids,
            "deleted": !deleted_ids.is_empty()
        }))?),
        if deleted_ids.is_empty() {
            Some(format!(
                "No DNS records {} (type {}) deleted",
                fqdn, params.record_type
            ))
        } else {
            Some(format!(
                "Deleted {} DNS record(s) for {} (type {})",
                deleted_ids.len(),
                fqdn,
                params.record_type
            ))
        },
    ))
}

pub fn cloudflare_dns(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct CloudflareDns;

impl Module for CloudflareDns {
    fn get_name(&self) -> &str {
        "cloudflare_dns"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            cloudflare_dns(parse_params(optional_params)?, check_mode)?,
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
            zone: example.com
            record: www
            type: A
            value: 192.168.1.1
            ttl: 300
            proxied: true
            state: present
            api_token: test-token
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.zone, "example.com");
        assert_eq!(params.record, "www");
        assert_eq!(params.record_type, RecordType::A);
        assert_eq!(params.value, Some("192.168.1.1".to_string()));
        assert_eq!(params.ttl, 300);
        assert!(params.proxied);
        assert_eq!(params.state, State::Present);
        assert_eq!(params.api_token, Some("test-token".to_string()));
    }

    #[test]
    fn test_parse_params_aaaa() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: www
            type: AAAA
            value: "2001:db8::1"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::AAAA);
    }

    #[test]
    fn test_parse_params_cname() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: blog
            type: CNAME
            value: www.example.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::CNAME);
        assert_eq!(params.value, Some("www.example.com".to_string()));
    }

    #[test]
    fn test_parse_params_mx() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: "@"
            type: MX
            value: mail.example.com
            priority: 10
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::MX);
        assert_eq!(params.priority, Some(10));
    }

    #[test]
    fn test_parse_params_txt() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: "@"
            type: TXT
            value: "v=spf1 include:_spf.example.com ~all"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::TXT);
    }

    #[test]
    fn test_parse_params_srv() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: "_sip._tcp"
            type: SRV
            value: sip.example.com
            priority: 10
            weight: 60
            port: 5060
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::SRV);
        assert_eq!(params.priority, Some(10));
        assert_eq!(params.weight, Some(60));
        assert_eq!(params.port, Some(5060));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: old
            type: A
            state: absent
            api_token: test-token
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_default_values() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record, "@");
        assert_eq!(params.record_type, RecordType::A);
        assert_eq!(params.ttl, 1);
        assert!(!params.proxied);
        assert_eq!(params.state, State::Absent);
        assert!(params.validate_certs);
        assert!(params.value.is_none());
        assert!(params.priority.is_none());
        assert!(params.weight.is_none());
        assert!(params.port.is_none());
    }

    #[test]
    fn test_parse_params_no_validate_certs() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            record: test
            type: A
            value: 10.0.0.1
            validate_certs: false
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.validate_certs);
    }

    #[test]
    fn test_build_fqdn() {
        assert_eq!(build_fqdn("example.com", "www"), "www.example.com");
        assert_eq!(build_fqdn("example.com", "@"), "example.com");
        assert_eq!(build_fqdn("example.com", "sub"), "sub.example.com");
    }

    #[test]
    fn test_record_type_display() {
        assert_eq!(RecordType::A.to_string(), "A");
        assert_eq!(RecordType::AAAA.to_string(), "AAAA");
        assert_eq!(RecordType::CNAME.to_string(), "CNAME");
        assert_eq!(RecordType::MX.to_string(), "MX");
        assert_eq!(RecordType::TXT.to_string(), "TXT");
        assert_eq!(RecordType::SRV.to_string(), "SRV");
    }

    #[test]
    fn test_record_matches() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "192.168.1.1",
            "ttl": 300,
            "proxied": true
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 300,
            proxied: true,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: None,
            weight: None,
            port: None,
            validate_certs: true,
        };

        assert!(record_matches(&existing, &params));
    }

    #[test]
    fn test_record_matches_different_value() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "192.168.1.1",
            "ttl": 300,
            "proxied": true
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.2".to_string()),
            ttl: 300,
            proxied: true,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: None,
            weight: None,
            port: None,
            validate_certs: true,
        };

        assert!(!record_matches(&existing, &params));
    }

    #[test]
    fn test_record_matches_different_ttl() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "192.168.1.1",
            "ttl": 300,
            "proxied": true
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 600,
            proxied: true,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: None,
            weight: None,
            port: None,
            validate_certs: true,
        };

        assert!(!record_matches(&existing, &params));
    }

    #[test]
    fn test_record_matches_different_proxied() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "192.168.1.1",
            "ttl": 300,
            "proxied": true
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 300,
            proxied: false,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: None,
            weight: None,
            port: None,
            validate_certs: true,
        };

        assert!(!record_matches(&existing, &params));
    }

    #[test]
    fn test_record_matches_with_priority() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "mail.example.com",
            "ttl": 300,
            "proxied": false,
            "priority": 10
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "@".to_string(),
            record_type: RecordType::MX,
            value: Some("mail.example.com".to_string()),
            ttl: 300,
            proxied: false,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: Some(10),
            weight: None,
            port: None,
            validate_certs: true,
        };

        assert!(record_matches(&existing, &params));
    }

    #[test]
    fn test_record_matches_srv_weight_port() {
        let existing = serde_json::json!({
            "id": "abc123",
            "content": "sip.example.com",
            "ttl": 300,
            "proxied": false,
            "priority": 10,
            "weight": 60,
            "port": 5060
        });

        let params = Params {
            zone: "example.com".to_string(),
            record: "_sip._tcp".to_string(),
            record_type: RecordType::SRV,
            value: Some("sip.example.com".to_string()),
            ttl: 300,
            proxied: false,
            state: State::Present,
            api_token: Some("test".to_string()),
            priority: Some(10),
            weight: Some(60),
            port: Some(5060),
            validate_certs: true,
        };

        assert!(record_matches(&existing, &params));

        let params_different_weight = Params {
            weight: Some(50),
            ..params.clone()
        };
        assert!(!record_matches(&existing, &params_different_weight));

        let params_different_port = Params {
            port: Some(5061),
            ..params.clone()
        };
        assert!(!record_matches(&existing, &params_different_port));
    }

    #[test]
    fn test_parse_params_missing_zone() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            record: www
            type: A
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }
}
