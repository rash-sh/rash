mod new;
mod valid;

use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};
use crate::task::new::TaskNew;
use crate::utils::tera::{is_render_string, render, render_as_json, render_string};
use crate::vars::Vars;

use rash_derive::FieldNames;

use std::process::exit;
use std::result::Result as StdResult;

use ipc_channel::ipc::IpcReceiver;
use ipc_channel::ipc::{self, IpcSender};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, setgid, setuid, ForkResult, Uid, User};
use serde_error::Error as SerdeError;
use serde_yaml::Value;

/// Parameters that can be set globally
pub struct GlobalParams<'a> {
    pub r#become: bool,
    pub become_user: &'a str,
    pub check_mode: bool,
}

impl Default for GlobalParams<'_> {
    fn default() -> Self {
        GlobalParams {
            r#become: Default::default(),
            become_user: "root",
            check_mode: Default::default(),
        }
    }
}

/// Main structure at definition level which prepares [`Module`] executions.
///
/// It implements a state machine using Rust Generics to enforce well done definitions.
/// Inspired by [Kanidm Entries](https://fy.blackhats.net.au/blog/html/2019/04/13/using_rust_generics_to_enforce_db_record_state.html).
///
/// [`Module`]: ../modules/struct.Module.html
#[derive(Debug, Clone, PartialEq, FieldNames)]
// ANCHOR: task
pub struct Task {
    /// run operations with become (does not imply password prompting).
    r#become: bool,
    /// run operations as this user (just works with become enabled).
    become_user: String,
    /// Run task in dry-run mode without modifications.
    check_mode: bool,
    /// Module could be any [`Module`] accessible by its name.
    ///
    /// [`Module`]: ../modules/struct.Module.html
    module: Module,
    /// Params are module execution params passed to [`Module::exec`].
    ///
    /// [`Module::exec`]: ../modules/struct.Module.html#method.exec
    params: Value,
    /// Template expression passed directly without {{ }};
    /// Overwrite changed field in [`ModuleResult`].
    ///
    /// [`ModuleResult`]: ../modules/struct.ModuleResult.html
    changed_when: Option<String>,
    /// Template expression passed directly without {{ }}; if true errors are ignored.
    ignore_errors: Option<bool>,
    /// Task name.
    name: Option<String>,
    /// `loop` receives a Template (with {{ }}) or a list to iterate over it.
    r#loop: Option<Value>,
    /// Variable name to store [`ModuleResult`].
    ///
    /// [`ModuleResult`]: ../modules/struct.ModuleResult.html
    register: Option<String>,
    /// Template expression passed directly without {{ }}; if false skip task execution.
    when: Option<String>,
}
// ANCHOR_END: task

/// A lists of [`Task`]
///
/// [`Task`]: struct.Task.html
pub type Tasks = Vec<Task>;

impl Task {
    /// Create a new Task from [`Value`].
    /// Enforcing all key values are valid using TaskNew and TaskValid internal states.
    ///
    /// All final values must be convertible to String and all keys must contain one module and
    /// [`Task`] fields.
    ///
    /// [`Task`]: struct.Task.html
    /// [`Value`]: ../../serde_yaml/enum.Value.html
    pub fn new(yaml: &Value, global_params: &GlobalParams) -> Result<Self> {
        trace!("new task: {:?}", yaml);
        TaskNew::from(yaml)
            .validate_attrs()?
            .get_task(global_params)
    }

    #[inline(always)]
    fn is_attr(attr: &str) -> bool {
        Self::get_field_names().contains(attr)
    }

