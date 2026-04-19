/// ANCHOR: module
/// # cloud_init
///
/// Manage cloud-init configuration for cloud instance initialization.
///
/// Cloud-init is the industry-standard multi-distro method for cross-platform
/// cloud instance initialization. This module manages cloud-init configuration
/// files, user-data, meta-data, and network configuration.
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
/// - name: Configure cloud-init user-data
///   cloud_init:
///     state: present
///     user_data:
///       users:
///         - name: admin
///           sudo: ALL=(ALL) NOPASSWD:ALL
///           shell: /bin/bash
///       packages:
///         - nginx
///         - curl
///       runcmd:
///         - systemctl enable nginx
///         - systemctl start nginx
///
/// - name: Configure cloud-init with network config
///   cloud_init:
///     state: present
///     network_config:
///       version: 2
///       ethernets:
///         eth0:
///           dhcp4: true
///
/// - name: Set instance metadata
///   cloud_init:
///     state: present
///     meta_data:
///       instance-id: i-12345678
///       local-hostname: web01
///
/// - name: Remove cloud-init configuration
///   cloud_init:
///     state: absent
///
/// - name: Write user-data from raw content
///   cloud_init:
///     state: present
///     user_data_content: |
///       #cloud-config
///       users:
///         - name: deploy
///           groups: sudo
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

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_CLOUD_CFG_DIR: &str = "/etc/cloud";
const DEFAULT_CLOUD_CFG_D: &str = "cloud.cfg.d";
const DEFAULT_USER_DATA_DIR: &str = "/var/lib/cloud/instance";
const RASH_CONFIG_NAME: &str = "99-rash.cfg";

#[derive(Debug, Default, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
    Updated,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Whether the configuration should exist or not.
    /// `updated` will only apply changes if the configuration differs.
    /// **[default: `"present"`]**
    #[serde(default)]
    pub state: State,
    /// Cloud-init user-data configuration (YAML map).
    /// Written as cloud-config YAML format.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub user_data: Option<YamlValue>,
    /// Raw user-data content string. Used as-is if provided
    /// (should start with `#cloud-config`).
    pub user_data_content: Option<String>,
    /// Instance metadata (YAML map).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub meta_data: Option<YamlValue>,
    /// Network configuration (YAML map).
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub network_config: Option<YamlValue>,
    /// Create backup of existing config files before modifying.
    /// **[default: `false`]**
    pub backup: Option<bool>,
    /// Path to the cloud-init configuration directory.
    /// **[default: `"/etc/cloud"`]**
    pub directory: Option<String>,
    /// Path to write user-data file.
    /// **[default: `"/var/lib/cloud/instance/user-data"`]**
    pub user_data_path: Option<String>,
    /// Path to write meta-data file.
    /// **[default: `"/var/lib/cloud/instance/meta-data"`]**
    pub meta_data_path: Option<String>,
    /// Path to write network config file.
    /// **[default: `"/var/lib/cloud/instance/network-config"`]**
    pub network_config_path: Option<String>,
}

#[derive(Debug)]
pub struct CloudInit;

impl Module for CloudInit {
    fn get_name(&self) -> &str {
        "cloud_init"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            cloud_init(parse_params(optional_params)?, check_mode)?,
            None,
        ))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn get_config_path(params: &Params) -> PathBuf {
    let dir = params.directory.as_deref().unwrap_or(DEFAULT_CLOUD_CFG_DIR);
    PathBuf::from(dir)
        .join(DEFAULT_CLOUD_CFG_D)
        .join(RASH_CONFIG_NAME)
}

fn create_backup(path: &Path) -> Result<PathBuf> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = path.with_extension(format!("bak.{}", timestamp));
    fs::copy(path, &backup_path)
        .map_err(|e| Error::new(ErrorKind::IOError, format!("Failed to create backup: {e}")))?;
    Ok(backup_path)
}

fn read_existing(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(content))
}

