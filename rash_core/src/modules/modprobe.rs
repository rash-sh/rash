/// ANCHOR: module
/// # modprobe
///
/// Load or unload kernel modules.
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
/// - name: Load overlay module for Docker
///   modprobe:
///     name: overlay
///     state: present
///
/// - name: Load br_netfilter with parameters
///   modprobe:
///     name: br_netfilter
///     params: nf_conntrack_brnetfilter=1
///     state: present
///
/// - name: Ensure wireguard is loaded at boot
///   modprobe:
///     name: wireguard
///     state: present
///     persistent: present
///
/// - name: Unload a module
///   modprobe:
///     name: dummy
///     state: absent
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
use std::path::Path;
use std::process::Command;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

const MODULES_LOAD_DIR: &str = "/etc/modules-load.d";
const MODPROBE_D_DIR: &str = "/etc/modprobe.d";

#[derive(Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Name of kernel module to manage.
    pub name: String,
    /// Module parameters.
    /// **[default: `""`]**
    pub params: Option<String>,
    /// Whether the module should be present or absent.
    /// **[default: `"present"`]**
    pub state: Option<State>,
    /// Persistency between reboots for configured module.
    /// Creates files in /etc/modules-load.d/ and /etc/modprobe.d/.
    /// **[default: `"disabled"`]**
    pub persistent: Option<Persistent>,
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
pub enum Persistent {
    #[default]
    Disabled,
    Absent,
    Present,
}

fn is_module_loaded(name: &str) -> Result<bool> {
    let content = fs::read_to_string("/proc/modules").map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to read /proc/modules: {e}"),
        )
    })?;

    Ok(content
        .lines()
        .any(|line| line.starts_with(&format!("{name} "))))
}

fn load_module(name: &str, params: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("modprobe");
    cmd.arg(name);

    if let Some(p) = params
        && !p.is_empty()
    {
        for param in p.split_whitespace() {
            cmd.arg(param);
        }
    }

    let output = cmd.output().map_err(|e| {
        Error::new(
            ErrorKind::SubprocessFail,
            format!("Failed to execute modprobe: {e}"),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "modprobe {} failed: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
}

fn unload_module(name: &str) -> Result<()> {
    let output = Command::new("modprobe")
        .args(["-r", name])
        .output()
        .map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to execute modprobe -r: {e}"),
            )
        })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::SubprocessFail,
            format!(
                "modprobe -r {} failed: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    Ok(())
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

fn find_module_in_lines(lines: &[String], module_name: &str) -> Option<usize> {
    lines.iter().position(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed == module_name
    })
}

fn find_params_in_lines(lines: &[String], module_name: &str) -> Option<usize> {
    let prefix = format!("options {module_name} ");
    lines.iter().position(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.starts_with(&prefix)
    })
}

fn update_modules_load_file(
    module_name: &str,
    persistent: &Persistent,
    check_mode: bool,
) -> Result<bool> {
    let path = Path::new(MODULES_LOAD_DIR).join("rash.conf");
    let lines = read_file_lines(&path);
    let original = lines.join("\n");

    let mut changed = false;
    let mut new_lines = lines.clone();

    match persistent {
        Persistent::Present => {
            if find_module_in_lines(&lines, module_name).is_none() {
                if !new_lines.is_empty() && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                {
                    new_lines.push(String::new());
                }
                new_lines.push(module_name.to_string());
                changed = true;
            }
        }
        Persistent::Absent => {
            if let Some(idx) = find_module_in_lines(&lines, module_name) {
                new_lines.remove(idx);
                changed = true;
            }
        }
        Persistent::Disabled => {}
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
            .open(&path)?;
        write!(file, "{new_content}")?;
    }

    Ok(changed)
}

fn update_modprobe_d_file(
    module_name: &str,
    params: Option<&str>,
    persistent: &Persistent,
    check_mode: bool,
) -> Result<bool> {
    let path = Path::new(MODPROBE_D_DIR).join("rash.conf");
    let lines = read_file_lines(&path);
    let original = lines.join("\n");

    let mut changed = false;
    let mut new_lines = lines.clone();

    match persistent {
        Persistent::Present => {
            if let Some(p) = params
                && !p.is_empty()
            {
                let new_entry = format!("options {module_name} {p}");
                if let Some(idx) = find_params_in_lines(&new_lines, module_name) {
                    if new_lines[idx].trim() != new_entry {
                        new_lines[idx] = new_entry;
                        changed = true;
                    }
                } else {
                    if !new_lines.is_empty()
                        && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                    {
                        new_lines.push(String::new());
                    }
                    new_lines.push(new_entry);
                    changed = true;
                }
            }
        }
        Persistent::Absent => {
            if let Some(idx) = find_params_in_lines(&new_lines, module_name) {
                new_lines.remove(idx);
                changed = true;
            }
        }
        Persistent::Disabled => {}
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
            .open(&path)?;
        write!(file, "{new_content}")?;
    }

    Ok(changed)
}

