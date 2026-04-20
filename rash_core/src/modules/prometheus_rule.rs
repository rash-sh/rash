/// ANCHOR: module
/// # prometheus_rule
///
/// Manage Prometheus alerting rule groups in rule files.
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
/// - prometheus_rule:
///     file: /etc/prometheus/alert.rules
///     name: node_alerts
///     rules:
///       - alert: HighCPU
///         expr: cpu_usage > 80
///         for: 5m
///         labels:
///           severity: warning
///
/// - prometheus_rule:
///     file: /etc/prometheus/alert.rules
///     name: node_alerts
///     interval: 30s
///     rules:
///       - alert: HighMemory
///         expr: memory_usage > 90
///         for: 10m
///
/// - prometheus_rule:
///     file: /etc/prometheus/alert.rules
///     name: deprecated_alerts
///     state: absent
/// ```
/// ANCHOR_END: examples
use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::logger::diff;
use crate::modules::{Module, ModuleResult, parse_params};

#[cfg(feature = "docs")]
use rash_derive::DocJsonSchema;

use std::fs::{self, OpenOptions};
use std::io::Write;
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
    /// The absolute path to the Prometheus rules file.
    pub file: String,
    /// The name of the rule group.
    pub name: String,
    /// List of alerting rules. Each rule must have `alert` and `expr` fields.
    /// Required if state=present.
    #[cfg_attr(feature = "docs", schemars(skip))]
    pub rules: Option<Vec<YamlValue>>,
    /// Evaluation interval for the rule group (e.g., `30s`, `5m`).
    pub interval: Option<String>,
    /// Whether the rule group should exist or not.
    /// **[default: `"present"`]**
    pub state: Option<State>,
}

#[derive(Debug, PartialEq, Default, Deserialize, Clone)]
#[cfg_attr(feature = "docs", derive(EnumString, Display, JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Present,
    Absent,
}

fn validate_rule(rule: &YamlValue) -> Result<()> {
    if let YamlValue::Mapping(map) = rule {
        let has_alert = map.get(YamlValue::String("alert".to_string())).is_some();
        let has_expr = map.get(YamlValue::String("expr".to_string())).is_some();
        if !has_alert || !has_expr {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "each rule must have 'alert' and 'expr' fields",
            ));
        }
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            "each rule must be a mapping",
        ))
    }
}

fn find_group_index(groups: &[YamlValue], name: &str) -> Option<usize> {
    groups.iter().position(|g| {
        if let YamlValue::Mapping(map) = g {
            map.get(YamlValue::String("name".to_string()))
                .map(|v| v.as_str() == Some(name))
                .unwrap_or(false)
        } else {
            false
        }
    })
}

fn build_group(params: &Params) -> Result<YamlValue> {
    let rules = params.rules.as_ref().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "rules is required when state=present",
        )
    })?;

    for rule in rules {
        validate_rule(rule)?;
    }

    let mut group = serde_norway::Mapping::new();
    group.insert(
        YamlValue::String("name".to_string()),
        YamlValue::String(params.name.clone()),
    );

    if let Some(ref interval) = params.interval {
        group.insert(
            YamlValue::String("interval".to_string()),
            YamlValue::String(interval.clone()),
        );
    }

    group.insert(
        YamlValue::String("rules".to_string()),
        YamlValue::Sequence(rules.clone()),
    );

    Ok(YamlValue::Mapping(group))
}

fn groups_equal(a: &YamlValue, b: &YamlValue) -> bool {
    a == b
}

