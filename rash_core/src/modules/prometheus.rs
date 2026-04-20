/// ANCHOR: module
/// # prometheus
///
/// Manage Prometheus monitoring configuration, including targets, alert rules, and scrape configs.
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
/// - prometheus:
///     action: add
///     targets:
///       - job_name: node
///         static_configs:
///           - targets: ['localhost:9100']
///     reload: true
///
/// - prometheus:
///     action: add
///     targets:
///       - job_name: prometheus
///         static_configs:
///           - targets: ['localhost:9090']
///     config_file: /etc/prometheus/prometheus.yml
///
/// - prometheus:
///     action: remove
///     targets:
///       - job_name: node
///
/// - prometheus:
///     action: update
///     targets:
///       - job_name: node
///         scrape_interval: 15s
///         static_configs:
///           - targets: ['localhost:9100', 'localhost:9101']
///
/// - prometheus:
///     action: add
///     alert_rules:
///       groups:
///         - name: example
///           rules:
///             - alert: HighRequestLatency
///               expr: job:request_latency_seconds:mean5m{job="myjob"} > 0.5
///               for: 10m
///               labels:
///                 severity: page
///               annotations:
///                 summary: High request latency
///
/// - prometheus:
///     action: get
///
/// - prometheus:
///     action: add
///     targets:
///       - job_name: node
///         static_configs:
///           - targets: ['localhost:9100']
///     alert_rules:
///       groups:
///         - name: node_alerts
///           rules:
///             - alert: NodeDown
///               expr: up == 0
///               for: 5m
///     reload: true
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
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
use serde_norway::{Mapping, Value as YamlValue};
#[cfg(feature = "docs")]
use strum_macros::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Get,
    Add,
    Remove,
    Update,
}

fn default_config_file() -> String {
    "/etc/prometheus/prometheus.yml".to_owned()
}

fn default_reload() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "docs", derive(JsonSchema, DocJsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Action to perform on the Prometheus configuration.
    /// **[default: `"get"`]**
    #[serde(default)]
    pub action: Action,
    /// List of scrape target configurations. Each entry should be a scrape job
    /// with at least a `job_name` key.
    pub targets: Option<Vec<YamlValue>>,
    /// Alert rule configuration in Prometheus alerting rule format.
    pub alert_rules: Option<YamlValue>,
    /// Path to the main Prometheus configuration file.
    /// **[default: `"/etc/prometheus/prometheus.yml"`]**
    #[serde(default = "default_config_file")]
    pub config_file: String,
    /// Path to the alert rules file. If not set, alert rules are written to
    /// a `rules` subdirectory next to the config file.
    pub alert_rules_file: Option<String>,
    /// Reload Prometheus after changes by sending SIGHUP.
    /// **[default: `true`]**
    #[serde(default = "default_reload")]
    pub reload: bool,
}

fn read_yaml_file(path: &Path) -> Result<YamlValue> {
    if !path.exists() {
        return Ok(YamlValue::Mapping(Mapping::new()));
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(YamlValue::Mapping(Mapping::new()));
    }
    serde_norway::from_str(&content).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

fn write_yaml_file(path: &Path, value: &YamlValue) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    let content = serde_norway::to_string(value)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    writeln!(file, "{content}")?;
    Ok(())
}

fn get_scrape_jobs(config: &YamlValue) -> Vec<YamlValue> {
    match config {
        YamlValue::Mapping(map) => match map.get(YamlValue::String("scrape_configs".to_owned())) {
            Some(YamlValue::Sequence(seq)) => seq.clone(),
            _ => vec![],
        },
        _ => vec![],
    }
}

fn set_scrape_jobs(config: &mut YamlValue, jobs: Vec<YamlValue>) {
    if let YamlValue::Mapping(map) = config {
        map.insert(
            YamlValue::String("scrape_configs".to_owned()),
            YamlValue::Sequence(jobs),
        );
    }
}

fn find_job_index(jobs: &[YamlValue], job_name: &str) -> Option<usize> {
    let key = YamlValue::String("job_name".to_owned());
    jobs.iter().position(|job| {
        let YamlValue::Mapping(map) = job else {
            return false;
        };
        let Some(YamlValue::String(name)) = map.get(&key) else {
            return false;
        };
        name == job_name
    })
}

