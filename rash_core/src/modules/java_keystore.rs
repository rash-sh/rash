/// ANCHOR: module
/// # java_keystore
///
/// Manage Java keystores for SSL/TLS certificate management.
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
/// - name: Import certificate into keystore
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     certificate: /etc/ssl/cert.pem
///     private_key: /etc/ssl/key.pem
///     alias: myapp
///
/// - name: Import certificate with CA chain
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     certificate: /etc/ssl/cert.pem
///     private_key: /etc/ssl/key.pem
///     alias: myapp
///     cacert_chain:
///       - /etc/ssl/ca-intermediate.pem
///       - /etc/ssl/ca-root.pem
///
/// - name: Import PKCS12 file into keystore
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     pkcs12_path: /etc/ssl/bundle.p12
///     pkcs12_password: pkcs12secret
///     alias: myapp
///
/// - name: Remove certificate from keystore
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     alias: oldcert
///     state: absent
///
/// - name: Create empty keystore
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     state: present
///
/// - name: Import certificate with force overwrite
///   java_keystore:
///     path: /etc/ssl/keystore.jks
///     password: secret
///     certificate: /etc/ssl/newcert.pem
///     private_key: /etc/ssl/newkey.pem
///     alias: myapp
///     force: true
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
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
    /// Path to the keystore file.
    pub path: String,
    /// Password for the keystore.
    pub password: String,
    /// Whether the entry should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Alias for the certificate in the keystore.
    pub alias: Option<String>,
    /// Path to the certificate file (PEM format).
    pub certificate: Option<String>,
    /// Path to the private key file (PEM format).
    pub private_key: Option<String>,
    /// List of CA certificate chain files (PEM format).
    pub cacert_chain: Option<Vec<String>>,
    /// Path to a PKCS12 file to import.
    pub pkcs12_path: Option<String>,
    /// Password for the PKCS12 file.
    pub pkcs12_password: Option<String>,
    /// Force overwrite existing entry with same alias.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn run_keytool(args: &[&str], password: &str) -> Result<String> {
    let mut cmd = Command::new("keytool");
    cmd.args(args);
    cmd.args(["-storepass", password]);
    cmd.arg("-noprompt");

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute keytool command: {e}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("KeyStoreException") || stderr.contains("IOException") {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!("Keytool command failed: {stderr}"),
            ));
        }
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn keystore_exists(path: &str) -> bool {
    Path::new(path).exists()
}

fn alias_exists(path: &str, alias: &str, password: &str) -> Result<bool> {
    let output = run_keytool(&["-list", "-keystore", path, "-alias", alias], password);

    match output {
        Ok(s) if s.contains(alias) || !s.contains("Alias <") => Ok(true),
        Ok(_) => Ok(false),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("does not exist") || err_str.contains("Alias <") {
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
}

fn create_empty_keystore(path: &str, password: &str) -> Result<()> {
    let parent = Path::new(path).parent().ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            format!("Cannot determine parent directory for: {path}"),
        )
    })?;

    if !parent.exists() {
        fs::create_dir_all(parent).map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to create directory {}: {e}", parent.display()),
            )
        })?;
    }

    let mut cmd = Command::new("keytool");
    cmd.args([
        "-genkeypair",
        "-keystore",
        path,
        "-alias",
        "temp_alias_for_creation",
        "-keyalg",
        "RSA",
        "-keysize",
        "2048",
        "-validity",
        "1",
        "-dname",
        "CN=temp",
        "-storepass",
        password,
        "-keypass",
        password,
        "-noprompt",
    ]);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute keytool command: {e}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create keystore: {stderr}"),
        ));
    }

    let mut cmd = Command::new("keytool");
    cmd.args([
        "-delete",
        "-keystore",
        path,
        "-alias",
        "temp_alias_for_creation",
        "-storepass",
        password,
        "-noprompt",
    ]);

    let _ = cmd.output();

    Ok(())
}

