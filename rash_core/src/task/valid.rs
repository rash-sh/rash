use crate::context::{BecomeMethod, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{MODULES, is_module};
use crate::task::{Task, parse_notify_value};

use std::collections::HashSet;

use serde_norway::Value;

#[derive(Debug)]
pub struct TaskValid {
    attrs: Value,
}

impl TaskValid {
    pub fn new(attrs: &Value) -> Self {
        Self {
            attrs: attrs.clone(),
        }
    }

    fn get_possible_attrs(&self) -> Result<HashSet<String>> {
        let mapping = self.attrs.as_mapping().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "task must be a YAML mapping")
        })?;
        mapping
            .keys()
            .map(|key| {
                key.as_str().map(String::from).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "task keys must be strings")
                })
            })
            .collect()
    }

    fn get_module_name(&self) -> Result<String> {
        let modules: Vec<String> = self
            .get_possible_attrs()?
            .into_iter()
            .filter(|key| is_module(key))
            .collect();
        match modules.as_slice() {
            [] => Err(Error::new(
                ErrorKind::NotFound,
                format!("No module found in task: {:?}", self.attrs),
            )),
            [module] => Ok(module.clone()),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Multiple modules found in task: {modules:?}"),
            )),
        }
    }

    fn parse_bool_or_string(&self, attr: &Value) -> Option<String> {
        attr.as_bool()
            .map(|value| value.to_string())
            .or_else(|| attr.as_str().map(String::from))
    }

    fn parse_expression(&self, attr: &Value) -> Option<String> {
        match attr.as_sequence() {
            Some(values) => Some(
                values
                    .iter()
                    .map(|value| self.parse_bool_or_string(value))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .map(|expression| format!("({expression})"))
                    .collect::<Vec<_>>()
                    .join(" and "),
            ),
            None => self.parse_bool_or_string(attr),
        }
    }

    fn optional_clone(&self, name: &str) -> Option<Value> {
        self.attrs.get(name).cloned()
    }

    fn validate_sequence_attr(&self, name: &str) -> Result<()> {
        if let Some(value) = self.attrs.get(name)
            && value.as_sequence().is_none()
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("{name} must be a task sequence"),
            ));
        }
        Ok(())
    }

    pub fn get_task<'a>(&self, global_params: &'a GlobalParams<'a>) -> Result<Task<'a>> {
        self.validate_sequence_attr("rescue")?;
        self.validate_sequence_attr("always")?;
        let module_name = self.get_module_name()?;
        let module = MODULES.get::<str>(&module_name).ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("Module not found in registry: {module_name}"),
            )
        })?;

        let become_method = match self.attrs["become_method"].as_str() {
            Some(value) => value.parse::<BecomeMethod>().map_err(|error| {
                Error::new(ErrorKind::InvalidData, error)
            })?,
            None => global_params.become_method,
        };

        Ok(Task {
            r#become: global_params.r#become
                || self.attrs["become"].as_bool().unwrap_or(false),
            become_user: self
                .attrs["become_user"]
                .as_str()
                .unwrap_or(global_params.become_user)
                .to_owned(),
            become_method,
            become_exe: self
                .attrs["become_exe"]
                .as_str()
                .unwrap_or(global_params.become_exe)
                .to_owned(),
            become_password: self.attrs["become_password"]
                .as_str()
                .map(String::from)
                .or_else(|| global_params.become_password.map(String::from)),
            check_mode: global_params.check_mode
                || self.attrs["check_mode"].as_bool().unwrap_or(false),
            module: &**module,
            params: self.attrs[&module_name].clone(),
            changed_when: self.parse_expression(&self.attrs["changed_when"]),
            failed_when: self.parse_expression(&self.attrs["failed_when"]),
            ignore_errors: self.attrs["ignore_errors"].as_bool(),
            quiet: self.attrs["quiet"].as_bool().unwrap_or(false),
            no_log: self.attrs["no_log"].as_bool().unwrap_or(false),
            name: self.attrs["name"].as_str().map(String::from),
            r#loop: self.optional_clone("loop"),
            register: self.attrs["register"].as_str().map(String::from),
            vars: self.optional_clone("vars"),
            when: self.parse_expression(&self.attrs["when"]),
            rescue: self.optional_clone("rescue"),
            always: self.optional_clone("always"),
            environment: self.optional_clone("environment"),
            notify: self.attrs.get("notify").and_then(parse_notify_value),
            retries: self.attrs["retries"].as_u64().map(|value| value as u32),
            delay: self.attrs["delay"].as_u64(),
            until: self.parse_expression(&self.attrs["until"]),
            r#async: self.attrs["async"].as_u64(),
            poll: self.attrs["poll"].as_u64(),
            global_params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_semantic_controls() {
        let yaml: Value = serde_norway::from_str(
            r#"
            command: echo hi
            changed_when: false
            failed_when: result.rc not in [0, 1]
            quiet: true
            no_log: true
            "#,
        )
        .unwrap();
        let params = GlobalParams::default();
        let task = TaskValid::new(&yaml).get_task(&params).unwrap();
        assert_eq!(task.changed_when.as_deref(), Some("false"));
        assert_eq!(task.failed_when.as_deref(), Some("result.rc not in [0, 1]"));
        assert!(task.quiet);
        assert!(task.no_log);
    }

    #[test]
    fn expression_arrays_are_anded() {
        let yaml: Value = serde_norway::from_str(
            r#"
            debug: { msg: hi }
            failed_when:
              - result.changed
              - true
            "#,
        )
        .unwrap();
        let valid = TaskValid::new(&yaml);
        assert_eq!(
            valid.parse_expression(&yaml["failed_when"]).as_deref(),
            Some("(result.changed) and (true)")
        );
    }

    #[test]
    fn invalid_become_method_is_rejected() {
        let yaml: Value = serde_norway::from_str(
            "debug: { msg: hi }\nbecome_method: nope",
        )
        .unwrap();
        let params = GlobalParams::default();
        assert!(TaskValid::new(&yaml).get_task(&params).is_err());
    }

    #[test]
    fn rescue_must_be_a_sequence() {
        let yaml: Value = serde_norway::from_str(
            "debug: { msg: hi }\nrescue: nope",
        )
        .unwrap();
        let params = GlobalParams::default();
        assert!(TaskValid::new(&yaml).get_task(&params).is_err());
    }
}