fn get_job_name(job: &YamlValue) -> Option<String> {
    let YamlValue::Mapping(map) = job else {
        return None;
    };
    let key = YamlValue::String("job_name".to_owned());
    let Some(YamlValue::String(name)) = map.get(&key) else {
        return None;
    };
    Some(name.clone())
}

fn extract_job_names(targets: &[YamlValue]) -> Vec<String> {
    let key = YamlValue::String("job_name".to_owned());
    targets
        .iter()
        .filter_map(|t| {
            let YamlValue::Mapping(map) = t else {
                return None;
            };
            let Some(YamlValue::String(name)) = map.get(&key) else {
                return None;
            };
            Some(name.clone())
        })
        .collect()
}

fn reload_prometheus() -> Result<()> {
    let pid_path = "/var/run/prometheus.pid";
    if Path::new(pid_path).exists() {
        let pid_str = fs::read_to_string(pid_path)?;
        let pid: u32 = pid_str
            .trim()
            .parse()
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        unsafe {
            if libc::kill(pid as i32, libc::SIGHUP) != 0 {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Failed to send SIGHUP to Prometheus",
                ));
            }
        }
    }
    Ok(())
}

fn determine_alert_rules_path(params: &Params) -> String {
    match &params.alert_rules_file {
        Some(path) => path.clone(),
        None => {
            let config_dir = Path::new(&params.config_file)
                .parent()
                .unwrap_or(Path::new("/etc/prometheus"));
            config_dir
                .join("rules")
                .join("alert_rules.yml")
                .to_str()
                .unwrap_or("/etc/prometheus/rules/alert_rules.yml")
                .to_owned()
        }
    }
}

fn exec_get(params: Params) -> Result<ModuleResult> {
    let path = Path::new(&params.config_file);
    let config = read_yaml_file(path)?;

    let jobs = get_scrape_jobs(&config);
    let mut extra = serde_norway::Mapping::new();
    extra.insert(
        YamlValue::String("scrape_configs".to_owned()),
        YamlValue::Sequence(jobs),
    );

    if params.alert_rules_file.is_some() || params.alert_rules.is_some() {
        let alert_path = determine_alert_rules_path(&params);
        let alert_config = read_yaml_file(Path::new(&alert_path))?;
        extra.insert(YamlValue::String("alert_rules".to_owned()), alert_config);
    }

    Ok(ModuleResult {
        changed: false,
        output: Some(params.config_file.clone()),
        extra: Some(YamlValue::Mapping(extra)),
    })
}

fn exec_add(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path = Path::new(&params.config_file);
    let mut config = read_yaml_file(path)?;
    let original_content = fs::read_to_string(path).unwrap_or_default();
    let mut changed = false;

    if let Some(targets) = &params.targets {
        let mut jobs = get_scrape_jobs(&config);
        for target in targets {
            let job_name = get_job_name(target).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Each target must have a job_name field",
                )
            })?;

            match find_job_index(&jobs, &job_name) {
                Some(idx) => {
                    if jobs[idx] != *target {
                        jobs[idx] = target.clone();
                        changed = true;
                    }
                }
                None => {
                    jobs.push(target.clone());
                    changed = true;
                }
            }
        }
        if changed {
            set_scrape_jobs(&mut config, jobs);
        }
    }

    let mut alert_changed = false;
    if let Some(alert_rules) = &params.alert_rules {
        let alert_path = determine_alert_rules_path(&params);
        let alert_path_ref = Path::new(&alert_path);
        let mut alert_config = read_yaml_file(alert_path_ref)?;
        let original_alert_content = fs::read_to_string(alert_path_ref).unwrap_or_default();

        merge_alert_rules(&mut alert_config, alert_rules);

        if alert_config != read_yaml_file(alert_path_ref)? {
            alert_changed = true;
            if !check_mode {
                diff(&original_alert_content, &serde_norway::to_string(&alert_config)?);
                write_yaml_file(alert_path_ref, &alert_config)?;
            }
        }
    }

    if changed {
        let new_content = serde_norway::to_string(&config)?;
        diff(&original_content, &new_content);

        if !check_mode {
            write_yaml_file(path, &config)?;
        }
    }

    let any_changed = changed || alert_changed;

    if any_changed && !check_mode && params.reload {
        let _ = reload_prometheus();
    }

    Ok(ModuleResult {
        changed: any_changed,
        output: Some(params.config_file.clone()),
        extra: None,
    })
}

