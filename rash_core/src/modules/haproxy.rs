/// ANCHOR: module
/// # haproxy
///
/// Manage HAProxy load balancer backend and frontend configurations.
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
/// - name: Create backend with servers
///   haproxy:
///     config_file: /etc/haproxy/haproxy.cfg
///     name: web_backend
///     state: present
///     servers:
///       - name: web1
///         address: 192.168.1.10:80
///       - name: web2
///         address: 192.168.1.11:80
///     balance: roundrobin
///
/// - name: Create backend with health checks
///   haproxy:
///     config_file: /etc/haproxy/haproxy.cfg
///     name: web_backend
///     state: present
///     balance: leastconn
///     check: option httpchk GET /health
///     servers:
///       - name: web1
///         address: 192.168.1.10:80
///         check: true
///       - name: web2
///         address: 192.168.1.11:80
///         check: true
///
/// - name: Remove backend
///   haproxy:
///     config_file: /etc/haproxy/haproxy.cfg
///     name: old_backend
///     state: absent
///
/// - name: Create frontend
///   haproxy:
///     config_file: /etc/haproxy/haproxy.cfg
///     name: http-in
///     section: frontend
///     state: present
///     check: bind *:80
/// ```
/// ANCHOR_END: examples
use crate::error::Result;
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::Write;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_HAPROXY_CONFIG: &str = "/etc/haproxy/haproxy.cfg";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Backend or frontend section name.
    name: String,
    /// Whether the section should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    state: State,
    /// Path to the HAProxy configuration file.
    /// **[default: `"/etc/haproxy/haproxy.cfg"`]**
    #[serde(default = "default_config_file")]
    config_file: String,
    /// Section type to manage.
    /// **[default: `"backend"`]**
    #[serde(default = "default_section")]
    section: Section,
    /// List of backend servers with name, address, and optional check flag.
    servers: Option<Vec<Server>>,
    /// Load balancing algorithm (e.g., roundrobin, leastconn, source).
    balance: Option<String>,
    /// Health check option string (e.g., "option httpchk GET /health").
    check: Option<String>,
}

fn default_state() -> State {
    State::Present
}

fn default_config_file() -> String {
    DEFAULT_HAPROXY_CONFIG.to_owned()
}

fn default_section() -> Section {
    Section::Backend
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum State {
    Present,
    Absent,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
enum Section {
    Backend,
    Frontend,
    Listen,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
pub struct Server {
    /// Server name.
    name: String,
    /// Server address (e.g., "192.168.1.10:80").
    address: String,
    /// Enable health checks for this server.
    /// **[default: `false`]**
    check: Option<bool>,
}

#[derive(Debug, Clone)]
struct ConfigBlock {
    section_type: String,
    name: Option<String>,
    lines: Vec<String>,
}

const SECTION_KEYWORDS: &[&str] = &["backend", "frontend", "listen", "defaults", "global"];

fn parse_config(content: &str) -> Vec<ConfigBlock> {
    let mut blocks: Vec<ConfigBlock> = Vec::new();
    let mut current: Option<ConfigBlock> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        let keyword = if trimmed.is_empty() || trimmed.starts_with('#') {
            String::new()
        } else {
            trimmed
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase()
        };

        if SECTION_KEYWORDS.contains(&keyword.as_str()) {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            let name = trimmed
                .split_once(char::is_whitespace)
                .map(|x| x.1)
                .map(|s| s.trim().to_string());
            current = Some(ConfigBlock {
                section_type: keyword,
                name,
                lines: vec![line.to_string()],
            });
        } else {
            match &mut current {
                Some(block) => block.lines.push(line.to_string()),
                None => {
                    current = Some(ConfigBlock {
                        section_type: String::new(),
                        name: None,
                        lines: vec![line.to_string()],
                    });
                }
            }
        }
    }

    if let Some(block) = current.take() {
        blocks.push(block);
    }

    blocks
}

fn find_block<'a>(
    blocks: &'a [ConfigBlock],
    section: &Section,
    name: &str,
) -> Option<(usize, &'a ConfigBlock)> {
    let type_str = section_to_str(section);
    blocks
        .iter()
        .enumerate()
        .find(|(_, b)| b.section_type == type_str && b.name.as_deref() == Some(name))
}

fn section_to_str(section: &Section) -> &'static str {
    match section {
        Section::Backend => "backend",
        Section::Frontend => "frontend",
        Section::Listen => "listen",
    }
}

