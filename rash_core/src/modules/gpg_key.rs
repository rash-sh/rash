/// ANCHOR: module
/// # gpg_key
///
/// Manage GPG keys for package verification, encryption, and signing.
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
/// - name: Add a GPG key from a keyserver
///   gpg_key:
///     key_id: 0x1234567890ABCDEF
///     keyserver: keyserver.ubuntu.com
///     state: present
///
/// - name: Add a GPG key from a file
///   gpg_key:
///     keyfile: /path/to/key.asc
///     state: present
///
/// - name: Remove a GPG key
///   gpg_key:
///     key_id: 0x1234567890ABCDEF
///     state: absent
///
/// - name: Add a key and set trust level
///   gpg_key:
///     key_id: 0x1234567890ABCDEF
///     keyserver: keyserver.ubuntu.com
///     trust: ultimate
///     state: present
///
/// - name: Add a secret key from file
///   gpg_key:
///     keyfile: /path/to/private.key
///     type: secret
///     state: present
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::path::Path;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_executable() -> Option<String> {
    Some("gpg".to_owned())
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    #[default]
    Public,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    Unknown,
    Undefined,
    None,
    Marginal,
    Full,
    Ultimate,
}

impl TrustLevel {
    pub fn to_gpg_value(&self) -> &'static str {
        match self {
            TrustLevel::Unknown => "1",
            TrustLevel::Undefined => "2",
            TrustLevel::None => "3",
            TrustLevel::Marginal => "4",
            TrustLevel::Full => "5",
            TrustLevel::Ultimate => "6",
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The Key ID or fingerprint of the GPG key.
    pub key_id: Option<String>,
    /// Whether the key should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// The keyserver to retrieve the key from.
    pub keyserver: Option<String>,
    /// Path to a file containing the GPG key to import.
    pub keyfile: Option<String>,
    /// The trust level to set for the key (unknown, undefined, none, marginal, full, ultimate).
    pub trust: Option<TrustLevel>,
    /// The type of key (public or secret).
    /// **[default: `"public"`]**
    #[serde(rename = "type")]
    pub key_type: Option<KeyType>,
    /// Path to the gpg executable.
    /// **[default: `"gpg"`]**
    #[serde(default = "default_executable")]
    pub executable: Option<String>,
    /// GPG home directory to use instead of default.
    pub gpg_home: Option<String>,
}

#[derive(Debug)]
pub struct GpgKey;

impl Module for GpgKey {
    fn get_name(&self) -> &str {
        "gpg_key"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((gpg_key(parse_params(optional_params)?, check_mode)?, None))
    }

