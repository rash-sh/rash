/// ANCHOR: module
/// # nsupdate
///
/// Manage DNS records using dynamic DNS updates (RFC 2136).
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
/// - name: Add A record
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: www
///     type: A
///     value: 192.168.1.1
///     ttl: 300
///     state: present
///
/// - name: Add A record with TSIG authentication
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: www
///     type: A
///     value: 192.168.1.1
///     ttl: 300
///     key_name: mykey
///     key_secret: "{{ dns_key }}"
///     key_algorithm: hmac-sha256
///     state: present
///
/// - name: Add AAAA record
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: www
///     type: AAAA
///     value: "2001:db8::1"
///     state: present
///
/// - name: Add CNAME record
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: blog
///     type: CNAME
///     value: www.example.com
///     state: present
///
/// - name: Add MX record
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: "@"
///     type: MX
///     value: mail.example.com
///     priority: 10
///     state: present
///
/// - name: Add TXT record
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: "@"
///     type: TXT
///     value: "v=spf1 include:_spf.example.com ~all"
///     state: present
///
/// - name: Add SRV record
///   nsupdate:
///     server: dns.example.com
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
///   nsupdate:
///     server: dns.example.com
///     zone: example.com
///     record: old
///     type: A
///     state: absent
///
/// - name: Add record using custom port
///   nsupdate:
///     server: dns.example.com
///     port: 5353
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

use std::io::Write;
use std::process::{Command, Stdio};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::json;
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
#[serde(rename_all = "kebab-case")]
pub enum KeyAlgorithm {
    HmacMd5,
    HmacSha1,
    HmacSha224,
    HmacSha256,
    HmacSha384,
    HmacSha512,
}

impl std::fmt::Display for KeyAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyAlgorithm::HmacMd5 => write!(f, "hmac-md5"),
            KeyAlgorithm::HmacSha1 => write!(f, "hmac-sha1"),
            KeyAlgorithm::HmacSha224 => write!(f, "hmac-sha224"),
            KeyAlgorithm::HmacSha256 => write!(f, "hmac-sha256"),
            KeyAlgorithm::HmacSha384 => write!(f, "hmac-sha384"),
            KeyAlgorithm::HmacSha512 => write!(f, "hmac-sha512"),
        }
    }
}

fn default_key_algorithm() -> KeyAlgorithm {
    KeyAlgorithm::HmacSha256
}

fn default_ttl() -> u32 {
    3600
}

fn default_port() -> u16 {
    53
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The DNS server to send updates to.
    pub server: String,
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
    /// The TTL in seconds.
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    /// The desired state of the record.
    #[serde(default)]
    pub state: State,
    /// TSIG key name for authentication.
    pub key_name: Option<String>,
    /// TSIG key secret (base64 encoded).
    pub key_secret: Option<String>,
    /// TSIG key algorithm.
    #[serde(default = "default_key_algorithm")]
    pub key_algorithm: KeyAlgorithm,
    /// DNS server port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Priority for MX and SRV records.
    pub priority: Option<u32>,
    /// Weight for SRV records.
    pub weight: Option<u32>,
    /// Port value for SRV records.
    pub srv_port: Option<u32>,
}

fn default_record() -> String {
    "@".to_string()
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
    NS,
    PTR,
    CAA,
    SOA,
}

fn default_record_type() -> RecordType {
    RecordType::A
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
            RecordType::NS => write!(f, "NS"),
            RecordType::PTR => write!(f, "PTR"),
            RecordType::CAA => write!(f, "CAA"),
            RecordType::SOA => write!(f, "SOA"),
        }
    }
}

fn build_fqdn(zone: &str, record: &str) -> String {
    if record == "@" {
        zone.to_string()
    } else if record.ends_with('.') {
        record.to_string()
    } else {
        format!("{record}.{zone}")
    }
}

fn format_rdata(params: &Params) -> Result<String> {
    match params.record_type {
        RecordType::SRV => {
            let weight = params.weight.unwrap_or(0);
            let port = params.srv_port.unwrap_or(0);
            let target = params.value.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "value is required for SRV records")
            })?;
            Ok(format!("{weight} {port} {target}"))
        }
        RecordType::MX => {
            let priority = params.priority.unwrap_or(10);
            let exchange = params.value.as_ref().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "value is required for MX records")
            })?;
            Ok(format!("{priority} {exchange}"))
        }
        _ => params.value.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "value is required when state=present",
            )
        }),
    }
}

