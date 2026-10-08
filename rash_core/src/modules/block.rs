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
/// ANCHOR: parameters
/// `block` takes either a list of tasks or a mapping:
///
/// | Parameter | Required | Type | Values | Description                                                |
/// | --------- | -------- | ---- | ------ | ---------------------------------------------------------- |
/// | tasks     | true     | list |        | Tasks to execute.                                          |
/// | defaults  | false    | map  |        | Task attributes applied to every child task it doesn't set. |
///
/// ANCHOR_END: parameters
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
use crate::task::parse_tasks_with_defaults;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde_norway::Value as YamlValue;

#[derive(Debug)]
pub struct Block;

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

    fn is_control_flow(&self) -> bool {
        true
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
        let tasks = parse_tasks_with_defaults(&task_yamls, defaults.as_ref(), global_params)?;
        let result_context = Context::new(tasks, vars.clone(), None).exec()?;
        Ok((
            ModuleResult::new(false, None, None),
            result_context.get_scoped_vars().cloned(),
        ))
    }

    fn force_string_on_params(&self) -> bool {
        false
    }

    fn defer_params_rendering(&self) -> bool {
        true
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema> {
        None
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
}
