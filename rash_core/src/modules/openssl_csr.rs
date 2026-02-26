/// ANCHOR: module
/// # openssl_csr
///
/// Generate Certificate Signing Requests (CSRs).
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
/// - name: Generate CSR
///   openssl_csr:
///     path: /etc/ssl/server.csr
///     privatekey_path: /etc/ssl/private/server.key
///     common_name: example.com
///     country_name: US
///     organization_name: Example Corp
///     subject_alt_name:
///       - DNS:example.com
///       - DNS:www.example.com
///
/// - name: Generate CSR with key usage
///   openssl_csr:
///     path: /etc/ssl/server.csr
///     privatekey_path: /etc/ssl/private/server.key
///     common_name: example.com
///     key_usage:
///       - digitalSignature
///       - keyEncipherment
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
use rcgen::string::Ia5String;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to write the CSR to.
    pub path: String,
    /// Path to the private key to use for signing the CSR.
    pub privatekey_path: String,
    /// Passphrase for the private key if it is encrypted.
    pub privatekey_passphrase: Option<String>,
    /// Common Name (CN) for the certificate subject.
    pub common_name: Option<String>,
    /// Country Name (C) for the certificate subject (2-letter code).
    pub country_name: Option<String>,
    /// State or Province Name (ST) for the certificate subject.
    pub state_or_province_name: Option<String>,
    /// Locality Name (L) for the certificate subject (city).
    pub locality_name: Option<String>,
    /// Organization Name (O) for the certificate subject (company).
    pub organization_name: Option<String>,
    /// Organizational Unit Name (OU) for the certificate subject (department).
    pub organizational_unit_name: Option<String>,
    /// Email Address for the certificate subject.
    pub email_address: Option<String>,
    /// Subject Alternative Name entries.
    /// Format: TYPE:value (e.g., DNS:example.com, IP:192.168.1.1)
    pub subject_alt_name: Option<Vec<String>>,
    /// Key Usage extensions for the certificate.
    /// Valid values: digitalSignature, nonRepudiation, keyEncipherment,
    /// dataEncipherment, keyAgreement, keyCertSign, cRLSign
    pub key_usage: Option<Vec<String>>,
}

fn parse_san_entry(entry: &str) -> Result<rcgen::SanType> {
    let parts: Vec<&str> = entry.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid SAN entry format: {}. Expected TYPE:value", entry),
        ));
    }

    let san_type = parts[0].to_lowercase();
    let value = parts[1];

    match san_type.as_str() {
        "dns" => Ok(rcgen::SanType::DnsName(
            Ia5String::try_from(value).map_err(|e| {
                Error::new(ErrorKind::InvalidData, format!("Invalid DNS name: {}", e))
            })?,
        )),
        "ip" => {
            let ip: std::net::IpAddr = value.parse().map_err(|e| {
                Error::new(ErrorKind::InvalidData, format!("Invalid IP address: {}", e))
            })?;
            Ok(rcgen::SanType::IpAddress(ip))
        }
        "email" => Ok(rcgen::SanType::Rfc822Name(
            Ia5String::try_from(value).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid email address: {}", e),
                )
            })?,
        )),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Unsupported SAN type: {}. Supported types: DNS, IP, email",
                san_type
            ),
        )),
    }
}

fn parse_key_usage(usage: &str) -> Result<rcgen::KeyUsagePurpose> {
    match usage.to_lowercase().as_str() {
        "digitalsignature" => Ok(rcgen::KeyUsagePurpose::DigitalSignature),
        "nonrepudiation" => Ok(rcgen::KeyUsagePurpose::ContentCommitment),
        "keyencipherment" => Ok(rcgen::KeyUsagePurpose::KeyEncipherment),
        "dataencipherment" => Ok(rcgen::KeyUsagePurpose::DataEncipherment),
        "keyagreement" => Ok(rcgen::KeyUsagePurpose::KeyAgreement),
        "keycertsign" => Ok(rcgen::KeyUsagePurpose::KeyCertSign),
        "crlsign" => Ok(rcgen::KeyUsagePurpose::CrlSign),
        "encipheronly" => Ok(rcgen::KeyUsagePurpose::EncipherOnly),
        "decipheronly" => Ok(rcgen::KeyUsagePurpose::DecipherOnly),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Unsupported key usage: {}", usage),
        )),
    }
}

