mod new;
mod valid;

use crate::error::{Error, ErrorKind, Result};
use crate::modules::Module;
use crate::task::new::TaskNew;
use crate::utils::get_yaml;
use crate::utils::tera::{is_render_string, render_as_json, render_string};
use crate::vars::Vars;

use rash_derive::FieldNames;

use std::fs;
use std::path::PathBuf;

use yaml_rust::{Yaml, YamlLoader};

/// Main structure at definition level which prepares [`Module`] executions.
///
/// It implements a state machine using Rust Generics to enforce well done definitions.
/// Inspired by [Kanidm Entries](https://fy.blackhats.net.au/blog/html/2019/04/13/using_rust_generics_to_enforce_db_record_state.html).
///
/// [`Module`]: ../modules/struct.Module.html
#[derive(Debug, Clone, PartialEq, FieldNames)]
// ANCHOR: task
pub struct Task {
    /// Module could be any [`Module`] accessible by its name.
    ///
    /// [`Module`]: ../modules/struct.Module.html
    module: Module,
    /// Params are module execution params passed to [`Module::exec`].
    ///
    /// [`Module::exec`]: ../modules/struct.Module.html#method.exec
    params: Yaml,
    /// Run task in dry-run mode without modifications.
    check_mode: bool,
    /// Task name.
    name: Option<String>,
    /// Template expression passed directly without {{ }}; if false skip task execution.
    when: Option<String>,
    /// Variable name to store [`ModuleResult`].
    ///
    /// [`ModuleResult`]: ../modules/struct.ModuleResult.html
    register: Option<String>,
    /// Template expression passed directly without {{ }}; if true errors are ignored.
    ignore_errors: Option<bool>,
    /// `loop` field receives a Template (with {{ }}) or a list to iterate over it.
    r#loop: Option<Yaml>,
}
// ANCHOR_END: task

/// A lists of [`Task`]
///
/// [`Task`]: struct.Task.html
pub type Tasks = Vec<Task>;

impl Task {
    /// Create a new Task from [`Yaml`].
    /// Enforcing all key values are valid using TaskNew and TaskValid internal states.
    ///
    /// All final values must be convertible to String and all keys must contain one module and
    /// [`Task`] fields.
    ///
    /// [`Task`]: struct.Task.html
    /// [`Yaml`]: ../../yaml_rust/struct.Yaml.html
    pub fn new(yaml: &Yaml, check: bool) -> Result<Self> {
        trace!("new task: {:?}", yaml);
        TaskNew::from(yaml).validate_attrs()?.get_task(check)
    }

    #[inline(always)]
    fn is_attr(attr: &str) -> bool {
        Self::get_field_names().contains(attr)
    }

