/// ANCHOR: module
/// # docker_config
///
/// Manage Docker daemon configuration (daemon.json).
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
/// - name: Set Docker storage driver
///   docker_config:
///     storage_driver: overlay2
///
/// - name: Configure Docker registry mirrors
///   docker_config:
///     registry_mirrors:
///       - "https://mirror1.example.com"
///       - "https://mirror2.example.com"
///
/// - name: Set Docker log configuration
///   docker_config:
///     log_driver: json-file
///     log_opts:
///       max-size: 10m
///       max-file: 3
///
/// - name: Configure Docker live restore
///   docker_config:
///     live_restore: true
///
/// - name: Set multiple Docker options
///   docker_config:
///     storage_driver: overlay2
///     default_ulimits:
///       nofile:
///         name: nofile
///         hard: 65536
///         soft: 65536
///     userland_proxy: false
///
/// - name: Remove a configuration option
///   docker_config:
///     userland_proxy: null
///     state: absent
///
/// - name: Configure Docker with custom path
///   docker_config:
///     path: /etc/docker/daemon.json
///     storage_driver: overlay2
///     backup: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use log::trace;
use std::fs;
use std::io::Write;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn default_docker_config_path() -> String {
    "/etc/docker/daemon.json".to_string()
}

#[derive(Debug, PartialEq, Deserialize, Default)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to the Docker daemon.json file.
    /// **[default: `/etc/docker/daemon.json`]**
    #[serde(default = "default_docker_config_path")]
    path: String,
    /// Docker storage driver (e.g., overlay2, devicemapper).
    storage_driver: Option<String>,
    /// List of Docker registry mirrors.
    registry_mirrors: Option<Vec<String>>,
    /// Default logging driver for containers.
    log_driver: Option<String>,
    /// Logging driver options as key-value pairs.
    log_opts: Option<serde_json::Map<String, JsonValue>>,
    /// Enable live restore of containers when daemon shuts down.
    live_restore: Option<bool>,
    /// Maximum number of concurrent downloads per pull.
    max_concurrent_downloads: Option<u32>,
    /// Maximum number of concurrent uploads per push.
    max_concurrent_uploads: Option<u32>,
    /// Default ulimits for containers.
    default_ulimits: Option<serde_json::Map<String, JsonValue>>,
    /// Enable userland proxy for loopback addresses.
    userland_proxy: Option<bool>,
    /// Disable legacy registry (v1) support.
    disable_legacy_registry: Option<bool>,
    /// Enable debug mode.
    debug: Option<bool>,
    /// Docker hosts to listen on (e.g., ["tcp://0.0.0.0:2375", "unix:///var/run/docker.sock"]).
    hosts: Option<Vec<String>>,
    /// TLS configuration.
    tls: Option<bool>,
    /// Path to TLS certificate.
    tlscert: Option<String>,
    /// Path to TLS key.
    tlskey: Option<String>,
    /// Path to TLS CA certificate.
    tlscacert: Option<String>,
    /// Arbitrary configuration key using dot notation.
    key: Option<String>,
    /// Value to set for arbitrary key.
    value: Option<JsonValue>,
    /// Whether configuration should exist or not.
    /// **[default: `"present"`]**
    #[serde(default)]
    state: State,
    /// Create a backup before modifying.
    /// **[default: `false`]**
    #[serde(default)]
    backup: bool,
    /// Restart Docker daemon after configuration change.
    /// **[default: `false`]**
    #[serde(default)]
    reload: bool,
}

fn parse_key_path(key: &str) -> Vec<String> {
    key.split('.').map(|s| s.to_string()).collect()
}

