/// ANCHOR: module
/// # openssl_certificate
///
/// Generate and manage SSL/TLS certificates.
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
/// - name: Generate self-signed certificate
///   openssl_certificate:
///     path: /etc/ssl/certs/server.crt
///     privatekey_path: /etc/ssl/private/server.key
///     common_name: example.com
///     provider: selfsigned
///     valid_in: 365
///
/// - name: Generate self-signed certificate with custom settings
///   openssl_certificate:
///     path: /etc/ssl/certs/server.crt
///     privatekey_path: /etc/ssl/private/server.key
///     common_name: example.com
///     provider: selfsigned
///     valid_in: 365
///     mode: "0644"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, File, OpenOptions, set_permissions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use minijinja::Value;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use time::Duration;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Absolute path to the certificate file.
    pub path: String,
    /// Path to the private key file.
    pub privatekey_path: String,
    /// Common Name (CN) for the certificate.
    pub common_name: String,
    /// Name of the provider to use.
    /// **[default: `"selfsigned"`]**
    pub provider: Option<Provider>,
    /// Number of days the certificate is valid.
    /// **[default: `365`]**
    pub valid_in: Option<u32>,
    /// Permissions of the certificate file.
    pub mode: Option<String>,
    /// Owner of the certificate file (name, not UID).
    pub owner: Option<String>,
    /// Group of the certificate file (name, not GID).
    pub group: Option<String>,
    /// Whether to force regeneration even if certificate exists.
    /// **[default: `false`]**
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Selfsigned,
}

fn read_file_content(path: &str) -> Result<String> {
    let mut content = String::new();
    File::open(path)
        .and_then(|mut f| f.read_to_string(&mut content))
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to read file '{}': {}", path, e),
            )
        })?;
    Ok(content)
}

fn extract_pem_body(pem_content: &str, label: &str) -> Option<String> {
    let start_marker = format!("-----BEGIN {}-----", label);
    let end_marker = format!("-----END {}-----", label);

    let start = pem_content.find(&start_marker)?;
    let end = pem_content.find(&end_marker)? + end_marker.len();

    Some(pem_content[start..end].to_string())
}

fn generate_self_signed_certificate(
    privatekey_content: &str,
    common_name: &str,
    valid_in_days: u32,
) -> Result<String> {
    let private_key_pem = extract_pem_body(privatekey_content, "PRIVATE KEY")
        .or_else(|| extract_pem_body(privatekey_content, "RSA PRIVATE KEY"))
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                "No valid private key found in privatekey_path",
            )
        })?;

    let key_pair = KeyPair::from_pem(&private_key_pem).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse private key: {}", e),
        )
    })?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    params.distinguished_name = dn;

    params.not_before = time::OffsetDateTime::now_utc() - Duration::seconds(24 * 60 * 60);
    params.not_after =
        time::OffsetDateTime::now_utc() + Duration::days(valid_in_days as i64);

    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);

    let cert = params.self_signed(&key_pair).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to generate certificate: {}", e),
        )
    })?;

    Ok(cert.pem())
}

fn apply_file_permissions(path: &Path, mode: Option<&str>) -> Result<()> {
    if let Some(mode_str) = mode {
        let octal_mode = parse_octal(mode_str)?;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(octal_mode);
        set_permissions(path, permissions)?;
    }
    Ok(())
}

