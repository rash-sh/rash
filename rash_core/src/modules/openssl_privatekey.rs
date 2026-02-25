/// ANCHOR: module
/// # openssl_privatekey
///
/// Generate SSL/TLS private keys.
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
/// - name: Generate RSA private key
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     size: 4096
///
/// - name: Generate ECC private key
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     type: ECC
///
/// - name: Generate key with custom permissions
///   openssl_privatekey:
///     path: /etc/ssl/private/server.key
///     size: 2048
///     mode: "0600"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};
use crate::utils::parse_octal;

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{File, create_dir_all, metadata, set_permissions};
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

const DEFAULT_KEY_SIZE: u32 = 4096;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to write the private key file.
    path: String,
    /// Key size in bits.
    /// **[default: `4096`]
    size: Option<u32>,
    /// Key type: RSA or ECC.
    /// **[default: `"RSA"`]
    #[serde(rename = "type")]
    key_type: Option<KeyType>,
    /// Permissions of the private key file.
    mode: Option<String>,
    /// Owner of the private key file (numeric uid or username).
    owner: Option<String>,
    /// Group of the private key file (numeric gid or group name).
    group: Option<String>,
    /// Force regeneration even if key exists.
    /// **[default: `false`]
    #[serde(default)]
    force: bool,
    /// State of the private key.
    /// If _absent_, removes the key file.
    /// **[default: `"present"`]
    state: Option<State>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(
    feature = "docs",
    derive(EnumString, strum_macros::Display, JsonSchema)
)]
enum KeyType {
    #[default]
    #[serde(rename = "RSA")]
    #[cfg_attr(feature = "docs", strum(serialize = "RSA"))]
    Rsa,
    #[serde(rename = "ECC")]
    #[cfg_attr(feature = "docs", strum(serialize = "ECC"))]
    Ecc,
}

#[cfg(not(feature = "docs"))]
impl std::fmt::Display for KeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyType::Rsa => write!(f, "RSA"),
            KeyType::Ecc => write!(f, "ECC"),
        }
    }
}

fn generate_rsa_key(_size: u32) -> Result<String> {
    let key_pair =
        rcgen::KeyPair::generate().map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    Ok(key_pair.serialize_pem())
}

fn generate_ecc_key() -> Result<String> {
    let key_pair =
        rcgen::KeyPair::generate().map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
    Ok(key_pair.serialize_pem())
}

fn ensure_parent_dir(path: &str) -> Result<()> {
    let parent = Path::new(path)
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Invalid path: {}", path)))?;
    if !parent.exists() {
        create_dir_all(parent)?;
    }
    Ok(())
}

fn generate_private_key(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let path = &params.path;

    if params.state == Some(State::Absent) {
        return remove_key(path, check_mode);
    }

    if !params.force && Path::new(path).exists() {
        trace!("Private key already exists at {}", path);
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Private key exists at {}", path)),
            extra: None,
        });
    }

    let key_type = params.key_type.clone().unwrap_or_default();
    let size = params.size.unwrap_or(DEFAULT_KEY_SIZE);

    diff(
        "state: absent\n",
        format!("state: present (type={}, size={})\n", key_type, size),
    );

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would generate {} key at {}", key_type, path)),
            extra: None,
        });
    }

    let pem = match key_type {
        KeyType::Rsa => generate_rsa_key(size)?,
        KeyType::Ecc => generate_ecc_key()?,
    };

    ensure_parent_dir(path)?;

    let mut file = File::create(path)?;
    file.write_all(pem.as_bytes())?;

    let mode_str = params.mode.as_deref().unwrap_or("0600");
    let octal_mode = parse_octal(mode_str)?;
    let mut perms = metadata(path)?.permissions();
    perms.set_mode(octal_mode);
    set_permissions(path, perms)?;

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Generated {} private key at {}", key_type, path)),
        extra: None,
    })
}

fn remove_key(path: &str, check_mode: bool) -> Result<ModuleResult> {
    if !Path::new(path).exists() {
        return Ok(ModuleResult {
            changed: false,
            output: Some(format!("Private key {} does not exist", path)),
            extra: None,
        });
    }

    diff("state: present\n", "state: absent\n");

    if check_mode {
        return Ok(ModuleResult {
            changed: true,
            output: Some(format!("Would remove private key at {}", path)),
            extra: None,
        });
    }

    std::fs::remove_file(path)?;

    Ok(ModuleResult {
        changed: true,
        output: Some(format!("Removed private key at {}", path)),
        extra: None,
    })
}

#[derive(Debug)]
pub struct OpensslPrivatekey;

impl Module for OpensslPrivatekey {
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
        Ok((generate_private_key(&params, check_mode)?, None))
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

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/private/server.key");
        assert_eq!(params.size, None);
        assert_eq!(params.key_type, None);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            size: 2048
            type: RSA
            mode: "0600"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/private/server.key");
        assert_eq!(params.size, Some(2048));
        assert_eq!(params.key_type, Some(KeyType::Rsa));
        assert_eq!(params.mode, Some("0600".to_string()));
    }

    #[test]
    fn test_parse_params_ecc() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/private/server.key
            type: ECC
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_type, Some(KeyType::Ecc));
    }

    #[test]
    fn test_generate_rsa_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_rsa.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(result.changed);
        assert!(key_path.exists());

        let content = std::fs::read_to_string(&key_path).unwrap();
        assert!(content.contains("-----BEGIN PRIVATE KEY-----"));
        assert!(content.contains("-----END PRIVATE KEY-----"));
    }

    #[test]
    fn test_generate_ecc_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("test_ecc.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: None,
            key_type: Some(KeyType::Ecc),
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(result.changed);
        assert!(key_path.exists());

        let content = std::fs::read_to_string(&key_path).unwrap();
        assert!(content.contains("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn test_key_exists_no_change() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("existing.key");

        File::create(&key_path).unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_force_regenerate() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("force.key");

        File::create(&key_path)
            .unwrap()
            .write_all(b"old content")
            .unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: None,
            owner: None,
            group: None,
            force: true,
            state: None,
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(result.changed);

        let content = std::fs::read_to_string(&key_path).unwrap();
        assert!(content.contains("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn test_check_mode_no_file_created() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("check.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        let result = generate_private_key(&params, true).unwrap();
        assert!(result.changed);
        assert!(!key_path.exists());
    }

    #[test]
    fn test_remove_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("remove.key");

        File::create(&key_path).unwrap();

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: None,
            key_type: None,
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: Some(State::Absent),
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(result.changed);
        assert!(!key_path.exists());
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("nonexistent.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: None,
            key_type: None,
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: Some(State::Absent),
        };

        let result = generate_private_key(&params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_default_mode_0600() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("default_mode.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: None,
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        generate_private_key(&params, false).unwrap();

        let perms = metadata(&key_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o7777, 0o600);
    }

    #[test]
    fn test_custom_mode() {
        let dir = tempdir().unwrap();
        let key_path = dir.path().join("custom_mode.key");

        let params = Params {
            path: key_path.to_str().unwrap().to_string(),
            size: Some(2048),
            key_type: Some(KeyType::Rsa),
            mode: Some("0644".to_string()),
            owner: None,
            group: None,
            force: false,
            state: None,
        };

        generate_private_key(&params, false).unwrap();

        let perms = metadata(&key_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o7777, 0o644);
    }
}