    fn render_params(&self, vars: Vars) -> Result<Value> {
        let original_params = self.params.clone();
        match original_params {
            Value::Mapping(map) => match map
                .iter()
                .filter_map(|t| match t.1 {
                    Value::String(s) => match render_string(s, &vars) {
                        Ok(s) => Some(Ok((t.0.clone(), Value::String(s)))),
                        Err(e) if e.kind() == ErrorKind::OmitParam => None,
                        Err(e) => Some(Err(e)),
                    },
                    Value::Sequence(x) => match x
                        .iter()
                        .map(|value| render(value.clone(), &vars))
                        .collect::<Result<Vec<Value>>>()
                    {
                        Ok(rendered_vec) => Some(Ok((t.0.clone(), Value::Sequence(rendered_vec)))),
                        Err(e) => Some(Err(e)),
                    },
                    _ => Some(Ok((t.0.clone(), t.1.clone()))),
                })
                .collect::<Result<_>>()
            {
                Ok(map) => Ok(Value::Mapping(map)),
                Err(e) => Err(e),
            },

            Value::String(s) => Ok(Value::String(render_string(&s, &vars)?)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{:?} must be a mapping or  a string", original_params),
            )),
        }
    }

    fn is_exec(&self, vars: &Vars) -> Result<bool> {
        trace!("when: {:?}", &self.when);
        match &self.when {
            Some(s) => is_render_string(s, vars),
            None => Ok(true),
        }
    }

    fn get_iterator(value: &Value, vars: Vars) -> Result<Vec<Value>> {
        match value.as_sequence() {
            Some(v) => Ok(v
                .iter()
                .map(|item| render(item.clone(), &vars))
                .collect::<Result<Vec<Value>>>()?),
            None => Err(Error::new(ErrorKind::NotFound, "loop is not iterable")),
        }
    }

    fn render_iterator(&self, vars: Vars) -> Result<Vec<Value>> {
        // safe unwrap, previous verification self.r#loop.is_some()
        let loop_some = self.r#loop.clone().unwrap();
        match loop_some.as_str() {
            Some(s) => {
                let value: Value = serde_yaml::from_str(&render_as_json(s, &vars)?)?;
                match value.as_str() {
                    Some(_) => Ok(vec![value]),
                    None => Task::get_iterator(&value, vars),
                }
            }
            None => Task::get_iterator(&loop_some, vars),
        }
    }

    fn is_changed(&self, result: &ModuleResult, vars: &Vars) -> Result<bool> {
        trace!("changed_when: {:?}", &self.changed_when);
        match &self.changed_when {
            Some(s) => is_render_string(s, vars),
            None => Ok(result.get_changed()),
        }
    }

    fn exec_module_rendered_with_user(
        &self,
        rendered_params: &Value,
        vars: &Vars,
        user: User,
    ) -> Result<Vars> {
        match setgid(user.gid) {
            Ok(_) => match setuid(user.uid) {
                Ok(_) => self.exec_module_rendered(rendered_params, vars),
                Err(_) => Err(Error::new(
                    ErrorKind::Other,
                    format!("gid cannot be changed to {}", user.gid),
                )),
            },
            Err(_) => Err(Error::new(
                ErrorKind::Other,
                format!("uid cannot be changed to {}", user.uid),
            )),
        }
    }

    fn exec_module_rendered(&self, rendered_params: &Value, vars: &Vars) -> Result<Vars> {
        match self
            .module
            .exec(rendered_params.clone(), vars.clone(), self.check_mode)
        {
            Ok((result, result_vars)) => {
                info!(target: if self.is_changed(&result, &result_vars)? {"changed"} else { "ok"},
                    "{}",
                    result.get_output().unwrap_or_else(
                        || format!("{:?}", rendered_params)
                    )
                );
                let mut new_vars = result_vars;
                if self.register.is_some() {
                    let register = self.register.as_ref().unwrap();
                    trace!("register {:?} in {:?}", &result, register);
                    new_vars.insert(register, &result);
                }
                Ok(new_vars)
            }
            Err(e) => match self.ignore_errors {
                Some(is_true) => {
                    if is_true {
                        info!(target: "ignoring", "{}", e);
                        Ok(vars.clone())
                    } else {
                        Err(e)
                    }
                }
                None => Err(e),
            },
        }
    }

    fn exec_module(&self, vars: Vars) -> Result<Vars> {
        if self.is_exec(&vars)? {
            let rendered_params = self.render_params(vars.clone())?;

            match self.r#become {
                true => {
                    let user = match User::from_name(&self.become_user)? {
                        Some(user) => Ok(user),
                        None => match self.become_user.parse::<u32>().map(Uid::from_raw) {
                            Ok(uid) => match User::from_uid(uid)? {
                                Some(user) => Ok(user),
                                None => Err(Error::new(
                                    ErrorKind::NotFound,
                                    format!("user: {} not found", &self.become_user),
                                )),
                            },
                            Err(e) => Err(Error::new(ErrorKind::Other, e)),
                        },
                    }?;

                    if user.uid != Uid::current() {
                        if self.module.get_name() == "command"
                            && rendered_params["transfer_pid"].as_bool().unwrap_or(false)
                        {
                            return self.exec_module_rendered_with_user(
                                &rendered_params,
                                &vars,
                                user,
                            );
                        }

                        #[allow(clippy::type_complexity)]
                        let (tx, rx): (
                            IpcSender<StdResult<String, SerdeError>>,
                            IpcReceiver<StdResult<String, SerdeError>>,
                        ) = ipc::channel().unwrap();

                        match unsafe { fork() } {
                            Ok(ForkResult::Child) => {
                                trace!("change uid to: {}", user.uid);
                                trace!("change gid to: {}", user.gid);
                                let result = self.exec_module_rendered_with_user(
                                    &rendered_params,
                                    &vars,
                                    user,
                                );

                                trace!("send result: {:?}", result);
                                tx.send(
                                    result
                                        .map(|x| x.into_json().to_string())
                                        .map_err(|e| SerdeError::new(&e)),
                                )
                                .unwrap_or_else(|e| {
                                    error!("child failed to send result: {}", e);
                                    exit(1)
                                });
                                exit(0);
                            }
                            Ok(ForkResult::Parent { child, .. }) => {
                                match waitpid(child, None) {
                                    Ok(WaitStatus::Exited(_, 0)) => Ok(()),
                                    Ok(WaitStatus::Exited(_, exit_code)) => Err(Error::new(
                                        ErrorKind::SubprocessFail,
                                        format!("child failed with exit_code {}", exit_code),
                                    )),
                                    Err(e) => Err(Error::new(ErrorKind::Other, e)),
                                    _ => Err(Error::new(
                                        ErrorKind::SubprocessFail,
                                        format!("child {} unknown status", child),
                                    )),
                                }?;
                                rx.recv()
                                    .unwrap_or_else(|e| {
                                        Err(SerdeError::new(&Error::new(
                                            ErrorKind::Other,
                                            // ipc::IpcError doesn't implement std::error:Error
                                            format!("{:?}", e),
                                        )))
                                    })
                                    .map(|x| {
                                        // safe unwrap: this value comes from vars.into_json()
                                        tera::Context::from_value(serde_json::from_str(&x).unwrap())
                                            // safe unwrap: json is object because comes from
                                            // child tera::Context
                                            .unwrap()
                                    })
                                    .map_err(|e| Error::new(ErrorKind::Other, e))
                            }
                            Err(e) => Err(Error::new(ErrorKind::Other, e)),
                        }
                    } else {
                        self.exec_module_rendered(&rendered_params, &vars)
                    }
                }
                false => self.exec_module_rendered(&rendered_params, &vars),
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
            &vars,
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
            r#become: GlobalParams::default().r#become,
            become_user: GlobalParams::default().become_user.to_string(),
            check_mode: GlobalParams::default().check_mode,
            module: Module::test_example(),
            params: serde_yaml::from_str("cmd: ls").unwrap(),
            changed_when: None,
            ignore_errors: None,
            name: None,
            r#loop: None,
            register: None,
            when: None,
        }
    }
}

