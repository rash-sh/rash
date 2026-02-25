/// ANCHOR: module
/// # grub
///
/// Manage GRUB bootloader installation, configuration, and updates.
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
/// - name: Install GRUB for BIOS boot
///   grub:
///     action: install
///     device: /dev/nvme0n1
///     boot_directory: /mnt/boot
///     target: i386-pc
///
/// - name: Install GRUB for UEFI boot
///   grub:
///     action: install
///     device: /dev/nvme0n1
///     efi_directory: /mnt/boot/efi
///     target: x86_64-efi
///     removable: true
///
/// - name: Configure GRUB for ZFS root
///   grub:
///     action: configure
///     config:
///       GRUB_CMDLINE_LINUX: "root=ZFS=rpool/ROOT/ubuntu boot=zfs"
///       GRUB_PRELOAD_MODULES: "zfs part_gpt"
///       GRUB_TIMEOUT: 0
///       GRUB_DISABLE_OS_PROBER: "true"
///       GRUB_ENABLE_CRYPTODISK: y
///
/// - name: Add kernel parameters
///   grub:
///     action: configure
///     kernel_params:
///       - root=ZFS=rpool/ROOT/ubuntu
///       - boot=zfs
///       - quiet
///       - splash
///     kernel_params_default:
///       - console=tty1
///       - console=ttyS0,115200n8
///
/// - name: Update GRUB configuration
///   grub:
///     action: update
///
/// - name: Configure serial console
///   grub:
///     action: configure
///     terminal: serial
///     serial: "--unit=0 --speed=115200 --word=8 --parity=no --stop=1"
///     config:
///       GRUB_SERIAL_COMMAND: "serial --unit=0 --speed=115200"
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::collections::HashMap;
use std::fs::{OpenOptions, read_to_string};
use std::io::prelude::*;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const DEFAULT_GRUB_CONFIG: &str = "/etc/default/grub";
const DEFAULT_BOOT_DIRECTORY: &str = "/boot";
const DEFAULT_EFI_DIRECTORY: &str = "/boot/efi";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Install,
    Configure,
    Update,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Terminal {
    Console,
    Serial,
    Gfxterm,
}

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Action to perform: install, configure, or update.
    pub action: Action,
    /// Device to install GRUB to (required for install action on BIOS).
    pub device: Option<String>,
    /// Boot directory path.
    /// **[default: `/boot`]**
    pub boot_directory: Option<String>,
    /// EFI directory path for UEFI installation.
    /// **[default: `/boot/efi`]**
    pub efi_directory: Option<String>,
    /// Target platform (i386-pc, x86_64-efi, arm64-efi).
    pub target: Option<String>,
    /// Install for removable media (UEFI only).
    /// **[default: `false`]**
    #[serde(default)]
    pub removable: bool,
    /// Recheck device map.
    /// **[default: `false`]**
    #[serde(default)]
    pub recheck: bool,
    /// Path to GRUB configuration file.
    /// **[default: `/etc/default/grub`]**
    pub config_file: Option<String>,
    /// Dictionary of GRUB configuration values.
    pub config: Option<HashMap<String, String>>,
    /// List of kernel parameters for GRUB_CMDLINE_LINUX.
    pub kernel_params: Option<Vec<String>>,
    /// List of kernel parameters for GRUB_CMDLINE_LINUX_DEFAULT.
    pub kernel_params_default: Option<Vec<String>>,
    /// Disable os-prober.
    /// **[default: `false`]**
    #[serde(default)]
    pub disable_os_prober: bool,
    /// Menu timeout in seconds.
    pub timeout: Option<u32>,
    /// Terminal type (console, serial, gfxterm).
    pub terminal: Option<Terminal>,
    /// Serial console settings (e.g., "--unit=0 --speed=115200").
    pub serial: Option<String>,
}

#[derive(Debug)]
pub struct Grub;