pub fn prometheus_rule(params: Params, check_mode: bool) -> Result<ModuleResult> {
    trace!("params: {params:?}");

    let state = params.state.clone().unwrap_or_default();
    let path = Path::new(&params.file);
    let file_str = params.file.clone();

    let (mut root, original_content) = if path.exists() {
        let content = fs::read_to_string(path)?;
        if content.trim().is_empty() {
            (serde_norway::Mapping::new(), content)
        } else {
            let root: serde_norway::Mapping = serde_norway::from_str(&content)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            (root, content)
        }
    } else {
        (serde_norway::Mapping::new(), String::new())
    };

    let groups_key = YamlValue::String("groups".to_string());
    let groups = root
        .entry(groups_key)
        .or_insert_with(|| YamlValue::Sequence(Vec::new()));

    let groups_list = match groups {
        YamlValue::Sequence(seq) => seq,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "'groups' must be a sequence in the rules file",
            ));
        }
    };

    let mut changed = false;

    match state {
        State::Present => {
            let new_group = build_group(&params)?;

            if let Some(idx) = find_group_index(groups_list, &params.name) {
                if !groups_equal(&groups_list[idx], &new_group) {
                    groups_list[idx] = new_group;
                    changed = true;
                }
            } else {
                groups_list.push(new_group);
                changed = true;
            }
        }
        State::Absent => {
            if let Some(idx) = find_group_index(groups_list, &params.name) {
                groups_list.remove(idx);
                changed = true;
            }
        }
    }

    if changed {
        let new_content = serde_norway::to_string(&YamlValue::Mapping(root))
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        let new_content_with_newline = format!("{new_content}\n");

        diff(&original_content, &new_content_with_newline);

        if !check_mode {
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent)?;
            }

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)?;
            file.write_all(new_content_with_newline.as_bytes())?;
        }
    }

    Ok(ModuleResult {
        changed,
        output: Some(file_str),
        extra: None,
    })
}

#[derive(Debug)]
pub struct PrometheusRule;

impl Module for PrometheusRule {
    fn get_name(&self) -> &str {
        "prometheus_rule"
    }

