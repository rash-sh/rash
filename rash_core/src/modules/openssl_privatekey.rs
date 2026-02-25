/// ANCHOR: module
/// # openssl_privatekey
///
/// Generate OpenSSL private keys.
///
/// This module generates SSL/TLS private keys using pure Rust implementation
/// (no OpenSSL dependency required). Supports RSA and ECC key types.
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
/// - name: Generate RSA private key with default size (4096)
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///
/// - name: Generate RSA private key with custom size
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     size: 2048
///
/// - name: Generate ECC private key
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     type: ECC
///
/// - name: Generate key with custom permissions
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     size: 4096
///     mode: "0600"
///
/// - name: Force regenerate key
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     force: true
///
/// - name: Remove private key
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, remove_file, set_permissions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_RSA_SIZE: u32 = 4096;
const DEFAULT_MODE: u32 = 0o600;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Absolute path to the private key file.
    path: String,
    /// Type of private key to generate.
    /// **[default: `"RSA"`]**
    #[serde(rename = "type", default)]
    key_type: KeyType,
    /// Size (in bits) of the RSA key to generate.
    /// Only used when type is RSA.
    /// **[default: `4096`]**
    #[serde(default = "default_size")]
    size: u32,
    /// Permissions of the private key file.
    /// **[default: `"0600"`]**
    mode: Option<String>,
    /// Force regeneration of the private key even if it already exists.
    #[serde(default)]
    force: bool,
    /// If _absent_, the private key file will be removed.
    /// If _present_, the private key will be generated if it does not exist.
    /// **[default: `"present"`]**
    state: Option<State>,
}

fn default_size() -> u32 {
    DEFAULT_RSA_SIZE
}

#[derive(Debug, PartialEq, Deserialize, Default)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum KeyType {
    #[default]
    Rsa,
    Ecc,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
}

fn generate_rsa_key(size: u32) -> Result<String> {
    let key_size = match size {
        2048 => rcgen::RsaKeySize::_2048,
        3072 => rcgen::RsaKeySize::_3072,
        4096 => rcgen::RsaKeySize::_4096,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Unsupported RSA key size: {}. Supported sizes: 2048, 3072, 4096",
                    size
                ),
            ));
        }
    };
    let key_pair = rcgen::KeyPair::generate_rsa_for(&rcgen::PKCS_RSA_SHA256, key_size)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    Ok(key_pair.serialize_pem())
}

fn generate_ecc_key() -> Result<String> {
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    Ok(key_pair.serialize_pem())
}

fn is_valid_pem_private_key(content: &str) -> bool {
    content.contains("-----BEGIN ") && content.contains(" PRIVATE KEY-----")
}

fn exec_present(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path = Path::new(&params.path);
    let octal_mode = match &params.mode {
        Some(mode) => parse_octal(mode)?,
        None => DEFAULT_MODE,
    };

    if path.exists() && !params.force {
        let content = std::fs::read_to_string(&params.path)?;
        if is_valid_pem_private_key(&content) {
            return Ok(ModuleResult::new(false, None, Some(params.path)));
        }
    }

    if path.exists() && params.force {
        diff("existing key\n", "new key (forced)\n");
    } else {
        diff("absent\n", "present\n");
    }

    if check_mode {
        return Ok(ModuleResult::new(true, None, Some(params.path)));
    }

    let key_content = match params.key_type {
        KeyType::Rsa => generate_rsa_key(params.size)?,
        KeyType::Ecc => generate_ecc_key()?,
    };

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = File::create(&params.path)?;
    file.write_all(key_content.as_bytes())?;

    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(octal_mode);
    set_permissions(&params.path, permissions)?;

    Ok(ModuleResult::new(true, None, Some(params.path)))
}

fn exec_absent(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path = Path::new(&params.path);

    if !path.exists() {
        return Ok(ModuleResult::new(false, None, Some(params.path)));
    }

    diff("present\n", "absent\n");

    if check_mode {
        return Ok(ModuleResult::new(true, None, Some(params.path)));
    }

    remove_file(&params.path)?;
    Ok(ModuleResult::new(true, None, Some(params.path)))
}

