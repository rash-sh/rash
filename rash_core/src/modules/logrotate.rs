/// ANCHOR: module
/// # logrotate
///
/// Manage log rotation configurations.
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
/// - name: Configure log rotation for app
///   logrotate:
///     path: /var/log/app.log
///     frequency: daily
///     rotate: 7
///     compress: true
///     missingok: true
///     notifempty: true
///
/// - name: Configure log rotation for multiple log files
///   logrotate:
///     path:
///       - /var/log/app1.log
///       - /var/log/app2.log
///     frequency: weekly
///     rotate: 4
///     compress: true
///     delaycompress: true
///
/// - name: Configure log rotation with size limit
///   logrotate:
///     path: /var/log/large-app.log
///     size: 100M
///     rotate: 5
///     compress: true
///
/// - name: Remove log rotation configuration
///   logrotate:
///     path: /var/log/old-app.log
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::Result;
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs;
use std::path::Path;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_norway::Value as YamlValue;
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

fn default_state() -> Option<State> {
    Some(State::Present)
}

fn default_frequency() -> Option<Frequency> {
    Some(Frequency::Daily)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Path to log file(s). Can be a single path or a list of paths.
    #[serde(alias = "name")]
    pub path: PathSpec,
    /// Whether the configuration should be present or absent.
    /// **[default: `"present"`]**
    #[serde(default = "default_state")]
    pub state: Option<State>,
    /// How often to rotate logs.
    /// **[default: `"daily"`]**
    #[serde(default = "default_frequency")]
    pub frequency: Option<Frequency>,
    /// Number of rotated files to keep.
    pub rotate: Option<u32>,
    /// Compress rotated log files.
    /// **[default: `false`]**
    #[serde(default)]
    pub compress: bool,
    /// Delay compression until the next rotation cycle.
    /// **[default: `false`]**
    #[serde(default)]
    pub delaycompress: bool,
    /// Don't report errors if log file is missing.
    /// **[default: `false`]**
    #[serde(default)]
    pub missingok: bool,
    /// Don't rotate empty log files.
    /// **[default: `false`]**
    #[serde(default)]
    pub notifempty: bool,
    /// Create new log file after rotation with specified permissions.
    /// Format: mode owner group (e.g., "0644 root root").
    pub create: Option<String>,
    /// Rotate when file exceeds this size (e.g., "100M", "1G").
    pub size: Option<String>,
    /// Use date as suffix for rotated files.
    /// **[default: `false`]**
    #[serde(default)]
    pub dateext: bool,
    /// Format for date suffix (strftime format).
    pub dateformat: Option<String>,
    /// Copy log file before truncating instead of moving.
    /// **[default: `false`]**
    #[serde(default)]
    pub copy: bool,
    /// Truncate original log file in place instead of moving.
    /// **[default: `false`]**
    #[serde(default)]
    pub copytruncate: bool,
    /// Don't rotate if sharedscripts is set and the script fails.
    /// **[default: `false`]**
    #[serde(default)]
    pub sharedscripts: bool,
    /// Script to run before rotation.
    pub prerotate: Option<String>,
    /// Script to run after rotation.
    pub postrotate: Option<String>,
    /// Execute prerotate/postrotate scripts only once for all matched files.
    /// **[default: `false`]**
    #[serde(default)]
    pub shared_scripts: bool,
    /// Custom configuration file path (default: /etc/logrotate.d/<name>).
    pub config_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    Present,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Frequency {
    fn to_logrotate_string(&self) -> &'static str {
        match self {
            Frequency::Daily => "daily",
            Frequency::Weekly => "weekly",
            Frequency::Monthly => "monthly",
            Frequency::Yearly => "yearly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema))]
#[serde(untagged)]
pub enum PathSpec {
    Single(String),
    Multiple(Vec<String>),
}

impl PathSpec {
    fn to_paths(&self) -> Vec<String> {
        match self {
            PathSpec::Single(p) => vec![p.clone()],
            PathSpec::Multiple(paths) => paths.clone(),
        }
    }

    fn to_config_name(&self) -> String {
        match self {
            PathSpec::Single(p) => Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("logrotate")
                .to_string(),
            PathSpec::Multiple(paths) => {
                if paths.is_empty() {
                    return "logrotate".to_string();
                }
                Path::new(&paths[0])
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("logrotate")
                    .to_string()
            }
        }
    }
}

fn get_config_path(config_file: &Option<String>, name: &str) -> String {
    if let Ok(test_file) = std::env::var("RASH_TEST_LOGROTATE_FILE") {
        return test_file;
    }

    if let Some(file) = config_file {
        if Path::new(file).is_absolute() {
            file.clone()
        } else {
            format!("/etc/logrotate.d/{}", file)
        }
    } else {
        format!("/etc/logrotate.d/{}", name)
    }
}

fn build_config_content(params: &Params) -> String {
    let mut content = String::new();

    let paths = params.path.to_paths();
    for path in &paths {
        content.push_str(path);
        content.push('\n');
    }

    content.push_str("{\n");

    if let Some(ref freq) = params.frequency {
        content.push_str(&format!("  {}\n", freq.to_logrotate_string()));
    }

    if let Some(rotate) = params.rotate {
        content.push_str(&format!("  rotate {}\n", rotate));
    }

    if let Some(ref size) = params.size {
        content.push_str(&format!("  size {}\n", size));
    }

    if params.compress {
        content.push_str("  compress\n");
    }

    if params.delaycompress {
        content.push_str("  delaycompress\n");
    }

    if params.missingok {
        content.push_str("  missingok\n");
    }

    if params.notifempty {
        content.push_str("  notifempty\n");
    }

    if let Some(ref create) = params.create {
        content.push_str(&format!("  create {}\n", create));
    }

    if params.dateext {
        content.push_str("  dateext\n");
    }

    if let Some(ref dateformat) = params.dateformat {
        content.push_str(&format!("  dateformat {}\n", dateformat));
    }

    if params.copy {
        content.push_str("  copy\n");
    }

    if params.copytruncate {
        content.push_str("  copytruncate\n");
    }

    if params.sharedscripts || params.shared_scripts {
        content.push_str("  sharedscripts\n");
    }

    if let Some(ref prerotate) = params.prerotate {
        content.push_str("  prerotate\n");
        content.push_str("    ");
        content.push_str(prerotate);
        content.push('\n');
        content.push_str("  endscript\n");
    }

    if let Some(ref postrotate) = params.postrotate {
        content.push_str("  postrotate\n");
        content.push_str("    ");
        content.push_str(postrotate);
        content.push('\n');
        content.push_str("  endscript\n");
    }

    content.push_str("}\n");

    content
}

fn normalize_content(content: &str) -> String {
    content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn logrotate(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or(State::Present);
    let config_name = params.path.to_config_name();
    let config_path = get_config_path(&params.config_file, &config_name);
    let path = Path::new(&config_path);

    let original_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let changed = match state {
        State::Present => {
            let new_content = build_config_content(&params);
            let normalized_original = normalize_content(&original_content);
            let normalized_new = normalize_content(&new_content);

            if normalized_original != normalized_new {
                diff(&original_content, &new_content);

                if !check_mode {
                    if let Some(parent) = path.parent()
                        && !parent.exists()
                    {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(path, &new_content)?;
                }
                true
            } else {
                false
            }
        }
        State::Absent => {
            if path.exists() {
                diff(&original_content, "");

                if !check_mode {
                    fs::remove_file(path)?;
                }
                true
            } else {
                false
            }
        }
    };

    let paths = params.path.to_paths();
    let output = paths.join(", ");

    Ok(ModuleResult {
        changed,
        output: Some(output),
        extra: None,
    })
}

#[derive(Debug)]
pub struct Logrotate;

impl Module for Logrotate {
    fn get_name(&self) -> &str {
        "logrotate"
    }

    fn exec(
        &self,
        _global_params: &GlobalParams,
        params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((logrotate(parse_params(params)?, check_mode)?, None))
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        Some(Params::get_json_schema())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_params_single_path() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /var/log/app.log
            frequency: daily
            rotate: 7
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.path,
            PathSpec::Single("/var/log/app.log".to_string())
        );
        assert_eq!(params.frequency, Some(Frequency::Daily));
        assert_eq!(params.rotate, Some(7));
    }

    #[test]
    fn test_parse_params_multiple_paths() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path:
              - /var/log/app1.log
              - /var/log/app2.log
            frequency: weekly
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(
            params.path,
            PathSpec::Multiple(vec![
                "/var/log/app1.log".to_string(),
                "/var/log/app2.log".to_string()
            ])
        );
    }

    #[test]
    fn test_parse_params_with_all_options() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            path: /var/log/app.log
            frequency: daily
            rotate: 7
            compress: true
            delaycompress: true
            missingok: true
            notifempty: true
            create: "0644 root root"
            size: 100M
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.compress);
        assert!(params.delaycompress);
        assert!(params.missingok);
        assert!(params.notifempty);
        assert_eq!(params.create, Some("0644 root root".to_string()));
        assert_eq!(params.size, Some("100M".to_string()));
    }

    #[test]
    fn test_build_config_content() {
        let params = Params {
            path: PathSpec::Single("/var/log/app.log".to_string()),
            state: Some(State::Present),
            frequency: Some(Frequency::Daily),
            rotate: Some(7),
            compress: true,
            delaycompress: false,
            missingok: true,
            notifempty: true,
            create: None,
            size: None,
            dateext: false,
            dateformat: None,
            copy: false,
            copytruncate: false,
            sharedscripts: false,
            prerotate: None,
            postrotate: None,
            shared_scripts: false,
            config_file: None,
        };
        let content = build_config_content(&params);
        assert!(content.contains("/var/log/app.log"));
        assert!(content.contains("daily"));
        assert!(content.contains("rotate 7"));
        assert!(content.contains("compress"));
        assert!(content.contains("missingok"));
        assert!(content.contains("notifempty"));
    }

    #[test]
    fn test_build_config_with_scripts() {
        let params = Params {
            path: PathSpec::Single("/var/log/app.log".to_string()),
            state: Some(State::Present),
            frequency: Some(Frequency::Weekly),
            rotate: Some(4),
            compress: false,
            delaycompress: false,
            missingok: false,
            notifempty: false,
            create: None,
            size: None,
            dateext: false,
            dateformat: None,
            copy: false,
            copytruncate: false,
            sharedscripts: true,
            prerotate: Some("/usr/bin/test-prerotate.sh".to_string()),
            postrotate: Some("/usr/bin/test-postrotate.sh".to_string()),
            shared_scripts: false,
            config_file: None,
        };
        let content = build_config_content(&params);
        assert!(content.contains("prerotate"));
        assert!(content.contains("/usr/bin/test-prerotate.sh"));
        assert!(content.contains("postrotate"));
        assert!(content.contains("/usr/bin/test-postrotate.sh"));
        assert!(content.contains("sharedscripts"));
    }

    #[test]
    fn test_frequency_to_string() {
        assert_eq!(Frequency::Daily.to_logrotate_string(), "daily");
        assert_eq!(Frequency::Weekly.to_logrotate_string(), "weekly");
        assert_eq!(Frequency::Monthly.to_logrotate_string(), "monthly");
        assert_eq!(Frequency::Yearly.to_logrotate_string(), "yearly");
    }

    #[test]
    fn test_path_spec_to_config_name() {
        let single = PathSpec::Single("/var/log/app.log".to_string());
        assert_eq!(single.to_config_name(), "app.log");

        let multiple = PathSpec::Multiple(vec![
            "/var/log/app1.log".to_string(),
            "/var/log/app2.log".to_string(),
        ]);
        assert_eq!(multiple.to_config_name(), "app1.log");
    }

    #[test]
    fn test_get_config_path_default() {
        let path = get_config_path(&None, "app");
        assert_eq!(path, "/etc/logrotate.d/app");
    }

    #[test]
    fn test_get_config_path_custom_absolute() {
        let path = get_config_path(&Some("/opt/logrotate.d/myapp".to_string()), "app");
        assert_eq!(path, "/opt/logrotate.d/myapp");
    }

    #[test]
    fn test_get_config_path_custom_relative() {
        let path = get_config_path(&Some("myapp".to_string()), "app");
        assert_eq!(path, "/etc/logrotate.d/myapp");
    }

    #[test]
    fn test_normalize_content() {
        let content = "  daily  \n  \n  rotate 7  \n";
        let normalized = normalize_content(content);
        assert_eq!(normalized, "daily\nrotate 7");
    }
}
