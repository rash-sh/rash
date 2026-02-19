mod handler;
mod new;
mod valid;

pub use handler::{Handlers, PendingHandlers, parse_notify_value};

use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::{
    is_render_string, merge_option, render, render_force_string, render_map, render_string,
};
use crate::job::{JobStatus, get_job_info, register_job};
use crate::modules::{Module, ModuleResult};
use crate::task::new::TaskNew;

use rash_derive::FieldNames;

use std::collections::HashMap;
use std::env;
use std::process::{Command as StdCommand, Stdio, exit};
use std::result::Result as StdResult;
use std::thread;
use std::time::Duration;

use ipc_channel::ipc::{self, IpcReceiver, IpcSender};
use minijinja::{Value, context};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, Uid, User, fork, setgid, setuid};
use serde::{Deserialize, Serialize};
use serde_error::Error as SerdeError;
use serde_norway::Value as YamlValue;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskExecResult {
    changed: bool,
    vars: Option<Value>,
    flush_handlers: bool,
}

impl TaskExecResult {
    pub fn new(changed: bool, vars: Option<Value>) -> Self {
        TaskExecResult {
            changed,
            vars,
            flush_handlers: false,
        }
    }

    pub fn with_flush_handlers(mut self) -> Self {
        self.flush_handlers = true;
        self
    }

    pub fn get_changed(&self) -> bool {
        self.changed
    }

    pub fn get_vars(&self) -> Option<&Value> {
        self.vars.as_ref()
    }

    pub fn take_vars(self) -> Option<Value> {
        self.vars
    }