fn generate_block(params: &Params) -> ConfigBlock {
    let section_type = section_to_str(&params.section);
    let mut lines = vec![format!("{} {}", section_type, params.name)];

    if let Some(ref balance) = params.balance {
        lines.push(format!("    balance {}", balance));
    }

    if let Some(ref check) = params.check {
        lines.push(format!("    {}", check));
    }

    if let Some(ref servers) = params.servers {
        for server in servers {
            let mut server_line = format!("    server {} {}", server.name, server.address);
            if server.check.unwrap_or(false) {
                server_line.push_str(" check");
            }
            lines.push(server_line);
        }
    }

    ConfigBlock {
        section_type: section_type.to_string(),
        name: Some(params.name.clone()),
        lines,
    }
}

fn trim_trailing_empty(lines: &[String]) -> Vec<String> {
    let mut result = lines.to_vec();
    while result.last().map(|l| l.is_empty()).unwrap_or(false) {
        result.pop();
    }
    result
}

fn blocks_to_string(blocks: &[ConfigBlock]) -> String {
    let lines: Vec<String> = blocks
        .iter()
        .flat_map(|b| b.lines.iter().cloned())
        .collect();
    let content = lines.join("\n");
    let trimmed = content.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}\n", trimmed)
    }
}

fn exec_haproxy(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let config_path = Path::new(&params.config_file);

    match params.state {
        State::Present => exec_present(&params, config_path, check_mode),
        State::Absent => exec_absent(&params, config_path, check_mode),
    }
}