fn generate_certificate(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let cert_path = Path::new(&params.path);

    if !params.force && cert_path.exists() {
        let existing_content = read_file_content(&params.path)?;
        if !existing_content.is_empty() {
            return Ok(ModuleResult {
                changed: false,
                output: Some(params.path.clone()),
                extra: None,
            });
        }
    }

    let privatekey_content = read_file_content(&params.privatekey_path)?;
    let valid_in = params.valid_in.unwrap_or(365);

    let certificate =
        generate_self_signed_certificate(&privatekey_content, &params.common_name, valid_in)?;

    if cert_path.exists() {
        let existing = read_file_content(&params.path)?;
        if existing == certificate {
            return Ok(ModuleResult {
                changed: false,
                output: Some(params.path.clone()),
                extra: None,
            });
        }
        diff(&existing, &certificate);
    } else {
        diff("", &certificate);
    }

    if !check_mode {
        if let Some(parent) = cert_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(cert_path)?;
        file.write_all(certificate.as_bytes())?;

        apply_file_permissions(cert_path, params.mode.as_deref())?;
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(params.path.clone()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct OpensslCertificate;

impl Module for OpensslCertificate {
    fn get_name(&self) -> &str {
        "openssl_certificate"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        let provider = params.provider.clone().unwrap_or_default();

        match provider {
            Provider::Selfsigned => {
                let result = generate_certificate(&params, check_mode)?;
                Ok((result, None))
            }
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
    use tempfile::tempdir;

    fn generate_test_key(dir: &std::path::Path) -> String {
        let key_pair = KeyPair::generate().unwrap();
        let private_key_pem = key_pair.serialize_pem();

        let key_path = dir.join("test.key");
        fs::write(&key_path, &private_key_pem).unwrap();

        key_path.to_string_lossy().to_string()
    }

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/certs/server.crt
            privatekey_path: /etc/ssl/private/server.key
            common_name: example.com
            provider: selfsigned
            valid_in: 365
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/certs/server.crt");
        assert_eq!(params.common_name, "example.com");
        assert_eq!(params.provider, Some(Provider::Selfsigned));
        assert_eq!(params.valid_in, Some(365));
    }

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/certs/server.crt
            privatekey_path: /etc/ssl/private/server.key
            common_name: example.com
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/certs/server.crt");
        assert_eq!(params.common_name, "example.com");
        assert_eq!(params.provider, None);
        assert_eq!(params.valid_in, None);
    }

    #[test]
    fn test_parse_params_with_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/certs/server.crt
            privatekey_path: /etc/ssl/private/server.key
            common_name: example.com
            mode: "0644"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Some("0644".to_string()));
    }

    #[test]
    fn test_generate_certificate() {
        let dir = tempdir().unwrap();
        let key_path = generate_test_key(dir.path());
        let cert_path = dir.path().join("server.crt");

        let params = Params {
            path: cert_path.to_string_lossy().to_string(),
            privatekey_path: key_path,
            common_name: "test.example.com".to_string(),
            provider: Some(Provider::Selfsigned),
            valid_in: Some(365),
            mode: None,
            owner: None,
            group: None,
            force: false,
        };

        let result = generate_certificate(&params, false).unwrap();
        assert!(result.changed);
        assert!(cert_path.exists());

        let content = fs::read_to_string(&cert_path).unwrap();
        assert!(content.contains("-----BEGIN CERTIFICATE-----"));
        assert!(content.contains("-----END CERTIFICATE-----"));
    }

    #[test]
    fn test_generate_certificate_check_mode() {
        let dir = tempdir().unwrap();
        let key_path = generate_test_key(dir.path());
        let cert_path = dir.path().join("server.crt");

        let params = Params {
            path: cert_path.to_string_lossy().to_string(),
            privatekey_path: key_path,
            common_name: "test.example.com".to_string(),
            provider: Some(Provider::Selfsigned),
            valid_in: Some(365),
            mode: None,
            owner: None,
            group: None,
            force: false,
        };

        let result = generate_certificate(&params, true).unwrap();
        assert!(result.changed);
        assert!(!cert_path.exists());
    }

    #[test]
    fn test_generate_certificate_no_change_when_exists() {
        let dir = tempdir().unwrap();
        let key_path = generate_test_key(dir.path());
        let cert_path = dir.path().join("server.crt");

        let params = Params {
            path: cert_path.to_string_lossy().to_string(),
            privatekey_path: key_path.clone(),
            common_name: "test.example.com".to_string(),
            provider: Some(Provider::Selfsigned),
            valid_in: Some(365),
            mode: None,
            owner: None,
            group: None,
            force: false,
        };

        let result1 = generate_certificate(&params, false).unwrap();
        assert!(result1.changed);

        let result2 = generate_certificate(&params, false).unwrap();
        assert!(!result2.changed);
    }

    #[test]
    fn test_generate_certificate_force_regenerate() {
        let dir = tempdir().unwrap();
        let key_path = generate_test_key(dir.path());
        let cert_path = dir.path().join("server.crt");

        let params = Params {
            path: cert_path.to_string_lossy().to_string(),
            privatekey_path: key_path.clone(),
            common_name: "test.example.com".to_string(),
            provider: Some(Provider::Selfsigned),
            valid_in: Some(365),
            mode: None,
            owner: None,
            group: None,
            force: false,
        };

        let result1 = generate_certificate(&params, false).unwrap();
        assert!(result1.changed);

        let params_force = Params {
            force: true,
            ..params
        };

        let result2 = generate_certificate(&params_force, false).unwrap();
        assert!(result2.changed);
    }

    #[test]
    fn test_extract_pem_body() {
        let pem = "-----BEGIN CERTIFICATE-----\nMIIBkTCB+wIJAKHBfp...\n-----END CERTIFICATE-----\n";
        let result = extract_pem_body(pem, "CERTIFICATE");
        assert!(result.is_some());
        assert!(result.unwrap().contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn test_extract_pem_body_not_found() {
        let pem = "some random text";
        let result = extract_pem_body(pem, "CERTIFICATE");
        assert!(result.is_none());
    }
}