    pub fn is_flush_handlers(&self) -> bool {
        self.flush_handlers
    }
}

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
    /// Rescue tasks to execute if the main task or block fails.
    rescue: Option<YamlValue>,
    /// Always tasks to execute regardless of success or failure.
    always: Option<YamlValue>,
    /// Environment variables to inject for this task.
    environment: Option<YamlValue>,
    /// Handler names to notify when this task reports changed.
    notify: Option<Vec<String>>,
    /// Number of retries before giving up. Default is 3 when `until` is specified.
    retries: Option<u32>,
    /// Delay between retries in seconds. Default is 0.
    delay: Option<u64>,
    /// Template expression passed directly without {{ }}; repeat task until this is true.
    until: Option<String>,
    /// Maximum runtime in seconds for async execution. If set, task runs in background.
    r#async: Option<u64>,
    /// Poll interval in seconds for async task status. 0 = fire and forget.
    poll: Option<u64>,
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
    /// [`Value`]: ../../serde_norway/enum.Value.html
    pub fn new(yaml: &YamlValue, global_params: &'a GlobalParams) -> Result<Self> {
        trace!("new task: {yaml:?}");
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
            YamlValue::Sequence(_) => {
                // For sequence parameters (like block tasks), pass through without string rendering
                if self.module.get_name() == "block" {
                    Ok(original_params)
                } else {
                    Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("{original_params:?} must be a mapping or a string"),
                    ))
                }
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{original_params:?} must be a mapping or a string"),
            )),
        }
    }

    fn render_environment(&self, vars: &Value) -> Result<Vec<(String, String)>> {
        trace!("environment: {:?}", &self.environment);
        match &self.environment {
            Some(env_yaml) => {
                let extended_vars = self.extend_vars(vars.clone())?;
                match env_yaml.as_mapping() {
                    Some(mapping) => {
                        let mut env_vars = Vec::new();
                        for (key, value) in mapping.iter() {
                            let key_str = key.as_str().ok_or_else(|| {
                                Error::new(
                                    ErrorKind::InvalidData,
                                    format!("Environment key must be a string: {key:?}"),
                                )
                            })?;
                            let rendered_value = match value.as_str() {
                                Some(s) => render_string(s, &extended_vars)?,
                                None => {
                                    // For non-string values, convert to string
                                    serde_json::to_string(value)
                                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
                                }
                            };
                            env_vars.push((key_str.to_owned(), rendered_value));
                        }
                        Ok(env_vars)
                    }
                    None => Err(Error::new(
                        ErrorKind::InvalidData,
                        "environment must be a mapping",
                    )),
                }
            }
            None => Ok(Vec::new()),
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
                .filter_map(|item| match render_force_string(item.clone(), &vars) {
                    Ok(rendered) => Some(Ok(rendered)),
                    Err(e) if e.kind() == ErrorKind::OmitParam => None,
                    Err(e) => Some(Err(e)),
                })
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
                let value: YamlValue = serde_norway::from_str(&render_string(s, &extended_vars)?)?;
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

    fn is_until_satisfied(&self, vars: &Value) -> Result<bool> {
        trace!("until: {:?}", &self.until);
        match &self.until {
            Some(s) => is_render_string(s, vars),
            None => Ok(true),
        }
    }

    fn exec_with_retry(&self, vars: Value) -> Result<TaskExecResult> {
        let max_retries = self.retries.unwrap_or(3);
        let delay_secs = self.delay.unwrap_or(0);

        for attempt in 0..=max_retries {
            let result = self.exec_module(vars.clone())?;

            if self.until.is_some() {
                let result_vars = result.clone().take_vars().unwrap_or(context! {});
                let check_vars =
                    context! {..vars.clone(), ..result_vars, ..context! {retries => attempt}};

                if self.is_until_satisfied(&check_vars)? {
                    debug!("until condition satisfied on attempt {}", attempt);
                    return Ok(result);
                }

                if attempt < max_retries {
                    debug!(
                        "until condition not satisfied on attempt {}, retrying in {} seconds",
                        attempt, delay_secs
                    );
                    if delay_secs > 0 {
                        std::thread::sleep(std::time::Duration::from_secs(delay_secs));
                    }
                } else {
                    warn!(
                        "until condition not satisfied after {} retries",
                        max_retries
                    );
                    return Err(Error::new(
                        ErrorKind::Other,
                        format!(
                            "Task failed: until condition not satisfied after {} retries",
                            max_retries
                        ),
                    ));
                }
            } else {
                return Ok(result);
            }
        }

        Ok(TaskExecResult::new(false, None))
    }

    fn is_async(&self) -> bool {
        self.r#async.is_some()
    }

    fn get_async_timeout(&self) -> Option<Duration> {
        self.r#async.map(Duration::from_secs)
    }

    fn get_poll_interval(&self) -> u64 {
        self.poll.unwrap_or(0)
    }

    fn spawn_async_command(&self, rendered_params: &YamlValue, vars: &Value) -> Result<u64> {
        let extended_vars = self.extend_vars(vars.clone())?;
        let env_vars = self.render_environment(&extended_vars)?;

        let module_name = self.module.get_name();
        if module_name != "command" && module_name != "shell" {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Async execution only supported for command/shell modules, got: {module_name}"
                ),
            ));
        }

        let cmd_str = match rendered_params.as_str() {
            Some(s) => s.to_owned(),
            None => match rendered_params.get("cmd") {
                Some(cmd) => cmd
                    .as_str()
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "cmd must be a string"))?
                    .to_owned(),
                None => return Err(Error::new(ErrorKind::InvalidData, "No command specified")),
            },
        };

        let chdir = rendered_params.get("chdir").and_then(|d| d.as_str());

        let mut cmd = StdCommand::new("/bin/sh");
        cmd.arg("-c").arg(&cmd_str);
        if let Some(dir) = chdir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        for (key, value) in &env_vars {
            cmd.env(key, value);
        }

        let child = cmd.spawn().map_err(|e| {
            Error::new(
                ErrorKind::SubprocessFail,
                format!("Failed to spawn async command: {e}"),
            )
        })?;

        let job_id = register_job(self.get_async_timeout(), child);

        info!(target: "async",
            "Started async job {} with timeout {:?}",
            job_id,
            self.get_async_timeout()
        );

        Ok(job_id)
    }

    fn poll_job(&self, job_id: u64, poll_interval: u64) -> Result<TaskExecResult> {
        let sleep_duration = Duration::from_secs(poll_interval);

        loop {
            let info = get_job_info(job_id).ok_or_else(|| {
                Error::new(ErrorKind::NotFound, format!("Job {job_id} not found"))
            })?;

            match info.status {
                JobStatus::Finished => {
                    let result = ModuleResult::new(info.changed, None, info.output.clone());
                    let register_vars = self.register.clone().map(|register| {
                        [(register.clone(), Value::from_serialize(&result))]
                            .into_iter()
                            .collect::<Value>()
                    });
                    return Ok(TaskExecResult::new(info.changed, register_vars));
                }
                JobStatus::Failed => {
                    return Err(Error::new(
                        ErrorKind::SubprocessFail,
                        format!(
                            "Async job {job_id} failed: {}",
                            info.error.unwrap_or_default()
                        ),
                    ));
                }
                JobStatus::Running | JobStatus::Pending => {
                    trace!(
                        "Job {} still running ({}s elapsed), sleeping for {}s",
                        job_id,
                        info.elapsed.as_secs(),
                        poll_interval
                    );
                    thread::sleep(sleep_duration);
                }
            }
        }
    }

    fn exec_module_rendered_with_user(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
        user: User,
    ) -> Result<TaskExecResult> {
        // Environment variables need to be set before changing user
        let extended_vars = self.extend_vars(vars.clone())?;
        let env_vars = self.render_environment(&extended_vars)?;

        for (key, value) in &env_vars {
            trace!(
                "setting environment variable (with user): {}={}",
                key, value
            );
            // SAFETY: We're setting environment variables for task execution.
            // This is safe as long as no other threads are modifying env vars concurrently.
            unsafe {
                env::set_var(key, value);
            }
        }

        match setgid(user.gid) {
            Ok(_) => match setuid(user.uid) {
                Ok(_) => {
                    // After changing user, call the inner module exec directly
                    let module_name = self.module.get_name();

                    let result = self.module.exec(
                        self.global_params,
                        rendered_params.clone(),
                        &extended_vars,
                        self.check_mode,
                    );

                    match result {
                        Ok((result, result_vars)) => {
                            let changed = self.is_changed(&result, &extended_vars)?;

                            if module_name != "include"
                                && module_name != "block"
                                && module_name != "meta"
                            {
                                let output = result.get_output();
                                let target = match changed {
                                    true => "changed",
                                    false => "ok",
                                };
                                let target_empty = &format!(
                                    "{}{}",
                                    target,
                                    if output.is_none() { "_empty" } else { "" }
                                );
                                info!(target: target_empty,
                                    "{}",
                                    output.unwrap_or_else(
                                        || "".to_owned()
                                    )
                                );
                            }

                            let register_vars = self.register.clone().map(|register| {
                                trace!("register {:?} in {:?}", &result, register);
                                [(register, Value::from_serialize(&result))]
                                    .into_iter()
                                    .collect::<Value>()
                            });

                            let new_vars_value = [result_vars, register_vars]
                                .into_iter()
                                .fold(context! {}, merge_option);
                            let new_vars = if new_vars_value == context! {} {
                                None
                            } else {
                                Some(new_vars_value)
                            };

                            Ok(TaskExecResult::new(changed, new_vars))
                        }
                        Err(e) => match self.ignore_errors {
                            Some(is_true) if is_true => {
                                info!(target: "ignoring", "{e}");
                                Ok(TaskExecResult::new(false, None))
                            }
                            _ => Err(e),
                        },
                    }
                }
                Err(_) => Err(Error::new(
                    ErrorKind::Other,
                    format!("uid cannot be changed to {}", user.uid),
                )),
            },
            Err(_) => Err(Error::new(
                ErrorKind::Other,
                format!("gid cannot be changed to {}", user.gid),
            )),
        }
    }

    fn exec_module_rendered(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<TaskExecResult> {
        let module_name = self.module.get_name();
        let extended_vars = self.extend_vars(vars.clone())?;

        // Render and set environment variables
        let env_vars = self.render_environment(&extended_vars)?;
        let mut original_env: HashMap<String, Option<String>> = HashMap::new();

        for (key, value) in &env_vars {
            trace!("setting environment variable: {}={}", key, value);
            // Save original value (if it exists)
            original_env.insert(key.clone(), env::var(key).ok());
            // Set new value
            // SAFETY: We're setting environment variables for task execution.
            // This is safe as long as no other threads are modifying env vars concurrently.
            unsafe {
                env::set_var(key, value);
            }
        }

        let result = self.module.exec(
            self.global_params,
            rendered_params.clone(),
            &extended_vars,
            self.check_mode,
        );

        // Restore original environment
        for (key, original_value) in original_env {
            // SAFETY: Restoring environment variables to their original state.
            unsafe {
                match original_value {
                    Some(value) => env::set_var(&key, value),
                    None => env::remove_var(&key),
                }
            }
        }

        match result {
            Ok((result, result_vars)) => {
                let changed = self.is_changed(&result, &extended_vars)?;
                let is_meta_flush = module_name == "meta"
                    && result
                        .get_extra()
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        == Some("flush_handlers".to_string());

                // Don't show output for control flow modules like include and block
                if module_name != "include" && module_name != "block" && module_name != "meta" {
                    let output = result.get_output();
                    let target = match changed {
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

                let register_vars = self.register.clone().map(|register| {
                    trace!("register {:?} in {:?}", &result, register);
                    [(register, Value::from_serialize(&result))]
                        .into_iter()
                        .collect::<Value>()
                });

                let new_vars_value = [result_vars, register_vars]
                    .into_iter()
                    .fold(context! {}, merge_option);
                let new_vars = if new_vars_value == context! {} {
                    None
                } else {
                    Some(new_vars_value)
                };

                let mut exec_result = TaskExecResult::new(changed, new_vars);
                if is_meta_flush {
                    exec_result = exec_result.with_flush_handlers();
                }
                Ok(exec_result)
            }
            Err(e) => match self.ignore_errors {
                Some(is_true) if is_true => {
                    info!(target: "ignoring", "{e}");
                    Ok(TaskExecResult::new(false, None))
                }
                _ => Err(e),
            },
        }
    }

    fn exec_module(&self, vars: Value) -> Result<TaskExecResult> {
        if self.is_exec(&vars)? {
            let rendered_params = self.render_params(vars.clone())?;

            // Handle async execution
            if self.is_async() {
                let job_id = self.spawn_async_command(&rendered_params, &vars)?;
                let poll_interval = self.get_poll_interval();

                if poll_interval == 0 {
                    // Fire and forget - return immediately with async job info
                    let result =
                        ModuleResult::new(true, None, Some(format!("async job started: {job_id}")));
                    let register_vars = self.register.clone().map(|register| {
                        [(register, Value::from_serialize(&result))]
                            .into_iter()
                            .collect::<Value>()
                    });
                    return Ok(TaskExecResult::new(true, register_vars));
                }

                // Poll for completion
                return self.poll_job(job_id, poll_interval);
            }

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

                                trace!("send result: {result:?}");
                                tx.send(
                                    result
                                        .map(|v| serde_json::to_string(&v))?
                                        .map_err(|e| SerdeError::new(&e)),
                                )
                                .unwrap_or_else(|e| {
                                    error!("child failed to send result: {e}");
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
                                    .map_err(|e| Error::new(ErrorKind::Other, format!("{e:?}")))
                                    .and_then(|result| {
                                        result.map_err(|e| {
                                            Error::new(ErrorKind::Other, format!("{e:?}"))
                                        })
                                    })
                                    .and_then(|s| {
                                        serde_json::from_str::<TaskExecResult>(&s)
                                            .map_err(|e| Error::new(ErrorKind::Other, e))
                                    })
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
            Ok(TaskExecResult::new(false, None))
        }
    }

    /// Execute [`Module`] rendering `self.params` with [`Vars`].
    ///
    /// [`Module`]: ../modules/trait.Module.html
    /// [`Vars`]: ../vars/struct.Vars.html
    pub fn exec(&self, vars: Value) -> Result<TaskExecResult> {
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);

        if self.rescue.is_some() || self.always.is_some() {
            return self.exec_with_rescue_always(vars);
        }

        self.exec_main_task(vars)
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

    /// Return notify handlers.
    pub fn get_notify(&self) -> Option<&[String]> {
        self.notify.as_deref()
    }

    /// Execute a task with comprehensive rescue and always handling.
    ///
    /// This method implements a try-catch-finally pattern similar to exception handling:
    /// - Try: Execute the main task (any module type)
    /// - Catch: If task fails, execute rescue tasks (if defined)
    /// - Finally: Always execute always tasks (if defined)
    ///
    /// The method uses functional programming patterns to handle each stage
    /// and provides detailed error context for debugging.
    fn exec_with_rescue_always(&self, vars: Value) -> Result<TaskExecResult> {
        let initial_vars = vars;

        // Stage 1: Execute main task and capture result
        let (main_result, main_exec_result) = match self.exec_main_task(initial_vars.clone()) {
            Ok(exec_result) => {
                trace!("Main task execution succeeded");
                (Ok(()), exec_result)
            }
            Err(task_error) => {
                warn!("Main task execution failed: {task_error}");
                (Err(task_error), TaskExecResult::new(false, None))
            }
        };

        let main_changed = main_exec_result.get_changed();
        let main_vars = main_exec_result.take_vars();
        let post_main_vars = merge_option(initial_vars, main_vars.clone());
        let (rescue_result, rescue_exec_result) = match (&main_result, &self.rescue) {
            (Err(_), Some(rescue_tasks)) => {
                info!("Executing rescue tasks due to main task failure");
                match self.execute_task_sequence(rescue_tasks, post_main_vars.clone()) {
                    Ok(rescue_result) => {
                        info!("Rescue tasks executed successfully");
                        (Ok(()), rescue_result)
                    }
                    Err(rescue_error) => {
                        error!("Rescue tasks failed: {rescue_error}");
                        (Err(rescue_error), TaskExecResult::new(false, None))
                    }
                }
            }
            (Err(_), None) => {
                trace!("Main task failed but no rescue tasks defined");
                (Ok(()), TaskExecResult::new(main_changed, main_vars.clone())) // No rescue available, but continue to always
            }
            (Ok(_), _) => {
                trace!("Main task succeeded, skipping rescue tasks");
                (Ok(()), TaskExecResult::new(main_changed, main_vars.clone())) // Task succeeded, no rescue needed
            }
        };

        let rescue_changed = rescue_exec_result.get_changed();
        let rescue_vars_taken = rescue_exec_result.take_vars();
        let post_rescue_vars = merge_option(post_main_vars, rescue_vars_taken.clone());
        let always_exec_result = match &self.always {
            Some(always_tasks) => {
                trace!("Executing always tasks");
                match self.execute_task_sequence(always_tasks, post_rescue_vars) {
                    Ok(always_result) => {
                        trace!("Always tasks executed successfully");
                        always_result
                    }
                    Err(always_error) => {
                        error!("Always tasks failed: {always_error}");
                        // Always tasks failing is critical - propagate the error
                        return Err(Error::new(
                            ErrorKind::Other,
                            format!("Always section failed: {always_error}"),
                        ));
                    }
                }
            }
            None => {
                trace!("No always tasks to execute");
                TaskExecResult::new(false, None)
            }
        };

        let always_vars = always_exec_result.take_vars();
        let all_vars_value = [main_vars, rescue_vars_taken, always_vars]
            .into_iter()
            .fold(context! {}, merge_option);
        let all_vars = if all_vars_value == context! {} {
            None
        } else {
            Some(all_vars_value)
        };

        // Stage 4: Determine final result based on execution stages
        match (&main_result, &rescue_result) {
            (Ok(_), Ok(_)) => Ok(TaskExecResult::new(main_changed, all_vars)),
            (Ok(_), Err(_)) => {
                warn!("Unexpected state: main task succeeded but rescue reported failure");
                Ok(TaskExecResult::new(main_changed, all_vars))
            }
            (Err(_main_error), Ok(_)) => {
                debug!("Task execution recovered through rescue tasks");
                Ok(TaskExecResult::new(rescue_changed, all_vars))
            }
            (Err(main_error), Err(_)) => {
                if self.rescue.is_some() {
                    Err(Error::new(
                        ErrorKind::Other,
                        format!(
                            "Task execution failed and rescue tasks could not recover: {main_error}"
                        ),
                    ))
                } else {
                    Err(Error::new(
                        ErrorKind::Other,
                        format!("Task execution failed with no rescue defined: {main_error}"),
                    ))
                }
            }
        }
    }

    /// Execute a sequence of tasks defined in YAML format.
    ///
    /// This is a helper method that provides consistent task sequence execution
    /// for rescue and always sections. It includes proper error handling and
    /// variable propagation.
    fn execute_task_sequence(&self, tasks_yaml: &YamlValue, vars: Value) -> Result<TaskExecResult> {
        match tasks_yaml {
            YamlValue::Sequence(tasks) => {
                if tasks.is_empty() {
                    warn!("Empty task sequence provided");
                    return Ok(TaskExecResult::new(false, None));
                }

                let mut current_vars = vars;
                let mut current_new_vars = context! {};
                let mut any_changed = false;
                for (index, task_yaml) in tasks.iter().enumerate() {
                    match Task::new(task_yaml, self.global_params) {
                        Ok(task) => {
                            info!(target: "task",
                                "[{}:{}] - ",
                                current_vars.get_attr("rash")?.get_attr("path")?,
                                task.get_rendered_name(current_vars.clone())
                                    .unwrap_or_else(|_| task.get_module().get_name().to_owned()),
                            );
                            match task.exec(current_vars.clone()) {
                                Ok(exec_result) => {
                                    if exec_result.get_changed() {
                                        any_changed = true;
                                    }
                                    if let Some(new_vars) = exec_result.take_vars() {
                                        current_vars =
                                            context! {..current_vars, ..new_vars.clone()};
                                        current_new_vars =
                                            context! {..current_new_vars, ..new_vars.clone()};
                                    }
                                    trace!("Task {index} in sequence completed successfully");
                                }
                                Err(task_error) => {
                                    error!("Task {index} in sequence failed: {task_error}");
                                    return Err(Error::new(
                                        ErrorKind::Other,
                                        format!(
                                            "Task sequence failed at index {index}: {task_error}"
                                        ),
                                    ));
                                }
                            }
                        }
                        Err(parse_error) => {
                            error!("Failed to parse task {index} in sequence: {parse_error}");
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Invalid task at index {index}: {parse_error}"),
                            ));
                        }
                    }
                }
                let final_vars = if current_new_vars == context! {} {
                    None
                } else {
                    Some(current_new_vars)
                };
                Ok(TaskExecResult::new(any_changed, final_vars))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Task sequence must be a YAML array, got: {tasks_yaml:?}"),
            )),
        }
    }

    /// Execute rescue tasks - this is now just an alias for consistency.
    #[deprecated(note = "Use execute_task_sequence instead for better error handling")]
    #[allow(dead_code)]
    fn execute_rescue_tasks(
        &self,
        rescue_tasks: &YamlValue,
        vars: Value,
    ) -> Result<TaskExecResult> {
        self.execute_task_sequence(rescue_tasks, vars)
    }

    /// Execute always tasks - this is now just an alias for consistency.
    #[deprecated(note = "Use execute_task_sequence instead for better error handling")]
    #[allow(dead_code)]
    fn execute_always_tasks(
        &self,
        always_tasks: &YamlValue,
        vars: Value,
    ) -> Result<TaskExecResult> {
        self.execute_task_sequence(always_tasks, vars)
    }

    /// Execute a task list - this is now just an alias for consistency.
    #[deprecated(note = "Use execute_task_sequence instead for better error handling")]
    #[allow(dead_code)]
    fn execute_task_list(&self, tasks_yaml: &YamlValue, vars: Value) -> Result<TaskExecResult> {
        self.execute_task_sequence(tasks_yaml, vars)
    }

    /// Execute the main task with proper loop handling.
    ///
    /// This method handles both single task execution and looped task execution,
    /// providing the foundation for rescue/always error handling patterns.
    /// When async is enabled, tasks run in background and can be polled.
    fn exec_main_task(&self, vars: Value) -> Result<TaskExecResult> {
        if self.r#loop.is_some() && self.is_async() {
            self.exec_parallel_loop(vars)
        } else if self.r#loop.is_some() && self.until.is_some() {
            self.exec_loop_with_retry(vars)
        } else if self.r#loop.is_some() {
            self.exec_sequential_loop(vars)
        } else if self.is_async() {
            self.exec_async_single(vars)
        } else if self.until.is_some() {
            self.exec_with_retry(vars)
        } else {
            self.exec_module(vars)
        }
    }

    fn exec_sequential_loop(&self, vars: Value) -> Result<TaskExecResult> {
        let mut changed = false;
        let mut all_new_vars = context! {};
        let mut flush_handlers = false;

        for item in self.render_iterator(vars.clone())?.into_iter() {
            let ctx = context! {item => &item, ..vars.clone()};
            trace!("pre execute loop: {:?}", &ctx);
            let exec_result = self.exec_module(ctx)?;
            if exec_result.get_changed() {
                changed = true;
            }
            if exec_result.is_flush_handlers() {
                flush_handlers = true;
            }
            if let Some(v) = exec_result.take_vars() {
                all_new_vars = context! {..all_new_vars, ..v};
            }
            trace!("post execute loop: {:?}", &all_new_vars);
        }

        let final_vars = if all_new_vars == context! {} {
            None
        } else {
            Some(all_new_vars)
        };

        let mut result = TaskExecResult::new(changed, final_vars);
        if flush_handlers {
            result = result.with_flush_handlers();
        }
        Ok(result)
    }

    fn exec_loop_with_retry(&self, vars: Value) -> Result<TaskExecResult> {
        let mut changed = false;
        let mut all_new_vars = context! {};
        let mut flush_handlers = false;

        for item in self.render_iterator(vars.clone())?.into_iter() {
            let ctx = context! {item => &item, ..vars.clone()};
            trace!("pre execute loop with retry: {:?}", &ctx);
            let exec_result = self.exec_with_retry(ctx)?;
            if exec_result.get_changed() {
                changed = true;
            }
            if exec_result.is_flush_handlers() {
                flush_handlers = true;
            }
            if let Some(v) = exec_result.take_vars() {
                all_new_vars = context! {..all_new_vars, ..v};
            }
            trace!("post execute loop with retry: {:?}", &all_new_vars);
        }

        let final_vars = if all_new_vars == context! {} {
            None
        } else {
            Some(all_new_vars)
        };

        let mut result = TaskExecResult::new(changed, final_vars);
        if flush_handlers {
            result = result.with_flush_handlers();
        }
        Ok(result)
    }

    fn exec_parallel_loop(&self, vars: Value) -> Result<TaskExecResult> {
        let items = self.render_iterator(vars.clone())?;
        let poll_interval = self.get_poll_interval();

        let mut job_ids: Vec<(u64, YamlValue)> = Vec::new();

        for item in items.into_iter() {
            let ctx = context! {item => &item, ..vars.clone()};
            let rendered_params = self.render_params(ctx.clone())?;

            if self.is_exec(&ctx)? {
                let job_id = self.spawn_async_command(&rendered_params, &ctx)?;
                job_ids.push((job_id, item));
            }
        }

        if poll_interval == 0 {
            let mut results = Vec::new();
            for (job_id, _item) in &job_ids {
                results.push(*job_id);
            }
            let job_ids_value: Vec<Value> = results.iter().map(|id| Value::from(*id)).collect();
            let register_vars = self.register.clone().map(|register| {
                [(
                    register.clone(),
                    Value::from_serialize(serde_json::json!({
                        "rash_job_ids": job_ids_value,
                        "changed": true,
                    })),
                )]
                .into_iter()
                .collect::<Value>()
            });
            return Ok(TaskExecResult::new(true, register_vars));
        }

        let sleep_duration = Duration::from_secs(poll_interval);
        let mut completed = vec![false; job_ids.len()];
        let mut outputs = vec![None; job_ids.len()];
        let mut errors = vec![None; job_ids.len()];
        let mut changed = vec![false; job_ids.len()];

        while !completed.iter().all(|&c| c) {
            for (idx, (job_id, _)) in job_ids.iter().enumerate() {
                if completed[idx] {
                    continue;
                }
                if let Some(info) = get_job_info(*job_id) {
                    match info.status {
                        JobStatus::Finished => {
                            completed[idx] = true;
                            outputs[idx] = info.output;
                            changed[idx] = info.changed;
                        }
                        JobStatus::Failed => {
                            completed[idx] = true;
                            errors[idx] = info.error;
                        }
                        JobStatus::Running | JobStatus::Pending => {}
                    }
                }
            }
            if !completed.iter().all(|&c| c) {
                thread::sleep(sleep_duration);
            }
        }

        for (err, _) in errors.iter().enumerate() {
            if let Some(e) = errors.get(err)
                && e.is_some()
            {
                return Err(Error::new(
                    ErrorKind::SubprocessFail,
                    format!("Async job failed: {:?}", errors[err]),
                ));
            }
        }

        let any_changed = changed.iter().any(|&c| c);
        let job_ids_value: Vec<Value> = job_ids.iter().map(|(id, _)| Value::from(*id)).collect();

        let register_vars = self.register.clone().map(|register| {
            [(
                register.clone(),
                Value::from_serialize(serde_json::json!({
                    "rash_job_ids": job_ids_value,
                    "changed": any_changed,
                })),
            )]
            .into_iter()
            .collect::<Value>()
        });

        Ok(TaskExecResult::new(any_changed, register_vars))
    }

    fn exec_async_single(&self, vars: Value) -> Result<TaskExecResult> {
        let rendered_params = self.render_params(vars.clone())?;
        let poll_interval = self.get_poll_interval();

        let job_id = self.spawn_async_command(&rendered_params, &vars)?;

        if poll_interval == 0 {
            let register_vars = self.register.clone().map(|register| {
                [(
                    register.clone(),
                    Value::from_serialize(serde_json::json!({
                        "rash_job_id": job_id,
                        "changed": true,
                    })),
                )]
                .into_iter()
                .collect::<Value>()
            });
            return Ok(TaskExecResult::new(true, register_vars));
        }

        self.poll_job(job_id, poll_interval)
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

/// Parse a YAML file returning Tasks.
///
/// Works with files that contain only a task list (no handlers section).
pub fn parse_file<'a>(
    file_content: &str,
    global_params: &'a GlobalParams<'a>,
) -> Result<Tasks<'a>> {
    let yaml: YamlValue = serde_norway::from_str(file_content)?;

    match yaml {
        YamlValue::Sequence(tasks_yaml) => {
            trace!("Parsing {} tasks from file", tasks_yaml.len());
            tasks_yaml
                .iter()
                .map(|task_yaml| Task::new(task_yaml, global_params))
                .collect::<Result<Tasks>>()
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected a YAML sequence of tasks, got: {yaml:?}"),
        )),
    }
}