fn build_nsupdate_commands(params: &Params) -> Result<String> {
    let fqdn = build_fqdn(&params.zone, &params.record);
    let mut commands = String::new();

    commands.push_str(&format!("server {} {}\n", params.server, params.port));
    commands.push_str(&format!("zone {}\n", params.zone));

    match params.state {
        State::Present => {
            let rdata = format_rdata(params)?;
            commands.push_str(&format!(
                "update add {} {} {} {}\n",
                fqdn, params.ttl, params.record_type, rdata
            ));
        }
        State::Absent => {
            commands.push_str(&format!("update delete {} {}\n", fqdn, params.record_type));
        }
    }

    commands.push_str("show\n");
    commands.push_str("send\n");

    Ok(commands)
}

fn run_nsupdate(commands: &str, params: &Params, check_mode: bool) -> Result<String> {
    let mut cmd = Command::new("nsupdate");

    if let Some(ref key_name) = params.key_name {
        let key_secret = params.key_secret.as_deref().unwrap_or("");
        cmd.arg("-y").arg(format!(
            "{}:{}:{}",
            params.key_algorithm, key_name, key_secret
        ));
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if check_mode {
        debug!("nsupdate commands (check mode):\n{commands}");
        return Ok(commands.to_string());
    }

    let mut child = cmd.spawn().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute nsupdate: {e}"),
        )
    })?;

    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            Error::new(
                ErrorKind::SubprocessFail,
                "Failed to open stdin for nsupdate",
            )
        })?;
        stdin.write_all(commands.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to write to nsupdate stdin: {e}"),
            )
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to wait for nsupdate: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "nsupdate failed with exit code {}: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn query_existing_record(params: &Params) -> Result<Option<String>> {
    let fqdn = build_fqdn(&params.zone, &params.record);

    let output = Command::new("dig")
        .args([
            "+short",
            &fqdn,
            &params.record_type.to_string(),
            &format!("@{}", params.server),
            "-p",
            &params.port.to_string(),
        ])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute dig: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "dig query failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if result.is_empty() {
        Ok(None)
    } else {
        Ok(Some(result))
    }
}

fn exec_present(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    if params.record_type == RecordType::SRV
        && (params.weight.is_none() || params.srv_port.is_none())
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "weight and srv_port are required for SRV records",
        ));
    }

    let value = params.value.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "value is required when state=present",
        )
    })?;

    let fqdn = build_fqdn(&params.zone, &params.record);

    let existing = query_existing_record(params).unwrap_or(None);

    let expected_value = format_rdata(params).ok();

    if let (Some(existing_val), Some(expected)) = (&existing, &expected_value) {
        let existing_lines: Vec<&str> = existing_val.lines().collect();
        if existing_lines
            .iter()
            .any(|line| line.trim() == expected.trim())
        {
            return Ok(ModuleResult::new(
                false,
                Some(value::to_value(json!({
                    "fqdn": fqdn,
                    "type": params.record_type.to_string(),
                    "value": value,
                    "ttl": params.ttl,
                    "changed": false
                }))?),
                Some(format!(
                    "DNS record {} (type {}) already up to date",
                    fqdn, params.record_type
                )),
            ));
        }
    }

    let commands = build_nsupdate_commands(params)?;
    let nsupdate_output = run_nsupdate(&commands, params, check_mode)?;

    let changed = !check_mode;
    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(json!({
            "fqdn": fqdn,
            "type": params.record_type.to_string(),
            "value": value,
            "ttl": params.ttl,
            "changed": changed,
            "nsupdate_output": nsupdate_output,
        }))?),
        if check_mode {
            Some(format!(
                "Would add DNS record {} (type {}) -> {}",
                fqdn, params.record_type, value
            ))
        } else {
            Some(format!(
                "Added DNS record {} (type {}) -> {}",
                fqdn, params.record_type, value
            ))
        },
    ))
}

fn exec_absent(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let fqdn = build_fqdn(&params.zone, &params.record);

    let existing = query_existing_record(params).unwrap_or(None);

    if existing.is_none() {
        return Ok(ModuleResult::new(
            false,
            Some(value::to_value(json!({
                "fqdn": fqdn,
                "type": params.record_type.to_string(),
                "changed": false
            }))?),
            Some(format!(
                "DNS record {} (type {}) not found",
                fqdn, params.record_type
            )),
        ));
    }

    let commands = build_nsupdate_commands(params)?;
    let nsupdate_output = run_nsupdate(&commands, params, check_mode)?;

    let changed = !check_mode;
    Ok(ModuleResult::new(
        changed,
        Some(value::to_value(json!({
            "fqdn": fqdn,
            "type": params.record_type.to_string(),
            "changed": changed,
            "nsupdate_output": nsupdate_output,
        }))?),
        if check_mode {
            Some(format!(
                "Would delete DNS record {} (type {})",
                fqdn, params.record_type
            ))
        } else {
            Some(format!(
                "Deleted DNS record {} (type {})",
                fqdn, params.record_type
            ))
        },
    ))
}

