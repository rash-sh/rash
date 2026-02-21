/// ANCHOR: module
/// # interfaces_file
///
/// Manage network interface configuration in /etc/network/interfaces.
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
/// - name: Configure static IP on eth0
///   interfaces_file:
///     iface: eth0
///     address: 192.168.1.100
///     netmask: 255.255.255.0
///     gateway: 192.168.1.1
///     dns_nameservers:
///       - 8.8.8.8
///       - 8.8.4.4
///
/// - name: Configure DHCP interface
///   interfaces_file:
///     iface: eth1
///     method: dhcp
///
/// - name: Remove interface configuration
///   interfaces_file:
///     iface: eth2
///     state: absent
///
/// - name: Configure interface without auto
///   interfaces_file:
///     iface: eth3
///     method: static
///     address: 10.0.0.100
///     netmask: 255.255.255.0
///     auto: false
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{OpenOptions, read_to_string};
use std::io::prelude::*;
use std::path::Path;

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
    /// The interface name (e.g., eth0, enp0s3, wlan0).
    pub iface: String,
    /// The address family (inet for IPv4, inet6 for IPv6).
    /// **[default: `"inet"`]**
    pub family: Option<Family>,
    /// The configuration method (static, dhcp, manual, etc.).
    /// **[default: `"static"`]**
    pub method: Option<Method>,
    /// The IP address for static configuration. Required if method=static.
    pub address: Option<String>,
    /// The netmask for static configuration. Required if method=static.
    pub netmask: Option<String>,
    /// The default gateway for static configuration.
    pub gateway: Option<String>,
    /// List of DNS nameservers.
    pub dns_nameservers: Option<Vec<String>>,
    /// List of DNS search domains.
    pub dns_search: Option<Vec<String>>,
    /// Whether the interface should be started at boot.
    /// **[default: `true`]**
    pub auto: Option<bool>,
    /// Whether the interface configuration should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Path to the interfaces file.
    /// **[default: `"/etc/network/interfaces"`]**
    pub path: Option<String>,
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
pub enum Family {
    #[default]
    Inet,
    Inet6,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Method {
    #[default]
    Static,
    Dhcp,
    Manual,
    Loopback,
}

#[derive(Debug, Clone)]
struct IfaceBlock {
    iface: String,
    family: String,
    method: String,
    start_line: usize,
    end_line: usize,
    options: Vec<(String, String)>,
    has_auto: bool,
    auto_line: Option<usize>,
}

fn parse_interfaces_content(content: &str) -> (Vec<IfaceBlock>, Vec<String>) {
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut blocks: Vec<IfaceBlock> = Vec::new();
    let mut auto_lines: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("auto ") || trimmed.starts_with("allow-auto ") {
            let interfaces: Vec<&str> = trimmed.split_whitespace().skip(1).collect();
            for iface in interfaces {
                auto_lines.insert(iface.to_string(), idx);
            }
        }
    }

    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].trim();

        if line.starts_with("iface ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let iface_name = parts[1].to_string();
                let family = parts[2].to_string();
                let method = parts[3].to_string();

                let start_line = idx;
                let mut end_line = idx;
                let mut options: Vec<(String, String)> = Vec::new();

                let mut opt_idx = idx + 1;
                while opt_idx < lines.len() {
                    let opt_line = lines[opt_idx].trim();
                    if opt_line.is_empty()
                        || opt_line.starts_with("iface ")
                        || opt_line.starts_with("auto ")
                        || opt_line.starts_with("allow-auto ")
                        || opt_line.starts_with("mapping ")
                    {
                        break;
                    }
                    if opt_line.starts_with('#') {
                        opt_idx += 1;
                        continue;
                    }
                    if let Some(space_pos) = opt_line.find(' ') {
                        let opt_name = opt_line[..space_pos].to_string();
                        let opt_value = opt_line[space_pos..].trim().to_string();
                        options.push((opt_name, opt_value));
                    }
                    end_line = opt_idx;
                    opt_idx += 1;
                }

                let has_auto = auto_lines.contains_key(&iface_name);
                let auto_line = auto_lines.get(&iface_name).copied();

                blocks.push(IfaceBlock {
                    iface: iface_name,
                    family,
                    method,
                    start_line,
                    end_line,
                    options,
                    has_auto,
                    auto_line,
                });

                idx = opt_idx;
                continue;
            }
        }
        idx += 1;
    }

    (blocks, lines)
}