#[cfg(test)]
impl From<Value> for Task {
    fn from(value: Value) -> Self {
        TaskNew::from(&value)
            .validate_attrs()
            .unwrap()
            .get_task(&GlobalParams::default())
            .unwrap()
    }
}

pub fn parse_file(tasks_file: &str, global_params: &GlobalParams) -> Result<Tasks> {
    let tasks: Vec<Value> = serde_yaml::from_str(tasks_file)?;
    tasks
        .into_iter()
        .map(|task| Task::new(&task, global_params))
        .collect::<Result<Tasks>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::modules::MODULES;
    use crate::vars;

    use std::collections::HashMap;

    use tera::Context;

    #[test]
    fn test_from_yaml() {
        let s: String = r#"
            name: 'Test task'
            command: 'example'
            "#
        .to_owned();
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task_err = Task::new(&yaml, &GlobalParams::default()).unwrap_err();
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task_err = Task::new(&yaml, &GlobalParams::default()).unwrap_err();
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_parsed_bool() {
        let s: String = r#"
            when: "boo | bool"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "false")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_false() {
        let s: String = r#"
            when: "boo != 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_bool_false() {
        let s: String = r#"
            when: false
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_array() {
        let s: String = r#"
            when:
              - true
              - "boo == 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_array_one_false() {
        let s: String = r#"
            when:
              - false
              - "boo == 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());
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
        let vars = vars::from_iter(vec![].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![Value::from(1), Value::from(2), Value::from(3)]
        );
    }

    #[test]
    fn test_is_changed() {
        let s: String = r#"
            changed_when: "boo == 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task
            .is_changed(&ModuleResult::new(false, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_bool_true() {
        let s: String = r#"
            changed_when: true
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task
            .is_changed(&ModuleResult::new(false, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_bool_false() {
        let s: String = r#"
            changed_when: false
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task
            .is_changed(&ModuleResult::new(true, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_string_false() {
        let s: String = r#"
            changed_when: "false"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task
            .is_changed(&ModuleResult::new(false, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_false() {
        let s: String = r#"
            changed_when: "boo != 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task
            .is_changed(&ModuleResult::new(false, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_array() {
        let s: String = r#"
            changed_when:
              - "boo == 'test'"
              - true
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task
            .is_changed(&ModuleResult::new(false, None, None), &vars)
            .unwrap(),);
    }

    #[test]
    fn test_is_changed_array_false() {
        let s: String = r#"
            changed_when:
              - "boo == 'test'"
              - false
            command: 'example'
            "#
        .to_owned();
        let vars = vars::from_iter(vec![("boo", "test")].into_iter());
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task
            .is_changed(&ModuleResult::new(true, None, None), &vars)
            .unwrap(),);
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        let result = task.exec(vars).unwrap();
        let mut expected = Context::new();
        expected.insert("item", &json!(3));
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![Value::from(0), Value::from(1), Value::from(2)]
        );
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
        let yaml: Value = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![Value::from("test"), Value::from(2)]
        );
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
        let file = r#"
            #!/bin/rash
            - name: task 1
              command:
                foo: boo

            - name: task 2
              command: boo
            "#;

        let tasks = parse_file(file, &GlobalParams::default()).unwrap();
        assert_eq!(tasks.len(), 2);

        let s0 = r#"
            name: task 1
            command:
              foo: boo
            "#
        .to_owned();
        let yaml: Value = serde_yaml::from_str(&s0).unwrap();
        let task_0 = Task::from(yaml);
        assert_eq!(tasks[0], task_0);

        let s1 = r#"
            name: task 2
            command: boo
            "#
        .to_owned();
        let yaml: Value = serde_yaml::from_str(&s1).unwrap();
        let task_1 = Task::from(yaml);
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
        let yaml: Value = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
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
        let yaml: Value = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
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
