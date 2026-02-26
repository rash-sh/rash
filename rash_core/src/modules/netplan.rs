/// ANCHOR: module
/// # netplan
///
/// Manage network configuration on Ubuntu systems using Netplan.
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
/// - name: Configure static IP on main interface
///   netplan:
///     state: present
///     renderer: networkd
///     ethernets:
///       eth0:
///         dhcp4: no
///         addresses:
///           - 192.168.1.100/24
///         routes:
///           - to: default
///             via: 192.168.1.1
///         nameservers:
///           addresses:
///             - 8.8.8.8
///             - 8.8.4.4
///
/// - name: Configure Hetzner-style networking (single IP with /32)
///   netplan:
///     state: present
///     renderer: networkd
///     ethernets:
///       eth0:
///         dhcp4: no
///         addresses:
///           - "{{ net_ip_addr }}/32"
///         routes:
///           - to: default
///             via: "{{ net_gateway }}"
///             on-link: true
///         nameservers:
///           addresses:
///             - 213.133.98.98
///             - 213.133.99.99
///
/// - name: Configure DHCP
///   netplan:
///     state: present
///     renderer: networkd
///     ethernets:
///       eth0:
///         dhcp4: true
///
/// - name: Configure bridge for VMs
///   netplan:
///     state: present
///     renderer: networkd
///     ethernets:
///       eth0:
///         dhcp4: false
///     bridges:
///       br0:
///         interfaces:
///           - eth0
///         dhcp4: true
///         parameters:
///           stp: false
///           forward-delay: 0
///
/// - name: Configure bond
///   netplan:
///     state: present
///     renderer: networkd
///     ethernets:
///       eth0:
///         dhcp4: false
///       eth1:
///         dhcp4: false
///     bonds:
///       bond0:
///         interfaces:
///           - eth0
///           - eth1
///         addresses:
///           - 192.168.1.100/24
///         parameters:
///           mode: 802.3ad
///           lacp-rate: fast
///
/// - name: Remove netplan configuration
///   netplan:
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_NETPLAN_DIR: &str = "/etc/netplan";
const DEFAULT_CONFIG_FILE: &str = "01-rash.yaml";