fn create_pkcs12_bundle(
    cert_path: &str,
    key_path: &str,
    ca_chain: &[&str],
    pkcs12_path: &str,
    password: &str,
) -> Result<()> {
    let mut cmd = Command::new("openssl");
    cmd.args(["pkcs12", "-export"]);
    cmd.args(["-in", cert_path]);
    cmd.args(["-inkey", key_path]);
    cmd.args(["-out", pkcs12_path]);
    cmd.args(["-passout", &format!("pass:{password}")]);

    for ca_cert in ca_chain {
        cmd.args(["-certfile", ca_cert]);
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute openssl command: {e}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to create PKCS12 bundle: {stderr}"),
        ));
    }

    Ok(())
}

fn import_pkcs12(
    keystore_path: &str,
    pkcs12_path: &str,
    pkcs12_password: &str,
    alias: &str,
    keystore_password: &str,
) -> Result<()> {
    let mut cmd = Command::new("keytool");
    cmd.args([
        "-importkeystore",
        "-srckeystore",
        pkcs12_path,
        "-srcstoretype",
        "PKCS12",
        "-srcstorepass",
        pkcs12_password,
        "-destkeystore",
        keystore_path,
        "-deststoretype",
        "JKS",
        "-deststorepass",
        keystore_password,
        "-srcalias",
        "1",
        "-destalias",
        alias,
        "-noprompt",
    ]);

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute keytool command: {e}"),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to import PKCS12: {stderr}"),
        ));
    }

    Ok(())
}

fn delete_alias(path: &str, alias: &str, password: &str) -> Result<()> {
    run_keytool(&["-delete", "-keystore", path, "-alias", alias], password)?;
    Ok(())
}

fn get_keystore_info(path: &str, password: &str) -> Result<String> {
    run_keytool(&["-list", "-keystore", path], password)
}

pub fn java_keystore(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();

    match state {
        State::Present => {
            if params.certificate.is_some() && params.private_key.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "private_key is required when certificate is provided",
                ));
            }

            if params.private_key.is_some() && params.certificate.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "certificate is required when private_key is provided",
                ));
            }

            if params.pkcs12_path.is_some() && params.alias.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "alias is required when pkcs12_path is provided",
                ));
            }

            if params.certificate.is_some() && params.alias.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "alias is required when importing certificates",
                ));
            }

            if !keystore_exists(&params.path) {
                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!("Would create keystore at {}", params.path)),
                        extra: None,
                    });
                }

                if params.pkcs12_path.is_some() || params.certificate.is_some() {
                    create_empty_keystore(&params.path, &params.password)?;
                }
            }

            if let Some(ref alias) = params.alias
                && alias_exists(&params.path, alias, &params.password)?
            {
                if !params.force {
                    let _info = get_keystore_info(&params.path, &params.password)?;
                    let extra = json!({
                        "path": params.path,
                        "alias": alias,
                        "exists": true,
                    });

                    return Ok(ModuleResult {
                        changed: false,
                        output: Some(format!(
                            "Alias '{}' already exists in keystore {}",
                            alias, params.path
                        )),
                        extra: Some(value::to_value(extra)?),
                    });
                }

                if check_mode {
                    return Ok(ModuleResult {
                        changed: true,
                        output: Some(format!(
                            "Would overwrite alias '{}' in keystore {}",
                            alias, params.path
                        )),
                        extra: None,
                    });
                }

                delete_alias(&params.path, alias, &params.password)?;
            }

            if check_mode {
                let action = if params.pkcs12_path.is_some() {
                    "Would import PKCS12 into keystore"
                } else if params.certificate.is_some() {
                    "Would import certificate into keystore"
                } else {
                    "Would ensure keystore exists"
                };
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("{} at {}", action, params.path)),
                    extra: None,
                });
            }

            if let Some(ref pkcs12_path) = params.pkcs12_path {
                let alias = params
                    .alias
                    .as_ref()
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "alias is required"))?;

                let pkcs12_password = params
                    .pkcs12_password
                    .as_deref()
                    .unwrap_or(&params.password);

                import_pkcs12(
                    &params.path,
                    pkcs12_path,
                    pkcs12_password,
                    alias,
                    &params.password,
                )?;

                let extra = json!({
                    "path": params.path,
                    "alias": alias,
                    "pkcs12_path": pkcs12_path,
                });

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "Imported PKCS12 '{}' as alias '{}' into keystore {}",
                        pkcs12_path, alias, params.path
                    )),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if let (Some(cert_path), Some(key_path), Some(alias)) =
                (&params.certificate, &params.private_key, &params.alias)
            {
                let temp_pkcs12 = format!("{}.temp.p12", params.path);
                let ca_chain = params.cacert_chain.clone().unwrap_or_default();

                let ca_refs: Vec<&str> = ca_chain.iter().map(|s| s.as_str()).collect();

                create_pkcs12_bundle(
                    cert_path,
                    key_path,
                    &ca_refs,
                    &temp_pkcs12,
                    &params.password,
                )?;

                let result = import_pkcs12(
                    &params.path,
                    &temp_pkcs12,
                    &params.password,
                    alias,
                    &params.password,
                );

                let _ = fs::remove_file(&temp_pkcs12);

                result?;

                let extra = json!({
                    "path": params.path,
                    "alias": alias,
                    "certificate": cert_path,
                    "private_key": key_path,
                    "ca_chain": ca_chain,
                });

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "Imported certificate '{}' as alias '{}' into keystore {}",
                        cert_path, alias, params.path
                    )),
                    extra: Some(value::to_value(extra)?),
                });
            }

            let extra = json!({
                "path": params.path,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("Keystore {} is present", params.path)),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => {
            if !keystore_exists(&params.path) {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Keystore {} does not exist", params.path)),
                    extra: None,
                });
            }

            let alias = params.alias.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "alias is required when state=absent",
                )
            })?;

            if !alias_exists(&params.path, alias, &params.password)? {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!(
                        "Alias '{}' does not exist in keystore {}",
                        alias, params.path
                    )),
                    extra: None,
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "Would remove alias '{}' from keystore {}",
                        alias, params.path
                    )),
                    extra: None,
                });
            }

            delete_alias(&params.path, alias, &params.password)?;

            let extra = json!({
                "path": params.path,
                "alias": alias,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!(
                    "Removed alias '{}' from keystore {}",
                    alias, params.path
                )),
                extra: Some(value::to_value(extra)?),
            })
        }
    }
}