/// Parsed result containing tasks and optional handlers.
#[derive(Debug)]
pub struct ParsedFile<'a> {
    pub tasks: Tasks<'a>,
    pub handlers: Option<Handlers<'a>>,
}

/// Parse a YAML file that may contain tasks and handlers sections.
///
/// This function supports files with the following structure:
/// ```yaml
/// tasks:
///   - name: First task
///     ...
/// handlers:
///   - name: Handler name
///     ...
/// ```
pub fn parse_file_with_handlers<'a>(
    file_content: &str,
    global_params: &'a GlobalParams,
) -> Result<ParsedFile<'a>> {
    let yaml: YamlValue = serde_norway::from_str(file_content)?;

    match yaml {
        YamlValue::Mapping(ref mapping) => {
            let tasks_yaml = mapping.get(YamlValue::String("tasks".to_string()));
            let handlers_yaml = mapping.get(YamlValue::String("handlers".to_string()));

            let tasks = match tasks_yaml {
                Some(YamlValue::Sequence(tasks_seq)) => tasks_seq
                    .iter()
                    .map(|task_yaml| Task::new(task_yaml, global_params))
                    .collect::<Result<Tasks>>()?,
                Some(_) => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "tasks must be a YAML sequence".to_string(),
                    ));
                }
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "No tasks section found in file".to_string(),
                    ));
                }
            };

            let handlers = match handlers_yaml {
                Some(YamlValue::Sequence(handlers_seq)) => {
                    Some(Handlers::from_yaml(handlers_seq, global_params)?)
                }
                Some(_) => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "handlers must be a YAML sequence".to_string(),
                    ));
                }
                None => None,
            };

            Ok(ParsedFile { tasks, handlers })
        }
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected a YAML mapping with tasks (and optional handlers), got: {yaml:?}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;
    use std::sync::LazyLock;

    pub static GLOBAL_PARAMS: LazyLock<GlobalParams> = LazyLock::new(GlobalParams::default);

    #[test]
    fn test_from_yaml() {
        let s: String = r#"
            name: 'Test task'
            command: 'example'
            "#
        .to_owned();
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
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
        let yaml: YamlValue = serde_norway::from_str(&s).unwrap();
        let task = Task::from(yaml);
        let vars = context! {};
        let iterator = task.render_iterator(vars).unwrap();
        assert_eq!(iterator.len(), 3);
    }

    #[test]
    fn test_task_new() {
        let yaml_str = r#"
        name: test task
        debug:
          msg: "hello"
        "#;
        let yaml: YamlValue = serde_norway::from_str(yaml_str).unwrap();
        let task = Task::new(&yaml, &GLOBAL_PARAMS).unwrap();
        assert_eq!(task.name, Some("test task".to_string()));
    }

    #[test]
    fn test_parse_file() {
        let file_content = r#"
        - name: task 1
          debug:
            msg: "first"
        - name: task 2
          debug:
            msg: "second"
        "#;
        let tasks = parse_file(file_content, &GLOBAL_PARAMS).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_parse_file_with_handlers() {
        let file_content = r#"
        tasks:
          - name: task 1
            debug:
              msg: "first"
            notify: handler1
        handlers:
          - name: handler1
            debug:
              msg: "handler"
        "#;
        let parsed = parse_file_with_handlers(file_content, &GLOBAL_PARAMS).unwrap();
        assert_eq!(parsed.tasks.len(), 1);
        assert!(parsed.handlers.is_some());
        let handlers = parsed.handlers.unwrap();
        assert!(handlers.get("handler1").is_some());
    }

    #[test]
    fn test_notify_parsing() {
        let yaml_str = r#"
        name: test notify
        debug:
          msg: "hello"
        notify: my_handler
        "#;
        let yaml: YamlValue = serde_norway::from_str(yaml_str).unwrap();
        let task = Task::new(&yaml, &GLOBAL_PARAMS).unwrap();
        assert_eq!(task.notify, Some(vec!["my_handler".to_string()]));
    }

    #[test]
    fn test_notify_list_parsing() {
        let yaml_str = r#"
        name: test notify list
        debug:
          msg: "hello"
        notify:
          - handler1
          - handler2
        "#;
        let yaml: YamlValue = serde_norway::from_str(yaml_str).unwrap();
        let task = Task::new(&yaml, &GLOBAL_PARAMS).unwrap();
        assert_eq!(
            task.notify,
            Some(vec!["handler1".to_string(), "handler2".to_string()])
        );
    }
}
