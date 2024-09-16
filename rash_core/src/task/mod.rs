mod new;
mod valid;

use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::{is_render_string, render, render_force_string, render_map, render_string};
use crate::modules::{Module, ModuleResult};
use crate::task::new::TaskNew;

use rash_derive::FieldNames;

use std::process::exit;
use std::result::Result as StdResult;

use ipc_channel::ipc::{self, IpcReceiver, IpcSender};
use minijinja::{context, Value};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, setgid, setuid, ForkResult, Uid, User};
use serde_error::Error as SerdeError;
use serde_yaml::Value as YamlValue;

/// Main structure at definition level which prepares [`Module`] executions.
///
/// It implements a state machine using Rust Generics to enforce well done definitions.
/// Inspired by [Kanidm Entries](https://fy.blackhats.net.au/blog/html/2019/04/13/using_rust_generics_to_enforce_db_record_state.html).
///
/// [`Module`]: ../modules/trait.Module.html
#[derive(Debug, Clone, FieldNames)]
// ANCHOR: task
pub struct Task<'a> {
    /// Run operations with become (does not imply password prompting).
    r#become: bool,
    /// Run operations as this user (just works with become enabled).
    become_user: String,
    /// Run task in dry-run mode without modifications.
    check_mode: bool,
    /// Module could be any [`Module`] accessible by its name.
    ///
    /// [`Module`]: ../modules/trait.Module.html
    module: &'static dyn Module,
    /// Params are module execution params passed to [`Module::exec`].
    ///
    /// [`Module::exec`]: ../modules/trait.Module.html#method.exec
    params: YamlValue,
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
    r#loop: Option<YamlValue>,
    /// Variable name to store [`ModuleResult`].
    ///
    /// [`ModuleResult`]: ../modules/struct.ModuleResult.html
    register: Option<String>,
    /// Variables definition with task scope.
    vars: Option<YamlValue>,
    /// Template expression passed directly without {{ }}; if false skip task execution.
    when: Option<String>,
    /// Global parameters.
    global_params: &'a GlobalParams<'a>,
}
// ANCHOR_END: task

/// A lists of [`Task`]
///
/// [`Task`]: struct.Task.html
pub type Tasks<'a> = Vec<Task<'a>>;

impl<'a> Task<'a> {
    /// Create a new Task from [`Value`].
    /// Enforcing all key values are valid using TaskNew and TaskValid internal states.
    ///
    /// All final values must be convertible to String and all keys must contain one module and
    /// [`Task`] fields.
    ///
    /// [`Task`]: struct.Task.html
    /// [`Value`]: ../../serde_yaml/enum.Value.html
    pub fn new(yaml: &YamlValue, global_params: &'a GlobalParams) -> Result<Self> {
        trace!("new task: {:?}", yaml);
        TaskNew::from(yaml)
            .validate_attrs()?
            .get_task(global_params)
    }

    #[inline(always)]
    fn is_attr(attr: &str) -> bool {
        Self::get_field_names().contains(attr)
    }

    #[inline(always)]
    fn extend_vars(&self, additional_vars: Value) -> Result<Value> {
        match self.vars.clone() {
            Some(v) => {
                trace!("extend vars: {:?}", &v);
                let rendered_value = match render(v.clone(), &additional_vars) {
                    Ok(v) => Value::from_serialize(v),
                    Err(e) if e.kind() == ErrorKind::OmitParam => context! {},
                    Err(e) => return Err(e),
                };
                Ok(context! { ..rendered_value, ..additional_vars})
            }
            None => Ok(additional_vars),
        }
    }

