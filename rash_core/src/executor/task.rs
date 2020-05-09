#![feature(proc_macro)]

use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, MODULES};

use rash_derive::FieldNames;

use yaml_rust::Yaml;

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

fn validate_task(task: &Yaml) -> Result<&Yaml> {
    let non_attrs: Vec<String> = task
        .clone()
        .into_hash()
        .unwrap()
        .iter()
        .filter(|(key, _)| !is_task_attr(key.as_str().unwrap()))
        .map(|(key, _)| key.as_str().unwrap().to_string())
        .collect();
    match non_attrs.len() > 1 {
        true => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Multiple possible modules found in task: {:?}", task),
            ))
        }
        false => (),
    };
    let possible_module = non_attrs.first().ok_or(Error::new(
        ErrorKind::NotFound,
        format!("Not module found in task: {:?}", task),
    ))?;
    match is_module(possible_module) {
        true => Ok(task),
        _ => Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Module `{}` not found in modules. Possibles values: {:?}",
                possible_module,
                MODULES.keys()
            ),
        )),
    }
}

fn find_module(task: &Yaml) -> Result<&Module> {
    let module_names: Vec<String> = task
        .clone()
        .into_hash()
        .unwrap()
        .iter()
        .filter(|(key, _)| is_module(key.as_str().unwrap()))
        .map(|(key, _)| key.as_str().unwrap().to_string())
        .collect();
    match module_names.len() > 1 {
        true => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Multiple modules found in task: {:?}", task),
            ))
        }
        false => (),
    };
    let module = module_names.first().ok_or(Error::new(
        ErrorKind::NotFound,
        format!("Not module found in task: {:?}", task),
    ))?;
    MODULES.get::<str>(module).ok_or(Error::new(
        ErrorKind::NotFound,
        format!("Module not found in modules: {:?}", MODULES.keys()),
    ))
}

impl Task {
    pub fn from(task: &Yaml) -> Result<Self> {
        validate_task(task)?;
        let module = find_module(task)?;
        Ok(Task {
            module: module.clone(),
            name: task["name"].as_str().map(String::from),
        })
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Task {
            module: Module::test_example(),
            name: None,
        }
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
        let task = Task::from(&yaml).unwrap();
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
        let task = Task::from(&yaml);
        assert!(task.is_err());
    }
}