fn default_version() -> u32 {
    2
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Default, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Renderer {
    #[default]
    Networkd,
    NetworkManager,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Whether the configuration should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Dict of netplan configuration (alternative to individual parameters).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub config: Option<YamlValue>,
    /// Backend renderer (networkd or NetworkManager).
    /// **[default: `"networkd"`]**
    #[serde(default)]
    pub renderer: Renderer,
    /// Ethernet interface configurations.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub ethernets: Option<YamlValue>,
    /// Bridge configurations.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub bridges: Option<YamlValue>,
    /// Bond configurations.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub bonds: Option<YamlValue>,
    /// VLAN configurations.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub vlans: Option<YamlValue>,
    /// WiFi configurations.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub wifis: Option<YamlValue>,
    /// Netplan version.
    /// **[default: `2`]**
    #[serde(default = "default_version")]
    pub version: u32,
    /// Apply configuration immediately using netplan apply.
    /// **[default: `true`]**
    #[serde(default = "default_true")]
    pub apply: bool,
    /// Create backup of existing config file.
    pub backup: Option<bool>,
    /// Path to the netplan configuration directory.
    /// **[default: `"/etc/netplan"`]**
    pub directory: Option<String>,
    /// Name of the configuration file to manage.
    /// **[default: `"01-rash.yaml"`]**
    pub filename: Option<String>,
}

#[derive(Debug)]
pub struct Netplan;

impl Module for Netplan {
    fn get_name(&self) -> &str {
        "netplan"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((netplan(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn get_renderer_string(renderer: &Renderer) -> &'static str {
    match renderer {
        Renderer::Networkd => "networkd",
        Renderer::NetworkManager => "NetworkManager",
    }
}

fn build_netplan_config(params: &Params) -> YamlValue {
    let mut netplan_config = serde_norway::Mapping::new();

    netplan_config.insert(
        YamlValue::String("version".to_string()),
        YamlValue::Number(params.version.into()),
    );

    let renderer_str = get_renderer_string(&params.renderer);
    netplan_config.insert(
        YamlValue::String("renderer".to_string()),
        YamlValue::String(renderer_str.to_string()),
    );

    if let Some(ref ethernets) = params.ethernets {
        netplan_config.insert(
            YamlValue::String("ethernets".to_string()),
            ethernets.clone(),
        );
    }

    if let Some(ref bridges) = params.bridges {
        netplan_config.insert(YamlValue::String("bridges".to_string()), bridges.clone());
    }

    if let Some(ref bonds) = params.bonds {
        netplan_config.insert(YamlValue::String("bonds".to_string()), bonds.clone());
    }

    if let Some(ref vlans) = params.vlans {
        netplan_config.insert(YamlValue::String("vlans".to_string()), vlans.clone());
    }

    if let Some(ref wifis) = params.wifis {
        netplan_config.insert(YamlValue::String("wifis".to_string()), wifis.clone());
    }

    let mut top_level = serde_norway::Mapping::new();
    top_level.insert(
        YamlValue::String("network".to_string()),
        YamlValue::Mapping(netplan_config),
    );

    YamlValue::Mapping(top_level)
}

fn get_config_path(params: &Params) -> PathBuf {
    let dir = params.directory.as_deref().unwrap_or(DEFAULT_NETPLAN_DIR);
    let filename = params.filename.as_deref().unwrap_or(DEFAULT_CONFIG_FILE);
    PathBuf::from(dir).join(filename)
}

fn read_existing_config(path: &Path) -> Result<Option<YamlValue>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }

    let config: YamlValue = serde_norway::from_str(&content).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to parse existing netplan config: {e}"),
        )
    })?;
    Ok(Some(config))
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = path.with_extension(format!("yaml.bak.{timestamp}"));
    fs::copy(path, &backup_path)
        .map_err(|e| Error::new(ErrorKind::IOError, format!("Failed to create backup: {e}")))?;
    Ok(backup_path)
}

fn configs_are_equal(config1: &YamlValue, config2: &YamlValue) -> bool {
    fn normalize_yaml(value: &YamlValue) -> YamlValue {
        match value {
            YamlValue::Mapping(map) => {
                let mut normalized: serde_norway::Mapping = serde_norway::Mapping::new();
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort_by(|a, b| {
                    let a_str = serde_norway::to_string(a).unwrap_or_default();
                    let b_str = serde_norway::to_string(b).unwrap_or_default();
                    a_str.cmp(&b_str)
                });
                for key in keys {
                    if let Some(val) = map.get(key) {
                        normalized.insert(normalize_yaml(key), normalize_yaml(val));
                    }
                }
                YamlValue::Mapping(normalized)
            }
            YamlValue::Sequence(seq) => {
                let normalized: serde_norway::Sequence = seq.iter().map(normalize_yaml).collect();
                YamlValue::Sequence(normalized)
            }
            other => other.clone(),
        }
    }

    let norm1 = normalize_yaml(config1);
    let norm2 = normalize_yaml(config2);

    norm1 == norm2
}

fn apply_netplan() -> Result<bool> {
    let output = Command::new("netplan").arg("apply").output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute netplan apply: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "netplan apply failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(true)
}

