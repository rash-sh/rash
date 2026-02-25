/// ANCHOR: module
/// # initramfs
///
/// Manage initramfs/initrd configuration, generation, and updates.
///
/// This module works with initramfs-tools (Debian/Ubuntu) to configure
/// and manage initramfs images, including modules, hooks, and configuration.
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
/// - name: Configure initramfs for ZFS
///   initramfs:
///     action: configure
///     config:
///       MODULES: most
///       BUSYBOX: auto
///       COMPRESS: zstd
///     modules:
///       - zfs
///       - spl
///
/// - name: Add ZFS hook to initramfs
///   initramfs:
///     action: configure
///     hooks:
///       - zfs
///     files:
///       - src: /etc/zfs/zfs-key
///         dest: /etc/zfs/zfs-key
///
/// - name: Update initramfs for all kernels
///   initramfs:
///     action: update
///     kernel: all
///
/// - name: Update initramfs for specific kernel
///   initramfs:
///     action: update
///     kernel: 6.8.0-48-generic
///
/// - name: Generate new initramfs
///   initramfs:
///     action: generate
///     kernel: 6.8.0-48-generic
///     compression: zstd
///
/// - name: Remove hook from initramfs
///   initramfs:
///     action: configure
///     hooks_absent:
///       - zfs
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const INITRAMFS_CONF: &str = "/etc/initramfs-tools/initramfs.conf";
const INITRAMFS_MODULES: &str = "/etc/initramfs-tools/modules";
const INITRAMFS_HOOKS_DIR: &str = "/etc/initramfs-tools/hooks";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Action to perform.
    /// **[required]**
    pub action: Action,
    /// Kernel version (e.g., 6.8.0-48-generic, all).
    /// Used with update and generate actions.
    /// **[default: `"all"`]**
    pub kernel: Option<String>,
    /// Dict of initramfs-tools configuration options.
    /// Keys: MODULES, BUSYBOX, COMPRESS, BOOT, NFSROOT, RUNSIZE, FSTYPE.
    pub config: Option<serde_json::Value>,
    /// List of modules to include in initramfs.
    pub modules: Option<Vec<String>>,
    /// List of hooks to ensure present.
    pub hooks: Option<Vec<String>>,
    /// List of hooks to ensure absent.
    pub hooks_absent: Option<Vec<String>>,
    /// List of files to include in initramfs.
    pub files: Option<Vec<InitramfsFile>>,
    /// Compression algorithm for generate action.
    /// **[default: `"gzip"`]**
    pub compression: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
pub enum Action {
    Update,
    Generate,
    Configure,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
pub struct InitramfsFile {
    /// Source file path.
    pub src: String,
    /// Destination path in initramfs.
    pub dest: String,
    /// File permissions (octal string like "0600").
    /// **[default: `"0644"`]**
    pub mode: Option<String>,
}

#[allow(clippy::lines_filter_map_ok)]
fn read_file_lines(path: &Path) -> Vec<String> {
    if !path.exists() {
        return Vec::new();
    }

    fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .filter_map(std::result::Result::ok)
                .collect()
        })
        .unwrap_or_default()
}

fn update_config_value(lines: &mut Vec<String>, key: &str, value: &str, changed: &mut bool) {
    let target_prefix = format!("{key}=");
    let new_line = format!("{key}={value}");

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with(&target_prefix) && !trimmed.starts_with('#') {
            if line.trim() != new_line {
                *line = new_line.clone();
                *changed = true;
            }
            return;
        }
    }

    if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
        lines.push(String::new());
    }
    lines.push(new_line);
    *changed = true;
}

fn configure_initramfs_conf(
    config: &serde_json::Map<String, serde_json::Value>,
    check_mode: bool,
) -> Result<bool> {
    let path = Path::new(INITRAMFS_CONF);
    let lines = read_file_lines(path);
    let original = lines.join("\n");

    let mut changed = false;
    let mut new_lines = lines.clone();

    for (key, value) in config {
        let value_str = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => continue,
        };
        update_config_value(&mut new_lines, key, &value_str, &mut changed);
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

    Ok(changed)
}