fn set_value_at_path(json: &mut JsonValue, path: &[String], value: JsonValue) -> bool {
    if path.is_empty() {
        *json = value;
        return true;
    }

    match json {
        JsonValue::Object(map) => {
            let key = &path[0];
            if path.len() == 1 {
                if let Some(existing) = map.get(key)
                    && existing == &value
                {
                    return false;
                }
                map.insert(key.clone(), value);
                true
            } else {
                if !map.contains_key(key) {
                    map.insert(key.clone(), JsonValue::Object(serde_json::Map::new()));
                }
                if let Some(child) = map.get_mut(key) {
                    set_value_at_path(child, &path[1..], value)
                } else {
                    false
                }
            }
        }
        JsonValue::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if idx < arr.len() {
                    if path.len() == 1 {
                        if arr[idx] == value {
                            return false;
                        }
                        arr[idx] = value;
                        true
                    } else {
                        set_value_at_path(&mut arr[idx], &path[1..], value)
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn remove_key_at_path(json: &mut JsonValue, path: &[String]) -> bool {
    if path.is_empty() {
        return false;
    }

    match json {
        JsonValue::Object(map) => {
            if path.len() == 1 {
                map.remove(&path[0]).is_some()
            } else if let Some(child) = map.get_mut(&path[0]) {
                remove_key_at_path(child, &path[1..])
            } else {
                false
            }
        }
        JsonValue::Array(arr) => {
            if let Ok(idx) = path[0].parse::<usize>() {
                if idx < arr.len() {
                    if path.len() == 1 {
                        arr.remove(idx);
                        true
                    } else {
                        remove_key_at_path(&mut arr[idx], &path[1..])
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn create_backup(path: &Path) -> Result<()> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::new(ErrorKind::Other, e))?
        .as_secs();
    let backup_path = format!("{}.{}.bak", path.display(), timestamp);
    fs::copy(path, &backup_path)?;
    Ok(())
}

fn merge_config(existing: &mut JsonValue, params: &Params, state: State) -> (bool, Vec<String>) {
    let mut changed = false;
    let mut changes = Vec::new();

    if state == State::Present {
        if let Some(ref storage_driver) = params.storage_driver {
            let key_path = vec!["storage-driver".to_string()];
            let value = JsonValue::String(storage_driver.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("storage-driver: {}", storage_driver));
            }
        }

        if let Some(ref registry_mirrors) = params.registry_mirrors {
            let key_path = vec!["registry-mirrors".to_string()];
            let value = JsonValue::Array(
                registry_mirrors
                    .iter()
                    .map(|s| JsonValue::String(s.clone()))
                    .collect(),
            );
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("registry-mirrors: {:?}", registry_mirrors));
            }
        }

        if let Some(ref log_driver) = params.log_driver {
            let key_path = vec!["log-driver".to_string()];
            let value = JsonValue::String(log_driver.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("log-driver: {}", log_driver));
            }
        }

        if let Some(ref log_opts) = params.log_opts {
            let key_path = vec!["log-opts".to_string()];
            let value = JsonValue::Object(log_opts.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("log-opts: {:?}", log_opts));
            }
        }

        if let Some(live_restore) = params.live_restore {
            let key_path = vec!["live-restore".to_string()];
            let value = JsonValue::Bool(live_restore);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("live-restore: {}", live_restore));
            }
        }

        if let Some(max_concurrent_downloads) = params.max_concurrent_downloads {
            let key_path = vec!["max-concurrent-downloads".to_string()];
            let value = JsonValue::Number(max_concurrent_downloads.into());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!(
                    "max-concurrent-downloads: {}",
                    max_concurrent_downloads
                ));
            }
        }

        if let Some(max_concurrent_uploads) = params.max_concurrent_uploads {
            let key_path = vec!["max-concurrent-uploads".to_string()];
            let value = JsonValue::Number(max_concurrent_uploads.into());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!(
                    "max-concurrent-uploads: {}",
                    max_concurrent_uploads
                ));
            }
        }

        if let Some(ref default_ulimits) = params.default_ulimits {
            let key_path = vec!["default-ulimits".to_string()];
            let value = JsonValue::Object(default_ulimits.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("default-ulimits: {:?}", default_ulimits));
            }
        }

        if let Some(userland_proxy) = params.userland_proxy {
            let key_path = vec!["userland-proxy".to_string()];
            let value = JsonValue::Bool(userland_proxy);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("userland-proxy: {}", userland_proxy));
            }
        }

        if let Some(disable_legacy_registry) = params.disable_legacy_registry {
            let key_path = vec!["disable-legacy-registry".to_string()];
            let value = JsonValue::Bool(disable_legacy_registry);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!(
                    "disable-legacy-registry: {}",
                    disable_legacy_registry
                ));
            }
        }

        if let Some(debug) = params.debug {
            let key_path = vec!["debug".to_string()];
            let value = JsonValue::Bool(debug);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("debug: {}", debug));
            }
        }

        if let Some(ref hosts) = params.hosts {
            let key_path = vec!["hosts".to_string()];
            let value =
                JsonValue::Array(hosts.iter().map(|s| JsonValue::String(s.clone())).collect());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("hosts: {:?}", hosts));
            }
        }

        if let Some(tls) = params.tls {
            let key_path = vec!["tls".to_string()];
            let value = JsonValue::Bool(tls);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("tls: {}", tls));
            }
        }

        if let Some(ref tlscert) = params.tlscert {
            let key_path = vec!["tlscert".to_string()];
            let value = JsonValue::String(tlscert.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("tlscert: {}", tlscert));
            }
        }

        if let Some(ref tlskey) = params.tlskey {
            let key_path = vec!["tlskey".to_string()];
            let value = JsonValue::String(tlskey.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("tlskey: {}", tlskey));
            }
        }

        if let Some(ref tlscacert) = params.tlscacert {
            let key_path = vec!["tlscacert".to_string()];
            let value = JsonValue::String(tlscacert.clone());
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("tlscacert: {}", tlscacert));
            }
        }

        if let Some(ref key) = params.key
            && let Some(ref value) = params.value
        {
            let key_path = parse_key_path(key);
            if set_value_at_path(existing, &key_path, value.clone()) {
                changed = true;
                changes.push(format!("{}: {}", key, value));
            }
        }
    } else if state == State::Absent {
        if let Some(ref key) = params.key {
            let key_path = parse_key_path(key);
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push(format!("removed key: {}", key));
            }
        }

        if params.storage_driver.is_some() {
            let key_path = vec!["storage-driver".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed storage-driver".to_string());
            }
        }

        if params.registry_mirrors.is_some() {
            let key_path = vec!["registry-mirrors".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed registry-mirrors".to_string());
            }
        }

        if params.log_driver.is_some() {
            let key_path = vec!["log-driver".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed log-driver".to_string());
            }
        }

        if params.log_opts.is_some() {
            let key_path = vec!["log-opts".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed log-opts".to_string());
            }
        }

        if params.live_restore.is_some() {
            let key_path = vec!["live-restore".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed live-restore".to_string());
            }
        }

        if params.max_concurrent_downloads.is_some() {
            let key_path = vec!["max-concurrent-downloads".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed max-concurrent-downloads".to_string());
            }
        }

        if params.max_concurrent_uploads.is_some() {
            let key_path = vec!["max-concurrent-uploads".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed max-concurrent-uploads".to_string());
            }
        }

        if params.default_ulimits.is_some() {
            let key_path = vec!["default-ulimits".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed default-ulimits".to_string());
            }
        }

        if params.userland_proxy.is_some() {
            let key_path = vec!["userland-proxy".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed userland-proxy".to_string());
            }
        }

        if params.disable_legacy_registry.is_some() {
            let key_path = vec!["disable-legacy-registry".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed disable-legacy-registry".to_string());
            }
        }

        if params.debug.is_some() {
            let key_path = vec!["debug".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed debug".to_string());
            }
        }

        if params.hosts.is_some() {
            let key_path = vec!["hosts".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed hosts".to_string());
            }
        }

        if params.tls.is_some() {
            let key_path = vec!["tls".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed tls".to_string());
            }
        }

        if params.tlscert.is_some() {
            let key_path = vec!["tlscert".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed tlscert".to_string());
            }
        }

        if params.tlskey.is_some() {
            let key_path = vec!["tlskey".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed tlskey".to_string());
            }
        }

        if params.tlscacert.is_some() {
            let key_path = vec!["tlscacert".to_string()];
            if remove_key_at_path(existing, &key_path) {
                changed = true;
                changes.push("removed tlscacert".to_string());
            }
        }
    }

    (changed, changes)
}

