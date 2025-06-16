/// ANCHOR: module
/// # block
///
/// This module allows grouping tasks together for execution.
/// Similar to Ansible's block directive.
///
/// Note: `vars` declared in a block are scoped to that block and do not persist to the parent context.
/// However, registered variables from tasks within the block will persist.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: full
/// ```
/// ANCHOR_END: module
/// ANCHOR: parameters
/// | Parameter | Required | Type | Values | Description                   |
/// | --------- | -------- | ---- | ------ | ----------------------------- |
/// | block     | true     | list |        | List of tasks to execute      |
///
/// ANCHOR_END: parameters
///
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - name: Example block
///   block:
///     - name: Create a file
///       copy:
///         content: "Hello World"
///         dest: "/tmp/test.txt"
///
///     - name: Run a command
///       command:
///         cmd: "echo 'Success'"
/// ```
/// ANCHOR_END: examples
use crate::context::{Context, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};
use crate::task::{Task, Tasks};

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde_yaml::Value as YamlValue;

#[derive(Debug)]
pub struct Block;

impl Module for Block {
    fn get_name(&self) -> &str {
        "block"
    }

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        match params {
            YamlValue::Sequence(task_yamls) => {
                trace!("Block module executing {} tasks", task_yamls.len());

                let tasks = self.parse_tasks_from_yaml(&task_yamls, global_params)?;

                let context = Context::new(tasks, vars);
                let result_context = context.exec()?;

                // Block is a control structure, so it doesn't display its own output
                let module_result = ModuleResult::new(false, None, None);

                Ok((module_result, result_context.get_vars().clone()))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "block parameter must be a sequence of tasks",
            )),
        }
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
    /// Parse YAML task definitions into validated Task objects.
    fn parse_tasks_from_yaml<'a>(
        &self,
        task_yamls: &[YamlValue],
        global_params: &'a GlobalParams,
    ) -> Result<Tasks<'a>> {
        task_yamls
            .iter()
            .enumerate()
            .map(|(index, task_yaml)| {
                Task::new(task_yaml, global_params).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Failed to parse task at index {}: {}", index, e),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GlobalParams;
    use minijinja::context;
    use serde_yaml;

    fn create_test_global_params() -> GlobalParams<'static> {
        GlobalParams::default()
    }

    #[test]
    fn test_block_module_get_name() {
        let block = Block;
        assert_eq!(block.get_name(), "block");
    }

    #[test]
    fn test_module_exec_with_empty_block() {
        let block = Block;
        let global_params = create_test_global_params();
        let params = YamlValue::Sequence(vec![]);
        let vars = context! {};

        let result = block.exec(&global_params, params, vars, false);
        assert!(result.is_ok());

        let (module_result, _final_vars) = result.unwrap();
        assert!(!module_result.changed);
    }

    #[test]
    fn test_module_exec_with_invalid_params() {
        let block = Block;
        let global_params = create_test_global_params();
        let params = YamlValue::String("not a sequence".to_string());
        let vars = context! {};

        let result = block.exec(&global_params, params, vars, false);
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("block parameter must be a sequence"));
    }

    #[test]
    fn test_parse_tasks_from_yaml_empty() {
        let block = Block;
        let global_params = create_test_global_params();
        let task_yamls: Vec<YamlValue> = vec![];

        let result = block.parse_tasks_from_yaml(&task_yamls, &global_params);
        assert!(result.is_ok());

        let tasks = result.unwrap();
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_parse_tasks_from_yaml_valid() {
        let block = Block;
        let global_params = create_test_global_params();

        let yaml_str = r#"
        name: test task
        debug:
          msg: test message
        "#;
        let task_yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_yamls = vec![task_yaml];

        let result = block.parse_tasks_from_yaml(&task_yamls, &global_params);
        assert!(result.is_ok());

        let tasks = result.unwrap();
        assert_eq!(tasks.len(), 1);
        // Note: Can't test task.name directly as it's private, but we can test length
    }

    #[test]
    fn test_parse_tasks_from_yaml_invalid_structure() {
        let block = Block;
        let global_params = create_test_global_params();

        // Invalid task structure - not a mapping
        let task_yamls = vec![YamlValue::String("invalid task".to_string())];

        let result = block.parse_tasks_from_yaml(&task_yamls, &global_params);
        assert!(result.is_err());

        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("Failed to parse task at index 0"));
    }
}