#[derive(Debug)]
pub struct JavaKeystore;

impl Module for JavaKeystore {
    fn get_name(&self) -> &str {
        "java_keystore"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            java_keystore(parse_params(optional_params)?, check_mode)?,
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
    fn test_parse_params_basic() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            certificate: /etc/ssl/cert.pem
            private_key: /etc/ssl/key.pem
            alias: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/ssl/keystore.jks");
        assert_eq!(params.password, "secret");
        assert_eq!(params.certificate, Some("/etc/ssl/cert.pem".to_string()));
        assert_eq!(params.private_key, Some("/etc/ssl/key.pem".to_string()));
        assert_eq!(params.alias, Some("myapp".to_string()));
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_with_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            alias: oldcert
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_force() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            certificate: /etc/ssl/cert.pem
            private_key: /etc/ssl/key.pem
            alias: myapp
            force: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.force);
    }

    #[test]
    fn test_parse_params_with_ca_chain() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            certificate: /etc/ssl/cert.pem
            private_key: /etc/ssl/key.pem
            alias: myapp
            cacert_chain:
              - /etc/ssl/ca-intermediate.pem
              - /etc/ssl/ca-root.pem
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.cacert_chain,
            Some(vec![
                "/etc/ssl/ca-intermediate.pem".to_string(),
                "/etc/ssl/ca-root.pem".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_params_with_pkcs12() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            pkcs12_path: /etc/ssl/bundle.p12
            pkcs12_password: pkcs12secret
            alias: myapp
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.pkcs12_path, Some("/etc/ssl/bundle.p12".to_string()));
        assert_eq!(params.pkcs12_password, Some("pkcs12secret".to_string()));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/ssl/keystore.jks
            password: secret
            unknown_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Present);
    }
}