fn load_private_key(path: &str, passphrase: Option<&str>) -> Result<rcgen::KeyPair> {
    let key_data = fs::read_to_string(path).map_err(|e| {
        Error::new(
            ErrorKind::IOError,
            format!("Failed to read private key file {}: {}", path, e),
        )
    })?;

    if passphrase.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Encrypted private keys are not supported. Please decrypt the key first.",
        ));
    }

    rcgen::KeyPair::from_pem(&key_data).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse private key: {}", e),
        )
    })
}

fn generate_csr(params: &Params) -> Result<String> {
    let key_pair = load_private_key(
        &params.privatekey_path,
        params.privatekey_passphrase.as_deref(),
    )?;

    let mut params_builder = rcgen::CertificateParams::default();

    let mut dn = rcgen::DistinguishedName::new();
    if let Some(cn) = &params.common_name {
        dn.push(rcgen::DnType::CommonName, cn);
    }
    if let Some(c) = &params.country_name {
        dn.push(rcgen::DnType::CountryName, c);
    }
    if let Some(st) = &params.state_or_province_name {
        dn.push(rcgen::DnType::StateOrProvinceName, st);
    }
    if let Some(l) = &params.locality_name {
        dn.push(rcgen::DnType::LocalityName, l);
    }
    if let Some(o) = &params.organization_name {
        dn.push(rcgen::DnType::OrganizationName, o);
    }
    if let Some(ou) = &params.organizational_unit_name {
        dn.push(rcgen::DnType::OrganizationalUnitName, ou);
    }
    params_builder.distinguished_name = dn;

    if let Some(email) = &params.email_address {
        params_builder
            .subject_alt_names
            .push(rcgen::SanType::Rfc822Name(
                Ia5String::try_from(email.clone()).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Invalid email address: {}", e),
                    )
                })?,
            ));
    }

    if let Some(san_entries) = &params.subject_alt_name {
        let san_list: Result<Vec<rcgen::SanType>> = san_entries
            .iter()
            .map(|entry| parse_san_entry(entry))
            .collect();
        params_builder.subject_alt_names.extend(san_list?);
    }

    if let Some(key_usages) = &params.key_usage {
        let usage_list: Result<Vec<rcgen::KeyUsagePurpose>> = key_usages
            .iter()
            .map(|usage| parse_key_usage(usage))
            .collect();
        params_builder.key_usages = usage_list?;
    }

    let csr = params_builder.serialize_request(&key_pair).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to generate CSR: {}", e),
        )
    })?;

    csr.pem().map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to serialize CSR to PEM: {}", e),
        )
    })
}

fn read_existing_csr(path: &str) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::new(
            ErrorKind::IOError,
            format!("Failed to read CSR file {}: {}", path, e),
        )),
    }
}

fn openssl_csr(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let new_csr = generate_csr(&params)?;
    let existing_csr = read_existing_csr(&params.path)?;

    match existing_csr {
        Some(existing) if existing.trim() == new_csr.trim() => {
            return Ok(ModuleResult::new(false, None, Some(params.path)));
        }
        Some(existing) => {
            diff(existing.trim().to_string(), new_csr.trim().to_string());
        }
        None => {
            diff("(absent)".to_string(), new_csr.trim().to_string());
        }
    }

    if !check_mode {
        if let Some(parent) = Path::new(&params.path).parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).map_err(|e| {
                Error::new(
                    ErrorKind::IOError,
                    format!("Failed to create directory {}: {}", parent.display(), e),
                )
            })?;
        }
        fs::write(&params.path, &new_csr).map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to write CSR to {}: {}", params.path, e),
            )
        })?;
    }

    Ok(ModuleResult::new(true, None, Some(params.path)))
}

#[derive(Debug)]
pub struct OpensslCsr;

