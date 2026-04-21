/// ANCHOR: module
/// # certbot
///
/// Manage SSL/TLS certificates using Let's Encrypt via certbot.
///
/// This module automates obtaining and renewing TLS certificates from Let's Encrypt
/// using the certbot tool. It supports both HTTP-01 and DNS-01 challenges and is
/// idempotent — it will only request a new certificate when one does not already exist
/// or is within `expire_days` of expiration.
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
/// - name: Obtain certificate for example.com
///   certbot:
///     domains:
///       - example.com
///       - www.example.com
///     email: admin@example.com
///
/// - name: Obtain certificate with DNS challenge
///   certbot:
///     domains:
///       - example.com
///     email: admin@example.com
///     challenge: dns
///     expire_days: 14
///
/// - name: Remove certificate for example.com
///   certbot:
///     domains:
///       - example.com
///     email: admin@example.com
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use std::time::{SystemTime, UNIX_EPOCH};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use serde_norway::value;

const CERTBOT_BIN: &str = "certbot";
const DEFAULT_CERT_DIR: &str = "/etc/letsencrypt/live";
const DEFAULT_EXPIRE_DAYS: u64 = 30;

#[derive(Default, Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    Absent,
    #[default]
    Present,
}

fn default_state() -> Option<State> {
    Some(State::default())
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// List of domain names for the certificate.
    pub domains: Vec<String>,
    /// Email address for Let's Encrypt registration and notifications.
    pub email: String,
    /// Challenge type to use for domain validation.
    /// **[default: `"http"`]**
    #[serde(default = "default_challenge")]
    pub challenge: String,
    /// Renew the certificate if it expires within this many days.
    /// **[default: `30`]**
    #[serde(default = "default_expire_days")]
    pub expire_days: u64,
    /// Whether the certificate should exist or not.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
}

fn default_challenge() -> String {
    "http".to_string()
}

fn default_expire_days() -> u64 {
    DEFAULT_EXPIRE_DAYS
}

fn primary_domain(domains: &[String]) -> &str {
    domains.first().map_or("", |v| v.as_str())
}

fn fullchain_path(domain: &str) -> String {
    format!("{}/{}/fullchain.pem", DEFAULT_CERT_DIR, domain)
}

fn check_certificate_expiry(domain: &str, expire_days: u64) -> Result<bool> {
    let chain_path = fullchain_path(domain);
    if !Path::new(&chain_path).exists() {
        return Ok(true);
    }

    let output = StdCommand::new("openssl")
        .args(["x509", "-enddate", "-noout", "-in", &chain_path])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Ok(true);
    }

    let expiry_str = String::from_utf8_lossy(&output.stdout);
    let needs_renewal = parse_expiry_days(&expiry_str, expire_days)?;
    Ok(needs_renewal)
}

fn parse_expiry_days(expiry_str: &str, expire_days: u64) -> Result<bool> {
    let date_part = expiry_str
        .trim()
        .strip_prefix("notAfter=")
        .unwrap_or(expiry_str.trim());

    let output = StdCommand::new("date")
        .args(["-d", date_part, "+%s"])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Ok(true);
    }

    let expiry_ts: i64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    let now_ts: i64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let remaining_days = (expiry_ts - now_ts) / 86400;
    Ok(remaining_days <= expire_days as i64)
}

fn obtain_certificate(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let domain = primary_domain(&params.domains);
    let chain_path = fullchain_path(domain);

    let needs_renewal = check_certificate_expiry(domain, params.expire_days)?;

    if !needs_renewal {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Certificate for {} is valid", domain)),
            extra: None,
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!(
                "Would obtain certificate for: {}",
                params.domains.join(", ")
            )),
            extra: None,
        });
    }

    let mut cmd = StdCommand::new(CERTBOT_BIN);
    cmd.args(["certonly", "--non-interactive", "--agree-tos"]);

    match params.challenge.as_str() {
        "dns" => {
            cmd.arg("--manual");
            cmd.arg("--preferred-challenges").arg("dns");
        }
        _ => {
            cmd.arg("--standalone");
            cmd.arg("--preferred-challenges").arg("http");
        }
    }

    cmd.arg("--email").arg(&params.email);

    for d in &params.domains {
        cmd.arg("-d").arg(d);
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute certbot: {}", e),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("certbot failed: {}", stderr.trim()),
        ));
    }

    let extra = Some(value::to_value(json!({
        "rc": output.status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "certificate_path": chain_path,
    }))?);

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Certificate obtained for {}", domain)),
        extra,
    })
}