impl Module for Grub {
    fn get_name(&self) -> &str {
        "grub"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((grub(parse_params(optional_params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

fn parse_grub_config(content: &str) -> HashMap<String, String> {
    let mut config = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let value = trimmed[eq_pos + 1..].trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            config.insert(key, value.to_string());
        }
    }

    config
}

fn format_grub_line(key: &str, value: &str) -> String {
    format!(r#"{}="{}""#, key, value)
}

fn update_grub_config_file(
    config_file: &str,
    updates: &HashMap<String, String>,
    check_mode: bool,
) -> Result<bool> {
    let path = Path::new(config_file);

    let (original_entries, mut lines) = if path.exists() {
        let content = read_to_string(path)?;
        (
            parse_grub_config(&content),
            content.lines().map(|s| s.to_string()).collect(),
        )
    } else {
        (HashMap::new(), Vec::new())
    };

    let original_content = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let mut changed = false;
    let empty_value = String::new();

    for (key, new_value) in updates {
        let formatted_line = format_grub_line(key, new_value);

        let mut found = false;
        for line in &mut lines {
            let trimmed = line.trim();
            if let Some(eq_pos) = trimmed.find('=') {
                let existing_key = trimmed[..eq_pos].trim();
                if existing_key == *key {
                    found = true;
                    let existing_value = original_entries.get(key).unwrap_or(&empty_value);
                    if existing_value != new_value {
                        *line = formatted_line.clone();
                        changed = true;
                    }
                    break;
                }
            }
        }

        if !found {
            lines.push(formatted_line);
            changed = true;
        }
    }

    if changed {
        let new_content = format!("{}\n", lines.join("\n"));
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

    Ok(changed)
}

fn json_to_yaml(value: JsonValue) -> YamlValue {
    match value {
        JsonValue::Null => YamlValue::Null,
        JsonValue::Bool(b) => YamlValue::Bool(b),
        JsonValue::Number(n) => {
            YamlValue::Number(serde_norway::Number::from(n.as_i64().unwrap_or(0)))
        }
        JsonValue::String(s) => YamlValue::String(s),
        JsonValue::Array(arr) => YamlValue::Sequence(arr.into_iter().map(json_to_yaml).collect()),
        JsonValue::Object(obj) => YamlValue::Mapping(
            obj.into_iter()
                .map(|(k, v)| (YamlValue::String(k), json_to_yaml(v)))
                .collect(),
        ),
    }
}

fn build_extra_from_json(extra: serde_json::Map<String, JsonValue>) -> Option<YamlValue> {
    Some(json_to_yaml(JsonValue::Object(extra)))
}

fn install_grub(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let boot_dir = params
        .boot_directory
        .as_deref()
        .unwrap_or(DEFAULT_BOOT_DIRECTORY);

    let is_uefi = params
        .target
        .as_ref()
        .map(|t| t.contains("efi"))
        .unwrap_or_else(|| params.efi_directory.is_some());

    if is_uefi && params.efi_directory.is_none() && params.boot_directory.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "efi_directory is required for UEFI installation",
        ));
    }

    if !is_uefi && params.device.is_none() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "device is required for BIOS installation",
        ));
    }

    if check_mode {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "device".to_string(),
            JsonValue::String(
                params
                    .device
                    .clone()
                    .unwrap_or_else(|| "N/A (UEFI)".to_string()),
            ),
        );
        extra.insert(
            "target".to_string(),
            JsonValue::String(params.target.clone().unwrap_or_else(|| {
                if is_uefi {
                    "x86_64-efi".to_string()
                } else {
                    "i386-pc".to_string()
                }
            })),
        );

        return Ok(ModuleResult::new(
            true,
            build_extra_from_json(extra),
            Some("GRUB would be installed".to_string()),
        ));
    }

    let mut cmd = Command::new("grub-install");
    cmd.arg(format!("--boot-directory={}", boot_dir));

    if let Some(target) = &params.target {
        cmd.arg(format!("--target={}", target));
    }

    if params.removable {
        cmd.arg("--removable");
    }

    if params.recheck {
        cmd.arg("--recheck");
    }

    if is_uefi {
        let efi_dir = params
            .efi_directory
            .as_deref()
            .unwrap_or(DEFAULT_EFI_DIRECTORY);
        cmd.arg(format!("--efi-directory={}", efi_dir));

        if let Some(device) = &params.device {
            cmd.arg(device);
        }
    } else if let Some(device) = &params.device {
        cmd.arg(device);
    }

    let output = cmd
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to install GRUB: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let mut extra = serde_json::Map::new();
    extra.insert(
        "device".to_string(),
        JsonValue::String(
            params
                .device
                .clone()
                .unwrap_or_else(|| "N/A (UEFI)".to_string()),
        ),
    );
    extra.insert(
        "target".to_string(),
        JsonValue::String(params.target.clone().unwrap_or_else(|| {
            if is_uefi {
                "x86_64-efi".to_string()
            } else {
                "i386-pc".to_string()
            }
        })),
    );

    Ok(ModuleResult::new(
        true,
        build_extra_from_json(extra),
        Some("GRUB installed successfully".to_string()),
    ))
}

