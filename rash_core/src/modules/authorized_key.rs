/// ANCHOR: module
/// # authorized_key
///
/// Add or remove SSH authorized keys for a user.
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
/// - authorized_key:
///     user: deploy
///     key: ssh-rsa AAAA... user@host
///     state: present
///
/// - authorized_key:
///     user: deploy
///     key: '{{ lookup("file", "~/.ssh/id_rsa.pub") }}'
///     state: present
///
/// - authorized_key:
///     user: deploy
///     key:
///       - ssh-rsa AAAA... user1@host
///       - ssh-ed25519 AAAA... user2@host
///     state: present
///
/// - authorized_key:
///     user: deploy
///     key: ssh-rsa AAAA... old@host
///     state: absent
///
/// - authorized_key:
///     user: deploy
///     key: ssh-rsa AAAA... deploy@host
///     exclusive: true
///     key_options: 'no-port-forwarding,from="10.0.1.1"'
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use minijinja::Value;
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
    /// The username whose authorized_keys file should be modified.
    pub user: String,
    /// The SSH public key(s). Can be a single key string or a list of keys.
    pub key: Option<KeyInput>,
    /// Whether the key should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Alternate path to the authorized_keys file.
    /// By default, uses ~/.ssh/authorized_keys.
    pub path: Option<String>,
    /// Whether to remove all other non-specified keys from the file.
    /// **[default: `false`]**
    #[serde(default)]
    pub exclusive: bool,
    /// Whether to create the .ssh directory if it doesn't exist.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub manage_dir: bool,
    /// A comment to attach to the key. By default, this is extracted from the key.
    pub comment: Option<String>,
    /// A string of ssh key options to be prepended to the key.
    pub key_options: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum KeyInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SshKey {
    pub key_type: String,
    pub key_data: String,
    pub comment: Option<String>,
    pub options: Option<String>,
}

impl SshKey {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let key_types = [
            "ssh-rsa",
            "ssh-dss",
            "ssh-ed25519",
            "ssh-ed448",
            "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521",
            "sk-ssh-ed25519@openssh.com",
            "sk-ecdsa-sha2-nistp256@openssh.com",
        ];

        for key_type in &key_types {
            if let Some(pos) = line.find(key_type) {
                let before = &line[..pos];
                let after = &line[pos + key_type.len()..];

                let options = if before.trim().is_empty() {
                    None
                } else {
                    Some(before.trim().to_string())
                };

                let after_parts: Vec<&str> = after.split_whitespace().collect();
                if after_parts.is_empty() {
                    continue;
                }

                let key_data = after_parts[0].to_string();
                let comment = if after_parts.len() > 1 {
                    Some(after_parts[1..].join(" "))
                } else {
                    None
                };

                return Some(SshKey {
                    key_type: key_type.to_string(),
                    key_data,
                    comment,
                    options,
                });
            }
        }

        None
    }

    pub fn to_line(&self) -> String {
        match (&self.options, &self.comment) {
            (Some(opts), Some(comment)) => {
                format!("{} {} {} {}", opts, self.key_type, self.key_data, comment)
            }
            (Some(opts), None) => format!("{} {} {}", opts, self.key_type, self.key_data),
            (None, Some(comment)) => format!("{} {} {}", self.key_type, self.key_data, comment),
            (None, None) => format!("{} {}", self.key_type, self.key_data),
        }
    }

    pub fn key_identifier(&self) -> String {
        format!("{} {}", self.key_type, self.key_data)
    }
}

fn get_user_home(username: &str) -> Option<String> {
    let passwd_path = if let Ok(test_file) = std::env::var("RASH_TEST_PASSWD_FILE") {
        test_file
    } else {
        "/etc/passwd".to_string()
    };

    if let Ok(passwd) = fs::read_to_string(&passwd_path) {
        for line in passwd.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 6 && parts[0] == username {
                return Some(parts[5].to_string());
            }
        }
    }
    None
}

fn get_authorized_keys_path(params: &Params) -> Result<PathBuf> {
    if let Some(ref path) = params.path {
        return Ok(PathBuf::from(path));
    }

    let home = get_user_home(&params.user).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Cannot determine home directory for user '{}'", params.user),
        )
    })?;

    Ok(PathBuf::from(home).join(".ssh/authorized_keys"))
}

fn normalize_key(
    key_str: &str,
    comment: Option<&str>,
    key_options: Option<&str>,
) -> Option<SshKey> {
    let mut ssh_key = SshKey::parse(key_str)?;
    if let Some(c) = comment {
        ssh_key.comment = Some(c.to_string());
    }
    if let Some(opts) = key_options {
        ssh_key.options = Some(opts.to_string());
    }
    Some(ssh_key)
}

