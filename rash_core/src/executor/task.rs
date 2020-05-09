#![feature(proc_macro)]

use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, MODULES};

use rash_derive::FieldNames;

use std::collections::HashSet;

use yaml_rust::Yaml;

/// Task is composed of Module and parameters to be executed in a concrete context
#[derive(Debug, FieldNames)]
pub struct Task {
    module: Module,
    name: Option<String>,
}

#[inline(always)]
fn is_task_attr(attr: &str) -> bool {
    Task::get_field_names().contains(attr)
}

#[inline(always)]
fn is_module(module: &str) -> bool {
    match MODULES.get(module) {
        Some(_) => true,
        None => false,
    }
}

impl Task {
    pub fn new(yaml: &Yaml) -> Result<Self> {
        TaskNew::from(yaml).validate_attrs()?.get_task()
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Task {
            module: Module::test_example(),
            name: None,
        }
    }
}

/// TaskValid is a task with valid yaml but without verify Task attributes and modules
#[derive(Debug)]
struct TaskValid {
    attrs: Yaml,
}

impl TaskValid {
    fn get_possible_attrs(&self) -> HashSet<String> {
        self.attrs
            .clone()
            .into_hash()
            .unwrap()
            .iter()
            .map(|(key, _)| key.as_str().unwrap().to_string())
            .collect::<HashSet<String>>()
    }

    fn get_module<'a>(&'a self) -> Result<&Module> {
        let module_names: HashSet<String> = self
            .get_possible_attrs()
            .iter()
            .filter(|&key| is_module(key))
            .map(String::clone)
            .collect();
        match module_names.len() > 1 {
            true => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Multiple modules found in task: {:?}", self),
                ))
            }
            false => (),
        };
        let module = module_names.iter().next().ok_or(Error::new(
            ErrorKind::NotFound,
            format!("Not module found in task: {:?}", self),
        ))?;
        MODULES.get::<str>(module).ok_or(Error::new(
            ErrorKind::NotFound,
            format!("Module not found in modules: {:?}", MODULES.keys()),
        ))
    }

    pub fn get_task<'task>(&self) -> Result<Task> {
        Ok(Task {
            name: self.attrs["name"].as_str().map(String::from),
            module: self.get_module()?.clone(),
        })
    }
}

/// TaskNew is a new task without checking yaml validity
#[derive(Debug)]
struct TaskNew {
    proto_attrs: Yaml,
}

impl From<&Yaml> for TaskNew {
    fn from(yaml: &Yaml) -> Self {
        TaskNew {
            proto_attrs: yaml.clone(),
        }
    }
}

impl TaskNew {
    pub fn validate_attrs(&self) -> Result<TaskValid> {
        let _ = self
            .proto_attrs
            .clone()
            .into_hash()
            .ok_or(Error::new(
                ErrorKind::InvalidData,
                format!("Task is not a dict {:?}", self.proto_attrs),
            ))?
            .iter()
            .map(|(key, _)| match key.as_str() {
                Some(s) => Ok(s.to_string()),
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Key is not valid in {:?}", self.proto_attrs),
                    ))
                }
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TaskValid {
            attrs: self.proto_attrs.clone(),
        })
    }
}

#[cfg(test)]
impl From<&Yaml> for Task {
    fn from(yaml: &Yaml) -> Self {
        TaskNew::from(yaml)
            .validate_attrs()
            .unwrap()
            .get_task()
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_from_yaml() {
        let s: String = r#"
        name: 'Test task'
        command: 'example'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        println!("{:?}", task);
        assert_eq!(task.name.unwrap(), "Test task");
        assert_eq!(&task.module, MODULES.get("command").unwrap());
    }

    #[test]
    fn test_from_yaml_no_module() {
        let s: String = r#"
        name: 'Test task'
        no_module: 'example'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::new(yaml).unwrap_err();
        assert_eq!(task.kind(), ErrorKind::NotFound);
    }
}