pub fn modprobe(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.unwrap_or_default();
    let persistent = params.persistent.unwrap_or_default();
    let module_params = params.params.as_deref().unwrap_or("");

    let mut changed = false;
    let is_loaded = is_module_loaded(&params.name)?;

    match state {
        State::Present => {
            if !is_loaded {
                if !check_mode {
                    load_module(&params.name, Some(module_params))?;
                }
                changed = true;
            }
        }
        State::Absent => {
            if is_loaded {
                if !check_mode {
                    unload_module(&params.name)?;
                }
                changed = true;
            }
        }
    }

    if persistent != Persistent::Disabled {
        let load_changed = update_modules_load_file(&params.name, &persistent, check_mode)?;
        let modprobe_changed =
            update_modprobe_d_file(&params.name, Some(module_params), &persistent, check_mode)?;
        changed = changed || load_changed || modprobe_changed;
    }

    Ok(ModuleResult::new(changed, None, Some(params.name)))
}

#[derive(Debug)]
pub struct Modprobe;

impl Module for Modprobe {
    fn get_name(&self) -> &str {
        "modprobe"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((modprobe(parse_params(optional_params)?, check_mode)?, None))
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
            name: overlay
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params,
            Params {
                name: "overlay".to_owned(),
                params: None,
                state: Some(State::Present),
                persistent: None,
            }
        );
    }

    #[test]
    fn test_parse_params_with_params() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: br_netfilter
            params: nf_conntrack_brnetfilter=1
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.name, "br_netfilter");
        assert_eq!(params.params, Some("nf_conntrack_brnetfilter=1".to_owned()));
    }

    #[test]
    fn test_parse_params_persistent() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            name: wireguard
            state: present
            persistent: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.persistent, Some(Persistent::Present));
    }

    #[test]
    fn test_find_module_in_lines() {
        let lines = vec![
            "# Comment".to_string(),
            "overlay".to_string(),
            "br_netfilter".to_string(),
        ];
        assert_eq!(find_module_in_lines(&lines, "overlay"), Some(1));
        assert_eq!(find_module_in_lines(&lines, "br_netfilter"), Some(2));
        assert_eq!(find_module_in_lines(&lines, "dummy"), None);
    }

    #[test]
    fn test_find_module_in_lines_ignores_commented() {
        let lines = vec!["#overlay".to_string(), "overlay".to_string()];
        assert_eq!(find_module_in_lines(&lines, "overlay"), Some(1));
    }

    #[test]
    fn test_find_params_in_lines() {
        let lines = vec![
            "# Comment".to_string(),
            "options br_netfilter nf_conntrack_brnetfilter=1".to_string(),
            "options dummy numdummies=2".to_string(),
        ];
        assert_eq!(find_params_in_lines(&lines, "br_netfilter"), Some(1));
        assert_eq!(find_params_in_lines(&lines, "dummy"), Some(2));
        assert_eq!(find_params_in_lines(&lines, "overlay"), None);
    }

    #[test]
    fn test_update_modules_load_file_add() {
        let dir = tempdir().unwrap();
        let test_path = dir.path().join("modules-load.d").join("rash.conf");

        let result =
            update_modules_load_file_at_path("overlay", &Persistent::Present, true, &test_path)
                .unwrap();
        assert!(result);
    }

    #[test]
    fn test_update_modules_load_file_no_change() {
        let dir = tempdir().unwrap();
        let modules_load_dir = dir.path().join("modules-load.d");
        fs::create_dir_all(&modules_load_dir).unwrap();
        let test_path = modules_load_dir.join("rash.conf");
        fs::write(&test_path, "overlay\n").unwrap();

        let result =
            update_modules_load_file_at_path("overlay", &Persistent::Present, true, &test_path)
                .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_update_modules_load_file_remove() {
        let dir = tempdir().unwrap();
        let modules_load_dir = dir.path().join("modules-load.d");
        fs::create_dir_all(&modules_load_dir).unwrap();
        let test_path = modules_load_dir.join("rash.conf");
        fs::write(&test_path, "overlay\ndummy\n").unwrap();

        let result =
            update_modules_load_file_at_path("overlay", &Persistent::Absent, true, &test_path)
                .unwrap();
        assert!(result);
    }

    fn update_modules_load_file_at_path(
        module_name: &str,
        persistent: &Persistent,
        check_mode: bool,
        path: &Path,
    ) -> Result<bool> {
        let lines = read_file_lines(path);
        let original = lines.join("\n");

        let mut changed = false;
        let mut new_lines = lines.clone();

        match persistent {
            Persistent::Present => {
                if find_module_in_lines(&lines, module_name).is_none() {
                    if !new_lines.is_empty()
                        && !new_lines.last().map(|l| l.is_empty()).unwrap_or(true)
                    {
                        new_lines.push(String::new());
                    }
                    new_lines.push(module_name.to_string());
                    changed = true;
                }
            }
            Persistent::Absent => {
                if let Some(idx) = find_module_in_lines(&lines, module_name) {
                    new_lines.remove(idx);
                    changed = true;
                }
            }
            Persistent::Disabled => {}
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
}