fn netplan(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let config_path = get_config_path(&params);
    let mut extra: HashMap<String, YamlValue> = HashMap::new();

    match params.state {
        State::Absent => {
            let changed = if config_path.exists() {
                if !check_mode {
                    if params.backup.unwrap_or(false) {
                        create_backup(&config_path)?;
                    }
                    fs::remove_file(&config_path)?;
                }
                true
            } else {
                false
            };

            if changed {
                diff(
                    format!("config file {} exists", config_path.display()),
                    format!("config file {} removed", config_path.display()),
                );
            }

            extra.insert(
                "config_file".to_string(),
                YamlValue::String(config_path.to_string_lossy().to_string()),
            );
            extra.insert("applied".to_string(), YamlValue::Bool(false));

            Ok(ModuleResult::new(
                changed,
                Some(YamlValue::Mapping(
                    extra
                        .into_iter()
                        .map(|(k, v)| (YamlValue::String(k), v))
                        .collect(),
                )),
                Some(format!(
                    "Netplan configuration removed: {}",
                    config_path.display()
                )),
            ))
        }
        State::Present => {
            let new_config = if let Some(ref config) = params.config {
                config.clone()
            } else {
                build_netplan_config(&params)
            };

            let existing_config = read_existing_config(&config_path)?;

            let (final_config, changed) = if let Some(ref existing) = existing_config {
                if configs_are_equal(existing, &new_config) {
                    (existing.clone(), false)
                } else {
                    (new_config.clone(), true)
                }
            } else {
                (new_config.clone(), true)
            };

            let mut applied = false;

            if changed && !check_mode {
                if let Some(parent) = config_path.parent()
                    && !parent.exists()
                {
                    fs::create_dir_all(parent)?;
                }

                if config_path.exists() && params.backup.unwrap_or(false) {
                    create_backup(&config_path)?;
                }

                let yaml_content = serde_norway::to_string(&final_config).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to serialize netplan config: {e}"),
                    )
                })?;

                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&config_path)
                    .map_err(|e| {
                        Error::new(
                            ErrorKind::IOError,
                            format!("Failed to write netplan config: {e}"),
                        )
                    })?;
                file.write_all(yaml_content.as_bytes())?;

                if params.apply {
                    applied = apply_netplan()?;
                }

                let old_content = existing_config
                    .map(|c| serde_norway::to_string(&c).unwrap_or_default())
                    .unwrap_or_default();
                diff(&old_content, &yaml_content);
            } else if changed && check_mode && params.apply {
                applied = true;
            }

            extra.insert(
                "config_file".to_string(),
                YamlValue::String(config_path.to_string_lossy().to_string()),
            );
            extra.insert("config".to_string(), final_config.clone());
            extra.insert("applied".to_string(), YamlValue::Bool(applied));

            Ok(ModuleResult::new(
                changed,
                Some(YamlValue::Mapping(
                    extra
                        .into_iter()
                        .map(|(k, v)| (YamlValue::String(k), v))
                        .collect(),
                )),
                Some(format!(
                    "Netplan configuration written to {}",
                    config_path.display()
                )),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.renderer, Renderer::Networkd);
        assert_eq!(params.version, 2);
        assert!(params.apply);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            renderer: networkd
            version: 2
            apply: true
            backup: true
            directory: /etc/netplan
            filename: test.yaml
            ethernets:
              eth0:
                dhcp4: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert_eq!(params.renderer, Renderer::Networkd);
        assert_eq!(params.version, 2);
        assert!(params.apply);
        assert_eq!(params.backup, Some(true));
        assert_eq!(params.directory, Some("/etc/netplan".to_string()));
        assert_eq!(params.filename, Some("test.yaml".to_string()));
        assert!(params.ethernets.is_some());
    }

    #[test]
    fn test_parse_params_with_bridges() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            renderer: networkd
            bridges:
              br0:
                interfaces:
                  - eth0
                dhcp4: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.bridges.is_some());
    }

    #[test]
    fn test_parse_params_with_bonds() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            renderer: networkd
            bonds:
              bond0:
                interfaces:
                  - eth0
                  - eth1
                addresses:
                  - 192.168.1.100/24
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.bonds.is_some());
    }

    #[test]
    fn test_build_netplan_config() {
        let params = Params {
            state: State::Present,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: Some(serde_norway::from_str("eth0:\n  dhcp4: true").unwrap()),
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: true,
            backup: None,
            directory: None,
            filename: None,
        };

        let config = build_netplan_config(&params);

        let yaml_str = serde_norway::to_string(&config).unwrap();
        assert!(yaml_str.contains("network:"));
        assert!(yaml_str.contains("version: 2"));
        assert!(yaml_str.contains("renderer: networkd"));
        assert!(yaml_str.contains("ethernets:"));
    }

    #[test]
    fn test_configs_are_equal_identical() {
        let config1: YamlValue = serde_norway::from_str(
            r#"
            network:
              version: 2
              renderer: networkd
              ethernets:
                eth0:
                  dhcp4: true
            "#,
        )
        .unwrap();

        let config2: YamlValue = serde_norway::from_str(
            r#"
            network:
              version: 2
              renderer: networkd
              ethernets:
                eth0:
                  dhcp4: true
            "#,
        )
        .unwrap();

        assert!(configs_are_equal(&config1, &config2));
    }

    #[test]
    fn test_configs_are_equal_different() {
        let config1: YamlValue = serde_norway::from_str(
            r#"
            network:
              version: 2
              renderer: networkd
              ethernets:
                eth0:
                  dhcp4: true
            "#,
        )
        .unwrap();

        let config2: YamlValue = serde_norway::from_str(
            r#"
            network:
              version: 2
              renderer: networkd
              ethernets:
                eth0:
                  dhcp4: false
            "#,
        )
        .unwrap();

        assert!(!configs_are_equal(&config1, &config2));
    }

    #[test]
    fn test_netplan_create_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.yaml");

        let params = Params {
            state: State::Present,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: Some(serde_norway::from_str("eth0:\n  dhcp4: true").unwrap()),
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("network:"));
        assert!(content.contains("dhcp4: true"));
    }

    #[test]
    fn test_netplan_no_change() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.yaml");

        let initial_content = r#"network:
  version: 2
  renderer: networkd
  ethernets:
    eth0:
      dhcp4: true
