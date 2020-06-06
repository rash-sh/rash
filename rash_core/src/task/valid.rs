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

    pub fn get_task(&self) -> Result<Task> {
        let module_name: &str = &self.get_module_name()?;
        Ok(Task {
            name: self.attrs["name"].as_str().map(String::from),
            when: self.attrs["when"].as_str().map(String::from),
            register: self.attrs["register"].as_str().map(String::from),
            ignore_errors: self.attrs["ignore_errors"].as_bool(),
            r#loop: if self.attrs["loop"].is_badvalue() {
                None
            } else {
                Some(self.attrs["loop"].clone())
            },
            module: MODULES
                .get::<str>(&module_name)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::NotFound,
                        format!("Module not found in modules: {:?}", MODULES.keys()),
                    )
                })?
                .clone(),
            params: self.attrs[module_name].clone(),
        })
    }
}
