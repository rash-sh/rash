/// ANCHOR: module
/// # known_hosts
///
/// Add or remove SSH known hosts entries.
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
/// - known_hosts:
///     name: github.com
///     key: github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC...
///
/// - known_hosts:
///     name: github.com
///     key: '{{ lookup("file", "~/.ssh/github_key.pub") }}'
///
/// - known_hosts:
///     name: old-server.local
///     state: absent
///
/// - known_hosts:
///     name: 192.168.1.100
///     key: 192.168.1.100 ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTY...
///     path: /home/deploy/.ssh/known_hosts
/// ```
/// ANCHOR_END: examples
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::env;
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

const DEFAULT_KNOWN_HOSTS_PATH: &str = "~/.ssh/known_hosts";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The host name or IP address to manage.
    pub name: String,
    /// The SSH public key string. Required when state=present.
    pub key: Option<String>,
    /// Whether the host should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the known_hosts file.
    /// **[default: `"~/.ssh/known_hosts"`]**
    pub path: Option<String>,
    /// Hash hostnames in the known_hosts file for privacy.
    /// **[default: `false`]**
    #[serde(default)]
    pub hash_host: bool,
    /// Fail if host not found when state=absent.
    /// **[default: `false`]**
    #[serde(default)]
    pub fail_on_notfound: bool,
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
pub struct KnownHostsEntry {
    pub hostnames: Vec<String>,
    pub key_type: String,
    pub key_data: String,
    pub hashed: bool,
}

impl KnownHostsEntry {
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
                let hostnames_str = &line[..pos];
                let hostnames_str = hostnames_str.trim();

                let hostnames: Vec<String> = if hostnames_str.starts_with('|') {
                    vec![hostnames_str.to_string()]
                } else {
                    hostnames_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect()
                };

                if hostnames.is_empty() || hostnames.iter().all(|h| h.is_empty()) {
                    continue;
                }

                let after = &line[pos + key_type.len()..];
                let after_parts: Vec<&str> = after.split_whitespace().collect();

                if after_parts.is_empty() {
                    continue;
                }

                let key_data = after_parts[0].to_string();
                let hashed = hostnames_str.starts_with('|');

                return Some(KnownHostsEntry {
                    hostnames,
                    key_type: key_type.to_string(),
                    key_data,
                    hashed,
                });
            }
        }

        None
    }

    pub fn to_line(&self) -> String {
        let hostnames = self.hostnames.join(",");
        format!("{} {} {}", hostnames, self.key_type, self.key_data)
    }

    pub fn key_identifier(&self) -> String {
        format!("{} {}", self.key_type, self.key_data)
    }

    pub fn matches_hostname(&self, hostname: &str) -> bool {
        for h in &self.hostnames {
            if h == hostname {
                return true;
            }
            if h.starts_with('|') {
                continue;
            }
            if (h.contains('*') || h.contains('?')) && matches_pattern(h, hostname) {
                return true;
            }
        }
        false
    }
}

