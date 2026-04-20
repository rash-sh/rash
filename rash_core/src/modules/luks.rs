/// ANCHOR: module
/// # luks
///
/// Manage LUKS (Linux Unified Key Setup) encrypted volumes.
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
/// - name: Create LUKS container on device
///   luks:
///     device: /dev/sdb1
///     passphrase: supersecret
///     state: present
///
/// - name: Create LUKS container with keyfile
///   luks:
///     device: /dev/sdb1
///     keyfile: /root/luks-key
///     state: present
///
/// - name: Create LUKS container with custom cipher and key size
///   luks:
///     device: /dev/sdb1
///     passphrase: supersecret
///     cipher: aes-xts-plain64
///     key_size: 512
///     state: present
///
/// - name: Open LUKS container
///   luks:
///     device: /dev/sdb1
///     passphrase: supersecret
///     name: cryptdata
///     state: opened
///
/// - name: Close LUKS container
///   luks:
///     device: /dev/sdb1
///     name: cryptdata
///     state: closed
///
/// - name: Remove LUKS header (destroy container)
///   luks:
///     device: /dev/sdb1
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::path::Path;
use std::process::{Command, Output};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json;
use serde_norway::{Value as YamlValue, value};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
    Opened,
    Closed,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Device path to manage (e.g., /dev/sdb1).
    device: String,
    /// Encryption passphrase.
    /// Required when state=present without keyfile, or state=opened without keyfile.
    passphrase: Option<String>,
    /// Path to keyfile for authentication.
    /// Alternative to passphrase.
    keyfile: Option<String>,
    /// Desired state of the LUKS container.
    /// **[default: `"present"`]**
    state: Option<State>,
    /// Encryption cipher algorithm.
    /// **[default: `"aes-xts-plain64"`]**
    cipher: Option<String>,
    /// Key size in bits.
    /// **[default: `512`]**
    key_size: Option<u32>,
    /// Mapper name for opened LUKS container.
    /// Required when state=opened or state=closed.
    name: Option<String>,
    /// LUKS type (luks1 or luks2).
    /// **[default: `"luks2"`]**
    luks_type: Option<String>,
}

#[derive(Debug)]
pub struct Luks;