    fn exec(
        &self,
        _: &GlobalParams,
        optional_params: YamlValue,
        _vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        Ok((
            prometheus_rule(parse_params(optional_params)?, check_mode)?,
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
            file: /etc/prometheus/alert.rules
            name: node_alerts
            rules:
              - alert: HighCPU
                expr: cpu_usage > 80
                for: 5m
            state: present
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.file, "/etc/prometheus/alert.rules");
        assert_eq!(params.name, "node_alerts");
        assert_eq!(params.state, Some(State::Present));
        assert!(params.rules.is_some());
    }

    #[test]
    fn test_parse_params_with_interval() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            file: /etc/prometheus/alert.rules
            name: node_alerts
            interval: 30s
            rules:
              - alert: HighCPU
                expr: cpu_usage > 80
            "#,
        )
        .unwrap();
        let params: Params = parse_params(yaml).unwrap();
        assert_eq!(params.interval, Some("30s".to_string()));
    }

    #[test]
    fn test_parse_params_invalid() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            unknown_field: bad
            "#,
        )
        .unwrap();
        let error = parse_params::<Params>(yaml).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_validate_rule_valid() {
        let rule: YamlValue = serde_norway::from_str(
            r#"
            alert: HighCPU
            expr: cpu_usage > 80
            for: 5m
            "#,
        )
        .unwrap();
        assert!(validate_rule(&rule).is_ok());
    }

    #[test]
    fn test_validate_rule_missing_alert() {
        let rule: YamlValue = serde_norway::from_str(
            r#"
            expr: cpu_usage > 80
            "#,
        )
        .unwrap();
        let error = validate_rule(&rule).unwrap_err();
        assert!(error.to_string().contains("alert"));
    }

    #[test]
    fn test_validate_rule_missing_expr() {
        let rule: YamlValue = serde_norway::from_str(
            r#"
            alert: HighCPU
            "#,
        )
        .unwrap();
        let error = validate_rule(&rule).unwrap_err();
        assert!(error.to_string().contains("expr"));
    }

    #[test]
    fn test_validate_rule_not_mapping() {
        let rule = YamlValue::String("not a mapping".to_string());
        let error = validate_rule(&rule).unwrap_err();
        assert!(error.to_string().contains("mapping"));
    }

    #[test]
    fn test_prometheus_rule_create_new_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                for: 5m
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("node_alerts"));
        assert!(content.contains("HighCPU"));
        assert!(content.contains("cpu_usage > 80"));
    }

    #[test]
    fn test_prometheus_rule_add_group_to_existing() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - name: existing_group\n    rules:\n      - alert: TestAlert\n        expr: up == 0\n",
        )
        .unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("existing_group"));
        assert!(content.contains("node_alerts"));
        assert!(content.contains("HighCPU"));
    }

    #[test]
    fn test_prometheus_rule_update_existing_group() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - name: node_alerts\n    rules:\n      - alert: HighCPU\n        expr: cpu_usage > 90\n",
        )
        .unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                "#,
                )
                .unwrap(),
            ]),
            interval: Some("30s".to_string()),
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("cpu_usage > 80"));
        assert!(content.contains("30s"));
        assert!(!content.contains("cpu_usage > 90"));
    }

    #[test]
    fn test_prometheus_rule_no_change() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        let yaml_content = "groups:\n  - name: node_alerts\n    rules:\n      - alert: HighCPU\n        expr: cpu_usage > 80\n        for: 5m\n";
        fs::write(&file_path, yaml_content).unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                for: 5m
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_prometheus_rule_remove_group() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - name: node_alerts\n    rules:\n      - alert: HighCPU\n        expr: cpu_usage > 80\n  - name: other_group\n    rules:\n      - alert: TestAlert\n        expr: up == 0\n",
        )
        .unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: None,
            interval: None,
            state: Some(State::Absent),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(!content.contains("node_alerts"));
        assert!(!content.contains("HighCPU"));
        assert!(content.contains("other_group"));
    }

    #[test]
    fn test_prometheus_rule_remove_nonexistent_group() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - name: other_group\n    rules:\n      - alert: TestAlert\n        expr: up == 0\n",
        )
        .unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "nonexistent".to_string(),
            rules: None,
            interval: None,
            state: Some(State::Absent),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(!result.changed);
    }

    #[test]
    fn test_prometheus_rule_check_mode() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - name: node_alerts\n    rules:\n      - alert: HighCPU\n        expr: cpu_usage > 90\n",
        )
        .unwrap();
        let original_content = fs::read_to_string(&file_path).unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, true).unwrap();
        assert!(result.changed);

        let content_after = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, content_after);
    }

    #[test]
    fn test_prometheus_rule_missing_rules_for_present() {
        let params = Params {
            file: "/tmp/test.rules".to_string(),
            name: "node_alerts".to_string(),
            rules: None,
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("rules is required")
        );
    }

    #[test]
    fn test_prometheus_rule_with_interval() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                "#,
                )
                .unwrap(),
            ]),
            interval: Some("5m".to_string()),
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("5m"));
    }

    #[test]
    fn test_prometheus_rule_with_labels_and_annotations() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                for: 5m
                labels:
                  severity: warning
                annotations:
                  summary: "High CPU usage detected"
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(result.changed);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("severity"));
        assert!(content.contains("summary"));
    }

    #[test]
    fn test_prometheus_rule_idempotent_with_different_key_order() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("alert.rules");

        fs::write(
            &file_path,
            "groups:\n  - rules:\n      - for: 5m\n        expr: cpu_usage > 80\n        alert: HighCPU\n    name: node_alerts\n",
        )
        .unwrap();

        let params = Params {
            file: file_path.to_str().unwrap().to_string(),
            name: "node_alerts".to_string(),
            rules: Some(vec![
                serde_norway::from_str(
                    r#"
                alert: HighCPU
                expr: cpu_usage > 80
                for: 5m
                "#,
                )
                .unwrap(),
            ]),
            interval: None,
            state: Some(State::Present),
        };

        let result = prometheus_rule(params, false).unwrap();
        assert!(!result.changed);
    }
}