fn matches_pattern(pattern: &str, hostname: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let hostname_chars: Vec<char> = hostname.chars().collect();
    let mut dp = vec![vec![false; hostname_chars.len() + 1]; pattern_chars.len() + 1];
    dp[0][0] = true;

    for i in 1..=pattern_chars.len() {
        if pattern_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pattern_chars.len() {
        for j in 1..=hostname_chars.len() {
            if pattern_chars[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pattern_chars[i - 1] == '?' || pattern_chars[i - 1] == hostname_chars[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }

    dp[pattern_chars.len()][hostname_chars.len()]
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(&path[2..]);
    }
    PathBuf::from(path)
}

fn get_known_hosts_path(params: &Params) -> PathBuf {
    if let Some(ref path) = params.path {
        expand_tilde(path)
    } else {
        expand_tilde(DEFAULT_KNOWN_HOSTS_PATH)
    }
}

fn parse_key_input(key_str: &str, name: &str) -> Option<KnownHostsEntry> {
    let key_str = key_str.trim();

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
        if let Some(pos) = key_str.find(key_type) {
            let hostnames_str = &key_str[..pos];
            let hostnames_str = hostnames_str.trim();

            let hostnames: Vec<String> = if hostnames_str.is_empty() {
                vec![name.to_string()]
            } else if hostnames_str.starts_with('|') {
                vec![hostnames_str.to_string()]
            } else {
                hostnames_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            };

            let after = &key_str[pos + key_type.len()..];
            let after_parts: Vec<&str> = after.split_whitespace().collect();

            if after_parts.is_empty() {
                continue;
            }

            return Some(KnownHostsEntry {
                hostnames,
                key_type: key_type.to_string(),
                key_data: after_parts[0].to_string(),
                hashed: false,
            });
        }
    }

    None
}

pub fn known_hosts(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let known_hosts_path = get_known_hosts_path(&params);

    match state {
        State::Present => {
            let key_str = params.key.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "key parameter is required when state=present",
                )
            })?;

            let mut entry = parse_key_input(key_str, &params.name)
                .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Invalid SSH key format"))?;

            if !entry.hostnames.contains(&params.name) {
                entry.hostnames.push(params.name.clone());
            }

            let original_content = if known_hosts_path.exists() {
                fs::read_to_string(&known_hosts_path)?
            } else {
                String::new()
            };

            let mut existing_entries: Vec<KnownHostsEntry> = original_content
                .lines()
                .filter_map(KnownHostsEntry::parse)
                .collect();

            let key_id = entry.key_identifier();
            let mut found_match = false;
            let mut changed = false;

            for existing in &mut existing_entries {
                if existing.key_identifier() == key_id {
                    found_match = true;
                    if !existing.hostnames.contains(&params.name) {
                        existing.hostnames.push(params.name.clone());
                        changed = true;
                    }
                    break;
                }
            }

            if !found_match {
                let mut host_found = false;
                for existing in &existing_entries {
                    if existing.matches_hostname(&params.name) {
                        host_found = true;
                        break;
                    }
                }

                if !host_found {
                    existing_entries.push(entry);
                    changed = true;
                }
            }

            if changed {
                let new_content = if existing_entries.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{}\n",
                        existing_entries
                            .iter()
                            .map(|e| e.to_line())
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };

                diff(&original_content, &new_content);

                if !check_mode {
                    if let Some(parent) = known_hosts_path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }

                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&known_hosts_path)?;
                    file.write_all(new_content.as_bytes())?;
                }
            }

            Ok(ModuleResult {
                changed,
                output: Some(known_hosts_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
        State::Absent => {
            let original_content = if known_hosts_path.exists() {
                fs::read_to_string(&known_hosts_path)?
            } else {
                if params.fail_on_notfound {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Host '{}' not found in known_hosts", params.name),
                    ));
                }
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(known_hosts_path.to_string_lossy().to_string()),
                    extra: None,
                });
            };

            let existing_entries: Vec<KnownHostsEntry> = original_content
                .lines()
                .filter_map(KnownHostsEntry::parse)
                .collect();

            let mut new_entries = Vec::new();
            let mut changed = false;

            for entry in existing_entries {
                if entry.matches_hostname(&params.name) {
                    changed = true;
                } else {
                    new_entries.push(entry);
                }
            }

            if !changed && params.fail_on_notfound {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Host '{}' not found in known_hosts", params.name),
                ));
            }

            if changed {
                let new_content = if new_entries.is_empty() {
                    String::new()
                } else {
                    format!(
                        "{}\n",
                        new_entries
                            .iter()
                            .map(|e| e.to_line())
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };

                diff(&original_content, &new_content);

                if !check_mode {
                    if new_entries.is_empty() {
                        fs::remove_file(&known_hosts_path)?;
                    } else {
                        let mut file = OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .open(&known_hosts_path)?;
                        file.write_all(new_content.as_bytes())?;
                    }
                }
            }

            Ok(ModuleResult {
                changed,
                output: Some(known_hosts_path.to_string_lossy().to_string()),
                extra: None,
            })
        }
    }
}

#[derive(Debug)]
pub struct KnownHosts;

