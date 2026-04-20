/// ANCHOR: module
/// # htpasswd
///
/// Manage htpasswd files for HTTP Basic Authentication.
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
/// - name: Add user with apr1 hash
///   htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: admin
///     password: secret123
///     state: present
///
/// - name: Add user with SHA-512 hash
///   htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: admin
///     password: secret123
///     crypt: sha512
///     state: present
///
/// - name: Remove user
///   htpasswd:
///     path: /etc/nginx/.htpasswd
///     name: admin
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use base64::Engine;
use md5::{Digest, Md5};
use minijinja::Value;
use rand::RngExt;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
use sha2::{Sha256, Sha512};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const APR1_SALT_CHARS: &[u8] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const APR1_SALT_LEN: usize = 8;

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the htpasswd file.
    pub path: String,
    /// Username to add or remove.
    pub name: String,
    /// Password for the user. Required when state=present.
    pub password: Option<String>,
    /// Hash algorithm to use.
    /// **[default: `"apr1"`]**
    pub crypt: Option<CryptScheme>,
    /// Whether the user should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum CryptScheme {
    #[default]
    Apr1,
    Sha256,
    Sha512,
}

fn generate_salt() -> String {
    let mut rng = rand::rng();
    (0..APR1_SALT_LEN)
        .map(|_| {
            let idx = rng.random_range(0..APR1_SALT_CHARS.len());
            APR1_SALT_CHARS[idx] as char
        })
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
    let mut final_hash = ctx1.finalize();

    let plen = password.len();
    let mut i = plen;
    loop {
        if i > 16 {
            ctx.update(final_hash);
        } else {
            ctx.update(&final_hash[..i]);
        }
        if i <= 16 {
            break;
        }
        i -= 16;
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

    final_hash = ctx.finalize();

    for j in 0..1000 {
        let mut ctx2 = Md5::new();
        if j & 1 != 0 {
            ctx2.update(password.as_bytes());
        } else {
            ctx2.update(final_hash);
        }
        if j % 3 != 0 {
            ctx2.update(salt.as_bytes());
        }
        if j % 7 != 0 {
            ctx2.update(password.as_bytes());
        }
        if j & 1 != 0 {
            ctx2.update(final_hash);
        } else {
            ctx2.update(password.as_bytes());
        }
        final_hash = ctx2.finalize();
    }

    let mut to_encode = [0u8; 16];
    to_encode.copy_from_slice(&final_hash);

    format!("$apr1${}${}", salt, apr1_custom_base64(&to_encode))
}

fn apr1_custom_base64(hash: &[u8]) -> String {
    let itoa64 = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    let to64 = |v: u32, n: usize| -> String {
        let mut s = String::new();
        let mut val = v;
        for _ in 0..n {
            s.push(itoa64[(val & 0x3f) as usize] as char);
            val >>= 6;
        }
        s
    };

    let mut result = String::new();

    result.push_str(&to64(
        ((hash[0] as u32) << 16) | ((hash[6] as u32) << 8) | (hash[12] as u32),
        4,
    ));
    result.push_str(&to64(
        ((hash[1] as u32) << 16) | ((hash[7] as u32) << 8) | (hash[13] as u32),
        4,
    ));
    result.push_str(&to64(
        ((hash[2] as u32) << 16) | ((hash[8] as u32) << 8) | (hash[14] as u32),
        4,
    ));
    result.push_str(&to64(
        ((hash[3] as u32) << 16) | ((hash[9] as u32) << 8) | (hash[15] as u32),
        4,
    ));
    result.push_str(&to64(
        ((hash[4] as u32) << 16) | ((hash[10] as u32) << 8) | (hash[5] as u32),
        4,
    ));
    result.push_str(&to64(hash[11] as u32, 2));

    result
}

fn hash_sha256(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    let result = hasher.finalize();
    format!(
        "{{SHA256}}{}",
        base64::engine::general_purpose::STANDARD.encode(result)
    )
}

fn hash_sha512(password: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(password.as_bytes());
    let result = hasher.finalize();
    format!(
        "{{SHA512}}{}",
        base64::engine::general_purpose::STANDARD.encode(result)
    )
}

fn hash_password(password: &str, scheme: &CryptScheme) -> String {
    match scheme {
        CryptScheme::Apr1 => {
            let salt = generate_salt();
            hash_apr1(password, &salt)
        }
        CryptScheme::Sha256 => hash_sha256(password),
        CryptScheme::Sha512 => hash_sha512(password),
    }
}

fn verify_password(password: &str, stored_hash: &str) -> bool {
    if let Some(rest) = stored_hash.strip_prefix("$apr1$") {
        let parts: Vec<&str> = rest.splitn(2, '$').collect();
        if parts.len() == 2 {
            let salt = parts[0];
            let computed = hash_apr1(password, salt);
            return computed == stored_hash;
        }
        false
    } else if let Some(hash_b64) = stored_hash.strip_prefix("{SHA256}") {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        let result = hasher.finalize();
        let computed = base64::engine::general_purpose::STANDARD.encode(result);
        computed == hash_b64
    } else if let Some(hash_b64) = stored_hash.strip_prefix("{SHA512}") {
        let mut hasher = Sha512::new();
        hasher.update(password.as_bytes());
        let result = hasher.finalize();
        let computed = base64::engine::general_purpose::STANDARD.encode(result);
        computed == hash_b64
    } else {
        false
    }
}

#[derive(Debug, Clone, PartialEq)]
struct HtpasswdEntry {
    username: String,
    password_hash: String,
}

impl HtpasswdEntry {
    fn from_line(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let (username, password_hash) = trimmed.split_once(':')?;
        Some(HtpasswdEntry {
            username: username.to_string(),
            password_hash: password_hash.to_string(),
        })
    }

    #[allow(dead_code)]
    fn to_line(&self) -> String {
        format!("{}:{}", self.username, self.password_hash)
    }
}

fn read_htpasswd_file(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .map_while(std::result::Result::ok)
                .collect()
        })
        .unwrap_or_default()
}

