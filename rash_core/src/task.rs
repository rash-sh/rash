use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult, MODULES};
use crate::utils::get_yaml;
use crate::vars::Vars;

use rash_derive::FieldNames;

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use tera::Tera;
use yaml_rust::{Yaml, YamlLoader};

/// Main structure at definition level which prepare [`Module`] executions.
///
/// It implements a state machine using Rust Generics to enforce well done definitions.
/// Inspired by [Kanidm Entries](https://fy.blackhats.net.au/blog/html/2019/04/13/using_rust_generics_to_enforce_db_record_state.html).
///
/// [`Module`]: ../modules/struct.Module.html
#[derive(Debug, Clone, PartialEq, FieldNames)]
pub struct Task {
    module: Module,
    params: Yaml,
    name: Option<String>,
    when: Option<String>,
    register: Option<String>,
    r#loop: Option<Yaml>,
}

/// A lists of [`Task`]
///
/// [`Task`]: struct.Task.html
pub type Tasks = Vec<Task>;

#[inline(always)]
fn is_module(module: &str) -> bool {
    MODULES.get(module).is_some()
}

impl Task {
    /// Create a new Task from [`Yaml`].
    /// Enforcing all key values are valid using TaskNew and TaskValid internal states.
    ///
    /// All final values must be convertible to String and all keys must contain one module and
    /// [`Task`] fields.
    ///
    /// [`Task`]: struct.Task.html
    /// [`Yaml`]: ../../yaml_rust/struct.Yaml.html
    pub fn new(yaml: &Yaml) -> Result<Self> {
        trace!("new task: {:?}", yaml);
        TaskNew::from(yaml).validate_attrs()?.get_task()
    }

    #[inline(always)]
    fn is_attr(attr: &str) -> bool {
        Self::get_field_names().contains(attr)
    }

    fn render_string(s: &str, vars: Vars) -> Result<String> {
        let mut tera = Tera::default();
        tera.render_str(s, &vars)
            .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))
    }

    fn render_params(&self, vars: Vars) -> Result<Yaml> {
        let original_params = self.params.clone();
        match original_params.as_hash() {
            Some(hash) => match hash
                .clone()
                .iter()
                .map(|t| match &t.1.clone().as_str() {
                    Some(s) => match Task::render_string(s, vars.clone()) {
                        Ok(s) => Ok((t.0.clone(), Yaml::String(s))),
                        Err(e) => Err(e),
                    },
                    None => Ok((t.0.clone(), t.1.clone())),
                })
                .collect::<Result<_>>()
            {
                Ok(hash) => Ok(Yaml::Hash(hash)),
                Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
            },

            None => Ok(Yaml::String(Task::render_string(
                // safe unwrap: validated attr
                original_params.as_str().unwrap(),
                vars,
            )?)),
        }
    }

    fn is_exec(&self, vars: Vars) -> Result<bool> {
        match &self.when {
            Some(s) => {
                match Task::render_string(
                    &format!("{{% if {} %}}true{{% else %}}false{{% endif %}}", s),
                    vars,
                )?
                .as_str()
                {
                    "false" => Ok(false),
                    _ => Ok(true),
                }
            }
            None => Ok(true),
        }
    }

    fn get_iterator(yaml: &Yaml, vars: Vars) -> Result<Vec<String>> {
        match yaml.as_vec() {
            Some(v) => Ok(v
                .iter()
                .map(|item| match item.clone() {
                    Yaml::Real(s) | Yaml::String(s) => Ok(Task::render_string(&s, vars.clone())?),
                    Yaml::Integer(x) => Ok(Task::render_string(&x.to_string(), vars.clone())?),
                    _ => Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("{:?} is not a valid string", item),
                    )),
                })
                .collect::<Result<Vec<String>>>()?),
            None => Err(Error::new(ErrorKind::NotFound, "loop is not iterable")),
        }
    }

    fn render_iterator(&self, vars: Vars) -> Result<Vec<String>> {
        // safe unwrap
        let loop_some = self.r#loop.clone().unwrap();
        match loop_some.as_str() {
            Some(s) => {
                let yaml = get_yaml(&Task::render_string(&s, vars.clone())?)?;
                match yaml.as_str() {
                    Some(s) => Ok(vec![s.to_string()]),
                    None => Task::get_iterator(&yaml, vars),
                }
            }
            None => Task::get_iterator(&loop_some, vars),
        }
    }

    /// Execute [`Module`] rendering `self.params` with [`Vars`].
    ///
    /// [`Module`]: ../modules/struct.Module.html
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn exec(&self, vars: Vars) -> Result<Vars> {
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);

        let new_vars = if self.is_exec(vars.clone())? {
            let result_json_vars: Result<(Value, Vars)> = if self.r#loop.is_some() {
                let results_with_vars = self
                    .render_iterator(vars.clone())?
                    .into_iter()
                    .map(|item| {
                        let mut exec_vars = vars.clone();
                        exec_vars.insert("item", &item);
                        let result_wrapped = self
                            .module
                            .exec(self.render_params(exec_vars.clone())?, exec_vars);
                        match result_wrapped {
                            Ok((result, new_vars)) => {
                                info!(target: if result.get_changed() {"changed"} else { "ok"},
                                    "{:?}",
                                    result.get_output().unwrap_or_else(|| "".to_string())
                                );
                                Ok((result, new_vars))
                            }
                            Err(e) => {
                                error!("{}", e);
                                Err(e)
                            }
                        }
                    })
                    .collect::<Result<Vec<(ModuleResult, Vars)>>>()?;
                let mut new_vars = Vars::new();
                results_with_vars
                    .iter()
                    .for_each(|(_, vars)| new_vars.extend(vars.clone()));
                let results: Vec<ModuleResult> = results_with_vars
                    .iter()
                    .map(|(result, _)| result)
                    .cloned()
                    .collect();
                Ok((json!(results), new_vars))
            } else {
                let (result, new_vars) =
                    self.module.exec(self.render_params(vars.clone())?, vars)?;
                info!(target: if result.get_changed() {"changed"} else { "ok"},
                    "{:?}",
                    result.get_output().unwrap_or_else(|| "".to_string())
                );
                Ok((json!(result), new_vars))
            };
            let json_vars = result_json_vars?;
            let result = json_vars.0;
            let mut new_vars = json_vars.1;
            if self.register.is_some() {
                new_vars.insert(self.register.as_ref().unwrap(), &result);
            }
            new_vars
        } else {
            info!(target: "skipping", "");
            vars
        };

        Ok(new_vars)
    }

    /// Return name.
    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    /// Return name rendered with [`Vars`].
    ///
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn get_rendered_name(&self, vars: Vars) -> Result<String> {
        Task::render_string(
            &self
                .name
                .clone()
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no name found"))?,
            vars,
        )
    }

    /// Return [`Module`].
    ///
    /// [`Module`]: ../modules/struct.Module.html
    pub fn get_module(&self) -> Module {
        self.module.clone()
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Task {
            module: Module::test_example(),
            name: None,
            when: None,
            register: None,
            r#loop: None,
            params: YamlLoader::load_from_str("cmd: ls")
                .unwrap()
                .first()
                .unwrap()
                .clone(),
        }
    }
}