    fn render_params(&self, vars: Vars) -> Result<Yaml> {
        let original_params = self.params.clone();
        match original_params.as_hash() {
            Some(hash) => match hash
                .clone()
                .iter()
                .map(|t| match &t.1.clone().as_str() {
                    Some(s) => match render_string(s, vars.clone()) {
                        Ok(s) => Ok((t.0.clone(), Yaml::String(s))),
                        Err(e) => Err(e),
                    },
                    None => match t.1.clone().as_vec() {
                        Some(x) => match x
                            .iter()
                            .map(|yaml| match yaml.as_str() {
                                Some(s) => Ok(s.to_string()),
                                None => Err(Error::new(
                                    ErrorKind::InvalidData,
                                    format!("{:?} invalid string", yaml),
                                )),
                            })
                            .map(|result_s| match result_s {
                                Ok(s) => match render_string(&s, vars.clone()) {
                                    Ok(rendered_s) => Ok(Yaml::String(rendered_s)),
                                    Err(e) => Err(e),
                                },
                                Err(e) => Err(e),
                            })
                            .collect::<Result<Vec<Yaml>>>()
                        {
                            Ok(rendered_vec) => Ok((t.0.clone(), Yaml::Array(rendered_vec))),
                            Err(e) => Err(e),
                        },
                        None => Ok((t.0.clone(), t.1.clone())),
                    },
                })
                .collect::<Result<_>>()
            {
                Ok(hash) => Ok(Yaml::Hash(hash)),
                Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
            },

            None => Ok(Yaml::String(render_string(
                original_params.as_str().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("{:?} must be a string", original_params),
                    )
                })?,
                vars,
            )?)),
        }
    }

    fn is_exec(&self, vars: Vars) -> Result<bool> {
        match &self.when {
            Some(s) => is_render_string(s, vars),
            None => Ok(true),
        }
    }

    fn get_iterator(yaml: &Yaml, vars: Vars) -> Result<Vec<String>> {
        match yaml.as_vec() {
            Some(v) => Ok(v
                .iter()
                .map(|item| match item.clone() {
                    Yaml::Real(s) | Yaml::String(s) => Ok(render_string(&s, vars.clone())?),
                    Yaml::Integer(x) => Ok(render_string(&x.to_string(), vars.clone())?),
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
        // safe unwrap, previous verification self.r#loop.is_some()
        let loop_some = self.r#loop.clone().unwrap();
        match loop_some.as_str() {
            Some(s) => {
                let yaml = get_yaml(&render_as_json(s, vars.clone())?)?;
                match yaml.as_str() {
                    Some(s) => Ok(vec![s.to_string()]),
                    None => Task::get_iterator(&yaml, vars),
                }
            }
            None => Task::get_iterator(&loop_some, vars),
        }
    }

    fn exec_module(&self, vars: Vars) -> Result<Vars> {
        if self.is_exec(vars.clone())? {
            let rendered_params = self.render_params(vars.clone())?;
            match self
                .module
                .exec(rendered_params.clone(), vars.clone(), self.check_mode)
            {
                Ok((result, result_vars)) => {
                    info!(target: if result.get_changed() {"changed"} else { "ok"},
                        "{}",
                        result.get_output().unwrap_or_else(
                            || format!("{:?}", rendered_params)
                        )
                    );
                    let mut new_vars = result_vars;
                    if self.register.is_some() {
                        new_vars.insert(self.register.as_ref().unwrap(), &result);
                    }
                    Ok(new_vars)
                }
                Err(e) => match self.ignore_errors {
                    Some(is_true) => {
                        if is_true {
                            info!(target: "ignoring", "{}", e);
                            Ok(vars)
                        } else {
                            Err(e)
                        }
                    }
                    None => Err(e),
                },
            }
        } else {
            info!(target: "skipping", "");
            Ok(vars)
        }
    }

    /// Execute [`Module`] rendering `self.params` with [`Vars`].
    ///
    /// [`Module`]: ../modules/struct.Module.html
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn exec(&self, vars: Vars) -> Result<Vars> {
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);

        if self.r#loop.is_some() {
            let mut new_vars = vars.clone();
            for item in self.render_iterator(vars)?.into_iter() {
                new_vars.insert("item", &item);
                new_vars = self.exec_module(new_vars.clone())?;
            }
            Ok(new_vars)
        } else {
            let new_vars = self.exec_module(vars)?;
            Ok(new_vars)
        }
    }

    /// Return name.
    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    /// Return name rendered with [`Vars`].
    ///
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn get_rendered_name(&self, vars: Vars) -> Result<String> {
        render_string(
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
            check_mode: false,
            when: None,
            register: None,
            ignore_errors: None,
            r#loop: None,
            params: YamlLoader::load_from_str("cmd: ls")
                .unwrap()
                .first()
                .unwrap()
                .clone(),
        }
    }
}

pub fn read_file(tasks_file_path: PathBuf, check: bool) -> Result<Tasks> {
    trace!("reading tasks from: {:?}", tasks_file_path);
    let tasks_file =
        fs::read_to_string(tasks_file_path).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    let docs = YamlLoader::load_from_str(&tasks_file)?;
    let yaml = docs.first().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Docs not contain yaml: {:?}", docs),
        )
    })?;

    yaml.clone()
        .into_iter()
        .map(|task| Task::new(&task, check))
        .collect::<Result<Tasks>>()
}

#[cfg(test)]
impl From<&Yaml> for Task {
    fn from(yaml: &Yaml) -> Self {
        TaskNew::from(yaml)
            .validate_attrs()
            .unwrap()
            .get_task(false)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::modules::MODULES;
    use crate::vars;

    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;

    use tempfile::tempdir;
    use tera::Context;
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
        let task_err = Task::new(yaml, false).unwrap_err();
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
        let task_err = Task::new(yaml, false).unwrap_err();
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
    fn test_when_in_loop() {
        let s: String = r#"
        command: echo 'example'
        loop:
          - 1
          - 2
          - 3
        when: item == 1
        "#
        .to_owned();
        let vars = vars::from_iter(vec![].into_iter());
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(yaml);
        let result = task.exec(vars).unwrap();
        let mut expected = Context::new();
        expected.insert("item", "3");
        assert_eq!(result, expected);
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
        let tasks = read_file(file_path, false).unwrap();
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