fn configure_grub(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let config_file = params.config_file.as_deref().unwrap_or(DEFAULT_GRUB_CONFIG);

    let mut config_updates = HashMap::new();

    if let Some(config) = &params.config {
        for (key, value) in config {
            config_updates.insert(key.clone(), value.clone());
        }
    }

    if let Some(kernel_params) = &params.kernel_params {
        config_updates.insert("GRUB_CMDLINE_LINUX".to_string(), kernel_params.join(" "));
    }

    if let Some(kernel_params_default) = &params.kernel_params_default {
        config_updates.insert(
            "GRUB_CMDLINE_LINUX_DEFAULT".to_string(),
            kernel_params_default.join(" "),
        );
    }

    if params.disable_os_prober {
        config_updates.insert("GRUB_DISABLE_OS_PROBER".to_string(), "true".to_string());
    }

    if let Some(timeout) = params.timeout {
        config_updates.insert("GRUB_TIMEOUT".to_string(), timeout.to_string());
    }

    if let Some(terminal) = &params.terminal {
        let terminal_str = match terminal {
            Terminal::Console => "console",
            Terminal::Serial => "serial",
            Terminal::Gfxterm => "gfxterm",
        };
        config_updates.insert("GRUB_TERMINAL".to_string(), terminal_str.to_string());
    }

    if let Some(serial) = &params.serial {
        config_updates.insert(
            "GRUB_SERIAL_COMMAND".to_string(),
            format!("serial {}", serial),
        );
    }

    if config_updates.is_empty() {
        return Ok(ModuleResult::new(false, None, None));
    }

    let changed = update_grub_config_file(config_file, &config_updates, check_mode)?;

    let mut extra = serde_json::Map::new();
    extra.insert(
        "config_file".to_string(),
        JsonValue::String(config_file.to_string()),
    );
    extra.insert(
        "config".to_string(),
        JsonValue::Object(
            config_updates
                .into_iter()
                .map(|(k, v)| (k, JsonValue::String(v)))
                .collect(),
        ),
    );

    Ok(ModuleResult::new(
        changed,
        build_extra_from_json(extra),
        if changed {
            Some(format!("GRUB configuration updated in {}", config_file))
        } else {
            None
        },
    ))
}

fn update_grub(params: &Params, check_mode: bool) -> Result<ModuleResult> {
    let config_file = params.config_file.as_deref().unwrap_or(DEFAULT_GRUB_CONFIG);

    if check_mode {
        let mut extra = serde_json::Map::new();
        extra.insert(
            "config_file".to_string(),
            JsonValue::String(config_file.to_string()),
        );
        return Ok(ModuleResult::new(
            true,
            build_extra_from_json(extra),
            Some("GRUB configuration would be updated".to_string()),
        ));
    }

    let output = Command::new("update-grub")
        .output()
        .map_err(|e| Error::new(ErrorKind::SubprocessFail, e))?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "Failed to update GRUB: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let output_str = if stdout.trim().is_empty() {
        "GRUB configuration updated"
    } else {
        &stdout
    };

    let mut extra = serde_json::Map::new();
    extra.insert(
        "config_file".to_string(),
        JsonValue::String(config_file.to_string()),
    );

    Ok(ModuleResult::new(
        true,
        build_extra_from_json(extra),
        Some(output_str.to_string()),
    ))
}

