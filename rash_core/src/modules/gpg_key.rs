/// ANCHOR: module
/// # gpg_key
///
/// Manage GPG keys for package verification and signing.
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
/// - name: Import a GPG key from a keyserver
///   gpg_key:
///     key_id: ABC123DEF456
///     keyserver: keys.openpgp.org
///     state: present
///
/// - name: Import a GPG key from inline data
///   gpg_key:
///     data: |
///       -----BEGIN PGP PUBLIC KEY BLOCK-----
///       ...
///       -----END PGP PUBLIC KEY BLOCK-----
///     state: present
///
/// - name: Import a GPG key from a file
///   gpg_key:
///     file: /path/to/key.asc
///     state: present
///
/// - name: Remove a GPG key
///   gpg_key:
///     key_id: ABC123DEF456
///     state: absent
///
/// - name: Set trust level for a key
///   gpg_key:
///     key_id: ABC123DEF456
///     trust: ultimate
///     state: present
///
/// - name: Import key with custom GPG homedir
///   gpg_key:
///     key_id: ABC123DEF456
///     keyserver: keys.openpgp.org
///     gpg_home: /root/.gnupg
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

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
    /// The key ID or fingerprint of the GPG key.
    pub key_id: Option<String>,
    /// The keyserver to use for fetching the key.
    /// **[default: `"keys.openpgp.org"`]**
    #[serde(default = "default_keyserver")]
    pub keyserver: String,
    /// Whether the key should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// The trust level to set for the key.
    /// Valid values: unknown, none, marginal, full, ultimate
    pub trust: Option<TrustLevel>,
    /// The GPG key data as a string (for importing directly).
    pub data: Option<String>,
    /// Path to a file containing the GPG key.
    pub file: Option<String>,
    /// Custom GPG home directory.
    pub gpg_home: Option<String>,
    /// Use the GnuPG 1.x binary instead of the default.
    #[serde(default)]
    pub use_gpg1: bool,
}

fn default_keyserver() -> String {
    "keys.openpgp.org".to_string()
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Unknown,
    None,
    Marginal,
    Full,
    Ultimate,
}

impl TrustLevel {
    fn to_gpg_value(&self) -> char {
        match self {
            TrustLevel::Unknown => '?',
            TrustLevel::None => 'n',
            TrustLevel::Marginal => 'm',
            TrustLevel::Full => 'f',
            TrustLevel::Ultimate => 'u',
        }
    }
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustLevel::Unknown => write!(f, "unknown"),
            TrustLevel::None => write!(f, "none"),
            TrustLevel::Marginal => write!(f, "marginal"),
            TrustLevel::Full => write!(f, "full"),
            TrustLevel::Ultimate => write!(f, "ultimate"),
        }
    }
}

fn gpg_binary(use_gpg1: bool) -> &'static str {
    if use_gpg1 { "gpg1" } else { "gpg" }
}

fn run_gpg_command(
    args: &[&str],
    gpg_home: Option<&str>,
    use_gpg1: bool,
    input: Option<&str>,
) -> Result<String> {
    let mut cmd = Command::new(gpg_binary(use_gpg1));
    cmd.args(args);

    if let Some(home) = gpg_home {
        cmd.arg("--homedir").arg(home);
    }

    cmd.arg("--batch").arg("--yes");

    let output = if let Some(data) = input {
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to execute gpg command: {e}"),
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(data.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write to gpg stdin: {e}"),
                )
            })?;
        }

        child.wait_with_output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for gpg command: {e}"),
            )
        })?
    } else {
        cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute gpg command: {e}"),
            )
        })?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!("GPG command failed: {stderr}"),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn key_exists(key_id: &str, gpg_home: Option<&str>, use_gpg1: bool) -> Result<bool> {
    let output = run_gpg_command(
        &["--list-keys", "--with-colons", key_id],
        gpg_home,
        use_gpg1,
        None,
    );

    match output {
        Ok(s) => Ok(s.contains("pub:") || s.contains("sub:")),
        Err(e) if e.to_string().contains("No public key") => Ok(false),
        Err(e) => Err(e),
    }
}