fn configs_are_equal(existing: &str, new_content: &str) -> bool {
    let existing_yaml: Result<YamlValue> =
        serde_norway::from_str(existing).map_err(|e| Error::new(ErrorKind::InvalidData, e));
    let new_yaml: Result<YamlValue> =
        serde_norway::from_str(new_content).map_err(|e| Error::new(ErrorKind::InvalidData, e));

    match (existing_yaml, new_yaml) {
        (Ok(e), Ok(n)) => normalize_yaml(&e) == normalize_yaml(&n),
        _ => existing.trim() == new_content.trim(),
    }
}

fn normalize_yaml(value: &YamlValue) -> YamlValue {
    match value {
        YamlValue::Mapping(map) => {
            let mut normalized = serde_norway::Mapping::new();
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

fn build_cloud_cfg_content(params: &Params) -> Option<String> {
    let mut cloud_cfg = serde_norway::Mapping::new();

    if let Some(YamlValue::Mapping(map)) = &params.user_data {
        for (k, v) in map {
            cloud_cfg.insert(k.clone(), v.clone());
        }
    }

    if cloud_cfg.is_empty() {
        return None;
    }

    let mut content = String::from("#cloud-config\n");
    let yaml_str = serde_norway::to_string(&YamlValue::Mapping(cloud_cfg)).unwrap_or_default();
    content.push_str(&yaml_str);
    Some(content)
}

fn write_config_file(path: &Path, content: &str, backup: bool, check_mode: bool) -> Result<bool> {
    let existing = read_existing(path)?;

    if let Some(ref existing_content) = existing
        && configs_are_equal(existing_content, content)
    {
        return Ok(false);
    }

    diff(existing.unwrap_or_default(), content.to_string());

    if check_mode {
        return Ok(true);
    }

    if backup && path.exists() {
        create_backup(path)?;
    }

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| {
            Error::new(
                ErrorKind::IOError,
                format!("Failed to write config {}: {e}", path.display()),
            )
        })?;
    file.write_all(content.as_bytes())?;

    Ok(true)
}

fn remove_config_file(path: &Path, backup: bool, check_mode: bool) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    diff(
        format!("config file {} exists", path.display()),
        format!("config file {} removed", path.display()),
    );

    if check_mode {
        return Ok(true);
    }

    if backup {
        create_backup(path)?;
    }
    fs::remove_file(path)?;

    Ok(true)
}

fn validate_user_data_content(content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "user_data_content cannot be empty",
        ));
    }

    if content.trim().starts_with('#')
        && !content.trim().starts_with("#!")
        && !content.trim().starts_with("#cloud-config")
        && !content.trim().starts_with("#include")
        && !content.trim().starts_with("#cloud-boothook")
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "user_data_content header not recognized. Expected #cloud-config, #!, #include, or #cloud-boothook",
        ));
    }

    Ok(())
}

