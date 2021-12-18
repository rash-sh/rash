use crate::error::{Error, ErrorKind, Result};
use crate::modules::{is_module, MODULES};
use crate::task::Task;

use std::collections::HashSet;

use yaml_rust::Yaml;

/// TaskValid is a ProtoTask with verified attrs (one module and valid attrs)
#[derive(Debug)]
pub struct TaskValid {
    attrs: Yaml,
}

impl TaskValid {
    pub fn new(attrs: &Yaml) -> Self {
        TaskValid {
            attrs: attrs.clone(),
        }
    }

    fn get_possible_attrs(&self) -> HashSet<String> {
        self.attrs
            .clone()
            // safe unwrap: validated attr
            .into_hash()
            .unwrap()
            .iter()
            // safe unwrap: validated attr
            .map(|(key, _)| key.as_str().unwrap().to_string())
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
                format!("Multiple modules found in task: {:?}", self),
            ));
        };
        module_names
            .iter()
            .map(String::clone)
            .next()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("Not module found in task: {:?}", self),
                )
            })
    }

    fn parse_bool_or_string(&'_ self, attr: &Yaml) -> Option<String> {
        match attr.as_bool() {
            Some(x) => match x {
                true => Some("true".to_string()),
                false => Some("false".to_string()),
            },
            None => attr.as_str().map(String::from),
        }
    }

    pub fn get_task(&self, check_mode: bool) -> Result<Task> {
        let module_name: &str = &self.get_module_name()?;
        Ok(Task {
            changed_when: self.parse_bool_or_string(&self.attrs["changed_when"]),
            check_mode: match check_mode {
                true => true,
                false => self.attrs["check_mode"].as_bool().unwrap_or(false),
            },
            module: MODULES
                .get::<str>(module_name)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::NotFound,
                        format!("Module not found in modules: {:?}", MODULES.keys()),
                    )
                })?
                .clone(),
            params: self.attrs[module_name].clone(),
            name: self.attrs["name"].as_str().map(String::from),
            ignore_errors: self.attrs["ignore_errors"].as_bool(),
            r#loop: if self.attrs["loop"].is_badvalue() {
                None
            } else {
                Some(self.attrs["loop"].clone())
            },
            register: self.attrs["register"].as_str().map(String::from),
            when: self.parse_bool_or_string(&self.attrs["when"]),
        })
    }
}