fn get_key_fingerprint(
    key_id: &str,
    gpg_home: Option<&str>,
    use_gpg1: bool,
) -> Result<Option<String>> {
    let output = run_gpg_command(
        &["--list-keys", "--with-colons", key_id],
        gpg_home,
        use_gpg1,
        None,
    )?;

    for line in output.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.first() == Some(&"fpr") && parts.len() > 9 {
            return Ok(Some(parts[9].to_string()));
        }
    }

    Ok(None)
}

fn import_key_from_keyserver(
    key_id: &str,
    keyserver: &str,
    gpg_home: Option<&str>,
    use_gpg1: bool,
) -> Result<()> {
    run_gpg_command(
        &["--keyserver", keyserver, "--recv-keys", key_id],
        gpg_home,
        use_gpg1,
        None,
    )?;
    Ok(())
}

fn import_key_from_data(data: &str, gpg_home: Option<&str>, use_gpg1: bool) -> Result<()> {
    run_gpg_command(&["--import"], gpg_home, use_gpg1, Some(data))?;
    Ok(())
}

fn import_key_from_file(file_path: &str, gpg_home: Option<&str>, use_gpg1: bool) -> Result<()> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Key file not found: {file_path}"),
        ));
    }

    run_gpg_command(&["--import", file_path], gpg_home, use_gpg1, None)?;
    Ok(())
}

fn delete_key(key_id: &str, gpg_home: Option<&str>, use_gpg1: bool) -> Result<()> {
    let fingerprint =
        get_key_fingerprint(key_id, gpg_home, use_gpg1)?.unwrap_or_else(|| key_id.to_string());

    let _ = run_gpg_command(
        &["--delete-secret-keys", "--yes", &fingerprint],
        gpg_home,
        use_gpg1,
        None,
    );

    run_gpg_command(
        &["--delete-keys", "--yes", &fingerprint],
        gpg_home,
        use_gpg1,
        None,
    )?;

    Ok(())
}

fn set_trust_level(
    key_id: &str,
    trust: &TrustLevel,
    gpg_home: Option<&str>,
    use_gpg1: bool,
) -> Result<()> {
    let fingerprint =
        get_key_fingerprint(key_id, gpg_home, use_gpg1)?.unwrap_or_else(|| key_id.to_string());

    let trust_input = format!("{}:{}\n", fingerprint, trust.to_gpg_value());

    run_gpg_command(
        &["--import-ownertrust"],
        gpg_home,
        use_gpg1,
        Some(&trust_input),
    )?;

    Ok(())
}