fn remove_certificate(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let domain = primary_domain(&params.domains);
    let chain_path = fullchain_path(domain);
    let cert_exists = Path::new(&chain_path).exists();

    if !cert_exists {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Certificate for {} does not exist", domain)),
            extra: None,
        });
    }

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would remove certificate for {}", domain)),
            extra: None,
        });
    }

    let output = StdCommand::new(CERTBOT_BIN)
        .args(["delete", "--non-interactive", "--cert-name", domain])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute certbot: {}", e),
            )
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let live_dir = format!("{}/{}", DEFAULT_CERT_DIR, domain);
        let archive_dir = format!("/etc/letsencrypt/archive/{}", domain);
        let _ = fs::remove_dir_all(&live_dir);
        let _ = fs::remove_dir_all(&archive_dir);
        let _ = fs::remove_file(format!("/etc/letsencrypt/renewal/{}.conf", domain));

        if !Path::new(&fullchain_path(domain)).exists() {
            return Ok(ModuleResult {
                changed: true,
                output: Some(format!(
                    "Certificate files removed for {} (fallback)",
                    domain
                )),
                extra: None,
            });
        }

        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("certbot delete failed: {}", stderr.trim()),
        ));
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Certificate removed for {}", domain)),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Certbot;

impl Module for Certbot {
    fn get_name(&self) -> &str {
        "certbot"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        if params.domains.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "domains must not be empty",
            ));
        }

        match params.state.unwrap_or_default() {
            State::Present => Ok((obtain_certificate(&params, check_mode)?, None)),
            State::Absent => Ok((remove_certificate(&params, check_mode)?, None)),
        }
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
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
              - www.example.com
            email: admin@example.com
            challenge: http
            expire_days: 30
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.domains, vec!["example.com", "www.example.com"]);
        assert_eq!(params.email, "admin@example.com");
        assert_eq!(params.challenge, "http");
        assert_eq!(params.expire_days, 30);
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            email: admin@example.com
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.domains, vec!["example.com"]);
        assert_eq!(params.email, "admin@example.com");
        assert_eq!(params.challenge, "http");
        assert_eq!(params.expire_days, 30);
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_dns_challenge() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            email: admin@example.com
            challenge: dns
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.challenge, "dns");
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            email: admin@example.com
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_missing_domains() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            email: admin@example.com
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_missing_email() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            email: admin@example.com
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_primary_domain() {
        let domains = vec!["example.com".to_string(), "www.example.com".to_string()];
        assert_eq!(primary_domain(&domains), "example.com");
    }

    #[test]
    fn test_fullchain_path() {
        assert_eq!(
            fullchain_path("example.com"),
            "/etc/letsencrypt/live/example.com/fullchain.pem"
        );
    }

    #[test]
    fn test_invalid_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains:
              - example.com
            email: admin@example.com
            state: invalid
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_empty_domains() {
        let certbot = Certbot;
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            domains: []
            email: admin@example.com
            "#,
        )
        .unwrap();
        let error = certbot
            .exec(&GlobalParams::default(), yaml, &Value::UNDEFINED, false)
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_challenge() {
        assert_eq!(default_challenge(), "http");
    }

    #[test]
    fn test_default_expire_days() {
        assert_eq!(default_expire_days(), 30);
    }

    #[test]
    fn test_default_state() {
        assert_eq!(default_state(), Some(State::Present));
    }
}