fn exec_nsupdate(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    if let Some(ref key_name) = params.key_name
        && params.key_secret.is_none()
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("key_secret is required when key_name '{key_name}' is specified"),
        ));
    }

    match params.state {
        State::Present => exec_present(&params, check_mode),
        State::Absent => exec_absent(&params, check_mode),
    }
}

#[derive(Debug)]
pub struct Nsupdate;

impl Module for Nsupdate {
    fn get_name(&self) -> &str {
        "nsupdate"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            exec_nsupdate(parse_params(optional_params)?, check_mode)?,
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
            server: dns.example.com
            zone: example.com
            record: www
            type: A
            value: 192.168.1.1
            ttl: 300
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.server, "dns.example.com");
        assert_eq!(params.zone, "example.com");
        assert_eq!(params.record, "www");
        assert_eq!(params.record_type, RecordType::A);
        assert_eq!(params.value, Some("192.168.1.1".to_string()));
        assert_eq!(params.ttl, 300);
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_with_tsig() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            record: www
            type: A
            value: 192.168.1.1
            key_name: mykey
            key_secret: " MyBase64Secret=="
            key_algorithm: hmac-sha256
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_name, Some("mykey".to_string()));
        assert_eq!(params.key_secret, Some(" MyBase64Secret==".to_string()));
        assert_eq!(params.key_algorithm, KeyAlgorithm::HmacSha256);
    }

    #[test]
    fn test_parse_params_aaaa() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
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
            server: dns.example.com
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
            server: dns.example.com
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
            server: dns.example.com
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
            server: dns.example.com
            zone: example.com
            record: "_sip._tcp"
            type: SRV
            value: sip.example.com
            priority: 10
            weight: 60
            srv_port: 5060
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::SRV);
        assert_eq!(params.priority, Some(10));
        assert_eq!(params.weight, Some(60));
        assert_eq!(params.srv_port, Some(5060));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            record: old
            type: A
            state: absent
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
            server: dns.example.com
            zone: example.com
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record, "@");
        assert_eq!(params.record_type, RecordType::A);
        assert_eq!(params.ttl, 3600);
        assert_eq!(params.state, State::Absent);
        assert_eq!(params.port, 53);
        assert_eq!(params.key_algorithm, KeyAlgorithm::HmacSha256);
        assert!(params.value.is_none());
        assert!(params.key_name.is_none());
        assert!(params.key_secret.is_none());
        assert!(params.priority.is_none());
        assert!(params.weight.is_none());
        assert!(params.srv_port.is_none());
    }

    #[test]
    fn test_parse_params_custom_port() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            port: 5353
            zone: example.com
            record: test
            type: A
            value: 10.0.0.1
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.port, 5353);
    }

    #[test]
    fn test_parse_params_missing_server() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            zone: example.com
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_missing_zone() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_deny_unknown() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            unknown_field: value
            "#,
        )
        .unwrap();
        let result: std::result::Result<Params, _> = parse_params(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_fqdn() {
        assert_eq!(build_fqdn("example.com", "www"), "www.example.com");
        assert_eq!(build_fqdn("example.com", "@"), "example.com");
        assert_eq!(build_fqdn("example.com", "sub"), "sub.example.com");
        assert_eq!(build_fqdn("example.com", "absolute."), "absolute.");
    }

    #[test]
    fn test_record_type_display() {
        assert_eq!(RecordType::A.to_string(), "A");
        assert_eq!(RecordType::AAAA.to_string(), "AAAA");
        assert_eq!(RecordType::CNAME.to_string(), "CNAME");
        assert_eq!(RecordType::MX.to_string(), "MX");
        assert_eq!(RecordType::TXT.to_string(), "TXT");
        assert_eq!(RecordType::SRV.to_string(), "SRV");
        assert_eq!(RecordType::NS.to_string(), "NS");
        assert_eq!(RecordType::PTR.to_string(), "PTR");
        assert_eq!(RecordType::CAA.to_string(), "CAA");
        assert_eq!(RecordType::SOA.to_string(), "SOA");
    }

    #[test]
    fn test_key_algorithm_display() {
        assert_eq!(KeyAlgorithm::HmacMd5.to_string(), "hmac-md5");
        assert_eq!(KeyAlgorithm::HmacSha1.to_string(), "hmac-sha1");
        assert_eq!(KeyAlgorithm::HmacSha224.to_string(), "hmac-sha224");
        assert_eq!(KeyAlgorithm::HmacSha256.to_string(), "hmac-sha256");
        assert_eq!(KeyAlgorithm::HmacSha384.to_string(), "hmac-sha384");
        assert_eq!(KeyAlgorithm::HmacSha512.to_string(), "hmac-sha512");
    }

    #[test]
    fn test_format_rdata_a() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 300,
            state: State::Present,
            key_name: None,
            key_secret: None,
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: None,
            weight: None,
            srv_port: None,
        };
        assert_eq!(format_rdata(&params).unwrap(), "192.168.1.1");
    }

    #[test]
    fn test_format_rdata_mx() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "@".to_string(),
            record_type: RecordType::MX,
            value: Some("mail.example.com".to_string()),
            ttl: 300,
            state: State::Present,
            key_name: None,
            key_secret: None,
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: Some(10),
            weight: None,
            srv_port: None,
        };
        assert_eq!(format_rdata(&params).unwrap(), "10 mail.example.com");
    }

    #[test]
    fn test_format_rdata_srv() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "_sip._tcp".to_string(),
            record_type: RecordType::SRV,
            value: Some("sip.example.com".to_string()),
            ttl: 300,
            state: State::Present,
            key_name: None,
            key_secret: None,
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: Some(10),
            weight: Some(60),
            srv_port: Some(5060),
        };
        assert_eq!(format_rdata(&params).unwrap(), "60 5060 sip.example.com");
    }

    #[test]
    fn test_build_nsupdate_commands_present() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 300,
            state: State::Present,
            key_name: None,
            key_secret: None,
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: None,
            weight: None,
            srv_port: None,
        };
        let commands = build_nsupdate_commands(&params).unwrap();
        assert!(commands.contains("server dns.example.com 53"));
        assert!(commands.contains("zone example.com"));
        assert!(commands.contains("update add www.example.com 300 A 192.168.1.1"));
        assert!(commands.contains("show\n"));
        assert!(commands.contains("send\n"));
    }

    #[test]
    fn test_build_nsupdate_commands_absent() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "old".to_string(),
            record_type: RecordType::A,
            value: None,
            ttl: 3600,
            state: State::Absent,
            key_name: None,
            key_secret: None,
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: None,
            weight: None,
            srv_port: None,
        };
        let commands = build_nsupdate_commands(&params).unwrap();
        assert!(commands.contains("update delete old.example.com A"));
    }

    #[test]
    fn test_build_nsupdate_commands_with_tsig() {
        let params = Params {
            server: "dns.example.com".to_string(),
            zone: "example.com".to_string(),
            record: "www".to_string(),
            record_type: RecordType::A,
            value: Some("192.168.1.1".to_string()),
            ttl: 300,
            state: State::Present,
            key_name: Some("mykey".to_string()),
            key_secret: Some("secret123".to_string()),
            key_algorithm: KeyAlgorithm::HmacSha256,
            port: 53,
            priority: None,
            weight: None,
            srv_port: None,
        };
        let commands = build_nsupdate_commands(&params).unwrap();
        assert!(commands.contains("server dns.example.com 53"));
        assert!(commands.contains("update add www.example.com 300 A 192.168.1.1"));
    }

    #[test]
    fn test_state_default() {
        assert_eq!(State::default(), State::Absent);
    }

    #[test]
    fn test_parse_params_ns_record() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            record: "@"
            type: NS
            value: ns1.example.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::NS);
    }

    #[test]
    fn test_parse_ptr_record() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: 1.168.192.in-addr.arpa
            record: "1"
            type: PTR
            value: www.example.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::PTR);
    }

    #[test]
    fn test_parse_caa_record() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            record: "@"
            type: CAA
            value: "0 issue letsencrypt.org"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.record_type, RecordType::CAA);
    }

    #[test]
    fn test_parse_key_algorithm_sha512() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            server: dns.example.com
            zone: example.com
            record: www
            type: A
            value: 192.168.1.1
            key_name: mykey
            key_secret: secret
            key_algorithm: hmac-sha512
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_algorithm, KeyAlgorithm::HmacSha512);
    }
}