fn find_iface_block<'a>(
    blocks: &'a [IfaceBlock],
    iface: &str,
    family: &str,
) -> Option<&'a IfaceBlock> {
    blocks
        .iter()
        .find(|b| b.iface == iface && b.family == family)
}

fn format_iface_block(
    iface: &str,
    family: &str,
    method: &str,
    options: &[(String, String)],
    include_auto: bool,
) -> Vec<String> {
    let mut result = Vec::new();

    if include_auto {
        result.push(format!("auto {iface}"));
    }

    result.push(format!("iface {iface} {family} {method}"));

    for (opt_name, opt_value) in options {
        result.push(format!("    {opt_name} {opt_value}"));
    }

    result
}

fn get_family_string(family: &Option<Family>) -> String {
    match family {
        Some(Family::Inet) | None => "inet".to_string(),
        Some(Family::Inet6) => "inet6".to_string(),
    }
}

fn get_method_string(method: &Option<Method>) -> String {
    match method {
        Some(Method::Static) | None => "static".to_string(),
        Some(Method::Dhcp) => "dhcp".to_string(),
        Some(Method::Manual) => "manual".to_string(),
        Some(Method::Loopback) => "loopback".to_string(),
    }
}

pub fn interfaces_file(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let family = get_family_string(&params.family);
    let method = get_method_string(&params.method);
    let include_auto = params.auto.unwrap_or(true);
    let path_str = params
        .path
        .clone()
        .unwrap_or_else(|| "/etc/network/interfaces".to_string());
    let path = Path::new(&path_str);

    if state == State::Present && method == "static" {
        if params.address.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "address is required when method=static",
            ));
        }
        if params.netmask.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "netmask is required when method=static",
            ));
        }
    }

    let (blocks, mut lines) = if path.exists() {
        let content = read_to_string(path)?;
        parse_interfaces_content(&content)
    } else {
        (Vec::new(), Vec::new())
    };

    let original_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let mut changed = false;

    match state {
        State::Present => {
            let mut new_options: Vec<(String, String)> = Vec::new();

            if let Some(addr) = &params.address {
                new_options.push(("address".to_string(), addr.clone()));
            }
            if let Some(mask) = &params.netmask {
                new_options.push(("netmask".to_string(), mask.clone()));
            }
            if let Some(gw) = &params.gateway {
                new_options.push(("gateway".to_string(), gw.clone()));
            }
            if let Some(nameservers) = &params.dns_nameservers {
                new_options.push(("dns-nameservers".to_string(), nameservers.join(" ")));
            }
            if let Some(search) = &params.dns_search {
                new_options.push(("dns-search".to_string(), search.join(" ")));
            }

            if let Some(existing) = find_iface_block(&blocks, &params.iface, &family) {
                let mut needs_update = false;

                if existing.method != method {
                    needs_update = true;
                }

                if existing.has_auto != include_auto {
                    needs_update = true;
                }

                for (opt_name, opt_value) in &new_options {
                    let found = existing
                        .options
                        .iter()
                        .any(|(n, v)| n == opt_name && v == opt_value);
                    if !found {
                        needs_update = true;
                        break;
                    }
                }

                for (opt_name, _) in &existing.options {
                    let is_managed = [
                        "address",
                        "netmask",
                        "gateway",
                        "dns-nameservers",
                        "dns-search",
                    ]
                    .contains(&opt_name.as_str());
                    if is_managed {
                        let still_present = new_options.iter().any(|(n, _)| n == opt_name);
                        if !still_present {
                            needs_update = true;
                            break;
                        }
                    }
                }

                if needs_update {
                    let new_block_lines = format_iface_block(
                        &params.iface,
                        &family,
                        &method,
                        &new_options,
                        include_auto,
                    );

                    let mut block_start = existing.start_line;

                    if existing.has_auto
                        && !include_auto
                        && let Some(auto_line) = existing.auto_line
                    {
                        let auto_line_content = lines[auto_line].trim();
                        let interfaces_in_auto: Vec<&str> =
                            auto_line_content.split_whitespace().skip(1).collect();

                        if interfaces_in_auto.len() == 1 {
                            lines.remove(auto_line);
                            if auto_line < block_start {
                                block_start -= 1;
                            }
                        } else {
                            let new_auto: String = interfaces_in_auto
                                .iter()
                                .filter(|&&i| i != params.iface)
                                .copied()
                                .collect::<Vec<&str>>()
                                .join(" ");
                            lines[auto_line] = format!("auto {new_auto}");
                        }
                    }

                    let block_length = existing.end_line - existing.start_line + 1;
                    for _ in 0..block_length {
                        lines.remove(block_start);
                    }

                    for (i, block_line) in new_block_lines.iter().enumerate() {
                        lines.insert(block_start + i, block_line.clone());
                    }

                    changed = true;
                }
            } else {
                let new_block_lines =
                    format_iface_block(&params.iface, &family, &method, &new_options, include_auto);

                if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                    lines.push(String::new());
                }

                for block_line in new_block_lines {
                    lines.push(block_line);
                }

                changed = true;
            }
        }
        State::Absent => {
            if let Some(existing) = find_iface_block(&blocks, &params.iface, &family) {
                let mut block_start = existing.start_line;

                if existing.has_auto
                    && let Some(auto_line) = existing.auto_line
                {
                    let auto_line_content = lines[auto_line].trim();
                    let interfaces_in_auto: Vec<&str> =
                        auto_line_content.split_whitespace().skip(1).collect();

                    if interfaces_in_auto.len() == 1 {
                        lines.remove(auto_line);
                        if auto_line < block_start {
                            block_start -= 1;
                        }
                    } else {
                        let new_auto: String = interfaces_in_auto
                            .iter()
                            .filter(|&&i| i != params.iface)
                            .copied()
                            .collect::<Vec<&str>>()
                            .join(" ");
                        lines[auto_line] = format!("auto {new_auto}");
                    }
                }

                let block_length = existing.end_line - existing.start_line + 1;
                for _ in 0..block_length {
                    lines.remove(block_start);
                }

                changed = true;
            }
        }
    }

    if changed {
        let new_content = if lines.is_empty() {
            String::new()
        } else {
            let trimmed: Vec<String> = lines.into_iter().collect();

            let mut result = String::new();
            let mut prev_empty = false;
            for line in trimmed {
                if line.is_empty() {
                    if !prev_empty {
                        result.push_str(&line);
                        result.push('\n');
                        prev_empty = true;
                    }
                } else {
                    result.push_str(&line);
                    result.push('\n');
                    prev_empty = false;
                }
            }
            result
        };

        diff(&original_content, &new_content);

        if !check_mode {
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                std::fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(path_str),
        extra: None,
    })
}

