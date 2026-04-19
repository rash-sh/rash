/// ANCHOR: module
/// # htpasswd
///
/// Manage htpasswd files for HTTP Basic Authentication.
/// Used by Apache, nginx, and other web servers.
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
/// - htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: admin
///     password: s3cur3p@ss
///     state: present
///
/// - htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: admin
///     password: s3cur3p@ss
///     crypt: apr1
///
/// - htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: olduser
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::Path;

use base64::Engine;
use md5::{Digest, Md5};
use minijinja::Value;
use rand::RngExt;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use sha1::Sha1;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the htpasswd file.
    pub path: String,
    /// The username to add or remove.
    pub name: String,
    /// The password for the user. Required when state=present.
    pub password: Option<String>,
    /// Whether the user should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: Option<State>,
    /// Hash algorithm to use for the password.
    /// **[default: `"apr1"`]**
    #[serde(default = "default_crypt")]
    pub crypt: Option<CryptScheme>,
}

fn default_crypt() -> Option<CryptScheme> {
    Some(CryptScheme::Apr1)
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
pub enum CryptScheme {
    #[default]
    Apr1,
    Sha1,
}

const APR1_BASE64_CHARS: &[u8; 64] =
    b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn to64(v: u32, n: usize) -> String {
    let mut s = String::with_capacity(n);
    let mut v = v;
    for _ in 0..n {
        s.push(APR1_BASE64_CHARS[(v & 0x3f) as usize] as char);
        v >>= 6;
    }
    s
}

fn encode_apr1_hash(final_hash: &[u8]) -> String {
    let f = final_hash;
    let mut result = String::with_capacity(22);
    result.push_str(&to64(
        (f[0] as u32) << 16 | (f[6] as u32) << 8 | (f[12] as u32),
        4,
    ));
    result.push_str(&to64(
        (f[1] as u32) << 16 | (f[7] as u32) << 8 | (f[13] as u32),
        4,
    ));
    result.push_str(&to64(
        (f[2] as u32) << 16 | (f[8] as u32) << 8 | (f[14] as u32),
        4,
    ));
    result.push_str(&to64(
        (f[3] as u32) << 16 | (f[9] as u32) << 8 | (f[15] as u32),
        4,
    ));
    result.push_str(&to64(
        (f[4] as u32) << 16 | (f[10] as u32) << 8 | (f[5] as u32),
        4,
    ));
    result.push_str(&to64(f[11] as u32, 2));
    result
}

fn generate_salt() -> String {
    let mut rng = rand::rng();
    (0..8)
        .map(|_| APR1_BASE64_CHARS[rng.random_range(0..64)] as char)
        .collect()
}

fn hash_apr1(password: &str, salt: &str) -> String {
    let magic = b"$apr1$";

    let mut ctx = Md5::new();
    ctx.update(password.as_bytes());
    ctx.update(magic);
    ctx.update(salt.as_bytes());

    let mut ctx1 = Md5::new();
    ctx1.update(password.as_bytes());
    ctx1.update(salt.as_bytes());
    ctx1.update(password.as_bytes());
    let final_ctx1 = ctx1.finalize();

    let plen = password.len();
    let mut i = plen;
    while i > 0 {
        let chunk_size = if i >= 16 { 16 } else { i };
        ctx.update(&final_ctx1[..chunk_size]);
        i -= chunk_size;
    }

    i = plen;
    while i > 0 {
        if i & 1 != 0 {
            ctx.update(b"\x00");
        } else {
            ctx.update(&password.as_bytes()[..1]);
        }
        i >>= 1;
    }

    let mut result = ctx.finalize();

    for j in 0..1000 {
        let mut ctx2 = Md5::new();
        if j & 1 != 0 {
            ctx2.update(password.as_bytes());
        } else {
            ctx2.update(result);
        }
        if j % 3 != 0 {
            ctx2.update(salt.as_bytes());
        }
        if j % 7 != 0 {
            ctx2.update(password.as_bytes());
        }
        if j & 1 != 0 {
            ctx2.update(result);
        } else {
            ctx2.update(password.as_bytes());
        }
        result = ctx2.finalize();
    }

    format!("$apr1${}${}", salt, encode_apr1_hash(&result))
}

fn hash_sha1(password: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(password.as_bytes());
    let result = hasher.finalize();
    let encoded = base64::engine::general_purpose::STANDARD.encode(result);
    format!("{{SHA}}{}", encoded)
}

fn hash_password(password: &str, scheme: &CryptScheme) -> String {
    match scheme {
        CryptScheme::Apr1 => {
            let salt = generate_salt();
            hash_apr1(password, &salt)
        }
        CryptScheme::Sha1 => hash_sha1(password),
    }
}

struct HtpasswdEntry {
    username: String,
    password_hash: String,
}

impl HtpasswdEntry {
    fn parse_line(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        Some(HtpasswdEntry {
            username: parts[0].to_string(),
            password_hash: parts[1].to_string(),
        })
    }

    fn to_line(&self) -> String {
        format!("{}:{}", self.username, self.password_hash)
    }
}

fn parse_htpasswd_file(content: &str) -> Vec<HtpasswdEntry> {
    content
        .lines()
        .filter_map(HtpasswdEntry::parse_line)
        .collect()
}

pub fn htpasswd(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let crypt_scheme = params.crypt.clone().unwrap_or_default();

    let path = Path::new(&params.path);

    let original_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut entries = parse_htpasswd_file(&original_content);

    match state {
        State::Present => {
            let password = params.password.ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "password is required when state=present",
                )
            })?;

            let new_hash = hash_password(&password, &crypt_scheme);
            let mut found = false;
            let mut changed = false;

            for entry in &mut entries {
                if entry.username == params.name {
                    found = true;
                    if entry.password_hash != new_hash {
                        entry.password_hash = new_hash.clone();
                        changed = true;
                    }
                    break;
                }
            }

            if !found {
                entries.push(HtpasswdEntry {
                    username: params.name,
                    password_hash: new_hash,
                });
                changed = true;
            }

            if changed {
                let new_content = entries
                    .iter()
                    .map(|e| e.to_line())
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";

                diff(&original_content, &new_content);

                if !check_mode {
                    if let Some(parent) = path.parent()
                        && !parent.as_os_str().is_empty()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }
                    let mut file = fs::File::create(path)?;
                    file.write_all(new_content.as_bytes())?;
                }

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(params.path),
                    extra: None,
                });
            }

            Ok(ModuleResult {
                changed: false,
                output: Some(params.path),
                extra: None,
            })
        }
        State::Absent => {
            let original_len = entries.len();
            entries.retain(|e| e.username != params.name);
            let changed = entries.len() != original_len;

            if changed {
                let new_content = if entries.is_empty() {
                    String::new()
                } else {
                    entries
                        .iter()
                        .map(|e| e.to_line())
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n"
                };

                diff(&original_content, &new_content);

                if !check_mode {
                    if entries.is_empty() && path.exists() {
                        fs::remove_file(path)?;
                    } else if !entries.is_empty() {
                        let mut file = fs::File::create(path)?;
                        file.write_all(new_content.as_bytes())?;
                    }
                }

                return Ok(ModuleResult {
                    changed: true,
                    output: Some(params.path),
                    extra: None,
                });
            }

            Ok(ModuleResult {
                changed: false,
                output: Some(params.path),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct Htpasswd;

impl Module for Htpasswd {
    fn get_name(&self) -> &str {
        "htpasswd"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((htpasswd(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/nginx/.htpasswd
            name: admin
            password: s3cur3
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/nginx/.htpasswd");
        assert_eq!(params.name, "admin");
        assert_eq!(params.password, Some("s3cur3".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/nginx/.htpasswd
            name: olduser
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "olduser");
        assert_eq!(params.state, Some(State::Absent));
    }

    #[test]
    fn test_parse_params_default_crypt() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/nginx/.htpasswd
            name: admin
            password: test123
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.crypt, Some(CryptScheme::Apr1));
    }

    #[test]
    fn test_hash_sha1() {
        let hash = hash_sha1("password");
        assert!(hash.starts_with("{SHA}"));
        assert_eq!(hash.len(), 33);
    }

    #[test]
    fn test_hash_sha1_known_value() {
        let hash = hash_sha1("password");
        let expected = "{SHA}W6ph5Mm5Pz8GgiULbPgzG37mj9g=";
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_hash_apr1_format() {
        let hash = hash_apr1("password", "12345678");
        assert!(hash.starts_with("$apr1$12345678$"));
        assert!(hash.len() > 20);
    }

    #[test]
    fn test_hash_apr1_deterministic() {
        let hash1 = hash_apr1("password", "12345678");
        let hash2 = hash_apr1("password", "12345678");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_apr1_different_salt() {
        let hash1 = hash_apr1("password", "12345678");
        let hash2 = hash_apr1("password", "abcdefgh");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_apr1_different_password() {
        let hash1 = hash_apr1("password1", "12345678");
        let hash2 = hash_apr1("password2", "12345678");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_apr1_known_value() {
        let hash = hash_apr1("password", "xxxxxxxx");
        assert_eq!(hash, "$apr1$xxxxxxxx$dxHfLAsjHkDRmG83UXe8K0");
    }

    #[test]
    fn test_parse_htpasswd_entry() {
        let entry = HtpasswdEntry::parse_line("admin:$apr1$salt$hash").unwrap();
        assert_eq!(entry.username, "admin");
        assert_eq!(entry.password_hash, "$apr1$salt$hash");
    }

    #[test]
    fn test_parse_htpasswd_entry_sha() {
        let entry = HtpasswdEntry::parse_line("user:{SHA}W6ph5Mm5Pz8GgiULbPgzG37mj9g=").unwrap();
        assert_eq!(entry.username, "user");
        assert_eq!(entry.password_hash, "{SHA}W6ph5Mm5Pz8GgiULbPgzG37mj9g=");
    }

    #[test]
    fn test_parse_htpasswd_entry_invalid() {
        assert!(HtpasswdEntry::parse_line("").is_none());
        assert!(HtpasswdEntry::parse_line("# comment").is_none());
        assert!(HtpasswdEntry::parse_line("nocolon").is_none());
    }

    #[test]
    fn test_htpasswd_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: Some("s3cur3".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Sha1),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.starts_with("admin:{SHA}"));
    }

    #[test]
    fn test_htpasswd_add_user_to_existing() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        fs::write(&file_path, "existing:{SHA}hash123\n").unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "newuser".to_string(),
            password: Some("pass".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Sha1),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("existing:{SHA}hash123"));
        assert!(content.contains("newuser:{SHA}"));
    }

    #[test]
    fn test_htpasswd_update_password() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        fs::write(&file_path, "admin:{SHA}oldhash\n").unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: Some("newpass".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Sha1),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let expected_hash = hash_sha1("newpass");
        assert!(content.contains(&format!("admin:{}", expected_hash)));
    }

    #[test]
    fn test_htpasswd_no_change_same_password() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        let expected_hash = hash_sha1("password");
        fs::write(&file_path, format!("admin:{}\n", expected_hash)).unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: Some("password".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Sha1),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_htpasswd_remove_user() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        fs::write(&file_path, "admin:{SHA}hash1\nuser2:{SHA}hash2\n").unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: None,
            state: Some(State::Absent),
            crypt: None,
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("admin"));
        assert!(content.contains("user2"));
    }

    #[test]
    fn test_htpasswd_remove_last_user_deletes_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        fs::write(&file_path, "admin:{SHA}hash1\n").unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: None,
            state: Some(State::Absent),
            crypt: None,
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_htpasswd_remove_nonexistent_user() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");
        fs::write(&file_path, "admin:{SHA}hash1\n").unwrap();

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "nobody".to_string(),
            password: None,
            state: Some(State::Absent),
            crypt: None,
        };

        let result = htpasswd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_htpasswd_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: Some("s3cur3".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Sha1),
        };

        let result = htpasswd(params, true).unwrap();
        assert!(result.changed);
        assert!(!file_path.exists());
    }

    #[test]
    fn test_htpasswd_apr1_crypt() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: Some("s3cur3".to_string()),
            state: Some(State::Present),
            crypt: Some(CryptScheme::Apr1),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.starts_with("admin:$apr1$"));
    }

    #[test]
    fn test_htpasswd_password_required_present() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join(".htpasswd");

        let params = Params {
            path: file_path.to_string_lossy().to_string(),
            name: "admin".to_string(),
            password: None,
            state: Some(State::Present),
            crypt: None,
        };

        let result = htpasswd(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_salt_length() {
        let salt = generate_salt();
        assert_eq!(salt.len(), 8);
    }

    #[test]
    fn test_generate_salt_chars() {
        let salt = generate_salt();
        for c in salt.chars() {
            assert!(APR1_BASE64_CHARS.contains(&(c as u8)));
        }
    }
}