pub fn gpg_key(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let gpg_home = params.gpg_home.as_deref();
    let use_gpg1 = params.use_gpg1;

    match state {
        State::Present => {
            let key_id = if params.data.is_some() || params.file.is_some() {
                None
            } else {
                params.key_id.clone()
            };

            let _key_id_ref = key_id.as_deref();

            if params.data.is_none() && params.file.is_none() && params.key_id.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "One of key_id, data, or file is required when state=present",
                ));
            }

            if let Some(ref key_id) = params.key_id
                && key_exists(key_id, gpg_home, use_gpg1)?
            {
                let mut changed = false;

                if let Some(ref trust) = params.trust {
                    if check_mode {
                        return Ok(ModuleResult {
                            changed: true,
                            output: Some(format!(
                                "Would set trust level to {} for key {}",
                                trust, key_id
                            )),
                            extra: None,
                        });
                    }
                    set_trust_level(key_id, trust, gpg_home, use_gpg1)?;
                    changed = true;
                }

                let extra = json!({
                    "key_id": key_id,
                    "fingerprint": get_key_fingerprint(key_id, gpg_home, use_gpg1)?,
                    "changed": changed,
                });

                return Ok(ModuleResult {
                    changed,
                    output: Some(format!("Key {} already exists", key_id)),
                    extra: Some(value::to_value(extra)?),
                });
            }

            if check_mode {
                let action = if params.data.is_some() {
                    "Would import GPG key from data"
                } else if params.file.is_some() {
                    "Would import GPG key from file"
                } else {
                    "Would import GPG key from keyserver"
                };
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!(
                        "{}{}",
                        action,
                        params
                            .key_id
                            .as_ref()
                            .map(|k| format!(": {}", k))
                            .unwrap_or_default()
                    )),
                    extra: None,
                });
            }

            if let Some(ref data) = params.data {
                import_key_from_data(data, gpg_home, use_gpg1)?;
            } else if let Some(ref file) = params.file {
                import_key_from_file(file, gpg_home, use_gpg1)?;
            } else if let Some(ref key_id) = params.key_id {
                import_key_from_keyserver(key_id, &params.keyserver, gpg_home, use_gpg1)?;
            }

            let actual_key_id = params
                .key_id
                .clone()
                .unwrap_or_else(|| "imported".to_string());
            let fingerprint = get_key_fingerprint(&actual_key_id, gpg_home, use_gpg1)?;

            if let Some(ref trust) = params.trust {
                if let Some(fp) = &fingerprint {
                    set_trust_level(fp, trust, gpg_home, use_gpg1)?;
                } else if params.key_id.is_some() {
                    set_trust_level(&actual_key_id, trust, gpg_home, use_gpg1)?;
                }
            }

            let extra = json!({
                "key_id": actual_key_id,
                "fingerprint": fingerprint,
                "keyserver": if params.key_id.is_some() { Some(&params.keyserver) } else { None },
                "changed": true,
            });

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("GPG key {} imported successfully", actual_key_id)),
                extra: Some(value::to_value(extra)?),
            })
        }
        State::Absent => {
            let key_id = params.key_id.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "key_id is required when state=absent",
                )
            })?;

            if !key_exists(key_id, gpg_home, use_gpg1)? {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(format!("Key {} does not exist", key_id)),
                    extra: None,
                });
            }

            if check_mode {
                return Ok(ModuleResult {
                    changed: true,
                    output: Some(format!("Would delete GPG key {}", key_id)),
                    extra: None,
                });
            }

            delete_key(key_id, gpg_home, use_gpg1)?;

            Ok(ModuleResult {
                changed: true,
                output: Some(format!("GPG key {} deleted successfully", key_id)),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct GpgKey;

impl Module for GpgKey {
    fn get_name(&self) -> &str {
        "gpg_key"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((gpg_key(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_keyserver() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_id, Some("ABC123DEF456".to_string()));
        assert_eq!(params.keyserver, "keys.openpgp.org");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_keyserver() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            keyserver: pgp.mit.edu
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.keyserver, "pgp.mit.edu");
    }

    #[test]
    fn test_parse_params_with_data() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            data: |
              -----BEGIN PGP PUBLIC KEY BLOCK-----
              testdata
              -----END PGP PUBLIC KEY BLOCK-----
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.data.is_some());
        assert!(params.data.unwrap().contains("testdata"));
    }

    #[test]
    fn test_parse_params_with_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            file: /path/to/key.asc
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.file, Some("/path/to/key.asc".to_string()));
    }

    #[test]
    fn test_parse_params_with_trust() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            trust: ultimate
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.trust, Some(TrustLevel::Ultimate));
    }

    #[test]
    fn test_parse_params_with_gpg_home() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            gpg_home: /root/.gnupg
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.gpg_home, Some("/root/.gnupg".to_string()));
    }

    #[test]
    fn test_parse_params_with_gpg1() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            use_gpg1: true
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.use_gpg1);
    }

    #[test]
    fn test_parse_params_state_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_unknown_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: ABC123DEF456
            unknown_field: value
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_trust_level_to_gpg_value() {
        assert_eq!(TrustLevel::Unknown.to_gpg_value(), '?');
        assert_eq!(TrustLevel::None.to_gpg_value(), 'n');
        assert_eq!(TrustLevel::Marginal.to_gpg_value(), 'm');
        assert_eq!(TrustLevel::Full.to_gpg_value(), 'f');
        assert_eq!(TrustLevel::Ultimate.to_gpg_value(), 'u');
    }

    #[test]
    fn test_gpg_binary() {
        assert_eq!(gpg_binary(false), "gpg");
        assert_eq!(gpg_binary(true), "gpg1");
    }

    #[test]
    fn test_default_state() {
        let state: State = Default::default();
        assert_eq!(state, State::Present);
    }
}