fn merge_alert_rules(config: &mut YamlValue, new_rules: &YamlValue) {
    let YamlValue::Mapping(config_map) = config else {
        return;
    };
    let YamlValue::Mapping(new_map) = new_rules else {
        return;
    };
    let Some(YamlValue::Sequence(new_groups)) =
        new_map.get(YamlValue::String("groups".to_owned()))
    else {
        return;
    };

    let existing_groups = config_map
        .entry(YamlValue::String("groups".to_owned()))
        .or_insert_with(|| YamlValue::Sequence(vec![]));

    let YamlValue::Sequence(groups) = existing_groups else {
        return;
    };

    let name_key = YamlValue::String("name".to_owned());
    for new_group in new_groups {
        let YamlValue::Mapping(new_gm) = new_group else {
            continue;
        };
        let Some(YamlValue::String(name)) = new_gm.get(&name_key) else {
            continue;
        };

        let existing_idx = groups.iter().position(|g| {
            let YamlValue::Mapping(gm) = g else {
                return false;
            };
            let Some(YamlValue::String(n)) = gm.get(&name_key) else {
                return false;
            };
            n == name
        });

        match existing_idx {
            Some(idx) => {
                groups[idx] = new_group.clone();
            }
            None => {
                groups.push(new_group.clone());
            }
        }
    }
}

fn exec_remove(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path = Path::new(&params.config_file);
    let mut config = read_yaml_file(path)?;
    let original_content = fs::read_to_string(path).unwrap_or_default();
    let mut changed = false;

    if let Some(targets) = &params.targets {
        let mut jobs = get_scrape_jobs(&config);
        let names_to_remove = extract_job_names(targets);

        let original_len = jobs.len();
        jobs.retain(|job| {
            if let Some(name) = get_job_name(job) {
                !names_to_remove.contains(&name)
            } else {
                true
            }
        });

        if jobs.len() != original_len {
            changed = true;
            set_scrape_jobs(&mut config, jobs);
        }
    }

    if let Some(alert_rules) = &params.alert_rules {
        let alert_path = determine_alert_rules_path(&params);
        let alert_path_ref = Path::new(&alert_path);
        let mut alert_config = read_yaml_file(alert_path_ref)?;

        if remove_alert_rules(&mut alert_config, alert_rules) {
            changed = true;
            if !check_mode {
                let original_alert = fs::read_to_string(alert_path_ref).unwrap_or_default();
                diff(&original_alert, &serde_norway::to_string(&alert_config)?);
                write_yaml_file(alert_path_ref, &alert_config)?;
            }
        }
    }

    if changed {
        let new_content = serde_norway::to_string(&config)?;
        diff(&original_content, &new_content);

        if !check_mode {
            write_yaml_file(path, &config)?;
        }
    }

    if changed && !check_mode && params.reload {
        let _ = reload_prometheus();
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.config_file.clone()),
        extra: None,
    })
}

fn remove_alert_rules(config: &mut YamlValue, rules_to_remove: &YamlValue) -> bool {
    let mut removed = false;
    let YamlValue::Mapping(config_map) = config else {
        return false;
    };
    let YamlValue::Mapping(remove_map) = rules_to_remove else {
        return false;
    };
    let Some(YamlValue::Sequence(remove_groups)) =
        remove_map.get(YamlValue::String("groups".to_owned()))
    else {
        return false;
    };

    let name_key = YamlValue::String("name".to_owned());
    let names_to_remove: Vec<String> = remove_groups
        .iter()
        .filter_map(|g| {
            let YamlValue::Mapping(m) = g else {
                return None;
            };
            let Some(YamlValue::String(name)) = m.get(&name_key) else {
                return None;
            };
            Some(name.clone())
        })
        .collect();

    if let Some(YamlValue::Sequence(groups)) =
        config_map.get_mut(YamlValue::String("groups".to_owned()))
    {
        let original_len = groups.len();
        groups.retain(|g| {
            let YamlValue::Mapping(m) = g else {
                return true;
            };
            let Some(YamlValue::String(name)) = m.get(&name_key) else {
                return true;
            };
            !names_to_remove.contains(name)
        });
        if groups.len() != original_len {
            removed = true;
        }
    }
    removed
}