fn find_entry_in_lines(lines: &[String], username: &str) -> Option<(usize, HtpasswdEntry)> {
    lines.iter().enumerate().find_map(|(idx, line)| {
        let entry = HtpasswdEntry::from_line(line)?;
        if entry.username == username {
            Some((idx, entry))
        } else {
            None
        }
    })
}

pub fn htpasswd(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let path = Path::new(&params.path);
    let lines = read_htpasswd_file(path);
    let original = lines.join("\n");
    let state = params.state.clone().unwrap_or_default();

    let mut changed = false;
    let mut new_lines = lines.clone();

    match state {
        State::Present => {
            let password = params.password.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "password is required when state=present",
                )
            })?;

            let crypt_scheme = params.crypt.clone().unwrap_or_default();

            if let Some((idx, existing_entry)) = find_entry_in_lines(&lines, &params.name) {
                if !verify_password(password, &existing_entry.password_hash) {
                    let new_hash = hash_password(password, &crypt_scheme);
                    new_lines[idx] = format!("{}:{}", params.name, new_hash);
                    changed = true;
                }
            } else {
                let new_hash = hash_password(password, &crypt_scheme);
                if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    new_lines.push(String::new());
                }
                new_lines.push(format!("{}:{}", params.name, new_hash));
                changed = true;
            }
        }
        State::Absent => {
            while let Some((idx, _)) = find_entry_in_lines(&new_lines, &params.name) {
                new_lines.remove(idx);
                changed = true;
            }
        }
    }

    if changed && !check_mode {
        let new_content = new_lines.join("\n");
        diff(format!("{original}\n"), format!("{new_content}\n"));

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        write!(file, "{new_content}")?;
    }

    Ok(ModuleResult::new(changed, None, Some(params.name)))
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
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/nginx/.htpasswd
            name: admin
            password: secret123
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/etc/nginx/.htpasswd");
        assert_eq!(params.name, "admin");
        assert_eq!(params.password, Some("secret123".to_string()));
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_parse_params_with_crypt() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /etc/nginx/.htpasswd
            name: admin
            password: secret123
            crypt: sha512
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.crypt, Some(CryptScheme::Sha512));
    }

    #[test]
    fn test_hash_sha256() {
        let hash = hash_sha256("password");
        assert!(hash.starts_with("{SHA256}"));
        assert_ne!(hash, "{SHA256}");
    }

    #[test]
    fn test_hash_sha512() {
        let hash = hash_sha512("password");
        assert!(hash.starts_with("{SHA512}"));
        assert_ne!(hash, "{SHA512}");
    }

    #[test]
    fn test_hash_sha256_deterministic() {
        let hash1 = hash_sha256("password");
        let hash2 = hash_sha256("password");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_sha512_deterministic() {
        let hash1 = hash_sha512("password");
        let hash2 = hash_sha512("password");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verify_sha256_password() {
        let hash = hash_sha256("secret");
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_verify_sha512_password() {
        let hash = hash_sha512("secret");
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_verify_apr1_password() {
        let hash = hash_apr1("secret", "testsalt");
        assert!(hash.starts_with("$apr1$testsalt$"));
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_htpasswd_entry_from_line() {
        let entry = HtpasswdEntry::from_line("admin:$apr1$salt$hash").unwrap();
        assert_eq!(entry.username, "admin");
        assert_eq!(entry.password_hash, "$apr1$salt$hash");
    }

    #[test]
    fn test_htpasswd_entry_from_line_sha256() {
        let entry = HtpasswdEntry::from_line("admin:{SHA256}abc123==").unwrap();
        assert_eq!(entry.username, "admin");
        assert_eq!(entry.password_hash, "{SHA256}abc123==");
    }

    #[test]
    fn test_htpasswd_entry_from_line_ignores_invalid() {
        assert!(HtpasswdEntry::from_line("").is_none());
        assert!(HtpasswdEntry::from_line("# comment").is_none());
        assert!(HtpasswdEntry::from_line("nocolon").is_none());
    }

    #[test]
    fn test_htpasswd_entry_to_line() {
        let entry = HtpasswdEntry {
            username: "admin".to_string(),
            password_hash: "$apr1$salt$hash".to_string(),
        };
        assert_eq!(entry.to_line(), "admin:$apr1$salt$hash");
    }

    #[test]
    fn test_apr1_known_hash() {
        let hash = hash_apr1("secret", "testsalt");
        assert_eq!(hash, "$apr1$testsalt$j7AAmGAhN8liB8qiU.irj1");
    }

    #[test]
    fn test_htpasswd_add_user() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        assert!(content.starts_with("admin:{SHA256}"));
    }

    #[test]
    fn test_htpasswd_same_password_no_change() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let hash = hash_sha256("secret123");
        fs::write(&htpasswd_file, format!("admin:{hash}\n")).unwrap();

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_htpasswd_update_password() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let old_hash = hash_sha256("oldpass");
        fs::write(&htpasswd_file, format!("admin:{old_hash}\n")).unwrap();

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("newpass".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        let new_hash = hash_sha256("newpass");
        assert!(content.contains(&new_hash));
        assert!(!content.contains(&old_hash));
    }

    #[test]
    fn test_htpasswd_remove_user() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");
        fs::write(&htpasswd_file, "admin:{SHA256}abc\nuser2:{SHA256}def\n").unwrap();

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: None,
            crypt: None,
            state: Some(State::Absent),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        assert!(!content.contains("admin:"));
        assert!(content.contains("user2:"));
    }

    #[test]
    fn test_htpasswd_check_mode() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, true).unwrap();
        assert!(result.changed);
        assert!(!htpasswd_file.exists());
    }

    #[test]
    fn test_htpasswd_apr1_hash() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Apr1),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        assert!(content.contains("admin:$apr1$"));

        let hash_line = content.lines().next().unwrap();
        let hash_val = hash_line.split_once(':').unwrap().1;
        assert!(verify_password("secret123", hash_val));
    }

    #[test]
    fn test_htpasswd_sha512_hash() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha512),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        assert!(content.contains("admin:{SHA512}"));
    }

    #[test]
    fn test_htpasswd_preserves_other_users() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join(".htpasswd");
        let other_hash = hash_sha256("otherpass");
        fs::write(&htpasswd_file, format!("other:{other_hash}\n")).unwrap();

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&htpasswd_file).unwrap();
        assert!(content.contains("other:"));
        assert!(content.contains("admin:"));
    }

    #[test]
    fn test_htpasswd_missing_password_error() {
        let params = Params {
            path: "/tmp/test".to_string(),
            name: "admin".to_string(),
            password: None,
            state: Some(State::Present),
            crypt: None,
        };

        let result = htpasswd(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("password is required")
        );
    }

    #[test]
    fn test_htpasswd_creates_parent_dir() {
        let dir = tempdir().unwrap();
        let htpasswd_file = dir.path().join("subdir").join(".htpasswd");

        let params = Params {
            path: htpasswd_file.to_str().unwrap().to_string(),
            name: "admin".to_string(),
            password: Some("secret123".to_string()),
            crypt: Some(CryptScheme::Sha256),
            state: Some(State::Present),
        };

        let result = htpasswd(params, false).unwrap();
        assert!(result.changed);
        assert!(htpasswd_file.exists());
    }
}