/// TaskValid is a ProtoTask with attrs verified (one module and valid attrs)
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

/// TaskNew is a new task without Yaml verified
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
    /// Validate all `proto_attrs` can be represented as String and are task fields or modules
    pub fn validate_attrs(&self) -> Result<TaskValid> {
        let attrs_hash = self.proto_attrs.clone().into_hash().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Task is not a dict {:?}", self.proto_attrs),
            )
        })?;
        let attrs_vec = attrs_hash
            .iter()
            .map(|(key, _)| {
                key.as_str().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Key is not valid in {:?}", self.proto_attrs),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !attrs_vec
            .into_iter()
            .all(|key| is_module(key) || Task::is_attr(key))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Keys are not valid in {:?} must be attr or module",
                    self.proto_attrs
                ),
            ));
        }
        Ok(TaskValid {
            attrs: self.proto_attrs.clone(),
        })
    }
}

pub fn read_file(tasks_file_path: PathBuf) -> Result<Tasks> {
    trace!("reading tasks from: {:?}", tasks_file_path);
    let tasks_file = fs::read_to_string(tasks_file_path)
        .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;

    let docs = YamlLoader::load_from_str(&tasks_file)?;
    let yaml = docs.first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Docs not contain yaml: {:?}", docs),
        )
    })?;

    yaml.clone()
        .into_iter()
        .map(|task| Task::new(&task))
        .collect::<Result<Tasks>>()
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

    use crate::vars;

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
        let task_err = Task::new(yaml).unwrap_err();
        assert_eq!(task_err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_from_yaml_invalid_attr() {
        let s = r#"
        name: 'Test task'
        command: 'example'
        invalid_attr: 'foo'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task_err = Task::new(yaml).unwrap_err();
        assert_eq!(task_err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_is_exec() {
        let s: String = r#"
        when: "boo == 'test'"
        command: 'example'
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.is_exec(vars).unwrap(), true);
    }

    #[test]
    fn test_is_exec_bool() {
        let s: String = r#"
        when: "boo | bool"
        command: 'example'
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "false")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.is_exec(vars).unwrap(), false);
    }

    #[test]
    fn test_is_exec_false() {
        let s: String = r#"
        when: "boo != 'test'"
        command: 'example'
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.is_exec(vars).unwrap(), false);
    }

    #[test]
    fn test_render_iterator() {
        let s: String = r#"
        command: 'example'
        loop:
          - 1
          - 2
          - 3
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.render_iterator(vars).unwrap(), vec!["1", "2", "3"]);
    }

    #[test]
    fn test_render_iterator_var() {
        let s: String = r#"
        command: 'example'
        loop: "{{ range(end=3) }}"
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.render_iterator(vars).unwrap(), vec!["0", "1", "2"]);
    }

    #[test]
    fn test_render_iterator_list_with_vars() {
        let s: String = r#"
        command: 'example'
        loop:
          - "{{ boo }}"
          - 2
        "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        assert_eq!(task.render_iterator(vars).unwrap(), vec!["test", "2"]);
    }

    #[test]
    fn test_task_execute() {
        let task = Task::test_example();
        let vars = vars::from_iter(vec![].into_iter());
        let result = task.exec(vars.clone()).unwrap();
        assert_eq!(result, vars);
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
        let yaml = get_yaml(&s0).unwrap();
        let task_0 = Task::from(&yaml);
        assert_eq!(tasks[0], task_0);

        let s1 = r#"
        name: task 2
        command: boo
        "#
        .to_owned();
        let yaml = get_yaml(&s1).unwrap();
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
        let yaml = get_yaml(&s0).unwrap();
        let task = Task::from(&yaml);
        let vars = Vars::from_serialize(
            [("directory", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>(),
        )
        .unwrap();

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_no_hash_map() {
        let s0 = r#"
        name: task 1
        command: ls {{ directory }}
        "#
        .to_owned();
        let yaml = get_yaml(&s0).unwrap();
        let task = Task::from(&yaml);
        let vars = Vars::from_serialize(
            [("directory", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<HashMap<String, String>>(),
        )
        .unwrap();

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params.as_str().unwrap(), "ls boo");
    }
}