pub fn authorized_key(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let keys_path = get_authorized_keys_path(&params)?;

    let key_strings: Vec<String> = match params.key {
        Some(KeyInput::Single(k)) => vec![k],
        Some(KeyInput::Multiple(ks)) => ks,
        None => {
            if state == State::Present {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "key parameter is required when state=present",
                ));
            }
            return Err(Error::new(
                ErrorKind::InvalidData,
                "key parameter is required",
            ));
        }
    };

    let keys_to_manage: Vec<SshKey> = key_strings
        .iter()
        .filter_map(|k| normalize_key(k, params.comment.as_deref(), params.key_options.as_deref()))
        .collect();

    if keys_to_manage.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "No valid SSH keys provided",
        ));
    }

    let original_content = if keys_path.exists() {
        fs::read_to_string(&keys_path)?
    } else {
        String::new()
    };

    let existing_keys: Vec<SshKey> = original_content.lines().filter_map(SshKey::parse).collect();

    let mut changed = false;
    let new_keys = match state {
        State::Present => {
            if params.exclusive {
                let mut result = Vec::new();
                for key in &keys_to_manage {
                    let key_id = key.key_identifier();
                    let exists = existing_keys.iter().any(|k| k.key_identifier() == key_id);
                    if !exists {
                        changed = true;
                    }
                    result.push(key.clone());
                }
                if existing_keys.len() != keys_to_manage.len()
                    || !existing_keys.iter().all(|ek| {
                        keys_to_manage
                            .iter()
                            .any(|nk| nk.key_identifier() == ek.key_identifier())
                    })
                {
                    changed = true;
                }
                result
            } else {
                let mut result = existing_keys.clone();
                for key in keys_to_manage {
                    let key_id = key.key_identifier();
                    let exists = result.iter().any(|k| k.key_identifier() == key_id);
                    if !exists {
                        result.push(key);
                        changed = true;
                    }
                }
                result
            }
        }
        State::Absent => {
            let mut result = Vec::new();
            for existing in existing_keys {
                let key_id = existing.key_identifier();
                let should_remove = keys_to_manage.iter().any(|k| k.key_identifier() == key_id);
                if should_remove {
                    changed = true;
                } else {
                    result.push(existing);
                }
            }
            result
        }
    };

    if changed {
        let new_content = if new_keys.is_empty() {
            String::new()
        } else {
            format!(
                "{}\n",
                new_keys
                    .iter()
                    .map(|k| k.to_line())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        diff(&original_content, &new_content);

        if !check_mode {
            if params.manage_dir
                && let Some(parent) = keys_path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            if new_keys.is_empty() {
                if keys_path.exists() {
                    fs::remove_file(&keys_path)?;
                }
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&keys_path)?;
                file.write_all(new_content.as_bytes())?;
            }
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(keys_path.to_string_lossy().to_string()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct AuthorizedKey;

impl Module for AuthorizedKey {
    fn get_name(&self) -> &str {
        "authorized_key"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            authorized_key(parse_params(optional_params)?, check_mode)?,
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
    use std::fs;
    use tempfile::tempdir;

    fn setup_test_passwd(home_dir: &std::path::Path) -> String {
        let passwd_path = home_dir.join("passwd");
        let passwd_content = format!(
            "deploy:x:1000:1000:Deploy User:{}:/bin/bash\n",
            home_dir.to_string_lossy()
        );
        fs::write(&passwd_path, passwd_content).unwrap();
        passwd_path.to_string_lossy().to_string()
    }

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: deploy
            key: ssh-rsa AAAA... user@host
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.user, "deploy");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_multiple_keys() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            user: deploy
            key:
              - ssh-rsa AAAA... user1@host
              - ssh-ed25519 BBBB... user2@host
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        match params.key {
            Some(KeyInput::Multiple(keys)) => assert_eq!(keys.len(), 2),
            _ => panic!("Expected multiple keys"),
        }
    }

    #[test]
    fn test_ssh_key_parse_rsa() {
        let line = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host";
        let key = SshKey::parse(line).unwrap();
        assert_eq!(key.key_type, "ssh-rsa");
        assert_eq!(key.key_data, "AAAAB3NzaC1yc2EAAAADAQABAAABgQC...");
        assert_eq!(key.comment, Some("user@host".to_string()));
        assert_eq!(key.options, None);
    }

    #[test]
    fn test_ssh_key_parse_ed25519() {
        let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI... deploy@example.com";
        let key = SshKey::parse(line).unwrap();
        assert_eq!(key.key_type, "ssh-ed25519");
    }

    #[test]
    fn test_ssh_key_parse_with_options() {
        let line = r#"command="echo hello",no-port-forwarding ssh-rsa AAAA... user@host"#;
        let key = SshKey::parse(line).unwrap();
        assert_eq!(key.key_type, "ssh-rsa");
        assert!(key.options.is_some());
        assert!(key.options.unwrap().contains("command"));
    }

    #[test]
    fn test_ssh_key_to_line() {
        let key = SshKey {
            key_type: "ssh-rsa".to_string(),
            key_data: "AAAA...".to_string(),
            comment: Some("user@host".to_string()),
            options: None,
        };
        assert_eq!(key.to_line(), "ssh-rsa AAAA... user@host");
    }

    #[test]
    fn test_ssh_key_to_line_with_options() {
        let key = SshKey {
            key_type: "ssh-rsa".to_string(),
            key_data: "AAAA...".to_string(),
            comment: Some("user@host".to_string()),
            options: Some(r#"command="echo hello""#.to_string()),
        };
        assert_eq!(
            key.to_line(),
            r#"command="echo hello" ssh-rsa AAAA... user@host"#
        );
    }

    #[test]
    fn test_authorized_key_add_key() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Present),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: false,
            manage_dir: true,
            comment: None,
            key_options: None,
        };

        let result = authorized_key(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&keys_path).unwrap();
        assert!(content.contains("ssh-rsa"));
        assert!(content.contains("user@host"));

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }

    #[test]
    fn test_authorized_key_add_existing_key_no_change() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        fs::create_dir_all(keys_path.parent().unwrap()).unwrap();
        fs::write(
            &keys_path,
            "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host\n",
        )
        .unwrap();

        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Present),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: false,
            manage_dir: true,
            comment: None,
            key_options: None,
        };

        let result = authorized_key(params, false).unwrap();
        assert!(!result.changed);

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }

    #[test]
    fn test_authorized_key_remove_key() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        fs::create_dir_all(keys_path.parent().unwrap()).unwrap();
        fs::write(&keys_path, "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host\nssh-ed25519 BBBB... other@host\n").unwrap();

        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Absent),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: false,
            manage_dir: true,
            comment: None,
            key_options: None,
        };

        let result = authorized_key(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&keys_path).unwrap();
        assert!(!content.contains("ssh-rsa"));
        assert!(content.contains("ssh-ed25519"));

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }

    #[test]
    fn test_authorized_key_exclusive() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        fs::create_dir_all(keys_path.parent().unwrap()).unwrap();
        fs::write(
            &keys_path,
            "ssh-rsa OLDKEY... old@host\nssh-ed25519 OLDKEY2... old2@host\n",
        )
        .unwrap();

        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Present),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: true,
            manage_dir: true,
            comment: None,
            key_options: None,
        };

        let result = authorized_key(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&keys_path).unwrap();
        assert!(content.contains("AAAAB3NzaC1yc2EAAAADAQABAAABgQC..."));
        assert!(!content.contains("OLDKEY"));

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }

    #[test]
    fn test_authorized_key_with_options() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Present),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: false,
            manage_dir: true,
            comment: None,
            key_options: Some(r#"no-port-forwarding,from="10.0.1.1""#.to_string()),
        };

        let result = authorized_key(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&keys_path).unwrap();
        assert!(content.contains("no-port-forwarding"));
        assert!(content.contains("from=\"10.0.1.1\""));

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }

    #[test]
    fn test_authorized_key_check_mode() {
        let dir = tempdir().unwrap();
        let passwd_path = setup_test_passwd(dir.path());
        unsafe {
            std::env::set_var("RASH_TEST_PASSWD_FILE", &passwd_path);
        }

        let keys_path = dir.path().join(".ssh/authorized_keys");
        let params = Params {
            user: "deploy".to_string(),
            key: Some(KeyInput::Single(
                "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host".to_string(),
            )),
            state: Some(State::Present),
            path: Some(keys_path.to_string_lossy().to_string()),
            exclusive: false,
            manage_dir: true,
            comment: None,
            key_options: None,
        };

        let result = authorized_key(params, true).unwrap();
        assert!(result.changed);
        assert!(!keys_path.exists());

        unsafe {
            std::env::remove_var("RASH_TEST_PASSWD_FILE");
        }
    }
}