    fn render_params(&self, vars: Value) -> Result<YamlValue> {
        let extended_vars = self.extend_vars(vars)?;

        let original_params = self.params.clone();
        match original_params {
            YamlValue::Mapping(x) => render_map(
                x.clone(),
                &extended_vars,
                self.module.force_string_on_params(),
            ),
            YamlValue::String(s) => Ok(YamlValue::String(render_string(&s, &extended_vars)?)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{original_params:?} must be a mapping or a string"),
            )),
        }
    }

    fn is_exec(&self, vars: &Value) -> Result<bool> {
        trace!("when: {:?}", &self.when);
        match &self.when {
            Some(s) => {
                let extended_vars = self.extend_vars(vars.clone())?;
                is_render_string(s, &extended_vars)
            }
            None => Ok(true),
        }
    }

    fn get_iterator(value: &YamlValue, vars: Value) -> Result<Vec<YamlValue>> {
        match value.as_sequence() {
            Some(v) => Ok(v
                .iter()
                .map(|item| render_force_string(item.clone(), &vars))
                .collect::<Result<Vec<YamlValue>>>()?),
            None => Err(Error::new(ErrorKind::NotFound, "loop is not iterable")),
        }
    }

    fn render_iterator(&self, vars: Value) -> Result<Vec<YamlValue>> {
        // safe unwrap, previous verification self.r#loop.is_some()
        let loop_some = self.r#loop.clone().unwrap();

        let extended_vars = self.extend_vars(context! {item => "",..vars})?;
        match loop_some.as_str() {
            Some(s) => {
                let value: YamlValue = serde_yaml::from_str(&render_string(s, &extended_vars)?)?;
                match value.as_str() {
                    Some(_) => Ok(vec![value]),
                    None => Task::get_iterator(&value, extended_vars),
                }
            }
            None => Task::get_iterator(&loop_some, extended_vars),
        }
    }

    fn is_changed(&self, result: &ModuleResult, vars: &Value) -> Result<bool> {
        trace!("changed_when: {:?}", &self.changed_when);
        match &self.changed_when {
            Some(s) => is_render_string(s, vars),
            None => Ok(result.get_changed()),
        }
    }

    fn exec_module_rendered_with_user(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
        user: User,
    ) -> Result<Value> {
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

    fn exec_module_rendered(&self, rendered_params: &YamlValue, vars: &Value) -> Result<Value> {
        match self.module.exec(
            self.global_params,
            rendered_params.clone(),
            vars.clone(),
            self.check_mode,
        ) {
            Ok((result, result_vars)) => {
                if self.module.get_name() != "include" {
                    let output = result.get_output();
                    let target = match self.is_changed(&result, &result_vars)? {
                        true => "changed",
                        false => "ok",
                    };
                    let target_empty =
                        &format!("{}{}", target, if output.is_none() { "_empty" } else { "" });
                    info!(target: target_empty,
                        "{}",
                        output.unwrap_or_else(
                            || "".to_owned()
                        )
                    );
                }
                let mut new_vars = context! {..result_vars};
                if self.register.is_some() {
                    let register = self.register.as_ref().unwrap();
                    trace!("register {:?} in {:?}", &result, register);
                    let v: Value = [(register, Value::from_serialize(&result))]
                        .into_iter()
                        .collect();
                    new_vars = context! { ..v, ..new_vars};
                }
                Ok(new_vars)
            }
            Err(e) => match self.ignore_errors {
                Some(is_true) if is_true => {
                    info!(target: "ignoring", "{}", e);
                    Ok(vars.clone())
                }
                _ => Err(e),
            },
        }
    }

    fn exec_module(&self, vars: Value) -> Result<Value> {
        if self.is_exec(&vars)? {
            let rendered_params = self.render_params(vars.clone())?;

            match self.r#become {
                true => {
                    let user_not_found_error = || {
                        Error::new(
                            ErrorKind::Other,
                            format!("User {:?} not found.", self.become_user),
                        )
                    };
                    let user = match User::from_name(&self.become_user)
                        .map_err(|_| user_not_found_error())?
                    {
                        Some(user) => Ok(user),
                        None => match self.become_user.parse::<u32>().map(Uid::from_raw) {
                            Ok(uid) => match User::from_uid(uid)? {
                                Some(user) => Ok(user),
                                None => Err(user_not_found_error()),
                            },
                            Err(_) => Err(user_not_found_error()),
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
                        ) = ipc::channel().map_err(|e| Error::new(ErrorKind::Other, e))?;

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
                                        .map(|v| serde_json::to_string(&v))?
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
                                        format!("child failed with exit_code {exit_code}"),
                                    )),
                                    Err(e) => Err(Error::new(ErrorKind::Other, e)),
                                    _ => Err(Error::new(
                                        ErrorKind::SubprocessFail,
                                        format!("child {child} unknown status"),
                                    )),
                                }?;
                                trace!("receive result");
                                rx.recv()
                                    .unwrap_or_else(|e| {
                                        Err(SerdeError::new(&Error::new(
                                            ErrorKind::Other,
                                            // ipc::IpcError doesn't implement std::error:Error
                                            format!("{e:?}"),
                                        )))
                                    })
                                    .map(|s| serde_json::from_str(&s))
                                    .map_err(|e| Error::new(ErrorKind::Other, e))?
                                    .map(Value::from_serialize::<Value>)
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
            debug!("skipping");
            Ok(vars)
        }
    }

    /// Execute [`Module`] rendering `self.params` with [`Vars`].
    ///
    /// [`Module`]: ../modules/trait.Module.html
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn exec(&self, vars: Value) -> Result<Value> {
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);

        if self.r#loop.is_some() {
            let mut ctx = vars.clone();
            for item in self.render_iterator(vars)?.into_iter() {
                let new_ctx = context! {item => &item, ..ctx};
                trace!("pre execute loop: {:?}", &new_ctx);
                ctx = self.exec_module(new_ctx)?;
                trace!("post execute loop: {:?}", &ctx);
            }
            Ok(ctx)
        } else {
            Ok(self.exec_module(vars)?)
        }
    }

    /// Return name.
    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    /// Return name rendered with [`Vars`].
    ///
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn get_rendered_name(&self, vars: Value) -> Result<String> {
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
    /// [`Module`]: ../modules/trait.Module.html
    pub fn get_module(&self) -> &dyn Module {
        self.module
    }
}