fn reload_docker_daemon(check_mode: bool) -> Result<bool> {
    if check_mode {
        return Ok(true);
    }

    use std::process::Command;

    let output = Command::new("pkill")
        .args(["-HUP", " dockerd"])
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    Ok(output.status.success())
}

pub fn docker_config(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let path = Path::new(&params.path);
    let state = params.state.clone();

    let (mut json, original_content) = if path.exists() {
        let content = fs::read_to_string(path)?;
        let json: JsonValue = if content.trim().is_empty() {
            JsonValue::Object(serde_json::Map::new())
        } else {
            serde_json::from_str(&content).map_err(|e| Error::new(ErrorKind::InvalidData, e))?
        };
        (json, content)
    } else {
        (JsonValue::Object(serde_json::Map::new()), String::new())
    };

    let (changed, changes) = merge_config(&mut json, &params, state);

    if changed {
        let new_content =
            serde_json::to_string_pretty(&json).map_err(|e| Error::new(ErrorKind::Other, e))?;
        let new_content_with_newline = format!("{}\n", new_content);

        diff(&original_content, &new_content_with_newline);

        if !check_mode {
            if params.backup && path.exists() {
                create_backup(path)?;
            }

            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let mut file = fs::File::create(path)?;
            file.write_all(new_content_with_newline.as_bytes())?;

            if params.reload {
                reload_docker_daemon(check_mode)?;
            }
        }
    }

    let output = if changes.is_empty() {
        None
    } else {
        Some(changes.join("\n"))
    };

    Ok(ModuleResult {
        changed,
        output,
        extra: None,
    })
}