impl Module for Luks {
    fn get_name(&self) -> &str {
        "luks"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            luks_module(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

struct LuksClient {
    check_mode: bool,
}

impl LuksClient {
    pub fn new(check_mode: bool) -> Self {
        LuksClient { check_mode }
    }

    fn exec_cmd(&self, cmd: &mut Command) -> Result<Output> {
        let output = cmd
            .output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn exec_cmd_with_stdin(&self, cmd: &mut Command, input: &[u8]) -> Result<Output> {
        use std::io::Write;
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        if let Some(ref mut stdin) = child.stdin {
            stdin
                .write_all(input)
                .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;
        trace!("command: `{cmd:?}`");
        trace!("{output:?}");
        Ok(output)
    }

    fn is_luks_device(&self, device: &str) -> Result<bool> {
        let output = self.exec_cmd(Command::new("cryptsetup").args(["isLuks", device]))?;
        Ok(output.status.success())
    }

    fn is_mapping_active(&self, name: &str) -> Result<bool> {
        let mapper_path = format!("/dev/mapper/{name}");
        Ok(Path::new(&mapper_path).exists())
    }

    fn create_container(&self, params: &Params) -> Result<LuksResult> {
        if self.is_luks_device(&params.device)? {
            return Ok(LuksResult::no_change());
        }

        let cipher = params.cipher.as_deref().unwrap_or("aes-xts-plain64");
        let key_size = params.key_size.unwrap_or(512);
        let luks_type = params.luks_type.as_deref().unwrap_or("luks2");

        diff(
            format!("{}: not a LUKS device", params.device),
            format!(
                "{}: LUKS container (cipher={}, key_size={}, type={})",
                params.device, cipher, key_size, luks_type
            ),
        );

        if self.check_mode {
            return Ok(LuksResult::new(true));
        }

        let mut cmd = Command::new("cryptsetup");
        cmd.args(["-q", "--cipher", cipher])
            .args(["--key-size", &key_size.to_string()])
            .args(["--type", luks_type])
            .arg("luksFormat")
            .arg(&params.device);

        if let Some(keyfile) = &params.keyfile {
            cmd.arg(keyfile);
            let output = self.exec_cmd(&mut cmd)?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create LUKS container: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        } else if let Some(passphrase) = &params.passphrase {
            let output = self.exec_cmd_with_stdin(&mut cmd, passphrase.as_bytes())?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to create LUKS container: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        } else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "passphrase or keyfile is required when state=present",
            ));
        }

        Ok(LuksResult::new(true))
    }

    fn remove_container(&self, params: &Params) -> Result<LuksResult> {
        if !self.is_luks_device(&params.device)? {
            return Ok(LuksResult::no_change());
        }

        diff(
            format!("{}: LUKS container present", params.device),
            format!("{}: LUKS container absent", params.device),
        );

        if self.check_mode {
            return Ok(LuksResult::new(true));
        }

        let output = self.exec_cmd(
            Command::new("cryptsetup")
                .args(["luksErase", &params.device])
                .arg("-q"),
        )?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to remove LUKS container: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(LuksResult::new(true))
    }

    fn open_container(&self, params: &Params) -> Result<LuksResult> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "name is required when state=opened")
        })?;

        if self.is_mapping_active(name)? {
            return Ok(LuksResult::no_change());
        }

        if !self.is_luks_device(&params.device)? {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("{} is not a LUKS device", params.device),
            ));
        }

        diff(
            format!("mapping {name}: absent"),
            format!("mapping {name}: opened ({})", params.device),
        );

        if self.check_mode {
            return Ok(LuksResult::new(true));
        }

        let mut cmd = Command::new("cryptsetup");
        cmd.args(["luksOpen", &params.device, name]);

        if let Some(keyfile) = &params.keyfile {
            cmd.arg("--key-file").arg(keyfile);
            let output = self.exec_cmd(&mut cmd)?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to open LUKS container: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        } else if let Some(passphrase) = &params.passphrase {
            let output = self.exec_cmd_with_stdin(&mut cmd, passphrase.as_bytes())?;
            if !output.status.success() {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!(
                        "Failed to open LUKS container: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                ));
            }
        } else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "passphrase or keyfile is required when state=opened",
            ));
        }

        Ok(LuksResult::new(true))
    }

    fn close_container(&self, params: &Params) -> Result<LuksResult> {
        let name = params.name.as_ref().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "name is required when state=closed")
        })?;

        if !self.is_mapping_active(name)? {
            return Ok(LuksResult::no_change());
        }

        diff(
            format!("mapping {name}: opened"),
            format!("mapping {name}: closed"),
        );

        if self.check_mode {
            return Ok(LuksResult::new(true));
        }

        let output = self.exec_cmd(Command::new("cryptsetup").args(["luksClose", name]))?;

        if !output.status.success() {
            return Err(Error::new(
                ErrorKind::SubprocessFail,
                format!(
                    "Failed to close LUKS container: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        Ok(LuksResult::new(true))
    }
}

#[derive(Debug)]
struct LuksResult {
    changed: bool,
}

impl LuksResult {
    fn new(changed: bool) -> Self {
        LuksResult { changed }
    }

    fn no_change() -> Self {
        LuksResult { changed: false }
    }
}

fn validate_params(params: &Params) -> Result<()> {
    if params.device.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "device cannot be empty"));
    }

    if !params.device.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "device must be an absolute path",
        ));
    }

    let state = params.state.unwrap_or(State::Present);

    match state {
        State::Present => {
            if params.passphrase.is_none() && params.keyfile.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "passphrase or keyfile is required when state=present",
                ));
            }
        }
        State::Opened => {
            if params.name.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required when state=opened",
                ));
            }
            if params.passphrase.is_none() && params.keyfile.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "passphrase or keyfile is required when state=opened",
                ));
            }
        }
        State::Closed => {
            if params.name.is_none() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "name is required when state=closed",
                ));
            }
        }
        State::Absent => {}
    }

    Ok(())
}