#[derive(Debug)]
pub struct OpenSslPrivateKey;

impl Module for OpenSslPrivateKey {
    fn get_name(&self) -> &str {
        "openssl_privatekey"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let params: Params = parse_params(optional_params)?;

        let result = match params.state {
            Some(State::Absent) | None if params.force => {
                if Path::new(&params.path).exists() {
                    exec_absent(params, check_mode)?
                } else {
                    exec_present(params, check_mode)?
                }
            }
            Some(State::Absent) => exec_absent(params, check_mode)?,
            Some(State::Present) | None => exec_present(params, check_mode)?,
        };

        Ok((result, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::metadata;

    use tempfile::tempdir;

    #[test]
    fn test_parse_params_rsa() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            size: 2048
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/private/server.key");
        assert_eq!(params.key_type, KeyType::Rsa);
        assert_eq!(params.size, 2048);
        assert!(!params.force);
    }

    #[test]
    fn test_parse_params_ecc() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            type: ecc
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/private/server.key");
        assert_eq!(params.key_type, KeyType::Ecc);
        assert_eq!(params.size, DEFAULT_RSA_SIZE);
    }

    #[test]
    fn test_parse_params_with_mode() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            mode: "0644"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.mode, Some("0644".to_owned()));
    }

    #[test]
    fn test_parse_params_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_generate_rsa_key() {
        let key = generate_rsa_key(2048).unwrap();
        assert!(is_valid_pem_private_key(&key));
        assert!(key.contains("PRIVATE KEY"));
    }

    #[test]
    fn test_generate_ecc_key() {
        let key = generate_ecc_key().unwrap();
        assert!(is_valid_pem_private_key(&key));
        assert!(key.contains("PRIVATE KEY"));
    }

    #[test]
    fn test_exec_present_creates_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };

        let result = exec_present(params, false).unwrap();
        assert!(result.get_changed());
        assert!(key_path.exists());

        let content = std::fs::read_to_string(&key_path).unwrap();
        assert!(is_valid_pem_private_key(&content));
    }

    #[test]
    fn test_exec_present_check_mode() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_check.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };

        let result = exec_present(params, true).unwrap();
        assert!(result.get_changed());
        assert!(!key_path.exists());
    }

    #[test]
    fn test_exec_present_existing_key_no_force() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_existing.key");

        let params_create = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };
        exec_present(params_create, false).unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };

        let result = exec_present(params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_exec_present_existing_key_with_force() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_force.key");

        let params_create = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };
        exec_present(params_create, false).unwrap();

        let original_content = std::fs::read_to_string(&key_path).unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: true,
            state: Some(State::Present),
        };

        let result = exec_present(params, false).unwrap();
        assert!(result.get_changed());

        let new_content = std::fs::read_to_string(&key_path).unwrap();
        assert_ne!(original_content, new_content);
    }

    #[test]
    fn test_exec_present_sets_permissions() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_perms.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: Some("0600".to_owned()),
            force: false,
            state: Some(State::Present),
        };

        exec_present(params, false).unwrap();

        let meta = metadata(&key_path).unwrap();
        let mode = meta.permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn test_exec_absent_removes_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_remove.key");

        let params_create = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };
        exec_present(params_create, false).unwrap();
        assert!(key_path.exists());

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Absent),
        };

        let result = exec_absent(params, false).unwrap();
        assert!(result.get_changed());
        assert!(!key_path.exists());
    }

    #[test]
    fn test_exec_absent_check_mode() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_remove_check.key");

        let params_create = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Present),
        };
        exec_present(params_create, false).unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Absent),
        };

        let result = exec_absent(params, true).unwrap();
        assert!(result.get_changed());
        assert!(key_path.exists());
    }

    #[test]
    fn test_exec_absent_nonexistent_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("nonexistent.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_owned(),
            key_type: KeyType::Rsa,
            size: 2048,
            mode: None,
            force: false,
            state: Some(State::Absent),
        };

        let result = exec_absent(params, false).unwrap();
        assert!(!result.get_changed());
    }
}