fn grub(params: Params, check_mode: bool) -> Result<ModuleResult> {
    match params.action {
        Action::Install => install_grub(&params, check_mode),
        Action::Configure => configure_grub(&params, check_mode),
        Action::Update => update_grub(&params, check_mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_install() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: install
            device: /dev/nvme0n1
            target: i386-pc
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Install);
        assert_eq!(params.device, Some("/dev/nvme0n1".to_string()));
        assert_eq!(params.target, Some("i386-pc".to_string()));
    }

    #[test]
    fn test_parse_params_configure() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: configure
            kernel_params:
              - quiet
              - splash
            timeout: 5
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Configure);
        assert_eq!(
            params.kernel_params,
            Some(vec!["quiet".to_string(), "splash".to_string()])
        );
        assert_eq!(params.timeout, Some(5));
    }

    #[test]
    fn test_parse_params_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: update
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Update);
    }

    #[test]
    fn test_parse_grub_config() {
        let content = r#"
# GRUB configuration
GRUB_DEFAULT=0
GRUB_TIMEOUT="5"
GRUB_CMDLINE_LINUX="quiet splash"
"#;
        let config = parse_grub_config(content);
        assert_eq!(config.get("GRUB_DEFAULT"), Some(&"0".to_string()));
        assert_eq!(config.get("GRUB_TIMEOUT"), Some(&"5".to_string()));
        assert_eq!(
            config.get("GRUB_CMDLINE_LINUX"),
            Some(&"quiet splash".to_string())
        );
    }

    #[test]
    fn test_format_grub_line() {
        assert_eq!(format_grub_line("GRUB_TIMEOUT", "5"), r#"GRUB_TIMEOUT="5""#);
        assert_eq!(
            format_grub_line("GRUB_CMDLINE_LINUX", "quiet splash"),
            r#"GRUB_CMDLINE_LINUX="quiet splash""#
        );
    }

    #[test]
    fn test_update_grub_config_file_add() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("grub");

        fs::write(&file_path, "GRUB_DEFAULT=0\n").unwrap();

        let mut updates = HashMap::new();
        updates.insert("GRUB_TIMEOUT".to_string(), "5".to_string());

        let changed =
            update_grub_config_file(file_path.to_str().unwrap(), &updates, false).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("GRUB_TIMEOUT=\"5\""));
    }

    #[test]
    fn test_update_grub_config_file_modify() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("grub");

        fs::write(&file_path, "GRUB_TIMEOUT=0\n").unwrap();

        let mut updates = HashMap::new();
        updates.insert("GRUB_TIMEOUT".to_string(), "5".to_string());

        let changed =
            update_grub_config_file(file_path.to_str().unwrap(), &updates, false).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("GRUB_TIMEOUT=\"5\""));
    }

    #[test]
    fn test_update_grub_config_file_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("grub");

        fs::write(&file_path, "GRUB_TIMEOUT=\"5\"\n").unwrap();

        let mut updates = HashMap::new();
        updates.insert("GRUB_TIMEOUT".to_string(), "5".to_string());

        let changed =
            update_grub_config_file(file_path.to_str().unwrap(), &updates, false).unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_update_grub_config_file_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("grub");

        fs::write(&file_path, "GRUB_TIMEOUT=0\n").unwrap();
        let original = fs::read_to_string(&file_path).unwrap();

        let mut updates = HashMap::new();
        updates.insert("GRUB_TIMEOUT".to_string(), "5".to_string());

        let changed = update_grub_config_file(file_path.to_str().unwrap(), &updates, true).unwrap();
        assert!(changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original, content);
    }

    #[test]
    fn test_configure_grub_no_changes() {
        let params = Params {
            action: Action::Configure,
            device: None,
            boot_directory: None,
            efi_directory: None,
            target: None,
            removable: false,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = configure_grub(&params, false).unwrap();
        assert!(!result.get_changed());
    }

    #[test]
    fn test_install_grub_check_mode_bios() {
        let params = Params {
            action: Action::Install,
            device: Some("/dev/sda".to_string()),
            boot_directory: Some("/mnt/boot".to_string()),
            efi_directory: None,
            target: Some("i386-pc".to_string()),
            removable: false,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = install_grub(&params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("would be installed"));
    }

    #[test]
    fn test_install_grub_check_mode_uefi() {
        let params = Params {
            action: Action::Install,
            device: None,
            boot_directory: Some("/mnt/boot".to_string()),
            efi_directory: Some("/mnt/boot/efi".to_string()),
            target: Some("x86_64-efi".to_string()),
            removable: true,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = install_grub(&params, true).unwrap();
        assert!(result.get_changed());
    }

    #[test]
    fn test_install_grub_missing_device_bios() {
        let params = Params {
            action: Action::Install,
            device: None,
            boot_directory: None,
            efi_directory: None,
            target: Some("i386-pc".to_string()),
            removable: false,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = install_grub(&params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("device is required")
        );
    }

    #[test]
    fn test_install_grub_missing_efi_directory() {
        let params = Params {
            action: Action::Install,
            device: None,
            boot_directory: None,
            efi_directory: None,
            target: Some("x86_64-efi".to_string()),
            removable: false,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = install_grub(&params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("efi_directory is required")
        );
    }

    #[test]
    fn test_update_grub_check_mode() {
        let params = Params {
            action: Action::Update,
            device: None,
            boot_directory: None,
            efi_directory: None,
            target: None,
            removable: false,
            recheck: false,
            config_file: None,
            config: None,
            kernel_params: None,
            kernel_params_default: None,
            disable_os_prober: false,
            timeout: None,
            terminal: None,
            serial: None,
        };

        let result = update_grub(&params, true).unwrap();
        assert!(result.get_changed());
        assert!(result.get_output().unwrap().contains("would be updated"));
    }

    #[test]
    fn test_parse_params_with_config() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: configure
            config:
              GRUB_CMDLINE_LINUX: "root=ZFS=rpool/ROOT/ubuntu"
              GRUB_PRELOAD_MODULES: "zfs part_gpt"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Configure);
        let config = params.config.unwrap();
        assert_eq!(
            config.get("GRUB_CMDLINE_LINUX"),
            Some(&"root=ZFS=rpool/ROOT/ubuntu".to_string())
        );
        assert_eq!(
            config.get("GRUB_PRELOAD_MODULES"),
            Some(&"zfs part_gpt".to_string())
        );
    }

    #[test]
    fn test_parse_params_terminal() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: configure
            terminal: serial
            serial: "--unit=0 --speed=115200"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.terminal, Some(Terminal::Serial));
        assert_eq!(params.serial, Some("--unit=0 --speed=115200".to_string()));
    }
}