#[cfg(test)]
use crate::context::GLOBAL_PARAMS;

#[cfg(test)]
impl From<YamlValue> for Task<'_> {
    fn from(value: YamlValue) -> Self {
        TaskNew::from(&value)
            .validate_attrs()
            .unwrap()
            .get_task(&GLOBAL_PARAMS)
            .unwrap()
    }
}

pub fn parse_file<'a>(tasks_file: &str, global_params: &'a GlobalParams) -> Result<Tasks<'a>> {
    let tasks: Vec<YamlValue> = serde_yaml::from_str(tasks_file)?;
    tasks
        .into_iter()
        .map(|task| Task::new(&task, global_params))
        .collect::<Result<Tasks>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use minijinja::context;

    #[test]
    fn test_from_yaml() {
        let s: String = r#"
            name: 'Test task'
            command: 'example'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);

        assert_eq!(task.name.unwrap(), "Test task");
        assert_eq!(&task.module.get_name(), &"command");
    }

    #[test]
    fn test_from_yaml_no_module() {
        let s = r#"
            name: 'Test task'
            no_module: 'example'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = Value::from_serialize(vec![("boo", "false")]);
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());

        let s: String = r#"
            command: 'example'
            when:
              - true
              - false
              - true
            "#
        .to_owned();
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());

        let s: String = r#"
            command: 'example'
            when:
              - true
              - true
              - true
            "#
        .to_owned();
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task.is_exec(&vars).unwrap());

        let s: String = r#"
            command: 'example'
            when:
              - true or true or true
              - false
              - true
            "#
        .to_owned();
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(!task.is_exec(&vars).unwrap());
    }

    #[test]
    fn test_is_exec_array_with_or_operator() {
        let s: String = r#"
            command: 'example'
            when:
              - true
              - boo == 'test' or false
            "#
        .to_owned();
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert!(task.is_exec(&vars).unwrap());
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
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![YamlValue::from(1), YamlValue::from(2), YamlValue::from(3)]
        );
    }

    #[test]
    fn test_is_changed() {
        let s: String = r#"
            changed_when: "boo == 'test'"
            command: 'example'
            "#
        .to_owned();
        let vars = context! { boo => "test" };
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
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
        let vars = context! {};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        let result = task.exec(vars).unwrap();
        let expected = context! {
            item => 3,
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_iterator_var() {
        let s: String = r#"
            command: 'example'
            loop: "{{ range(3) }}"
            "#
        .to_owned();
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![YamlValue::from(0), YamlValue::from(1), YamlValue::from(2)]
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
        let vars = context! { boo => "test"};
        let yaml: YamlValue = serde_yaml::from_str(&s).unwrap();
        let task = Task::from(yaml);
        assert_eq!(
            task.render_iterator(vars).unwrap(),
            vec![YamlValue::from("test"), YamlValue::from(2)]
        );
    }

    #[test]
    fn test_task_execute() {
        let s0 = r#"
            name: task 1
            command: echo foo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);

        let vars = context! {};
        let result = task.exec(vars.clone()).unwrap();
        assert_eq!(result, vars);
    }

    #[test]
    fn test_task_execute_keep_vars() {
        let s0 = r#"
            name: task 1
            command: echo foo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);

        let vars = context! {buu => "boo"};
        let result = task.exec(vars.clone()).unwrap();
        assert_eq!(result, vars);

        let s0 = r#"
            name: task 1
            debug:
              msg: "foo"
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);

        let vars = context! {buu => "boo"};
        let result = task.exec(vars.clone()).unwrap();
        assert_eq!(result, vars);
    }

    #[test]
    fn test_task_execute_register() {
        let s0 = r#"
            name: task 1
            command: echo foo
            register: yea
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);

        let vars = context! {};
        let result = task.exec(vars.clone()).unwrap();
        assert!(result.get_attr("yea").map(|x| !x.is_undefined()).unwrap());
    }

    // check item is removed from vars after task loop execution
    #[test]
    fn test_task_execute_item_var_removed() {
        let s0 = r#"
            name: task 1
            command: echo foo
            loop: "{{ range(3) }}"
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);

        let vars = context! {};
        let result = task.exec(vars.clone()).unwrap();
        assert!(result.get_attr("item").map(|x| !x.is_undefined()).unwrap());
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

        let global_params = GlobalParams::default();
        let tasks = parse_file(file, &global_params).unwrap();
        assert_eq!(tasks.len(), 2);

        let s0 = r#"
            name: task 1
            command:
              foo: boo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task_0 = Task::from(yaml);

        assert_eq!(tasks[0].name, task_0.name);
        assert_eq!(tasks[0].params, task_0.params);
        assert_eq!(tasks[0].module.get_name(), task_0.module.get_name());

        let s1 = r#"
            name: task 2
            command: boo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s1).unwrap();
        let task_1 = Task::from(yaml);
        assert_eq!(tasks[1].name, task_1.name);
        assert_eq!(tasks[1].params, task_1.params);
        assert_eq!(tasks[1].module.get_name(), task_1.module.get_name());
    }

    #[test]
    fn test_render_params() {
        let s0 = r#"
            name: task 1
            command:
              cmd: ls {{ directory }}
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = Value::from_serialize(
            [("directory", "boo"), ("xuu", "zoo")]
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<HashMap<String, String>>(),
        );

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_with_vars() {
        let s0 = r#"
            name: task 1
            command:
              cmd: ls {{ foo }}
            vars:
              foo: boo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {};

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_with_render_vars() {
        let s0 = r#"
            name: task 1
            command:
              cmd: ls {{ foo }}
            vars:
              foo: '{{ directory }}'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {
            directory => "boo",
            xuu => "zoo",
        };

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_with_concat_render_vars() {
        let s0 = r#"
            name: task 1
            command:
              cmd: ls {{ foo }}
            vars:
              boo: '{{ directory }}'
              foo: '{{ boo }}'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {
            directory => "boo",
            xuu => "zoo",
        };

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "ls boo");
    }

    #[test]
    fn test_render_params_with_vars_array_not_valid() {
        let s0 = r#"
            name: task 1
            command:
              cmd: ls {{ foo }}
            vars:
              - foo: boo
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {};

        let rendered_params_err = task.render_params(vars).unwrap_err();
        assert_eq!(rendered_params_err.kind(), ErrorKind::JinjaRenderError);
    }

    #[test]
    fn test_render_params_with_vars_array_concat() {
        let s0 = r#"
            name: task 1
            command:
              cmd: echo {{ (boo + buu) | join(' ') }}
            vars:
              boo:
                - 1
                - 23
              buu:
                - 13
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {};

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "echo 1 23 13");
    }

    #[test]
    fn test_render_params_with_vars_array_concat_in_vars() {
        let s0 = r#"
            name: task 1
            command:
              cmd: echo {{ all | join(' ') }}
            vars:
              all: '{{ boo + buu }}'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {boo => &[1, 23], buu => &[13]};

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "echo 1 23 13");
    }

    #[test]
    fn test_render_params_with_vars_array_concat_in_vars_recursive() {
        let s0 = r#"
            name: task 1
            command:
              cmd: echo {{ all | join(' ') }}
            vars:
              boo:
                - 1
                - 23
              buu:
                - 13
              all: '{{ boo + buu }}'
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {};

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params["cmd"].as_str().unwrap(), "echo 1 23 13");
    }

    #[test]
    fn test_render_params_no_hash_map() {
        let s0 = r#"
            name: task 1
            command: ls {{ directory }}
            "#
        .to_owned();
        let yaml: YamlValue = serde_yaml::from_str(&s0).unwrap();
        let task = Task::from(yaml);
        let vars = context! {
            directory => "boo",
            xuu => "zoo",
        };

        let rendered_params = task.render_params(vars).unwrap();
        assert_eq!(rendered_params.as_str().unwrap(), "ls boo");
    }
}