#[derive(Debug)]
pub struct DockerConfig;

impl Module for DockerConfig {
    fn get_name(&self) -> &str {
        "docker_config"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            docker_config(parse_params(optional_params)?, check_mode)?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_minimal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            storage_driver: overlay2
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.storage_driver, Some("overlay2".to_string()));
        assert_eq!(params.path, "/etc/docker/daemon.json");
        assert_eq!(params.state, State::Present);
    }

    #[test]
    fn test_parse_params_full() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /custom/docker/daemon.json
            storage_driver: overlay2
            registry_mirrors:
              - "https://mirror1.example.com"
              - "https://mirror2.example.com"
            log_driver: json-file
            log_opts:
              max-size: 10m
              max-file: 3
            live_restore: true
            backup: true
            reload: true
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.path, "/custom/docker/daemon.json");
        assert_eq!(params.storage_driver, Some("overlay2".to_string()));
        assert_eq!(
            params.registry_mirrors,
            Some(vec![
                "https://mirror1.example.com".to_string(),
                "https://mirror2.example.com".to_string()
            ])
        );
        assert_eq!(params.log_driver, Some("json-file".to_string()));
        assert!(params.backup);
        assert!(params.reload);
    }

    #[test]
    fn test_parse_params_arbitrary_key() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            key: custom.nested.option
            value: myvalue
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.key, Some("custom.nested.option".to_string()));
        assert_eq!(params.value, Some(json!("myvalue")));
    }

    #[test]
    fn test_set_value_at_path_new() {
        let mut json = json!({});
        let changed = set_value_at_path(
            &mut json,
            &["storage-driver".to_string()],
            json!("overlay2"),
        );
        assert!(changed);
        assert_eq!(json, json!({"storage-driver": "overlay2"}));
    }

    #[test]
    fn test_set_value_at_path_nested() {
        let mut json = json!({});
        let changed = set_value_at_path(
            &mut json,
            &["log-opts".to_string(), "max-size".to_string()],
            json!("10m"),
        );
        assert!(changed);
        assert_eq!(json, json!({"log-opts": {"max-size": "10m"}}));
    }

    #[test]
    fn test_set_value_at_path_no_change() {
        let mut json = json!({"storage-driver": "overlay2"});
        let changed = set_value_at_path(
            &mut json,
            &["storage-driver".to_string()],
            json!("overlay2"),
        );
        assert!(!changed);
    }

    #[test]
    fn test_remove_key_at_path() {
        let mut json = json!({"storage-driver": "overlay2", "debug": true});
        let removed = remove_key_at_path(&mut json, &["storage-driver".to_string()]);
        assert!(removed);
        assert_eq!(json, json!({"debug": true}));
    }

    #[test]
    fn test_remove_key_at_path_nested() {
        let mut json = json!({"log-opts": {"max-size": "10m", "max-file": "3"}});
        let removed =
            remove_key_at_path(&mut json, &["log-opts".to_string(), "max-size".to_string()]);
        assert!(removed);
        assert_eq!(json, json!({"log-opts": {"max-file": "3"}}));
    }

    #[test]
    fn test_remove_key_at_path_not_found() {
        let mut json = json!({"storage-driver": "overlay2"});
        let removed = remove_key_at_path(&mut json, &["nonexistent".to_string()]);
        assert!(!removed);
    }

    #[test]
    fn test_docker_config_add_storage_driver() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"storage-driver": "overlay2"}));
    }

    #[test]
    fn test_docker_config_add_multiple_options() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            log_driver: Some("json-file".to_string()),
            live_restore: Some(true),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(
            json,
            json!({
                "storage-driver": "overlay2",
                "log-driver": "json-file",
                "live-restore": true
            })
        );
    }

    #[test]
    fn test_docker_config_modify_existing() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        fs::write(&file_path, r#"{"storage-driver": "devicemapper"}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"storage-driver": "overlay2"}));
    }

    #[test]
    fn test_docker_config_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        fs::write(&file_path, r#"{"storage-driver": "overlay2"}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_docker_config_remove_option() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        fs::write(
            &file_path,
            r#"{"storage-driver": "overlay2", "debug": true}"#,
        )
        .unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            debug: Some(true),
            state: State::Absent,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"storage-driver": "overlay2"}));
    }

    #[test]
    fn test_docker_config_arbitrary_key() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            key: Some("custom.nested.option".to_string()),
            value: Some(json!("myvalue")),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(json, json!({"custom": {"nested": {"option": "myvalue"}}}));
    }

    #[test]
    fn test_docker_config_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, true).unwrap();
        assert!(result.changed);

        assert!(!file_path.exists());
    }

    #[test]
    fn test_docker_config_registry_mirrors() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            registry_mirrors: Some(vec![
                "https://mirror1.example.com".to_string(),
                "https://mirror2.example.com".to_string(),
            ]),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(
            json,
            json!({"registry-mirrors": ["https://mirror1.example.com", "https://mirror2.example.com"]})
        );
    }

    #[test]
    fn test_docker_config_log_opts() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        let mut log_opts = serde_json::Map::new();
        log_opts.insert("max-size".to_string(), json!("10m"));
        log_opts.insert("max-file".to_string(), json!(3));

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            log_opts: Some(log_opts.clone()),
            state: State::Present,
            backup: false,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        let json: JsonValue = serde_json::from_str(&content).unwrap();
        assert_eq!(
            json,
            json!({"log-opts": {"max-size": "10m", "max-file": 3}})
        );
    }

    #[test]
    fn test_docker_config_backup() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.json");

        fs::write(&file_path, r#"{"storage-driver": "devicemapper"}"#).unwrap();

        let params = Params {
            path: file_path.to_str().unwrap().to_string(),
            storage_driver: Some("overlay2".to_string()),
            state: State::Present,
            backup: true,
            reload: false,
            ..Default::default()
        };

        let result = docker_config(params, false).unwrap();
        assert!(result.changed);

        let backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn test_parse_params_default_ulimits() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            default_ulimits:
              nofile:
                name: nofile
                hard: 65536
                soft: 65536
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        let ulimits = params.default_ulimits.unwrap();
        let nofile = ulimits.get("nofile").unwrap();
        assert_eq!(nofile.get("hard").unwrap(), &json!(65536));
        assert_eq!(nofile.get("soft").unwrap(), &json!(65536));
    }
}