impl Module for KnownHosts {
    fn get_name(&self) -> &str {
        "known_hosts"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            known_hosts(parse_params(optional_params)?, check_mode)?,
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

    #[test]
    fn test_parse_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: github.com
            key: github.com ssh-rsa AAAA...
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "github.com");
        assert_eq!(params.state, Some(State::Present));
    }

    #[test]
    fn test_known_hosts_entry_parse() {
        let line = "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... user@host";
        let entry = KnownHostsEntry::parse(line).unwrap();
        assert_eq!(entry.hostnames, vec!["github.com"]);
        assert_eq!(entry.key_type, "ssh-rsa");
        assert_eq!(entry.key_data, "AAAAB3NzaC1yc2EAAAADAQABAAABgQC...");
        assert!(!entry.hashed);
    }

    #[test]
    fn test_known_hosts_entry_parse_multiple_hosts() {
        let line = "github.com,gitlab.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC...";
        let entry = KnownHostsEntry::parse(line).unwrap();
        assert_eq!(entry.hostnames, vec!["github.com", "gitlab.com"]);
    }

    #[test]
    fn test_known_hosts_entry_parse_ecdsa() {
        let line = "192.168.1.1 ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTY...";
        let entry = KnownHostsEntry::parse(line).unwrap();
        assert_eq!(entry.key_type, "ecdsa-sha2-nistp256");
        assert_eq!(entry.hostnames, vec!["192.168.1.1"]);
    }

    #[test]
    fn test_known_hosts_entry_to_line() {
        let entry = KnownHostsEntry {
            hostnames: vec!["github.com".to_string()],
            key_type: "ssh-rsa".to_string(),
            key_data: "AAAA...".to_string(),
            hashed: false,
        };
        assert_eq!(entry.to_line(), "github.com ssh-rsa AAAA...");
    }

    #[test]
    fn test_known_hosts_entry_matches_hostname() {
        let entry = KnownHostsEntry {
            hostnames: vec!["github.com".to_string()],
            key_type: "ssh-rsa".to_string(),
            key_data: "AAAA...".to_string(),
            hashed: false,
        };
        assert!(entry.matches_hostname("github.com"));
        assert!(!entry.matches_hostname("gitlab.com"));
    }

    #[test]
    fn test_known_hosts_add_entry() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        let params = Params {
            name: "github.com".to_string(),
            key: Some(
                "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host".to_string(),
            ),
            state: Some(State::Present),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: false,
        };

        let result = known_hosts(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&known_hosts_path).unwrap();
        assert!(content.contains("github.com"));
        assert!(content.contains("ssh-rsa"));
    }

    #[test]
    fn test_known_hosts_add_existing_no_change() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        fs::create_dir_all(known_hosts_path.parent().unwrap()).unwrap();
        fs::write(
            &known_hosts_path,
            "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host\n",
        )
        .unwrap();

        let params = Params {
            name: "github.com".to_string(),
            key: Some(
                "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host".to_string(),
            ),
            state: Some(State::Present),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: false,
        };

        let result = known_hosts(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_known_hosts_remove_entry() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        fs::create_dir_all(known_hosts_path.parent().unwrap()).unwrap();
        fs::write(
            &known_hosts_path,
            "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host\n\
             gitlab.com ssh-rsa BBBB... other@host\n",
        )
        .unwrap();

        let params = Params {
            name: "github.com".to_string(),
            key: None,
            state: Some(State::Absent),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: false,
        };

        let result = known_hosts(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&known_hosts_path).unwrap();
        assert!(!content.contains("github.com"));
        assert!(content.contains("gitlab.com"));
    }

    #[test]
    fn test_known_hosts_remove_not_found_no_fail() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        fs::create_dir_all(known_hosts_path.parent().unwrap()).unwrap();
        fs::write(
            &known_hosts_path,
            "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host\n",
        )
        .unwrap();

        let params = Params {
            name: "nonexistent.com".to_string(),
            key: None,
            state: Some(State::Absent),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: false,
        };

        let result = known_hosts(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_known_hosts_remove_not_found_with_fail() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        fs::create_dir_all(known_hosts_path.parent().unwrap()).unwrap();
        fs::write(
            &known_hosts_path,
            "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host\n",
        )
        .unwrap();

        let params = Params {
            name: "nonexistent.com".to_string(),
            key: None,
            state: Some(State::Absent),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: true,
        };

        let result = known_hosts(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_known_hosts_check_mode() {
        let dir = tempdir().unwrap();
        let known_hosts_path = dir.path().join(".ssh/known_hosts");
        let params = Params {
            name: "github.com".to_string(),
            key: Some(
                "github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC... test@host".to_string(),
            ),
            state: Some(State::Present),
            path: Some(known_hosts_path.to_string_lossy().to_string()),
            hash_host: false,
            fail_on_notfound: false,
        };

        let result = known_hosts(params, true).unwrap();
        assert!(result.changed);
        assert!(!known_hosts_path.exists());
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("*.example.com", "test.example.com"));
        assert!(matches_pattern("*.example.com", "sub.example.com"));
        assert!(!matches_pattern("*.example.com", "example.org"));
        assert!(matches_pattern("host?", "host1"));
        assert!(matches_pattern("host?", "host2"));
        assert!(!matches_pattern("host?", "host10"));
    }
}