fn exec_present(params: &Params, config_path: &Path, check_mode: bool) -> Result<ModuleResult> {
    let original_content = if config_path.exists() {
        fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    let blocks = parse_config(&original_content);
    let existing = find_block(&blocks, &params.section, &params.name);
    let desired = generate_block(params);

    let new_blocks = match existing {
        Some((idx, _)) => {
            let trimmed_existing = trim_trailing_empty(&blocks[idx].lines);
            let trimmed_desired = trim_trailing_empty(&desired.lines);
            if trimmed_existing == trimmed_desired {
                return Ok(ModuleResult {
                    changed: false,
                    output: Some(params.config_file.clone()),
                    extra: None,
                });
            }
            let mut new_blocks = blocks.clone();
            let trailing_blanks: Vec<String> = blocks[idx]
                .lines
                .iter()
                .rev()
                .take_while(|l| l.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let mut replacement = desired;
            replacement.lines.extend(trailing_blanks);
            new_blocks[idx] = replacement;
            new_blocks
        }
        None => {
            let mut new_blocks = blocks;
            if let Some(last) = new_blocks.last_mut()
                && last.lines.last().map(|l| !l.is_empty()).unwrap_or(false)
            {
                last.lines.push(String::new());
            }
            new_blocks.push(desired);
            new_blocks
        }
    };

    let new_content = blocks_to_string(&new_blocks);

    diff(&original_content, &new_content);

    if !check_mode {
        if let Some(parent) = config_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(config_path)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(params.config_file.clone()),
        extra: None,
    })
}

fn exec_absent(params: &Params, config_path: &Path, check_mode: bool) -> Result<ModuleResult> {
    if !config_path.exists() {
        return Ok(ModuleResult {
            changed: false,
            output: Some(params.config_file.clone()),
            extra: None,
        });
    }

    let original_content = fs::read_to_string(config_path)?;
    let blocks = parse_config(&original_content);
    let existing = find_block(&blocks, &params.section, &params.name);

    let (idx, _) = match existing {
        Some(found) => found,
        None => {
            return Ok(ModuleResult {
                changed: false,
                output: Some(params.config_file.clone()),
                extra: None,
            });
        }
    };

    let mut new_blocks = blocks;
    new_blocks.remove(idx);

    let new_content = blocks_to_string(&new_blocks);

    diff(&original_content, &new_content);

    if !check_mode {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(config_path)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(ModuleResult {
        changed: true,
        output: Some(params.config_file.clone()),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Haproxy;

impl Module for Haproxy {
    fn get_name(&self) -> &str {
        "haproxy"
    }

    fn exec(
        &self,
        _: &crate::context::GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            exec_haproxy(parse_params(optional_params)?, check_mode)?,
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
    use crate::error::ErrorKind;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_present() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: web_backend
            state: present
            config_file: /etc/haproxy/haproxy.cfg
            balance: roundrobin
            servers:
              - name: web1
                address: 192.168.1.10:80
              - name: web2
                address: 192.168.1.11:80
                check: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "web_backend");
        assert_eq!(params.state, State::Present);
        assert_eq!(params.config_file, "/etc/haproxy/haproxy.cfg");
        assert_eq!(params.balance, Some("roundrobin".to_string()));
        let servers = params.servers.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "web1");
        assert_eq!(servers[0].address, "192.168.1.10:80");
        assert_eq!(servers[0].check, None);
        assert_eq!(servers[1].check, Some(true));
    }

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_backend
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.config_file, "/etc/haproxy/haproxy.cfg");
        assert_eq!(params.section, Section::Backend);
        assert!(params.servers.is_none());
        assert!(params.balance.is_none());
    }

    #[test]
    fn test_parse_params_absent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: old_backend
            state: absent
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Absent);
    }

    #[test]
    fn test_parse_params_no_name() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: my_backend
            invalid: true
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_config_empty() {
        let blocks = parse_config("");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_parse_config_single_section() {
        let content =
            "backend web_backend\n    balance roundrobin\n    server web1 192.168.1.10:80\n";
        let blocks = parse_config(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].section_type, "backend");
        assert_eq!(blocks[0].name, Some("web_backend".to_string()));
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn test_parse_config_multiple_sections() {
        let content = "\
global
    log /dev/log local0

defaults
    mode http

backend web_backend
    balance roundrobin
    server web1 192.168.1.10:80

frontend http-in
    bind *:80
    default_backend web_backend
";
        let blocks = parse_config(content);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].section_type, "global");
        assert_eq!(blocks[1].section_type, "defaults");
        assert_eq!(blocks[2].section_type, "backend");
        assert_eq!(blocks[2].name, Some("web_backend".to_string()));
        assert_eq!(blocks[3].section_type, "frontend");
        assert_eq!(blocks[3].name, Some("http-in".to_string()));
    }

    #[test]
    fn test_exec_present_creates_section() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![
                Server {
                    name: "web1".to_string(),
                    address: "192.168.1.10:80".to_string(),
                    check: None,
                },
                Server {
                    name: "web2".to_string(),
                    address: "192.168.1.11:80".to_string(),
                    check: Some(true),
                },
            ]),
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("backend web_backend\n"));
        assert!(content.contains("    balance roundrobin\n"));
        assert!(content.contains("    server web1 192.168.1.10:80\n"));
        assert!(content.contains("    server web2 192.168.1.11:80 check\n"));
    }

    #[test]
    fn test_exec_present_appends_to_existing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");
        fs::write(
            &config_path,
            "global\n    log /dev/log local0\n\ndefaults\n    mode http\n",
        )
        .unwrap();

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![Server {
                name: "web1".to_string(),
                address: "192.168.1.10:80".to_string(),
                check: None,
            }]),
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("global"));
        assert!(content.contains("defaults"));
        assert!(content.contains("backend web_backend"));
        assert!(content.contains("server web1 192.168.1.10:80"));
    }

    #[test]
    fn test_exec_present_idempotent() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![Server {
                name: "web1".to_string(),
                address: "192.168.1.10:80".to_string(),
                check: None,
            }]),
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result1 = exec_haproxy(params.clone(), false).unwrap();
        assert!(result1.changed);

        let result2 = exec_haproxy(params, false).unwrap();
        assert!(!result2.changed);
    }

    #[test]
    fn test_exec_present_updates_section() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");
        fs::write(
            &config_path,
            "backend web_backend\n    balance roundrobin\n    server web1 192.168.1.10:80\n",
        )
        .unwrap();

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![
                Server {
                    name: "web1".to_string(),
                    address: "192.168.1.10:80".to_string(),
                    check: None,
                },
                Server {
                    name: "web2".to_string(),
                    address: "192.168.1.11:80".to_string(),
                    check: None,
                },
            ]),
            balance: Some("leastconn".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("balance leastconn"));
        assert!(content.contains("server web2 192.168.1.11:80"));
        assert!(!content.contains("balance roundrobin"));
    }

    #[test]
    fn test_exec_absent_removes_section() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");
        fs::write(
            &config_path,
            "global\n    log local0\n\nbackend web_backend\n    balance roundrobin\n\nfrontend http-in\n    bind *:80\n",
        )
        .unwrap();

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Absent,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: None,
            balance: None,
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(!content.contains("web_backend"));
        assert!(content.contains("global"));
        assert!(content.contains("frontend http-in"));
    }

    #[test]
    fn test_exec_absent_idempotent() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");
        fs::write(&config_path, "global\n    log local0\n").unwrap();

        let params = Params {
            name: "nonexistent".to_string(),
            state: State::Absent,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: None,
            balance: None,
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_exec_absent_file_not_found() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("nonexistent.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Absent,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: None,
            balance: None,
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_exec_present_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![Server {
                name: "web1".to_string(),
                address: "192.168.1.10:80".to_string(),
                check: None,
            }]),
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, true).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_exec_present_with_health_check() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: Some(vec![Server {
                name: "web1".to_string(),
                address: "192.168.1.10:80".to_string(),
                check: Some(true),
            }]),
            balance: Some("leastconn".to_string()),
            check: Some("option httpchk GET /health".to_string()),
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("option httpchk GET /health"));
        assert!(content.contains("server web1 192.168.1.10:80 check"));
        assert!(content.contains("balance leastconn"));
    }

    #[test]
    fn test_exec_present_frontend() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "http-in".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Frontend,
            servers: None,
            balance: None,
            check: Some("bind *:80".to_string()),
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("frontend http-in"));
        assert!(content.contains("bind *:80"));
    }

    #[test]
    fn test_exec_absent_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");
        fs::write(
            &config_path,
            "backend web_backend\n    balance roundrobin\n",
        )
        .unwrap();

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Absent,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: None,
            balance: None,
            check: None,
        };

        let result = exec_haproxy(params, true).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("backend web_backend"));
    }

    #[test]
    fn test_exec_present_listen_section() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("haproxy.cfg");

        let params = Params {
            name: "web_listener".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Listen,
            servers: Some(vec![Server {
                name: "web1".to_string(),
                address: "192.168.1.10:80".to_string(),
                check: Some(true),
            }]),
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("listen web_listener"));
        assert!(content.contains("server web1 192.168.1.10:80 check"));
    }

    #[test]
    fn test_exec_present_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("subdir").join("haproxy.cfg");

        let params = Params {
            name: "web_backend".to_string(),
            state: State::Present,
            config_file: config_path.to_string_lossy().to_string(),
            section: Section::Backend,
            servers: None,
            balance: Some("roundrobin".to_string()),
            check: None,
        };

        let result = exec_haproxy(params, false).unwrap();
        assert!(result.changed);
        assert!(config_path.exists());
    }
}