fn luks_module(params: Params, check_mode: bool) -> Result<ModuleResult> {
    validate_params(&params)?;

    let client = LuksClient::new(check_mode);
    let state = params.state.unwrap_or(State::Present);

    let result = match state {
        State::Present => client.create_container(&params)?,
        State::Absent => client.remove_container(&params)?,
        State::Opened => client.open_container(&params)?,
        State::Closed => client.close_container(&params)?,
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        serde_json::Value::String(params.device.clone()),
    );

    if let Some(name) = &params.name {
        extra.insert("name".to_string(), serde_json::Value::String(name.clone()));
    }

    extra.insert(
        "state".to_string(),
        serde_json::Value::String(
            match state {
                State::Present => "present",
                State::Absent => "absent",
                State::Opened => "opened",
                State::Closed => "closed",
            }
            .to_string(),
        ),
    );

    Ok(ModuleResult {
        changed: result.changed,
        output: None,
        extra: Some(value::to_value(extra)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            passphrase: supersecret
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                device: "/dev/sdb1".to_owned(),
                passphrase: Some("supersecret".to_owned()),
                keyfile: None,
                state: Some(State::Present),
                cipher: None,
                key_size: None,
                name: None,
                luks_type: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_keyfile() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            keyfile: /root/luks-key
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.keyfile, Some("/root/luks-key".to_owned()));
        assert_eq!(params.passphrase, None);
    }

    #[test]
    fn test_parse_params_with_cipher() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            passphrase: supersecret
            cipher: aes-cbc-essiv:sha256
            key_size: 256
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.cipher, Some("aes-cbc-essiv:sha256".to_owned()));
        assert_eq!(params.key_size, Some(256));
    }

    #[test]
    fn test_parse_params_opened() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            passphrase: supersecret
            name: cryptdata
            state: opened
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Opened));
        assert_eq!(params.name, Some("cryptdata".to_owned()));
    }

    #[test]
    fn test_parse_params_closed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            name: cryptdata
            state: closed
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Closed));
        assert_eq!(params.name, Some("cryptdata".to_owned()));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_with_luks_type() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            passphrase: supersecret
            luks_type: luks1
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.luks_type, Some("luks1".to_owned()));
    }

    #[test]
    fn test_parse_params_default_state() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            passphrase: supersecret
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, None);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            device: /dev/sdb1
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_params_empty_device() {
        let params = Params {
            device: String::new(),
            passphrase: Some("secret".to_string()),
            keyfile: None,
            state: Some(State::Present),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_relative_device() {
        let params = Params {
            device: "dev/sdb1".to_string(),
            passphrase: Some("secret".to_string()),
            keyfile: None,
            state: Some(State::Present),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_present_no_creds() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: None,
            keyfile: None,
            state: Some(State::Present),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("passphrase or keyfile is required")
        );
    }

    #[test]
    fn test_validate_params_opened_no_name() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: Some("secret".to_string()),
            keyfile: None,
            state: Some(State::Opened),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name is required"));
    }

    #[test]
    fn test_validate_params_opened_no_creds() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: None,
            keyfile: None,
            state: Some(State::Opened),
            cipher: None,
            key_size: None,
            name: Some("cryptdata".to_string()),
            luks_type: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("passphrase or keyfile is required")
        );
    }

    #[test]
    fn test_validate_params_closed_no_name() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: None,
            keyfile: None,
            state: Some(State::Closed),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        let result = validate_params(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name is required"));
    }

    #[test]
    fn test_validate_params_absent_valid() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: None,
            keyfile: None,
            state: Some(State::Absent),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_present_with_passphrase() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: Some("secret".to_string()),
            keyfile: None,
            state: Some(State::Present),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_present_with_keyfile() {
        let params = Params {
            device: "/dev/sdb1".to_string(),
            passphrase: None,
            keyfile: Some("/root/key".to_string()),
            state: Some(State::Present),
            cipher: None,
            key_size: None,
            name: None,
            luks_type: None,
        };
        assert!(validate_params(&params).is_ok());
    }
}