    fn force_string_on_params(&self) -> bool {
        true
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct GpgClient {
    executable: String,
    gpg_home: Option<String>,
    check_mode: bool,
}

impl GpgClient {
    pub fn new(executable: String, gpg_home: Option<String>, check_mode: bool) -> Self {
        GpgClient {
            executable,
            gpg_home,
            check_mode,
        }
    }

    fn get_cmd(&self) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.arg("--batch");
        cmd.arg("--no-tty");
        if let Some(ref home) = self.gpg_home {
            cmd.arg("--homedir").arg(home);
        }
        cmd
    }

    fn exec_cmd(&self, cmd: &mut Command, check_success: bool) -> Result<Output> {
        let output = cmd.output().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to execute '{}': {e}. The executable may not be installed or not in the PATH.",
                    self.executable
                ),
            )
        })?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");

        if check_success && !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::new(ErrorKind::SubprocessFail, stderr));
        }
        Ok(output)
    }

    pub fn key_exists(&self, key_id: &str, key_type: &KeyType) -> Result<bool> {
        let list_arg = match key_type {
            KeyType::Public => "--list-public-keys",
            KeyType::Secret => "--list-secret-keys",
        };

        let mut cmd = self.get_cmd();
        cmd.arg(list_arg)
            .arg("--with-colons")
            .arg("--fixed-list-mode")
            .arg(key_id);

        let output = self.exec_cmd(&mut cmd, false)?;

        Ok(output.status.success() && !output.stdout.is_empty())
    }

    pub fn get_key_fingerprint(&self, key_id: &str) -> Result<Option<String>> {
        let mut cmd = self.get_cmd();
        cmd.arg("--list-keys")
            .arg("--with-colons")
            .arg("--fixed-list-mode")
            .arg(key_id);

        let output = self.exec_cmd(&mut cmd, false)?;

        if !output.status.success() {
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("fpr:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 9 {
                    return Ok(Some(parts[9].to_string()));
                }
            }
        }

        Ok(None)
    }

    pub fn import_key_from_keyserver(&self, key_id: &str, keyserver: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let mut cmd = self.get_cmd();
        cmd.arg("--keyserver")
            .arg(keyserver)
            .arg("--recv-keys")
            .arg(key_id);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn import_key_from_file(&self, keyfile: &str) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let path = Path::new(keyfile);
        if !path.exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Key file does not exist: {keyfile}"),
            ));
        }

        let mut cmd = self.get_cmd();
        cmd.arg("--import").arg(keyfile);

        self.exec_cmd(&mut cmd, true)?;
        Ok(())
    }

    pub fn delete_key(&self, key_id: &str, key_type: &KeyType) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        match key_type {
            KeyType::Public => {
                let mut cmd = self.get_cmd();
                cmd.arg("--delete-keys").arg("--yes").arg(key_id);
                self.exec_cmd(&mut cmd, true)?;
            }
            KeyType::Secret => {
                let mut cmd = self.get_cmd();
                cmd.arg("--delete-secret-keys").arg("--yes").arg(key_id);
                self.exec_cmd(&mut cmd, true)?;
            }
        }

        Ok(())
    }

    pub fn set_trust(&self, fingerprint: &str, trust: &TrustLevel) -> Result<()> {
        if self.check_mode {
            return Ok(());
        }

        let trust_input = format!("{}:{}\n", fingerprint, trust.to_gpg_value());

        let mut cmd = self.get_cmd();
        cmd.arg("--import-ownertrust");

        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to spawn gpg process: {e}"),
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(trust_input.as_bytes()).map_err(|e| {
                Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Failed to write to gpg stdin: {e}"),
                )
            })?;
        }

        let status = child.wait().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to wait for gpg process: {e}"),
            )
        })?;

        if !status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                "Failed to set key trust level",
            ));
        }

        Ok(())
    }
}