fn configure_modules(modules: &[String], check_mode: bool) -> Result<bool> {
    let path = Path::new(INITRAMFS_MODULES);
    let lines = read_file_lines(path);
    let original = lines.join("\n");

    let mut changed = false;
    let mut new_lines = lines.clone();

    for module in modules {
        let module_exists = new_lines.iter().any(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with('#') && trimmed == module
        });

        if !module_exists {
            if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true) {
                new_lines.push(String::new());
            }
            new_lines.push(module.clone());
            changed = true;
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

    Ok(changed)
}

fn configure_hooks(hooks: &[String], hooks_absent: &[String], check_mode: bool) -> Result<bool> {
    let hooks_dir = Path::new(INITRAMFS_HOOKS_DIR);
    let mut changed = false;

    if (!hooks.is_empty() || !hooks_absent.is_empty()) && !check_mode && !hooks_dir.exists() {
        fs::create_dir_all(hooks_dir)?;
    }

    for hook in hooks {
        let hook_path = hooks_dir.join(hook);
        if !hook_path.exists() {
            if !check_mode {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&hook_path)?;
                writeln!(file, "#!/bin/sh\n# Initramfs hook: {hook}\nexit 0")?;
                fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
            }
            changed = true;
        }
    }

    for hook in hooks_absent {
        let hook_path = hooks_dir.join(hook);
        if hook_path.exists() {
            if !check_mode {
                fs::remove_file(&hook_path)?;
            }
            changed = true;
        }
    }

    Ok(changed)
}

fn copy_initramfs_files(files: &[InitramfsFile], check_mode: bool) -> Result<bool> {
    let mut changed = false;

    for file in files {
        let src_path = Path::new(&file.src);
        let dest_path = Path::new(&file.dest);

        if !src_path.exists() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Source file does not exist: {}", file.src),
            ));
        }

        let needs_copy = if !dest_path.exists() {
            true
        } else {
            let src_metadata = fs::metadata(src_path)?;
            let dest_metadata = fs::metadata(dest_path)?;
            src_metadata.len() != dest_metadata.len()
                || src_metadata.modified()? > dest_metadata.modified()?
        };

        if needs_copy {
            if !check_mode {
                if let Some(parent) = dest_path.parent()
                    && !parent.exists()
                {
                    fs::create_dir_all(parent)?;
                }

                fs::copy(src_path, dest_path)?;

                if let Some(mode_str) = &file.mode
                    && let Ok(mode) = u32::from_str_radix(mode_str.trim_start_matches('0'), 8)
                {
                    fs::set_permissions(dest_path, fs::Permissions::from_mode(mode))?;
                }
            }
            changed = true;
        }
    }

    Ok(changed)
}

#[allow(dead_code)]
fn get_installed_kernels() -> Result<Vec<String>> {
    let output = Command::new("ls")
        .arg("/lib/modules")
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to list /lib/modules: {e}"),
            )
        })?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let kernels: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(kernels)
}

fn run_update_initramfs(kernel: &str, generate: bool, check_mode: bool) -> Result<(bool, String)> {
    if check_mode {
        let kernel_display = if kernel == "all" {
            "all kernels".to_string()
        } else {
            kernel.to_string()
        };
        return Ok((true, format!("Would update initramfs for {kernel_display}")));
    }

    let mut cmd = Command::new("update-initramfs");

    if generate {
        cmd.arg("-c");
    } else {
        cmd.arg("-u");
    }

    if kernel == "all" {
        cmd.arg("-k").arg("all");
    } else {
        cmd.arg("-k").arg(kernel);
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute update-initramfs: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "update-initramfs failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let kernel_display = if kernel == "all" {
        "all kernels".to_string()
    } else {
        kernel.to_string()
    };

    Ok((true, format!("Updated initramfs for {kernel_display}")))
}

#[allow(dead_code)]
fn get_initramfs_path(kernel: &str) -> String {
    format!("/boot/initrd.img-{kernel}")
}

