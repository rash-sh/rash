/// ANCHOR: module
/// # block
///
/// Group tasks together for execution. The traditional sequence form remains supported. The
/// mapping form adds `defaults`, which are merged into every child task unless that task overrides
/// them. `vars` and `environment` maps are merged key-by-key.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - block:
///     - command: echo simple
///
/// - block:
///     tasks:
///       - command: ./migrate
///       - command: ./verify
///     defaults:
///       environment:
///         APP_ENV: production
///       become: true
/// ```
/// ANCHOR_END: examples
use crate::context::{Context, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};
use crate::task::{Task, Tasks};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde_norway::Value as YamlValue;

#[derive(Debug)]
pub struct Block;

fn merge_mapping(
    defaults: &serde_norway::Mapping,
    task: &serde_norway::Mapping,
) -> serde_norway::Mapping {
    let mut merged = defaults.clone();
    for (key, value) in task {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

pub(crate) fn apply_task_defaults(task: &YamlValue, defaults: &YamlValue) -> Result<YamlValue> {
    let task_map = task.as_mapping().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "task receiving defaults must be a mapping",
        )
    })?;
    let defaults_map = defaults
        .as_mapping()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "defaults must be a mapping"))?;

    let mut merged = defaults_map.clone();
    for (key, value) in task_map {
        let key_name = key.as_str();
        if matches!(key_name, Some("vars" | "environment"))
            && let (Some(default_map), Some(task_value_map)) = (
                defaults_map.get(key).and_then(YamlValue::as_mapping),
                value.as_mapping(),
            )
        {
            merged.insert(
                key.clone(),
                YamlValue::Mapping(merge_mapping(default_map, task_value_map)),
            );
            continue;
        }
        merged.insert(key.clone(), value.clone());
    }
    Ok(YamlValue::Mapping(merged))
}

fn parse_block_params(params: YamlValue) -> Result<(Vec<YamlValue>, Option<YamlValue>)> {
    match params {
        YamlValue::Sequence(tasks) => Ok((tasks, None)),
        YamlValue::Mapping(mapping) => {
            let tasks = mapping
                .get(YamlValue::String("tasks".to_owned()))
                .and_then(YamlValue::as_sequence)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "block mapping requires a 'tasks' sequence",
                    )
                })?
                .clone();
            let defaults = mapping
                .get(YamlValue::String("defaults".to_owned()))
                .cloned();
            for key in mapping.keys() {
                if !matches!(key.as_str(), Some("tasks" | "defaults")) {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Unknown block parameter: {key:?}"),
                    ));
                }
            }
            Ok((tasks, defaults))
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            "block must be a task sequence or mapping with tasks/defaults",
        )),
    }
}

impl Module for Block {
    fn get_name(&self) -> &str {
        "block"
    }

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: &Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)> {
        let (task_yamls, defaults) = parse_block_params(params)?;
        trace!("Block module executing {} tasks", task_yamls.len());
        let tasks = self.parse_tasks_from_yaml(&task_yamls, defaults.as_ref(), global_params)?;
        let result_context = Context::new(tasks, vars.clone(), None).exec()?;
        Ok((
            ModuleResult::new(false, None, None),
            result_context.get_scoped_vars().cloned(),
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        None
    }
}

impl Block {
    fn parse_tasks_from_yaml<'a>(
        &self,
        task_yamls: &[YamlValue],
        defaults: Option<&YamlValue>,
        global_params: &'a GlobalParams,
    ) -> Result<Tasks<'a>> {
        task_yamls
            .iter()
            .enumerate()
            .map(|(index, task_yaml)| {
                let effective = match defaults {
                    Some(defaults) => apply_task_defaults(task_yaml, defaults)?,
                    None => task_yaml.clone(),
                };
                Task::new(&effective, global_params).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to parse task at index {index}: {e}"),
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_sequence_form_still_parses() {
        let params: YamlValue = serde_norway::from_str("- debug: { msg: hi }").unwrap();
        let (tasks, defaults) = parse_block_params(params).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(defaults.is_none());
    }

    #[test]
    fn mapping_form_accepts_defaults() {
        let params: YamlValue = serde_norway::from_str(
            r#"
            tasks:
              - debug: { msg: hi }
            defaults:
              environment:
                APP_ENV: production
            "#,
        )
        .unwrap();
        let (tasks, defaults) = parse_block_params(params).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(defaults.is_some());
    }

    #[test]
    fn task_values_override_and_maps_merge() {
        let task: YamlValue = serde_norway::from_str(
            r#"
            command: echo hi
            become: false
            environment:
              B: task
            "#,
        )
        .unwrap();
        let defaults: YamlValue = serde_norway::from_str(
            r#"
            become: true
            environment:
              A: default
              B: default
            "#,
        )
        .unwrap();
        let merged = apply_task_defaults(&task, &defaults).unwrap();
        assert_eq!(merged["become"].as_bool(), Some(false));
        assert_eq!(merged["environment"]["A"].as_str(), Some("default"));
        assert_eq!(merged["environment"]["B"].as_str(), Some("task"));
    }
}