fn gpg_key(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let key_id = params.key_id.clone();
    let state = params.state.unwrap_or_default();
    let key_type = params.key_type.unwrap_or_default();

    if key_id.is_none() && params.keyfile.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Either key_id or keyfile must be specified",
        ));
    }

    if params.key_id.is_some() && params.keyfile.is_some() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Only one of key_id or keyfile can be specified, not both",
        ));
    }

    let client = GpgClient::new(
        params.executable.clone().unwrap(),
        params.gpg_home.clone(),
        check_mode,
    );

    let key_ref = key_id.clone().or_else(|| params.keyfile.clone());

    let result = match state {
        State::Present => {
            if let Some(ref keyfile) = params.keyfile {
                let exists = if check_mode {
                    false
                } else {
                    let output = client.exec_cmd(
                        client.get_cmd().arg("--list-keys").arg("--with-colons"),
                        false,
                    )?;
                    output.status.success() && !output.stdout.is_empty()
                };

                if exists {
                    (false, format!("Key from {keyfile} already exists"))
                } else {
                    client.import_key_from_file(keyfile)?;
                    let fingerprint = if let Some(ref id) = key_id {
                        client.get_key_fingerprint(id)?
                    } else {
                        None
                    };

                    if let (Some(fp), Some(trust)) = (fingerprint, &params.trust) {
                        client.set_trust(&fp, trust)?;
                    }

                    (true, format!("Imported key from {keyfile}"))
                }
            } else if let Some(ref id) = key_id {
                let exists = client.key_exists(id, &key_type)?;

                if exists {
                    (false, format!("Key {id} already exists"))
                } else {
                    if let Some(ref keyserver) = params.keyserver {
                        client.import_key_from_keyserver(id, keyserver)?;
                    } else {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "keyserver is required when using key_id to import a key",
                        ));
                    }

                    if let Some(ref trust) = params.trust
                        && let Some(fp) = client.get_key_fingerprint(id)?
                    {
                        client.set_trust(&fp, trust)?;
                    }

                    (true, format!("Imported key {id}"))
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "Either key_id or keyfile must be specified",
                ));
            }
        }
        State::Absent => {
            if let Some(ref id) = key_id {
                let exists = client.key_exists(id, &key_type)?;

                if exists {
                    client.delete_key(id, &key_type)?;
                    (true, format!("Removed key {id}"))
                } else {
                    (false, format!("Key {id} does not exist"))
                }
            } else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "key_id is required when state=absent",
                ));
            }
        }
    };

    let (changed, message) = result;

    if changed {
        match state {
            State::Present => logger::add(&[key_ref.unwrap_or_default()]),
            State::Absent => logger::remove(&[key_ref.unwrap_or_default()]),
        }
    }

    Ok(ModuleResult::new(changed, None, Some(message)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_with_key_id() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            keyserver: keyserver.ubuntu.com
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_id, Some("0x1234567890ABCDEF".to_string()));
        assert_eq!(params.keyserver, Some("keyserver.ubuntu.com".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_keyfile() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            keyfile: /path/to/key.asc
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.keyfile, Some("/path/to/key.asc".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_trust() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            keyserver: keyserver.ubuntu.com
            trust: ultimate
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.trust, Some(TrustLevel::Ultimate));
    }

    #[test]
    fn test_parse_params_with_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            type: secret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key_type, Some(KeyType::Secret));
    }

    #[test]
    fn test_parse_params_with_gpg_home() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            gpg_home: /custom/gpg/home
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.gpg_home, Some("/custom/gpg/home".to_string()));
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_trust_level_values() {
        assert_eq!(TrustLevel::Unknown.to_gpg_value(), "1");
        assert_eq!(TrustLevel::Undefined.to_gpg_value(), "2");
        assert_eq!(TrustLevel::None.to_gpg_value(), "3");
        assert_eq!(TrustLevel::Marginal.to_gpg_value(), "4");
        assert_eq!(TrustLevel::Full.to_gpg_value(), "5");
        assert_eq!(TrustLevel::Ultimate.to_gpg_value(), "6");
    }

    #[test]
    fn test_parse_params_no_key_id_or_keyfile() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.key_id.is_none());
        assert!(params.keyfile.is_none());
    }

    #[test]
    fn test_parse_params_both_key_id_and_keyfile() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            keyfile: /path/to/key.asc
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.key_id.is_some());
        assert!(params.keyfile.is_some());
    }

    #[test]
    fn test_parse_params_random_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key_id: "0x1234567890ABCDEF"
            foo: bar
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_gpg_key_no_key_id_or_keyfile_error() {
        let params = Params {
            key_id: None,
            state: Some(State::Present),
            keyserver: None,
            keyfile: None,
            trust: None,
            key_type: Some(KeyType::Public),
            executable: Some("gpg".to_string()),
            gpg_home: None,
        };
        let result = gpg_key(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Either key_id or keyfile must be specified")
        );
    }

    #[test]
    fn test_gpg_key_both_key_id_and_keyfile_error() {
        let params = Params {
            key_id: Some("0x1234567890ABCDEF".to_string()),
            state: Some(State::Present),
            keyserver: Some("keyserver.ubuntu.com".to_string()),
            keyfile: Some("/path/to/key.asc".to_string()),
            trust: None,
            key_type: Some(KeyType::Public),
            executable: Some("gpg".to_string()),
            gpg_home: None,
        };
        let result = gpg_key(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Only one of key_id or keyfile can be specified")
        );
    }

    #[test]
    fn test_gpg_key_absent_requires_key_id() {
        let params = Params {
            key_id: None,
            state: Some(State::Absent),
            keyserver: None,
            keyfile: Some("/path/to/key.asc".to_string()),
            trust: None,
            key_type: Some(KeyType::Public),
            executable: Some("gpg".to_string()),
            gpg_home: None,
        };
        let result = gpg_key(params, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("key_id is required when state=absent")
        );
    }

    #[test]
    fn test_gpg_key_present_key_id_requires_keyserver() {
        let params = Params {
            key_id: Some("0x1234567890ABCDEF".to_string()),
            state: Some(State::Present),
            keyserver: None,
            keyfile: None,
            trust: None,
            key_type: Some(KeyType::Public),
            executable: Some("gpg".to_string()),
            gpg_home: None,
        };
        let result = gpg_key(params, true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("keyserver is required when using key_id to import a key")
        );
    }
}