fn exec_update(params: Params, check_mode: bool) -> Result<ModuleResult> {
    let path = Path::new(&params.config_file);
    let mut config = read_yaml_file(path)?;
    let original_content = fs::read_to_string(path).unwrap_or_default();
    let mut changed = false;

    if let Some(targets) = &params.targets {
        let mut jobs = get_scrape_jobs(&config);
        for target in targets {
            let job_name = get_job_name(target).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    "Each target must have a job_name field",
                )
            })?;

            match find_job_index(&jobs, &job_name) {
                Some(idx) => {
                    if let YamlValue::Mapping(existing_map) = &mut jobs[idx]
                        && let YamlValue::Mapping(new_map) = target
                    {
                        for (key, value) in new_map {
                            if key.as_str() == Some("job_name") {
                                continue;
                            }
                            let existing = existing_map.get(key);
                            if existing != Some(value) {
                                existing_map.insert(key.clone(), value.clone());
                                changed = true;
                            }
                        }
                    }
                }
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Job '{job_name}' not found in scrape_configs"),
                    ));
                }
            }
        }
        if changed {
            set_scrape_jobs(&mut config, jobs);
        }
    }

    if let Some(alert_rules) = &params.alert_rules {
        let alert_path = determine_alert_rules_path(&params);
        let alert_path_ref = Path::new(&alert_path);
        let mut alert_config = read_yaml_file(alert_path_ref)?;
        let original_alert = fs::read_to_string(alert_path_ref).unwrap_or_default();

        merge_alert_rules(&mut alert_config, alert_rules);

        let new_alert_content = serde_norway::to_string(&alert_config)?;
        if original_alert.trim() != new_alert_content.trim() {
            changed = true;
            if !check_mode {
                diff(&original_alert, &new_alert_content);
                write_yaml_file(alert_path_ref, &alert_config)?;
            }
        }
    }

    if changed {
        let new_content = serde_norway::to_string(&config)?;
        diff(&original_content, &new_content);

        if !check_mode {
            write_yaml_file(path, &config)?;
        }
    }

    if changed && !check_mode && params.reload {
        let _ = reload_prometheus();
    }

    Ok(ModuleResult {
        changed,
        output: Some(params.config_file.clone()),
        extra: None,
    })
}

fn prometheus(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    match params.action {
        Action::Get => exec_get(params),
        Action::Add => exec_add(params, check_mode),
        Action::Remove => exec_remove(params, check_mode),
        Action::Update => exec_update(params, check_mode),
    }
}

#[derive(Debug)]
pub struct Prometheus;