fn cloud_init(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let mut changed = false;
    let mut extra: HashMap<String, YamlValue> = HashMap::new();
    let backup = params.backup.unwrap_or(false);

    match params.state {
        State::Absent => {
            let config_path = get_config_path(&params);
            let dir = params.directory.as_deref().unwrap_or(DEFAULT_CLOUD_CFG_DIR);
            let cfg_d = PathBuf::from(dir).join(DEFAULT_CLOUD_CFG_D);

            if cfg_d.exists() {
                for entry in fs::read_dir(&cfg_d)? {
                    let entry = entry?;
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with("99-rash") {
                        let path = entry.path();
                        if remove_config_file(&path, backup, check_mode)? {
                            changed = true;
                        }
                    }
                }
            }

            let user_data_path = params
                .user_data_path
                .as_deref()
                .unwrap_or("/var/lib/cloud/instance/user-data");
            let user_data_path = Path::new(user_data_path);
            if user_data_path.exists() && remove_config_file(user_data_path, backup, check_mode)? {
                changed = true;
            }

            let meta_data_path = params
                .meta_data_path
                .as_deref()
                .unwrap_or("/var/lib/cloud/instance/meta-data");
            let meta_data_path = Path::new(meta_data_path);
            if meta_data_path.exists() && remove_config_file(meta_data_path, backup, check_mode)? {
                changed = true;
            }

            let network_config_path = params
                .network_config_path
                .as_deref()
                .unwrap_or("/var/lib/cloud/instance/network-config");
            let network_config_path = Path::new(network_config_path);
            if network_config_path.exists()
                && remove_config_file(network_config_path, backup, check_mode)?
            {
                changed = true;
            }

            extra.insert(
                "config_file".to_string(),
                YamlValue::String(config_path.to_string_lossy().to_string()),
            );
        }
        State::Present | State::Updated => {
            if let Some(ref user_data_content) = params.user_data_content {
                validate_user_data_content(user_data_content)?;
            }

            if let Some(ref cloud_cfg) = build_cloud_cfg_content(&params) {
                let config_path = get_config_path(&params);
                if write_config_file(&config_path, cloud_cfg, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "config_file".to_string(),
                    YamlValue::String(config_path.to_string_lossy().to_string()),
                );
            } else if let Some(ref user_data_content) = params.user_data_content {
                let dir = params.directory.as_deref().unwrap_or(DEFAULT_CLOUD_CFG_DIR);
                let config_path = PathBuf::from(dir)
                    .join(DEFAULT_CLOUD_CFG_D)
                    .join(RASH_CONFIG_NAME);
                if write_config_file(&config_path, user_data_content, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "config_file".to_string(),
                    YamlValue::String(config_path.to_string_lossy().to_string()),
                );
            }

            if let Some(ref user_data) = params.user_data {
                let user_data_path = params
                    .user_data_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_USER_DATA_DIR).join("user-data"));

                let yaml_content = if user_data_path.extension().is_some_and(|e| e == "json") {
                    serde_json::to_string_pretty(&user_data)
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
                } else {
                    let mut content = String::from("#cloud-config\n");
                    content.push_str(&serde_norway::to_string(user_data).unwrap_or_default());
                    content
                };

                if write_config_file(&user_data_path, &yaml_content, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "user_data_file".to_string(),
                    YamlValue::String(user_data_path.to_string_lossy().to_string()),
                );
            } else if let Some(ref user_data_content) = params.user_data_content {
                let user_data_path = params
                    .user_data_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_USER_DATA_DIR).join("user-data"));
                if write_config_file(&user_data_path, user_data_content, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "user_data_file".to_string(),
                    YamlValue::String(user_data_path.to_string_lossy().to_string()),
                );
            }

            if let Some(ref meta_data) = params.meta_data {
                let meta_data_path = params
                    .meta_data_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_USER_DATA_DIR).join("meta-data"));

                let yaml_content = serde_norway::to_string(meta_data).unwrap_or_default();

                if write_config_file(&meta_data_path, &yaml_content, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "meta_data_file".to_string(),
                    YamlValue::String(meta_data_path.to_string_lossy().to_string()),
                );
            }

            if let Some(ref network_config) = params.network_config {
                let network_config_path = params
                    .network_config_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_USER_DATA_DIR).join("network-config"));

                let yaml_content = serde_norway::to_string(network_config).unwrap_or_default();

                if write_config_file(&network_config_path, &yaml_content, backup, check_mode)? {
                    changed = true;
                }
                extra.insert(
                    "network_config_file".to_string(),
                    YamlValue::String(network_config_path.to_string_lossy().to_string()),
                );
            }
        }
    }

    let msg = match params.state {
        State::Absent => "Cloud-init configuration removed".to_string(),
        State::Present | State::Updated => "Cloud-init configuration applied".to_string(),
    };

    Ok(ModuleResult::new(
        changed,
        Some(YamlValue::Mapping(
            extra
                .into_iter()
                .map(|(k, v)| (YamlValue::String(k), v))
                .collect(),
        )),
        Some(msg),
    ))
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
        assert!(params.user_data.is_none());
        assert!(params.meta_data.is_none());
        assert!(params.network_config.is_none());
    }

    #[test]
    fn test_parse_params_with_user_data() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            user_data:
              users:
                - name: admin
                  shell: /bin/bash
              packages:
                - nginx
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.state, State::Present);
        assert!(params.user_data.is_some());
    }

    #[test]
    fn test_parse_params_with_meta_data() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            meta_data:
              instance-id: i-12345678
              local-hostname: web01
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.meta_data.is_some());
    }

    #[test]
    fn test_parse_params_with_network_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            network_config:
              version: 2
              ethernets:
                eth0:
                  dhcp4: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.network_config.is_some());
    }

    #[test]
    fn test_parse_params_with_raw_content() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            user_data_content: |
              #cloud-config
              users:
                - name: deploy
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.user_data_content.is_some());
        assert!(
            params
                .user_data_content
                .as_ref()
                .unwrap()
                .contains("#cloud-config")
        );
    }

    #[test]
    fn test_parse_params_all_paths() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            state: present
            user_data:
              packages:
                - nginx
            directory: /etc/cloud
            user_data_path: /tmp/user-data
            meta_data_path: /tmp/meta-data
            network_config_path: /tmp/network-config
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.directory, Some("/etc/cloud".to_string()));
        assert_eq!(params.user_data_path, Some("/tmp/user-data".to_string()));
        assert_eq!(params.meta_data_path, Some("/tmp/meta-data".to_string()));
        assert_eq!(
            params.network_config_path,
            Some("/tmp/network-config".to_string())
        );
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
    fn test_build_cloud_cfg_content_with_user_data() {
        let params = Params {
            state: State::Present,
            user_data: Some(serde_norway::from_str("users:\n  - name: admin").unwrap()),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: None,
            user_data_path: None,
            meta_data_path: None,
            network_config_path: None,
        };

        let content = build_cloud_cfg_content(&params);
        assert!(content.is_some());
        let content = content.unwrap();
        assert!(content.starts_with("#cloud-config\n"));
        assert!(content.contains("users:"));
    }

    #[test]
    fn test_build_cloud_cfg_content_empty() {
        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: None,
            user_data_path: None,
            meta_data_path: None,
            network_config_path: None,
        };

        let content = build_cloud_cfg_content(&params);
        assert!(content.is_none());
    }

    #[test]
    fn test_configs_are_equal_same() {
        let c1 = "users:\n  - name: admin\n";
        let c2 = "users:\n  - name: admin\n";
        assert!(configs_are_equal(c1, c2));
    }

    #[test]
    fn test_configs_are_equal_different() {
        let c1 = "users:\n  - name: admin\n";
        let c2 = "users:\n  - name: deploy\n";
        assert!(!configs_are_equal(c1, c2));
    }

    #[test]
    fn test_cloud_init_present_user_data() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_string_lossy().to_string();
        let user_data_file = dir.path().join("user-data");

        let params = Params {
            state: State::Present,
            user_data: Some(
                serde_norway::from_str("users:\n  - name: admin\n    shell: /bin/bash").unwrap(),
            ),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir_path.clone()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&user_data_file).unwrap();
        assert!(content.contains("#cloud-config"));
        assert!(content.contains("admin"));
    }

    #[test]
    fn test_cloud_init_no_change() {
        let dir = tempdir().unwrap();
        let user_data_file = dir.path().join("user-data");

        let first_params = Params {
            state: State::Present,
            user_data: Some(
                serde_norway::from_str("users:\n  - name: admin\n    shell: /bin/bash").unwrap(),
            ),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let first_result = cloud_init(first_params, false).unwrap();
        assert!(first_result.changed);

        let second_params = Params {
            state: State::Present,
            user_data: Some(
                serde_norway::from_str("users:\n  - name: admin\n    shell: /bin/bash").unwrap(),
            ),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let second_result = cloud_init(second_params, false).unwrap();
        assert!(!second_result.changed);
    }

    #[test]
    fn test_cloud_init_check_mode() {
        let dir = tempdir().unwrap();
        let user_data_file = dir.path().join("user-data");

        let params = Params {
            state: State::Present,
            user_data: Some(serde_norway::from_str("users:\n  - name: admin").unwrap()),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let result = cloud_init(params, true).unwrap();
        assert!(result.changed);
        assert!(!user_data_file.exists());
    }

    #[test]
    fn test_cloud_init_absent() {
        let dir = tempdir().unwrap();
        let user_data_file = dir.path().join("user-data");
        let meta_data_file = dir.path().join("meta-data");

        fs::write(&user_data_file, "#cloud-config\n").unwrap();
        fs::write(&meta_data_file, "instance-id: i-123\n").unwrap();

        let params = Params {
            state: State::Absent,
            user_data: None,
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: Some(meta_data_file.to_string_lossy().to_string()),
            network_config_path: None,
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);
        assert!(!user_data_file.exists());
        assert!(!meta_data_file.exists());
    }

    #[test]
    fn test_cloud_init_with_network_config() {
        let dir = tempdir().unwrap();
        let network_file = dir.path().join("network-config");

        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: None,
            meta_data: None,
            network_config: Some(
                serde_norway::from_str("version: 2\nethernets:\n  eth0:\n    dhcp4: true").unwrap(),
            ),
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: None,
            meta_data_path: None,
            network_config_path: Some(network_file.to_string_lossy().to_string()),
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&network_file).unwrap();
        assert!(content.contains("ethernets:"));
        assert!(content.contains("dhcp4: true"));
    }

    #[test]
    fn test_cloud_init_with_meta_data() {
        let dir = tempdir().unwrap();
        let meta_file = dir.path().join("meta-data");

        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: None,
            meta_data: Some(
                serde_norway::from_str("instance-id: i-12345678\nlocal-hostname: web01").unwrap(),
            ),
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: None,
            meta_data_path: Some(meta_file.to_string_lossy().to_string()),
            network_config_path: None,
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&meta_file).unwrap();
        assert!(content.contains("instance-id: i-12345678"));
        assert!(content.contains("local-hostname: web01"));
    }

    #[test]
    fn test_cloud_init_with_raw_content() {
        let dir = tempdir().unwrap();
        let user_data_file = dir.path().join("user-data");

        let raw_content = "#cloud-config\nusers:\n  - name: deploy\n    groups: sudo\n";

        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: Some(raw_content.to_string()),
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&user_data_file).unwrap();
        assert_eq!(content, raw_content);
    }

    #[test]
    fn test_cloud_init_with_backup() {
        let dir = tempdir().unwrap();
        let user_data_file = dir.path().join("user-data");

        fs::write(&user_data_file, "#cloud-config\nusers:\n  - name: old\n").unwrap();

        let params = Params {
            state: State::Present,
            user_data: Some(serde_norway::from_str("users:\n  - name: new").unwrap()),
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: Some(true),
            directory: Some(dir.path().to_string_lossy().to_string()),
            user_data_path: Some(user_data_file.to_string_lossy().to_string()),
            meta_data_path: None,
            network_config_path: None,
        };

        let result = cloud_init(params, false).unwrap();
        assert!(result.changed);

        let backup_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert_eq!(backup_files.len(), 1);
    }

    #[test]
    fn test_validate_user_data_content() {
        assert!(validate_user_data_content("#cloud-config\nusers: []").is_ok());
        assert!(validate_user_data_content("#!/bin/bash\necho hello").is_ok());
        assert!(validate_user_data_content("#include\nhttp://example.com").is_ok());
        assert!(validate_user_data_content("#cloud-boothook\necho hello").is_ok());
        assert!(validate_user_data_content("").is_err());
        assert!(validate_user_data_content("#unknown-header").is_err());
    }

    #[test]
    fn test_normalize_yaml() {
        let yaml1: YamlValue = serde_norway::from_str("a: 1\nb: 2").unwrap();
        let yaml2: YamlValue = serde_norway::from_str("b: 2\na: 1").unwrap();
        assert_eq!(normalize_yaml(&yaml1), normalize_yaml(&yaml2));
    }

    #[test]
    fn test_get_config_path_default() {
        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: None,
            user_data_path: None,
            meta_data_path: None,
            network_config_path: None,
        };
        let path = get_config_path(&params);
        assert_eq!(path, PathBuf::from("/etc/cloud/cloud.cfg.d/99-rash.cfg"));
    }

    #[test]
    fn test_get_config_path_custom() {
        let params = Params {
            state: State::Present,
            user_data: None,
            user_data_content: None,
            meta_data: None,
            network_config: None,
            backup: None,
            directory: Some("/custom/cloud".to_string()),
            user_data_path: None,
            meta_data_path: None,
            network_config_path: None,
        };
        let path = get_config_path(&params);
        assert_eq!(path, PathBuf::from("/custom/cloud/cloud.cfg.d/99-rash.cfg"));
    }
}