impl Module for OpensslCsr {
    fn get_name(&self) -> &str {
        "openssl_csr"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;
        Ok((openssl_csr(params, check_mode)?, None))
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

    fn generate_test_key(path: &std::path::Path) -> rcgen::KeyPair {
        let key_pair = rcgen::KeyPair::generate().unwrap();
        fs::write(path, key_pair.serialize_pem()).unwrap();
        key_pair
    }

    #[test]
    fn test_parse_san_entry_dns() {
        let san = parse_san_entry("DNS:example.com").unwrap();
        match san {
            rcgen::SanType::DnsName(name) => assert_eq!(name.as_ref(), "example.com"),
            _ => panic!("Expected DnsName"),
        }
    }

    #[test]
    fn test_parse_san_entry_ip() {
        let san = parse_san_entry("IP:192.168.1.1").unwrap();
        match san {
            rcgen::SanType::IpAddress(ip) => {
                assert_eq!(ip.to_string(), "192.168.1.1");
            }
            _ => panic!("Expected IpAddress"),
        }
    }

    #[test]
    fn test_parse_san_entry_email() {
        let san = parse_san_entry("email:test@example.com").unwrap();
        match san {
            rcgen::SanType::Rfc822Name(email) => assert_eq!(email.as_ref(), "test@example.com"),
            _ => panic!("Expected Rfc822Name"),
        }
    }

    #[test]
    fn test_parse_san_entry_invalid() {
        let result = parse_san_entry("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_key_usage() {
        assert!(matches!(
            parse_key_usage("digitalSignature"),
            Ok(rcgen::KeyUsagePurpose::DigitalSignature)
        ));
        assert!(matches!(
            parse_key_usage("keyEncipherment"),
            Ok(rcgen::KeyUsagePurpose::KeyEncipherment)
        ));
        assert!(parse_key_usage("invalid").is_err());
    }

    #[test]
    fn test_generate_csr_basic() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let csr_path = dir.path().join("test.csr");

        generate_test_key(&key_path);

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: key_path.to_string_lossy().to_string(),
            privatekey_passphrase: None,
            common_name: Some("example.com".to_string()),
            country_name: Some("US".to_string()),
            organization_name: Some("Example Corp".to_string()),
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: None,
            key_usage: None,
        };

        let result = openssl_csr(params, false).unwrap();
        assert!(result.get_changed());
        assert!(csr_path.exists());
    }

    #[test]
    fn test_generate_csr_with_san() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let csr_path = dir.path().join("test.csr");

        generate_test_key(&key_path);

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: key_path.to_string_lossy().to_string(),
            privatekey_passphrase: None,
            common_name: Some("example.com".to_string()),
            country_name: None,
            organization_name: None,
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: Some(vec![
                "DNS:example.com".to_string(),
                "DNS:www.example.com".to_string(),
            ]),
            key_usage: None,
        };

        let result = openssl_csr(params, false).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_generate_csr_twice() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let csr_path = dir.path().join("test.csr");

        generate_test_key(&key_path);

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: key_path.to_string_lossy().to_string(),
            privatekey_passphrase: None,
            common_name: Some("example.com".to_string()),
            country_name: None,
            organization_name: None,
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: None,
            key_usage: None,
        };

        let result1 = openssl_csr(params.clone(), false).unwrap();
        assert!(result1.get_changed());

        let result2 = openssl_csr(params, false).unwrap();
        assert!(result2.get_changed());
        assert!(csr_path.exists());
    }

    #[test]
    fn test_generate_csr_check_mode() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let csr_path = dir.path().join("test.csr");

        generate_test_key(&key_path);

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: key_path.to_string_lossy().to_string(),
            privatekey_passphrase: None,
            common_name: Some("example.com".to_string()),
            country_name: None,
            organization_name: None,
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: None,
            key_usage: None,
        };

        let result = openssl_csr(params, true).unwrap();
        assert!(result.get_changed());
        assert!(!csr_path.exists());
    }

    #[test]
    fn test_generate_csr_missing_key() {
        let dir = tempdir().unwrap();
        let csr_path = dir.path().join("test.csr");

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: "/nonexistent/key.pem".to_string(),
            privatekey_passphrase: None,
            common_name: Some("example.com".to_string()),
            country_name: None,
            organization_name: None,
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: None,
            key_usage: None,
        };

        let result = openssl_csr(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_key_not_supported() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");
        let csr_path = dir.path().join("test.csr");

        generate_test_key(&key_path);

        let params = Params {
            path: csr_path.to_string_lossy().to_string(),
            privatekey_path: key_path.to_string_lossy().to_string(),
            privatekey_passphrase: Some("secret".to_string()),
            common_name: Some("example.com".to_string()),
            country_name: None,
            organization_name: None,
            state_or_province_name: None,
            locality_name: None,
            organizational_unit_name: None,
            email_address: None,
            subject_alt_name: None,
            key_usage: None,
        };

        let result = openssl_csr(params, false);
        assert!(result.is_err());
    }
}
