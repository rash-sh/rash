use crate::error::{Error, ErrorKind, Result};
use crate::facts::Facts;
use crate::modules::{Module, MODULES};

use rash_derive::FieldNames;

use std::collections::HashSet;

use tera::Tera;
use yaml_rust::Yaml;

#[cfg(test)]
use yaml_rust::YamlLoader;

/// Task is composed of Module and parameters to be executed in a concrete context
#[derive(Debug, Clone, PartialEq, FieldNames)]
pub struct Task {
    module: Module,
    params: Yaml,
    name: Option<String>,
}

pub type Tasks = Vec<Task>;

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
        trace!("new task: {:?}", yaml);
        TaskNew::from(yaml).validate_attrs()?.get_task()
    }

    fn render_string(s: &str, facts: Facts) -> Result<String> {
        let mut tera = Tera::default();
        tera.add_raw_template(s, &s)
            .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;
        tera.render(s, &facts)
            .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))
    }

    fn render_params(&self, facts: Facts) -> Result<Yaml> {
        let original_params = self.params.clone();
        match original_params.as_hash() {
            Some(hash) => match hash
                .clone()
                .iter()
                .map(|t| {
                    match Task::render_string(
                        // safe unwrap: validated attr
                        &t.1.clone().as_str().unwrap().to_string(),
                        facts.clone(),
                    ) {
                        Ok(s) => Ok((t.0.clone(), Yaml::String(s))),
                        Err(e) => Err(e),
                    }
                })
                .collect::<Result<_>>()
            {
                Ok(hash) => Ok(Yaml::Hash(hash)),
                Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
            },

            None => Ok(Yaml::String(Task::render_string(
                // safe unwrap: validated attr
                original_params.as_str().unwrap(),
                facts.clone(),
            )?)),
        }
    }

    pub fn exec(&self, facts: Facts) -> Result<Facts> {
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);
        let result = self.module.exec(self.render_params(facts.clone())?)?;
        info!(target: match result.get_changed() {
            true => "changed",
            false => "ok",
            },
            "{:?}",
            result.get_output().unwrap_or("".to_string())
        );
        Ok(facts)
    }

    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn get_rendered_name(&self, facts: Facts) -> Result<String> {
        Task::render_string(
            &self
                .name
                .clone()
                .ok_or(Error::new(ErrorKind::NotFound, "no name found"))?,
            facts,
        )
    }

    pub fn get_module(&self) -> Module {
        self.module.clone()
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Task {
            module: Module::test_example(),
            name: None,
            params: YamlLoader::load_from_str("cmd: ls")
                .unwrap()
                .first()
                .unwrap()
                .clone(),
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
            // safe unwrap: validated attr
            .map(|(key, _)| key.as_str().unwrap().to_string())
            .collect::<HashSet<String>>()
    }

    fn get_module_name<'a>(&'a self) -> Result<String> {
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
            .ok_or(Error::new(
                ErrorKind::NotFound,
                format!("Not module found in task: {:?}", self),
            ))
    }

    pub fn get_task<'task>(&self) -> Result<Task> {
        let module_name: &str = &self.get_module_name()?;
        Ok(Task {
            name: self.attrs["name"].as_str().map(String::from),
            module: MODULES
                .get::<str>(&module_name)
                .ok_or(Error::new(
                    ErrorKind::NotFound,
                    format!("Module not found in modules: {:?}", MODULES.keys()),
                ))?
                .clone(),
            params: self.attrs[module_name].clone(),
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
            .map(|(key, _)| {
                key.as_str().ok_or(Error::new(
                    ErrorKind::InvalidData,
                    format!("Key is not valid in {:?}", self.proto_attrs),
                ))
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

    use crate::facts;
    use crate::input::read_file;

    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;

    use tempfile::tempdir;
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
        let s = r#"
        name: 'Test task'
        no_module: 'example'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::new(yaml).unwrap_err();
        assert_eq!(task.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_task_execute() {
        let task = Task::test_example();
        let facts = facts::test_example();
        let result = task.exec(facts.clone()).unwrap();
        assert_eq!(result, facts);
    }

    fn get_yaml(s: String) -> Yaml {
        let doc = YamlLoader::load_from_str(&s).unwrap();
        doc.first().unwrap().clone()
    }

    #[test]
    fn test_read_tasks() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("entrypoint.rh");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(
            file,
            r#"
        #!/bin/rash
        - name: task 1
          command:
            foo: boo

        - name: task 2
          command: boo
        "#
        )
        .unwrap();
        let tasks = read_file(file_path).unwrap();
        assert_eq!(tasks.len(), 2);

        let s0 = r#"
        name: task 1
        command:
          foo: boo
        "#
        .to_owned();
        let yaml = get_yaml(s0);
        let task_0 = Task::from(&yaml);
        assert_eq!(tasks[0], task_0);

        let s1 = r#"
        name: task 2
        command: boo
        "#
        .to_owned();
        let yaml = get_yaml(s1);
        let task_1 = Task::from(&yaml);
        assert_eq!(tasks[1], task_1);
    }

    #[test]
    fn test_render_params() {
        let s0 = r#"
        name: task 1
        command:
          cmd: ls {{ directory }}
        "#
        .to_owned();
        let yaml = get_yaml(s0);
        let task = Task::from(&yaml);
        let facts = Facts::from_serialize(
            [("directory", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>(),
        )
        .unwrap();

        let rendered_params = task.render_params(facts).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_no_hash_map() {
        let s0 = r#"
        name: task 1
        command: ls {{ directory }}
        "#
        .to_owned();
        let yaml = get_yaml(s0);
        let task = Task::from(&yaml);
        let facts = Facts::from_serialize(
            [("directory", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>(),
        )
        .unwrap();

        let rendered_params = task.render_params(facts).unwrap();
        assert_eq!(rendered_params.as_str().unwrap(), "ls boo");
    }
}
