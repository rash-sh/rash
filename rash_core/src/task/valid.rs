use crate::error::{Error, ErrorKind, Result};
use crate::modules::{is_module, MODULES};
use crate::task::{GlobalParams, Task};

use std::collections::HashSet;

use serde_yaml::Value;

/// TaskValid is a ProtoTask with verified attrs (one module and valid attrs)
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
        if module_names.len() > 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Multiple modules found in task: {self:?}"),
            ));
        };
        module_names
            .iter()
            .map(String::clone)
            .next()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("Not module found in task: {self:?}"),
                )
            })
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

    pub fn get_task(&self, global_params: &GlobalParams) -> Result<Task> {
        let module_name: &str = &self.get_module_name()?;

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
        })
    }
}