impl Module for Prometheus {
    fn get_name(&self) -> &str {
        "prometheus"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            prometheus(parse_params(optional_params)?, check_mode)?,
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
    use tempfile::tempdir;

    #[test]
    fn test_parse_params_defaults() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Get);
        assert_eq!(params.config_file, "/etc/prometheus/prometheus.yml");
        assert!(params.reload);
        assert!(params.targets.is_none());
        assert!(params.alert_rules.is_none());
    }

    #[test]
    fn test_parse_params_add_targets() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            targets:
              - job_name: node
                static_configs:
                  - targets: ['localhost:9100']
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.action, Action::Add);
        assert!(params.targets.is_some());
        let targets = params.targets.unwrap();
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn test_parse_params_custom_config_file() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: get
            config_file: /opt/prometheus/prometheus.yml
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.config_file, "/opt/prometheus/prometheus.yml");
    }

    #[test]
    fn test_parse_params_alert_rules() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            alert_rules:
              groups:
                - name: test
                  rules:
                    - alert: TestAlert
                      expr: up == 0
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(params.alert_rules.is_some());
    }

    #[test]
    fn test_parse_params_no_reload() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            action: add
            targets:
              - job_name: node
                static_configs:
                  - targets: ['localhost:9100']
            reload: false
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert!(!params.reload);
    }

    #[test]
    fn test_get_scrape_jobs() {
        let config: YamlValue = serde_norway::from_str(
            r#"
            global:
              scrape_interval: 15s
            scrape_configs:
              - job_name: prometheus
                static_configs:
                  - targets: ['localhost:9090']
              - job_name: node
                static_configs:
                  - targets: ['localhost:9100']
            "#,
        )
        .unwrap();
        let jobs = get_scrape_jobs(&config);
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn test_get_scrape_jobs_empty() {
        let config: YamlValue = serde_norway::from_str(
            r#"
            global:
              scrape_interval: 15s
            "#,
        )
        .unwrap();
        let jobs = get_scrape_jobs(&config);
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_find_job_index() {
        let jobs: Vec<YamlValue> = serde_norway::from_str(
            r#"
            - job_name: prometheus
              static_configs:
                - targets: ['localhost:9090']
            - job_name: node
              static_configs:
                - targets: ['localhost:9100']
            "#,
        )
        .unwrap();
        assert_eq!(find_job_index(&jobs, "prometheus"), Some(0));
        assert_eq!(find_job_index(&jobs, "node"), Some(1));
        assert_eq!(find_job_index(&jobs, "missing"), None);
    }

    #[test]
    fn test_extract_job_names() {
        let targets: Vec<YamlValue> = serde_norway::from_str(
            r#"
            - job_name: node
            - job_name: custom
            "#,
        )
        .unwrap();
        let names = extract_job_names(&targets);
        assert_eq!(names, vec!["node", "custom"]);
    }

    #[test]
    fn test_add_targets_new_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        let params = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: node
                static_configs:
                  - targets: ['localhost:9100']
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        let config: YamlValue = serde_norway::from_str(&content).unwrap();
        let jobs = get_scrape_jobs(&config);
        assert_eq!(jobs.len(), 1);
        assert_eq!(find_job_index(&jobs, "node"), Some(0));
    }

    #[test]
    fn test_add_targets_to_existing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"global:
  scrape_interval: 15s
scrape_configs:
  - job_name: prometheus
    static_configs:
      - targets: ['localhost:9090']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: node
                static_configs:
                  - targets: ['localhost:9100']
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        let config: YamlValue = serde_norway::from_str(&content).unwrap();
        let jobs = get_scrape_jobs(&config);
        assert_eq!(jobs.len(), 2);
        assert_eq!(find_job_index(&jobs, "prometheus"), Some(0));
        assert_eq!(find_job_index(&jobs, "node"), Some(1));
    }

    #[test]
    fn test_add_targets_idempotent() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        let target_yaml = r#"
        job_name: node
        static_configs:
          - targets: ['localhost:9100']
        "#;

        let params1 = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(target_yaml).unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result1 = prometheus(params1, false).unwrap();
        assert!(result1.changed);

        let params2 = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(target_yaml).unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result2 = prometheus(params2, false).unwrap();
        assert!(!result2.changed);
    }

    #[test]
    fn test_remove_targets() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: prometheus
    static_configs:
      - targets: ['localhost:9090']
  - job_name: node
    static_configs:
      - targets: ['localhost:9100']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Remove,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: node
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        let config: YamlValue = serde_norway::from_str(&content).unwrap();
        let jobs = get_scrape_jobs(&config);
        assert_eq!(jobs.len(), 1);
        assert_eq!(find_job_index(&jobs, "prometheus"), Some(0));
        assert_eq!(find_job_index(&jobs, "node"), None);
    }

    #[test]
    fn test_remove_targets_not_found() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: prometheus
    static_configs:
      - targets: ['localhost:9090']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Remove,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: nonexistent
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_update_targets() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: node
    scrape_interval: 30s
    static_configs:
      - targets: ['localhost:9100']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Update,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: node
                scrape_interval: 15s
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&config_path).unwrap();
        let config: YamlValue = serde_norway::from_str(&content).unwrap();
        let jobs = get_scrape_jobs(&config);
        let job = &jobs[0];
        if let YamlValue::Mapping(map) = job {
            if let Some(YamlValue::String(val)) =
                map.get(&YamlValue::String("scrape_interval".to_owned()))
            {
                assert_eq!(val, "15s");
            }
        }
    }

    #[test]
    fn test_update_targets_not_found() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: node
    static_configs:
      - targets: ['localhost:9100']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Update,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: nonexistent
                scrape_interval: 15s
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_action() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: node
    static_configs:
      - targets: ['localhost:9100']