#[derive(Debug)]
pub struct InterfacesFile;

impl Module for InterfacesFile {
    fn get_name(&self) -> &str {
        "interfaces_file"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            interfaces_file(parse_params(optional_params)?, check_mode)?,
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
            iface: eth0
            address: 192.168.1.100
            netmask: 255.255.255.0
            gateway: 192.168.1.1
            dns_nameservers:
              - 8.8.8.8
              - 8.8.4.4
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.iface, "eth0");
        assert_eq!(params.address, Some("192.168.1.100".to_string()));
        assert_eq!(params.netmask, Some("255.255.255.0".to_string()));
        assert_eq!(params.gateway, Some("192.168.1.1".to_string()));
        assert_eq!(
            params.dns_nameservers,
            Some(vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()])
        );
    }

    #[test]
    fn test_interfaces_file_add_static() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: Some("255.255.255.0".to_string()),
            gateway: Some("192.168.1.1".to_string()),
            dns_nameservers: Some(vec!["8.8.8.8".to_string()]),
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("auto eth0"));
        assert!(content.contains("iface eth0 inet static"));
        assert!(content.contains("address 192.168.1.100"));
        assert!(content.contains("netmask 255.255.255.0"));
        assert!(content.contains("gateway 192.168.1.1"));
        assert!(content.contains("dns-nameservers 8.8.8.8"));
    }

    #[test]
    fn test_interfaces_file_add_dhcp() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let params = Params {
            iface: "eth1".to_string(),
            family: None,
            method: Some(Method::Dhcp),
            address: None,
            netmask: None,
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("auto eth1"));
        assert!(content.contains("iface eth1 inet dhcp"));
    }

    #[test]
    fn test_interfaces_file_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        fs::write(
            &file_path,
            "auto eth0\niface eth0 inet static\n    address 192.168.1.100\n    netmask 255.255.255.0\n",
        )
        .unwrap();

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: Some("255.255.255.0".to_string()),
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_interfaces_file_modify() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        fs::write(
            &file_path,
            "auto eth0\niface eth0 inet static\n    address 192.168.1.50\n    netmask 255.255.255.0\n",
        )
        .unwrap();

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: Some("255.255.255.0".to_string()),
            gateway: Some("192.168.1.1".to_string()),
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("address 192.168.1.100"));
        assert!(content.contains("gateway 192.168.1.1"));
        assert!(!content.contains("192.168.1.50"));
    }

    #[test]
    fn test_interfaces_file_remove() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        fs::write(
            &file_path,
            "auto eth0\niface eth0 inet static\n    address 192.168.1.100\n    netmask 255.255.255.0\n",
        )
        .unwrap();

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: None,
            netmask: None,
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: Some(State::Absent),
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("eth0"));
    }

    #[test]
    fn test_interfaces_file_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let original_content = "";
        fs::write(&file_path, original_content).unwrap();

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: Some("255.255.255.0".to_string()),
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_interfaces_file_missing_address_for_static() {
        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: None,
            netmask: Some("255.255.255.0".to_string()),
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some("/tmp/interfaces".to_string()),
        };

        let result = interfaces_file(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("address is required")
        );
    }

    #[test]
    fn test_interfaces_file_missing_netmask_for_static() {
        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: None,
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some("/tmp/interfaces".to_string()),
        };

        let result = interfaces_file(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("netmask is required")
        );
    }

    #[test]
    fn test_interfaces_file_no_auto() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: None,
            address: Some("192.168.1.100".to_string()),
            netmask: Some("255.255.255.0".to_string()),
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: Some(false),
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("auto eth0"));
        assert!(content.contains("iface eth0 inet static"));
    }

    #[test]
    fn test_interfaces_file_ipv6() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let params = Params {
            iface: "eth0".to_string(),
            family: Some(Family::Inet6),
            method: Some(Method::Static),
            address: Some("2001:db8::1".to_string()),
            netmask: Some("64".to_string()),
            gateway: Some("2001:db8::ffff".to_string()),
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("iface eth0 inet6 static"));
        assert!(content.contains("address 2001:db8::1"));
    }

    #[test]
    fn test_parse_interfaces_content() {
        let content = "auto lo\niface lo inet loopback\n\nauto eth0\niface eth0 inet static\n    address 192.168.1.100\n    netmask 255.255.255.0\n";
        let (blocks, lines) = parse_interfaces_content(content);

        assert_eq!(lines.len(), 7);
        assert_eq!(blocks.len(), 2);

        assert_eq!(blocks[0].iface, "lo");
        assert_eq!(blocks[0].method, "loopback");
        assert!(blocks[0].has_auto);

        assert_eq!(blocks[1].iface, "eth0");
        assert_eq!(blocks[1].method, "static");
        assert_eq!(blocks[1].options.len(), 2);
        assert!(blocks[1].has_auto);
    }

    #[test]
    fn test_format_iface_block() {
        let options = vec![
            ("address".to_string(), "192.168.1.100".to_string()),
            ("netmask".to_string(), "255.255.255.0".to_string()),
        ];

        let lines = format_iface_block("eth0", "inet", "static", &options, true);

        assert_eq!(lines[0], "auto eth0");
        assert_eq!(lines[1], "iface eth0 inet static");
        assert_eq!(lines[2], "    address 192.168.1.100");
        assert_eq!(lines[3], "    netmask 255.255.255.0");
    }

    #[test]
    fn test_interfaces_file_manual_method() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("interfaces");

        let params = Params {
            iface: "eth0".to_string(),
            family: None,
            method: Some(Method::Manual),
            address: None,
            netmask: None,
            gateway: None,
            dns_nameservers: None,
            dns_search: None,
            auto: None,
            state: None,
            path: Some(file_path.to_str().unwrap().to_string()),
        };

        let result = interfaces_file(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("iface eth0 inet manual"));
    }
}