pub fn initramfs(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let kernel = params.kernel.unwrap_or_else(|| "all".to_string());
    let mut changed = false;
    let mut messages = Vec::new();

    match params.action {
        Action::Configure => {
            if let Some(config) = &params.config
                && let serde_json::Value::Object(config_map) = config
            {
                let config_changed = configure_initramfs_conf(config_map, check_mode)?;
                if config_changed {
                    messages.push("Updated initramfs configuration".to_string());
                }
                changed = changed || config_changed;
            }

            if let Some(modules) = &params.modules {
                let modules_changed = configure_modules(modules, check_mode)?;
                if modules_changed {
                    messages.push(format!("Added {} module(s) to initramfs", modules.len()));
                }
                changed = changed || modules_changed;
            }

            let hooks = params.hooks.unwrap_or_default();
            let hooks_absent = params.hooks_absent.unwrap_or_default();
            if !hooks.is_empty() || !hooks_absent.is_empty() {
                let hooks_changed = configure_hooks(&hooks, &hooks_absent, check_mode)?;
                if hooks_changed {
                    if !hooks.is_empty() {
                        messages.push(format!("Added {} hook(s)", hooks.len()));
                    }
                    if !hooks_absent.is_empty() {
                        messages.push(format!("Removed {} hook(s)", hooks_absent.len()));
                    }
                }
                changed = changed || hooks_changed;
            }

            if let Some(files) = &params.files {
                let files_changed = copy_initramfs_files(files, check_mode)?;
                if files_changed {
                    messages.push(format!("Copied {} file(s)", files.len()));
                }
                changed = changed || files_changed;
            }
        }
        Action::Update => {
            let (update_changed, msg) = run_update_initramfs(&kernel, false, check_mode)?;
            messages.push(msg);
            changed = changed || update_changed;
        }
        Action::Generate => {
            let (gen_changed, msg) = run_update_initramfs(&kernel, true, check_mode)?;
            messages.push(msg);
            changed = changed || gen_changed;
        }
    }

    let extra = if !messages.is_empty() {
        Some(YamlValue::Mapping(
            messages
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    (
                        YamlValue::String(format!("message_{}", i)),
                        YamlValue::String(m.clone()),
                    )
                })
                .collect(),
        ))
    } else {
        None
    };

    let output = if messages.is_empty() {
        None
    } else {
        Some(messages.join("; "))
    };

    Ok(ModuleResult::new(changed, extra, output))
}

#[derive(Debug)]
pub struct Initramfs;

