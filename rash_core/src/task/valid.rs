use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{MODULES, is_module};
use crate::task::Task;

use std::collections::HashSet;

use serde_yaml::Value;

/// TaskValid is a ProtoTask with verified attrs: one module with valid attrs
#[derive(Debug)]
pub struct TaskValid {
    attrs: Value,
}

impl TaskValid {
    pub fn new(attrs: &Value) -> Self {
        TaskValid {
            attrs: attrs.clone(),
        }
    }

    fn get_possible_attrs(&self) -> HashSet<String> {
        self.attrs
            .clone()
            // safe unwrap: validated attr
            .as_mapping()
            .unwrap()
            .iter()
            // safe unwrap: validated attr
            .map(|(key, _)| key.as_str().unwrap().to_owned())
            .collect::<HashSet<String>>()
    }

    fn get_module_name(&'_ self) -> Result<String> {
        let module_names: HashSet<String> = self
            .get_possible_attrs()
            .iter()
            .filter(|&key| is_module(key))
            .map(String::clone)
            .collect();

        match module_names.len() {
            0 => Err(Error::new(
                ErrorKind::NotFound,
                format!("Not module found in task: {self:?}"),
            )),
            1 => Ok(module_names
                .iter()
                .map(String::clone)
                .next()
                //safe unwrap()
                .unwrap()),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Multiple modules found in task: {self:?}"),
            )),
        }
    }

    fn parse_array(&'_ self, attr: &Value) -> Option<String> {
        match attr.as_sequence() {
            Some(v) => Some(
                v.iter()
                    .map(|x| self.parse_bool_or_string(x))
                    .collect::<Option<Vec<String>>>()?
                    .iter()
                    .map(|s| format!("({})", s))
                    .collect::<Vec<String>>()
                    .join(" and "),
            ),
            None => self.parse_bool_or_string(attr),
        }
    }

    fn parse_bool_or_string(&'_ self, attr: &Value) -> Option<String> {
        match attr.as_bool() {
            Some(x) => match x {
                true => Some("true".to_owned()),
                false => Some("false".to_owned()),
            },
            None => attr.as_str().map(String::from),
        }
    }

    /// Validate rescue and always attributes (now allowed on any task)
    fn validate_block_only_attributes(&self) -> Result<()> {
        // Rescue and always attributes are now allowed on any task, not just blocks
        // This provides more flexible error handling and cleanup capabilities
        Ok(())
    }

    pub fn get_task<'a>(&self, global_params: &'a GlobalParams) -> Result<Task<'a>> {
        let module_name: &str = &self.get_module_name()?;

        // Validate that rescue and always attributes are only used with block modules
        self.validate_block_only_attributes()?;

        Ok(Task {
            r#become: match global_params.r#become {
                true => true,
                false => self.attrs["become"].as_bool().unwrap_or(false),
            },
            become_user: match self.attrs["become_user"].as_str() {
                Some(s) => s,
                None => global_params.become_user,
            }
            .to_owned(),
            changed_when: self.parse_array(&self.attrs["changed_when"]),
            check_mode: match global_params.check_mode {
                true => true,
                false => self.attrs["check_mode"].as_bool().unwrap_or(false),
            },
            // &dyn Module from &Box<dyn Module>
            module: &**MODULES.get::<str>(module_name).ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("Module not found in modules: {:?}", MODULES.keys()),
                )
            })?,
            params: self.attrs[module_name].clone(),
            name: self.attrs["name"].as_str().map(String::from),
            ignore_errors: self.attrs["ignore_errors"].as_bool(),
            r#loop: self.attrs.get("loop").map(|_| self.attrs["loop"].clone()),
            register: self.attrs["register"].as_str().map(String::from),
            vars: self.attrs.get("vars").map(|_| self.attrs["vars"].clone()),
            when: self.parse_array(&self.attrs["when"]),
            rescue: self
                .attrs
                .get("rescue")
                .map(|_| self.attrs["rescue"].clone()),
            always: self
                .attrs
                .get("always")
                .map(|_| self.attrs["always"].clone()),
            global_params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GlobalParams;
    use serde_yaml::Value as YamlValue;

    fn create_test_global_params() -> GlobalParams<'static> {
        GlobalParams::default()
    }

    #[test]
    fn test_rescue_with_debug_module_succeeds() {
        let yaml_str = r#"
        name: test task
        debug:
          msg: test
        rescue:
          - name: rescue task
            debug:
              msg: rescue
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test task".to_string()));
        assert!(task.rescue.is_some());
        // Verify rescue is an array with one task
        if let Some(YamlValue::Sequence(rescue_tasks)) = &task.rescue {
            assert_eq!(rescue_tasks.len(), 1);
        } else {
            panic!("Expected rescue to be a sequence");
        }
    }

    #[test]
    fn test_always_with_command_module_succeeds() {
        let yaml_str = r#"
        name: test task
        command:
          cmd: echo test
        always:
          - name: always task
            debug:
              msg: always
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test task".to_string()));
        assert!(task.always.is_some());
        // Verify always is an array with one task
        if let Some(YamlValue::Sequence(always_tasks)) = &task.always {
            assert_eq!(always_tasks.len(), 1);
        } else {
            panic!("Expected always to be a sequence");
        }
    }

    #[test]
    fn test_both_rescue_and_always_with_debug_module_succeeds() {
        let yaml_str = r#"
        name: test task
        debug:
          msg: test
        rescue:
          - name: rescue task
            debug:
              msg: rescue
        always:
          - name: always task
            debug:
              msg: always
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test task".to_string()));

        // Verify both rescue and always are present
        assert!(task.rescue.is_some());
        assert!(task.always.is_some());

        // Verify rescue is an array with one task
        if let Some(YamlValue::Sequence(rescue_tasks)) = &task.rescue {
            assert_eq!(rescue_tasks.len(), 1);
        } else {
            panic!("Expected rescue to be a sequence");
        }

        // Verify always is an array with one task
        if let Some(YamlValue::Sequence(always_tasks)) = &task.always {
            assert_eq!(always_tasks.len(), 1);
        } else {
            panic!("Expected always to be a sequence");
        }
    }

    #[test]
    fn test_rescue_and_always_with_block_module_succeeds() {
        let yaml_str = r#"
        name: test block
        block:
          - name: main task
            debug:
              msg: main
        rescue:
          - name: rescue task
            debug:
              msg: rescue
        always:
          - name: always task
            debug:
              msg: always
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.module.get_name(), "block");
        assert!(task.rescue.is_some());
        assert!(task.always.is_some());
    }

    #[test]
    fn test_block_without_rescue_and_always_succeeds() {
        let yaml_str = r#"
        name: test block
        block:
          - name: main task
            debug:
              msg: main
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.module.get_name(), "block");
        assert!(task.rescue.is_none());
        assert!(task.always.is_none());
    }

    #[test]
    fn test_non_block_task_without_rescue_and_always_succeeds() {
        let yaml_str = r#"
        name: test task
        debug:
          msg: test
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.module.get_name(), "debug");
        assert!(task.rescue.is_none());
        assert!(task.always.is_none());
    }

    #[test]
    fn test_rescue_with_copy_module_succeeds() {
        let yaml_str = r#"
        name: test copy task
        copy:
          content: "test content"
          dest: "/tmp/test.txt"
        rescue:
          - name: handle copy failure
            debug:
              msg: "Copy failed, cleaning up"
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test copy task".to_string()));
        assert!(task.rescue.is_some());
        assert!(task.always.is_none());
    }

    #[test]
    fn test_always_with_file_module_succeeds() {
        let yaml_str = r#"
        name: test file task
        file:
          path: "/tmp/testfile"
          state: touch
        always:
          - name: cleanup
            debug:
              msg: "Always running cleanup"
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test file task".to_string()));
        assert!(task.rescue.is_none());
        assert!(task.always.is_some());
    }

    #[test]
    fn test_rescue_and_always_with_loop_succeeds() {
        let yaml_str = r#"
        name: test loop with rescue/always
        debug:
          msg: "Item: {{ item }}"
        loop:
          - one
          - two
          - three
        rescue:
          - name: handle loop failure
            debug:
              msg: "Loop item failed: {{ item }}"
        always:
          - name: loop cleanup
            debug:
              msg: "Cleaning up after loop item: {{ item }}"
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test loop with rescue/always".to_string()));
        assert!(task.rescue.is_some());
        assert!(task.always.is_some());
        assert!(task.r#loop.is_some());
    }

    #[test]
    fn test_rescue_and_always_empty_sequences_succeeds() {
        let yaml_str = r#"
        name: test empty rescue/always
        debug:
          msg: "test"
        rescue: []
        always: []
        "#;
        let yaml: YamlValue = serde_yaml::from_str(yaml_str).unwrap();
        let task_valid = TaskValid::new(&yaml);
        let global_params = create_test_global_params();

        let result = task_valid.get_task(&global_params);
        assert!(result.is_ok());

        let task = result.unwrap();
        assert_eq!(task.name, Some("test empty rescue/always".to_string()));
        assert!(task.rescue.is_some());
        assert!(task.always.is_some());

        // Verify they are empty sequences
        if let Some(YamlValue::Sequence(rescue_tasks)) = &task.rescue {
            assert_eq!(rescue_tasks.len(), 0);
        } else {
            panic!("Expected rescue to be a sequence");
        }

        if let Some(YamlValue::Sequence(always_tasks)) = &task.always {
            assert_eq!(always_tasks.len(), 0);
        } else {
            panic!("Expected always to be a sequence");
        }
    }
}