"#;
        fs::write(&config_path, initial_content).unwrap();

        let params = Params {
            state: State::Present,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: Some(serde_norway::from_str("eth0:\n  dhcp4: true").unwrap()),
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_netplan_remove_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.yaml");

        fs::write(&config_path, "network:\n  version: 2").unwrap();
        assert!(config_path.exists());

        let params = Params {
            state: State::Absent,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: None,
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, false).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_netplan_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.yaml");

        let params = Params {
            state: State::Present,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: Some(serde_norway::from_str("eth0:\n  dhcp4: true").unwrap()),
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, true).unwrap();
        assert!(result.changed);
        assert!(!config_path.exists());
    }

    #[test]
    fn test_netplan_with_config_param() {
        let dir = tempdir().unwrap();

        let config_yaml: YamlValue = serde_norway::from_str(
            r#"
            network:
              version: 2
              renderer: networkd
              ethernets:
                eth0:
                  dhcp4: true
            "#,
        )
        .unwrap();

        let params = Params {
            state: State::Present,
            config: Some(config_yaml),
            renderer: Renderer::Networkd,
            ethernets: None,
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, false).unwrap();
        assert!(result.changed);
    }

    #[test]
    fn test_parse_params_invalid_field() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            invalid_field: value
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_get_renderer_string() {
        assert_eq!(get_renderer_string(&Renderer::Networkd), "networkd");
        assert_eq!(
            get_renderer_string(&Renderer::NetworkManager),
            "NetworkManager"
        );
    }

    #[test]
    fn test_netplan_with_backup() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test.yaml");

        fs::write(&config_path, "network:\n  version: 2").unwrap();

        let params = Params {
            state: State::Present,
            config: None,
            renderer: Renderer::Networkd,
            ethernets: Some(serde_norway::from_str("eth0:\n  dhcp4: true").unwrap()),
            bridges: None,
            bonds: None,
            vlans: None,
            wifis: None,
            version: 2,
            apply: false,
            backup: Some(true),
            directory: Some(dir.path().to_string_lossy().to_string()),
            filename: Some("test.yaml".to_string()),
        };

        let result = netplan(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("test.yaml.bak"))
            .collect();
        assert_eq!(backup_files.len(), 1);
    }
}