impl Module for Initramfs {
    fn get_name(&self) -> &str {
        "initramfs"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((initramfs(parse_params(optional_params)?, check_mode)?, None))
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
    fn test_parse_params_configure() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: configure
            config:
              MODULES: most
              COMPRESS: zstd
            modules:
              - zfs
              - spl
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Configure);
        assert!(params.config.is_some());
        assert_eq!(
            params.modules,
            Some(vec!["zfs".to_string(), "spl".to_string()])
        );
    }

    #[test]
    fn test_parse_params_update() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: update
            kernel: all
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Update);
        assert_eq!(params.kernel, Some("all".to_string()));
    }

    #[test]
    fn test_parse_params_generate() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: generate
            kernel: 6.8.0-48-generic
            compression: zstd
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Generate);
        assert_eq!(params.kernel, Some("6.8.0-48-generic".to_string()));
        assert_eq!(params.compression, Some("zstd".to_string()));
    }

    #[test]
    fn test_parse_params_with_files() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: configure
            files:
              - src: /etc/zfs/zfs-key
                dest: /etc/zfs/zfs-key
                mode: "0600"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.files.as_ref().unwrap().len(), 1);
        assert_eq!(params.files.as_ref().unwrap()[0].src, "/etc/zfs/zfs-key");
        assert_eq!(
            params.files.as_ref().unwrap()[0].mode,
            Some("0600".to_string())
        );
    }

    #[test]
    fn test_update_config_value_adds_new() {
        let mut lines = vec!["MODULES=most".to_string()];
        let mut changed = false;
        update_config_value(&mut lines, "COMPRESS", "zstd", &mut changed);

        assert!(changed);
        assert!(lines.iter().any(|l| l.contains("COMPRESS=zstd")));
    }

    #[test]
    fn test_update_config_value_modifies_existing() {
        let mut lines = vec!["MODULES=most".to_string(), "COMPRESS=gzip".to_string()];
        let mut changed = false;
        update_config_value(&mut lines, "COMPRESS", "zstd", &mut changed);

        assert!(changed);
        assert!(lines.iter().any(|l| l == "COMPRESS=zstd"));
    }

    #[test]
    fn test_update_config_value_no_change_when_same() {
        let mut lines = vec!["MODULES=most".to_string()];
        let mut changed = false;
        update_config_value(&mut lines, "MODULES", "most", &mut changed);

        assert!(!changed);
    }

    fn configure_initramfs_conf_at_path(
        config: &serde_json::Map<String, serde_json::Value>,
        check_mode: bool,
        path: &Path,
    ) -> Result<bool> {
        let lines = read_file_lines(path);
        let original = lines.join("\n");

        let mut changed = false;
        let mut new_lines = lines.clone();

        for (key, value) in config {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            update_config_value(&mut new_lines, key, &value_str, &mut changed);
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

        Ok(changed)
    }

    #[test]
    fn test_configure_initramfs_conf() {
        let dir = tempdir().unwrap();
        let conf_path = dir.path().join("initramfs.conf");

        let mut config = serde_json::Map::new();
        config.insert(
            "COMPRESS".to_string(),
            serde_json::Value::String("zstd".to_string()),
        );

        let result = configure_initramfs_conf_at_path(&config, false, &conf_path).unwrap();
        assert!(result);

        let content = fs::read_to_string(&conf_path).unwrap();
        assert!(content.contains("COMPRESS=zstd"));
    }

    fn configure_modules_at_path(
        modules: &[String],
        check_mode: bool,
        path: &Path,
    ) -> Result<bool> {
        let lines = read_file_lines(path);
        let original = lines.join("\n");

        let mut changed = false;
        let mut new_lines = lines.clone();

        for module in modules {
            let module_exists = new_lines.iter().any(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with('#') && trimmed == module
            });

            if !module_exists {
                if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    new_lines.push(String::new());
                }
                new_lines.push(module.clone());
                changed = true;
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

        Ok(changed)
    }

    #[test]
    fn test_configure_modules() {
        let dir = tempdir().unwrap();
        let modules_path = dir.path().join("modules");

        let modules = vec!["zfs".to_string(), "spl".to_string()];
        let result = configure_modules_at_path(&modules, false, &modules_path).unwrap();
        assert!(result);

        let content = fs::read_to_string(&modules_path).unwrap();
        assert!(content.contains("zfs"));
        assert!(content.contains("spl"));
    }

    #[test]
    fn test_configure_modules_idempotent() {
        let dir = tempdir().unwrap();
        let modules_path = dir.path().join("modules");
        fs::write(&modules_path, "zfs\n").unwrap();

        let modules = vec!["zfs".to_string()];
        let result = configure_modules_at_path(&modules, false, &modules_path).unwrap();
        assert!(!result);
    }

    fn configure_hooks_at_path(
        hooks: &[String],
        hooks_absent: &[String],
        check_mode: bool,
        hooks_dir: &Path,
    ) -> Result<bool> {
        let mut changed = false;

        if (!hooks.is_empty() || !hooks_absent.is_empty()) && !check_mode && !hooks_dir.exists() {
            fs::create_dir_all(hooks_dir)?;
        }

        for hook in hooks {
            let hook_path = hooks_dir.join(hook);
            if !hook_path.exists() {
                if !check_mode {
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&hook_path)?;
                    writeln!(file, "#!/bin/sh\n# Initramfs hook: {hook}\nexit 0")?;
                    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
                }
                changed = true;
            }
        }

        for hook in hooks_absent {
            let hook_path = hooks_dir.join(hook);
            if hook_path.exists() {
                if !check_mode {
                    fs::remove_file(&hook_path)?;
                }
                changed = true;
            }
        }

        Ok(changed)
    }

    #[test]
    fn test_configure_hooks_add() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");

        let hooks = vec!["zfs".to_string()];
        let result = configure_hooks_at_path(&hooks, &[], false, &hooks_dir).unwrap();
        assert!(result);

        let hook_path = hooks_dir.join("zfs");
        assert!(hook_path.exists());
    }

    #[test]
    fn test_configure_hooks_remove() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let hook_path = hooks_dir.join("zfs");
        fs::write(&hook_path, "#!/bin/sh\nexit 0").unwrap();

        let hooks_absent = vec!["zfs".to_string()];
        let result = configure_hooks_at_path(&[], &hooks_absent, false, &hooks_dir).unwrap();
        assert!(result);
        assert!(!hook_path.exists());
    }

    #[test]
    fn test_configure_hooks_check_mode() {
        let dir = tempdir().unwrap();
        let hooks_dir = dir.path().join("hooks");

        let hooks = vec!["zfs".to_string()];
        let result = configure_hooks_at_path(&hooks, &[], true, &hooks_dir).unwrap();
        assert!(result);
        assert!(!hooks_dir.exists());
    }
}