"#,
        )
        .unwrap();

        let params = Params {
            action: Action::Get,
            targets: None,
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(!result.changed);
        assert!(result.extra.is_some());
    }

    #[test]
    fn test_check_mode() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        fs::write(
            &config_path,
            r#"scrape_configs:
  - job_name: node
    static_configs:
      - targets: ['localhost:9100']
"#,
        )
        .unwrap();
        let original = fs::read_to_string(&config_path).unwrap();

        let params = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(
                r#"
                job_name: prometheus
                static_configs:
                  - targets: ['localhost:9090']
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&config_path).unwrap();
        assert_eq!(original, content_after);
    }

    #[test]
    fn test_add_alert_rules() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");
        let alert_path = dir.path().join("rules").join("alert_rules.yml");

        let params = Params {
            action: Action::Add,
            targets: None,
            alert_rules: Some(serde_norway::from_str(
                r#"
                groups:
                  - name: test
                    rules:
                      - alert: HighLatency
                        expr: up == 0
                "#,
            )
            .unwrap()),
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: Some(alert_path.to_str().unwrap().to_owned()),
            reload: false,
        };

        let result = prometheus(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&alert_path).unwrap();
        let alert_config: YamlValue = serde_norway::from_str(&content).unwrap();
        if let YamlValue::Mapping(map) = &alert_config {
            if let Some(YamlValue::Sequence(groups)) =
                map.get(&YamlValue::String("groups".to_owned()))
            {
                assert_eq!(groups.len(), 1);
            } else {
                panic!("Expected groups sequence");
            }
        } else {
            panic!("Expected mapping");
        }
    }

    #[test]
    fn test_remove_alert_rules() {
        let dir = tempdir().unwrap();
        let alert_path = dir.path().join("alert_rules.yml");

        fs::write(
            &alert_path,
            r#"groups:
  - name: test
    rules:
      - alert: HighLatency
        expr: up == 0
  - name: other
    rules:
      - alert: OtherAlert
        expr: up == 1
"#,
        )
        .unwrap();

        let mut config = read_yaml_file(&alert_path).unwrap();
        let rules_to_remove: YamlValue = serde_norway::from_str(
            r#"
            groups:
              - name: test
            "#,
        )
        .unwrap();

        let removed = super::remove_alert_rules(&mut config, &rules_to_remove);
        assert!(removed);

        if let YamlValue::Mapping(map) = &config {
            if let Some(YamlValue::Sequence(groups)) =
                map.get(&YamlValue::String("groups".to_owned()))
            {
                assert_eq!(groups.len(), 1);
            }
        }
    }

    #[test]
    fn test_determine_alert_rules_path_default() {
        let params = Params {
            action: Action::Get,
            targets: None,
            alert_rules: None,
            config_file: "/etc/prometheus/prometheus.yml".to_owned(),
            alert_rules_file: None,
            reload: true,
        };
        assert_eq!(
            determine_alert_rules_path(&params),
            "/etc/prometheus/rules/alert_rules.yml"
        );
    }

    #[test]
    fn test_determine_alert_rules_path_custom() {
        let params = Params {
            action: Action::Get,
            targets: None,
            alert_rules: None,
            config_file: "/etc/prometheus/prometheus.yml".to_owned(),
            alert_rules_file: Some("/opt/prometheus/my_rules.yml".to_owned()),
            reload: true,
        };
        assert_eq!(
            determine_alert_rules_path(&params),
            "/opt/prometheus/my_rules.yml"
        );
    }

    #[test]
    fn test_add_targets_missing_job_name() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("prometheus.yml");

        let params = Params {
            action: Action::Add,
            targets: Some(vec![serde_norway::from_str(
                r#"
                static_configs:
                  - targets: ['localhost:9100']
                "#,
            )
            .unwrap()]),
            alert_rules: None,
            config_file: config_path.to_str().unwrap().to_owned(),
            alert_rules_file: None,
            reload: false,
        };

        let result = prometheus(params, false);
        assert!(result.is_err());
    }
}
