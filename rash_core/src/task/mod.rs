mod asynchronous;
mod handler;
mod new;
mod privilege;
mod valid;

pub use handler::{Handlers, PendingHandlers, parse_notify_value};
pub use privilege::{
    BecomeOutcome, InternalTaskData, RASH_INTERNAL_OUTPUT_ENV, RASH_INTERNAL_RESULT_ENV,
    RASH_INTERNAL_TASK_ENV, RASH_INTERNAL_TASK_FLAG, get_internal_output, get_internal_result_path,
    is_internal_execution, is_internal_task_execution,
};

use crate::context::{BecomeMethod, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::{
    is_render_string, merge_option, render, render_force_string, render_map, render_string,
};
use crate::logger::{is_json_output, suppress_logs};
use crate::modules::{Module, ModuleResult};
use crate::task::new::TaskNew;

use std::collections::HashMap;
use std::env;
use std::thread;
use std::time::Duration;

use minijinja::{Value, context};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskExecResult {
    changed: bool,
    failed: bool,
    error: Option<String>,
    vars: Option<Value>,
    flush_handlers: bool,
}

impl TaskExecResult {
    pub fn new(changed: bool, vars: Option<Value>) -> Self {
        Self {
            changed,
            failed: false,
            error: None,
            vars,
            flush_handlers: false,
        }
    }

    fn failed(changed: bool, vars: Option<Value>, error: impl Into<String>) -> Self {
        Self {
            changed,
            failed: true,
            error: Some(error.into()),
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

    pub fn get_failed(&self) -> bool {
        self.failed
    }

    pub fn get_error(&self) -> Option<&str> {
        self.error.as_deref()
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

#[derive(Debug, Clone, Serialize)]
struct JsonResult {
    changed: bool,
    failed: bool,
    output: Option<String>,
    extra: Option<serde_json::Value>,
}

impl JsonResult {
    fn new(changed: bool, failed: bool, result: &ModuleResult) -> Self {
        Self {
            changed,
            failed,
            output: result.get_output(),
            extra: result
                .get_extra()
                .and_then(|value| serde_json::to_value(value).ok()),
        }
    }
}

/// Failure message reported instead of the real one for `no_log` tasks (as Ansible does).
pub const NO_LOG_MESSAGE: &str =
    "the output has been hidden due to the fact that 'no_log: true' was specified for this result";

fn log_module_result(changed: bool, failed: bool, result: &ModuleResult, hide_output: bool) {
    let hidden;
    let result = if hide_output {
        hidden = ModuleResult::new(result.get_changed(), result.get_extra(), None);
        &hidden
    } else {
        result
    };
    if is_json_output() {
        let json_result = JsonResult::new(changed, failed, result);
        match serde_json::to_string(&json_result) {
            Ok(json_str) => {
                let target = if changed { "changed" } else { "ok" };
                info!(target: target, "{json_str}");
            }
            Err(e) => error!("Failed to serialize JSON result: {e}"),
        }
        return;
    }

    let output = result.get_output();
    let target = if changed { "changed" } else { "ok" };
    let target_empty = format!("{}{}", target, if output.is_none() { "_empty" } else { "" });
    info!(target: &target_empty, "{}", output.unwrap_or_default());
}

#[derive(Debug, Clone)]
// ANCHOR: task
pub struct Task<'a> {
    r#become: bool,
    become_user: String,
    become_method: BecomeMethod,
    become_exe: String,
    become_password: Option<String>,
    check_mode: bool,
    module: &'static dyn Module,
    params: YamlValue,
    changed_when: Option<String>,
    failed_when: Option<String>,
    ignore_errors: Option<bool>,
    quiet: bool,
    no_log: bool,
    name: Option<String>,
    r#loop: Option<YamlValue>,
    register: Option<String>,
    vars: Option<YamlValue>,
    when: Option<String>,
    rescue: Option<YamlValue>,
    always: Option<YamlValue>,
    environment: Option<YamlValue>,
    notify: Option<Vec<String>>,
    retries: Option<u32>,
    delay: Option<u64>,
    until: Option<String>,
    r#async: Option<u64>,
    poll: Option<u64>,
    global_params: &'a GlobalParams<'a>,
}
// ANCHOR_END: task

pub type Tasks<'a> = Vec<Task<'a>>;

impl<'a> Task<'a> {
    pub fn new(yaml: &YamlValue, global_params: &'a GlobalParams) -> Result<Self> {
        trace!("new task: {yaml:?}");
        TaskNew::from(yaml)
            .validate_attrs()?
            .get_task(global_params)
    }

    #[inline(always)]
    fn is_attr(attr: &str) -> bool {
        matches!(
            attr,
            "become"
                | "become_user"
                | "become_method"
                | "become_exe"
                | "become_password"
                | "check_mode"
                | "changed_when"
                | "failed_when"
                | "ignore_errors"
                | "quiet"
                | "no_log"
                | "name"
                | "loop"
                | "register"
                | "vars"
                | "when"
                | "rescue"
                | "always"
                | "environment"
                | "notify"
                | "retries"
                | "delay"
                | "until"
                | "async"
                | "poll"
        )
    }

    fn extend_vars(&self, additional_vars: Value) -> Result<Value> {
        match self.vars.clone() {
            Some(vars) => {
                let rendered = match render(vars, &additional_vars) {
                    Ok(value) => Value::from_serialize(value),
                    Err(e) if e.kind() == ErrorKind::OmitParam => context! {},
                    Err(e) => return Err(e),
                };
                Ok(context! {..rendered, ..additional_vars})
            }
            None => Ok(additional_vars),
        }
    }

    fn render_params(&self, vars: Value) -> Result<YamlValue> {
        let original = self.params.clone();
        if self.module.defer_params_rendering() {
            return Ok(original);
        }
        let extended_vars = self.extend_vars(vars)?;
        match original {
            YamlValue::Mapping(mapping) => render_map(
                mapping,
                &extended_vars,
                self.module.force_string_on_params(),
            ),
            YamlValue::String(value) => {
                Ok(YamlValue::String(render_string(&value, &extended_vars)?))
            }
            YamlValue::Null => Ok(YamlValue::Mapping(serde_norway::Mapping::new())),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("{original:?} must be a mapping or a string"),
            )),
        }
    }

    fn render_environment(&self, vars: &Value) -> Result<Vec<(String, String)>> {
        let Some(env_yaml) = &self.environment else {
            return Ok(Vec::new());
        };
        let extended_vars = self.extend_vars(vars.clone())?;
        let mapping = env_yaml
            .as_mapping()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "environment must be a mapping"))?;
        mapping
            .iter()
            .map(|(key, value)| {
                let key = key.as_str().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "environment keys must be strings")
                })?;
                let value = match value.as_str() {
                    Some(value) => render_string(value, &extended_vars)?,
                    None => serde_json::to_string(value)
                        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?,
                };
                Ok((key.to_owned(), value))
            })
            .collect()
    }

    fn is_exec(&self, vars: &Value) -> Result<bool> {
        match &self.when {
            Some(expression) => {
                let extended = self.extend_vars(vars.clone())?;
                is_render_string(expression, &extended)
            }
            None => Ok(true),
        }
    }

    fn is_until_satisfied(&self, vars: &Value) -> Result<bool> {
        match &self.until {
            Some(expression) => is_render_string(expression, vars),
            None => Ok(true),
        }
    }

    fn get_iterator(value: &YamlValue, vars: Value) -> Result<Vec<YamlValue>> {
        let sequence = value
            .as_sequence()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "loop is not iterable"))?;
        sequence
            .iter()
            .filter_map(|item| match render_force_string(item.clone(), &vars) {
                Ok(rendered) => Some(Ok(rendered)),
                Err(e) if e.kind() == ErrorKind::OmitParam => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    fn render_iterator(&self, vars: Value) -> Result<Vec<YamlValue>> {
        let loop_value = self
            .r#loop
            .clone()
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "loop is not defined"))?;
        let extended = self.extend_vars(context! {item => "", ..vars})?;
        if let Some(template) = loop_value.as_str() {
            let value: YamlValue = serde_norway::from_str(&render_string(template, &extended)?)?;
            if value.as_str().is_some() {
                Ok(vec![value])
            } else {
                Self::get_iterator(&value, extended)
            }
        } else {
            Self::get_iterator(&loop_value, extended)
        }
    }

    fn module_default_failed(result: &ModuleResult) -> bool {
        result
            .get_extra()
            .and_then(|extra| extra.get("failed").and_then(YamlValue::as_bool))
            .unwrap_or(false)
    }

    fn result_value(
        result: Option<&ModuleResult>,
        changed: bool,
        failed: bool,
        error: Option<&str>,
    ) -> Value {
        let output = result.and_then(ModuleResult::get_output);
        let extra_yaml = result.and_then(ModuleResult::get_extra);
        let extra_json = extra_yaml
            .clone()
            .and_then(|value| serde_json::to_value(value).ok())
            .unwrap_or(serde_json::Value::Null);

        let mut object = serde_json::Map::new();
        object.insert("changed".into(), serde_json::json!(changed));
        object.insert("failed".into(), serde_json::json!(failed));
        object.insert("output".into(), serde_json::json!(output.clone()));
        // Compatibility alias used by older Rash/Ansible-style scripts. `output` is the
        // canonical generic field, while `stdout` is convenient for command results.
        object.insert("stdout".into(), serde_json::json!(output));
        object.insert("extra".into(), extra_json.clone());
        object.insert("error".into(), serde_json::json!(error));

        if let serde_json::Value::Object(extra) = extra_json {
            for (key, value) in extra {
                object.entry(key).or_insert(value);
            }
        }
        Value::from_serialize(serde_json::Value::Object(object))
    }

    fn expression_vars(&self, vars: &Value, result: Value) -> Value {
        let result_binding = [("result", result.clone())].into_iter().collect::<Value>();
        let register_binding = self
            .register
            .as_ref()
            .map(|register| [(register.as_str(), result)].into_iter().collect::<Value>());
        let additions = merge_option(result_binding, register_binding);
        context! {..additions, ..vars.clone()}
    }

    fn failure_message(&self, result: &ModuleResult) -> String {
        if let Some(extra) = result.get_extra() {
            let rc = extra.get("rc").and_then(YamlValue::as_i64);
            let stderr = extra.get("stderr").and_then(YamlValue::as_str);
            if let Some(rc) = rc {
                if let Some(stderr) = stderr.filter(|value| !value.is_empty()) {
                    return format!("{} exited with code {rc}: {stderr}", self.module.get_name());
                }
                return format!("{} exited with code {rc}", self.module.get_name());
            }
        }
        format!(
            "Task '{}' failed",
            self.name.as_deref().unwrap_or(self.module.get_name())
        )
    }

    /// `hide_output`: never log the module output (it is still registered).
    fn finalize_module_result(
        &self,
        result: ModuleResult,
        result_vars: Option<Value>,
        vars: &Value,
        hide_output: bool,
    ) -> Result<TaskExecResult> {
        let default_changed = result.get_changed();
        let default_failed = Self::module_default_failed(&result);
        let preliminary = Self::result_value(
            Some(&result),
            default_changed,
            default_failed,
            default_failed
                .then(|| self.failure_message(&result))
                .as_deref(),
        );
        let expression_vars = self.expression_vars(vars, preliminary);

        let changed = match &self.changed_when {
            Some(expression) => is_render_string(expression, &expression_vars)?,
            None => default_changed,
        };
        let failed = match &self.failed_when {
            Some(expression) => is_render_string(expression, &expression_vars)?,
            None => default_failed,
        };
        let error = failed.then(|| self.failure_message(&result));
        let final_value = Self::result_value(Some(&result), changed, failed, error.as_deref());

        let register_vars = self.register.as_ref().map(|register| {
            [(register.as_str(), final_value)]
                .into_iter()
                .collect::<Value>()
        });
        let new_vars_value = [result_vars, register_vars]
            .into_iter()
            .fold(context! {}, merge_option);
        let new_vars = (new_vars_value != context! {}).then_some(new_vars_value);

        let module_name = self.module.get_name();
        if !self.quiet && !matches!(module_name, "include" | "block" | "meta") {
            log_module_result(changed, failed, &result, hide_output);
        }

        let is_meta_flush = module_name == "meta"
            && result
                .get_extra()
                .and_then(|value| value.as_str().map(str::to_owned))
                == Some("flush_handlers".to_owned());

        let mut exec_result = if failed {
            TaskExecResult::failed(
                changed,
                new_vars,
                error.unwrap_or_else(|| "task failed".into()),
            )
        } else {
            TaskExecResult::new(changed, new_vars)
        };
        if is_meta_flush {
            exec_result = exec_result.with_flush_handlers();
        }
        Ok(exec_result)
    }

    fn module_error_result(&self, error: Error) -> TaskExecResult {
        let message = error.to_string();
        let value = Self::result_value(None, false, true, Some(&message));
        let register_vars = self
            .register
            .as_ref()
            .map(|register| [(register.as_str(), value)].into_iter().collect::<Value>());
        TaskExecResult::failed(false, register_vars, message)
    }

    /// Global params as seen by this task's module: the task-level become and check mode
    /// settings, so child tasks of control-flow modules (block, include) inherit them.
    fn effective_global_params(&self) -> GlobalParams<'_> {
        GlobalParams {
            r#become: self.r#become,
            become_user: &self.become_user,
            become_method: self.become_method,
            become_exe: &self.become_exe,
            become_password: self.become_password.as_deref(),
            check_mode: self.check_mode,
        }
    }

    fn execute_module_with_environment(
        &self,
        rendered_params: &YamlValue,
        vars: &Value,
    ) -> Result<TaskExecResult> {
        let extended_vars = self.extend_vars(vars.clone())?;
        let env_vars = self.render_environment(&extended_vars)?;
        let mut original_env: HashMap<String, Option<String>> = HashMap::new();

        for (key, value) in &env_vars {
            original_env.insert(key.clone(), env::var(key).ok());
            // SAFETY: Rash task execution is sequential. Background processes receive their
            // environment directly through ProcessSpec and do not observe this temporary mutation.
            unsafe { env::set_var(key, value) };
        }

        let module_result = self.module.exec(
            &self.effective_global_params(),
            rendered_params.clone(),
            &extended_vars,
            self.check_mode,
        );

        for (key, value) in original_env {
            // SAFETY: restore the exact pre-task environment before returning.
            unsafe {
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }

        match module_result {
            Ok((result, result_vars)) => {
                let hide_output = self.module.hides_output(rendered_params);
                self.finalize_module_result(result, result_vars, &extended_vars, hide_output)
            }
            Err(error) if error.is_termination() => Err(error),
            Err(error) => Ok(self.module_error_result(error)),
        }
    }

    fn exec_module(&self, vars: Value) -> Result<TaskExecResult> {
        if !self.is_exec(&vars)? {
            debug!("skipping");
            return Ok(TaskExecResult::new(false, None));
        }
        let rendered_params = self.render_params(vars.clone())?;
        if self.runs_with_become() {
            return self.exec_module_with_become(&rendered_params, &vars);
        }
        self.execute_module_with_environment(&rendered_params, &vars)
    }

    fn exec_with_retry(&self, vars: Value) -> Result<TaskExecResult> {
        let max_retries = self.retries.unwrap_or(3);
        let delay = self.delay.unwrap_or(0);
        let mut last_result = TaskExecResult::new(false, None);

        for attempt in 0..=max_retries {
            let result = self.exec_module(vars.clone())?;
            let result_vars = result.get_vars().cloned().unwrap_or(context! {});
            let merged = context! {..result_vars, ..vars.clone()};
            let check_vars = context! {retries => attempt, ..merged};
            if self.is_until_satisfied(&check_vars)? {
                return Ok(result);
            }
            last_result = result;
            if attempt < max_retries && delay > 0 {
                thread::sleep(Duration::from_secs(delay));
            }
        }

        let vars = last_result.take_vars();
        Ok(TaskExecResult::failed(
            false,
            vars,
            format!("until condition not satisfied after {max_retries} retries"),
        ))
    }

    fn exec_sequential_loop(&self, vars: Value) -> Result<TaskExecResult> {
        let mut changed = false;
        let mut failed = false;
        let mut error = None;
        let mut all_vars = context! {};
        let mut flush_handlers = false;

        for item in self.render_iterator(vars.clone())? {
            let item_vars = context! {item => &item, ..vars.clone()};
            let result = self.exec_module(item_vars)?;
            changed |= result.get_changed();
            failed |= result.get_failed();
            if error.is_none() {
                error = result.get_error().map(str::to_owned);
            }
            flush_handlers |= result.is_flush_handlers();
            if let Some(new_vars) = result.take_vars() {
                all_vars = context! {..new_vars, ..all_vars};
            }
            if failed && !self.ignore_errors.unwrap_or(false) {
                break;
            }
        }

        let vars = (all_vars != context! {}).then_some(all_vars);
        let mut result = if failed {
            TaskExecResult::failed(
                changed,
                vars,
                error.unwrap_or_else(|| "loop item failed".into()),
            )
        } else {
            TaskExecResult::new(changed, vars)
        };
        if flush_handlers {
            result = result.with_flush_handlers();
        }
        Ok(result)
    }

    fn exec_loop_with_retry(&self, vars: Value) -> Result<TaskExecResult> {
        let mut changed = false;
        let mut failed = false;
        let mut error = None;
        let mut all_vars = context! {};
        for item in self.render_iterator(vars.clone())? {
            let item_vars = context! {item => &item, ..vars.clone()};
            let result = self.exec_with_retry(item_vars)?;
            changed |= result.get_changed();
            failed |= result.get_failed();
            if error.is_none() {
                error = result.get_error().map(str::to_owned);
            }
            if let Some(new_vars) = result.take_vars() {
                all_vars = context! {..new_vars, ..all_vars};
            }
            if failed && !self.ignore_errors.unwrap_or(false) {
                break;
            }
        }
        let vars = (all_vars != context! {}).then_some(all_vars);
        Ok(if failed {
            TaskExecResult::failed(
                changed,
                vars,
                error.unwrap_or_else(|| "loop retry failed".into()),
            )
        } else {
            TaskExecResult::new(changed, vars)
        })
    }

    fn exec_main_task(&self, vars: Value) -> Result<TaskExecResult> {
        if self.r#loop.is_some() && self.r#async.is_some() {
            self.exec_parallel_loop(vars)
        } else if self.r#loop.is_some() && self.until.is_some() {
            self.exec_loop_with_retry(vars)
        } else if self.r#loop.is_some() {
            self.exec_sequential_loop(vars)
        } else if self.r#async.is_some() {
            self.exec_async_single(vars)
        } else if self.until.is_some() {
            self.exec_with_retry(vars)
        } else {
            self.exec_module(vars)
        }
    }

    fn execute_task_sequence(&self, tasks_yaml: &YamlValue, vars: Value) -> Result<TaskExecResult> {
        let tasks = tasks_yaml.as_sequence().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "task sequence must be a YAML array")
        })?;
        let mut current_vars = vars;
        let mut new_vars = context! {};
        let mut changed = false;
        let mut flush_handlers = false;

        for (index, task_yaml) in tasks.iter().enumerate() {
            let task = Task::new(task_yaml, self.global_params).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid task at index {index}: {e}"),
                )
            })?;
            let result = task.exec(current_vars.clone())?;
            changed |= result.get_changed();
            flush_handlers |= result.is_flush_handlers();
            if let Some(vars) = result.take_vars() {
                current_vars = context! {..vars.clone(), ..current_vars};
                new_vars = context! {..vars, ..new_vars};
            }
        }

        let vars = (new_vars != context! {}).then_some(new_vars);
        let mut result = TaskExecResult::new(changed, vars);
        if flush_handlers {
            result = result.with_flush_handlers();
        }
        Ok(result)
    }

    fn exec_with_rescue_always(&self, vars: Value) -> Result<TaskExecResult> {
        let initial_vars = vars;
        let mut pending_exit = None;
        let mut pending_rescue_error = None;

        let (main_result, main_hard_error) = match self.exec_main_task(initial_vars.clone()) {
            Ok(result) => (result, None),
            Err(error) if error.is_termination() => {
                pending_exit = Some(error);
                (TaskExecResult::new(false, None), None)
            }
            Err(error) => (
                TaskExecResult::failed(false, None, error.to_string()),
                Some(error),
            ),
        };

        let main_failed = pending_exit.is_none()
            && main_result.get_failed()
            && !self.ignore_errors.unwrap_or(false);
        let main_changed = main_result.get_changed();
        let main_error = main_result.get_error().map(str::to_owned);
        let main_vars = main_result.take_vars();
        let post_main_vars = merge_option(initial_vars.clone(), main_vars.clone());

        let mut recovered = !main_failed;
        let rescue = if main_failed {
            match &self.rescue {
                Some(rescue_tasks) => {
                    match self.execute_task_sequence(rescue_tasks, post_main_vars.clone()) {
                        Ok(result) => {
                            recovered = true;
                            Some(result)
                        }
                        Err(error) if error.is_termination() => {
                            pending_exit = Some(error);
                            None
                        }
                        Err(error) => {
                            pending_rescue_error = Some(error);
                            None
                        }
                    }
                }
                None => None,
            }
        } else {
            None
        };

        let rescue_changed = rescue.as_ref().is_some_and(TaskExecResult::get_changed);
        let rescue_vars = rescue.and_then(TaskExecResult::take_vars);
        let post_rescue_vars = merge_option(post_main_vars, rescue_vars.clone());

        // `always` is a true finally section: execute it after main failures, rescue
        // failures, explicit exits and interrupts. A failure/exit in `always` itself
        // takes precedence.
        let always = if let Some(always_tasks) = &self.always {
            Some(self.execute_task_sequence(always_tasks, post_rescue_vars)?)
        } else {
            None
        };
        let always_changed = always.as_ref().is_some_and(TaskExecResult::get_changed);
        let always_vars = always.and_then(TaskExecResult::take_vars);

        if let Some(exit) = pending_exit {
            return Err(exit);
        }
        if let Some(error) = pending_rescue_error {
            return Err(error);
        }

        let all_vars_value = [main_vars, rescue_vars, always_vars]
            .into_iter()
            .fold(context! {}, merge_option);
        let all_vars = (all_vars_value != context! {}).then_some(all_vars_value);
        let changed = main_changed || rescue_changed || always_changed;

        if main_failed && !recovered {
            let message = main_hard_error
                .map(|e| e.to_string())
                .or(main_error)
                .unwrap_or_else(|| "task failed".to_owned());
            Ok(TaskExecResult::failed(changed, all_vars, message))
        } else {
            Ok(TaskExecResult::new(changed, all_vars))
        }
    }
    /// Hide failure details of `no_log` tasks: they may contain secrets (e.g. a command's
    /// stderr) and are reported after log suppression ended. Registered results keep them.
    fn redact_error(&self, error: Error) -> Error {
        if !self.no_log || error.is_termination() {
            return error;
        }
        Error::new(error.kind(), NO_LOG_MESSAGE)
    }

    pub fn exec(&self, vars: Value) -> Result<TaskExecResult> {
        self.exec_unredacted(vars)
            .map_err(|error| self.redact_error(error))
    }

    fn exec_unredacted(&self, vars: Value) -> Result<TaskExecResult> {
        let _no_log_guard = self.no_log.then(suppress_logs);
        debug!("Module: {}", self.module.get_name());
        debug!("Params: {:?}", self.params);

        let execution = if self.rescue.is_some() || self.always.is_some() {
            self.exec_with_rescue_always(vars.clone())
        } else {
            self.exec_main_task(vars.clone())
        };

        let result = match execution {
            Ok(result) => result,
            Err(error) if error.is_termination() => return Err(error),
            Err(error) if self.ignore_errors.unwrap_or(false) => self.module_error_result(error),
            Err(error) => return Err(error),
        };

        if result.get_failed() {
            if self.ignore_errors.unwrap_or(false) {
                info!(target: "ignoring", "{}", result.get_error().unwrap_or("task failed"));
                return Ok(result);
            }
            return Err(Error::new(
                ErrorKind::Other,
                result.get_error().unwrap_or("task failed").to_owned(),
            ));
        }
        Ok(result)
    }

    pub fn get_name(&self) -> Option<String> {
        self.name.clone()
    }

    pub fn get_rendered_name(&self, vars: Value) -> Result<String> {
        render_string(
            self.name
                .as_deref()
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "no name found"))?,
            &vars,
        )
    }

    pub fn get_module(&self) -> &dyn Module {
        self.module
    }

    pub fn get_notify(&self) -> Option<&[String]> {
        self.notify.as_deref()
    }

    pub fn get_no_log(&self) -> bool {
        self.no_log
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

fn merge_default_mappings(
    defaults: &serde_norway::Mapping,
    task: &serde_norway::Mapping,
) -> serde_norway::Mapping {
    let mut merged = defaults.clone();
    for (key, value) in task {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

fn apply_task_defaults(task: &YamlValue, defaults: &YamlValue) -> Result<YamlValue> {
    let task_map = task
        .as_mapping()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "task must be a mapping"))?;
    let defaults_map = defaults
        .as_mapping()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "defaults must be a mapping"))?;
    let mut merged = defaults_map.clone();
    for (key, value) in task_map {
        if matches!(key.as_str(), Some("vars" | "environment"))
            && let (Some(default_map), Some(task_map)) = (
                defaults_map.get(key).and_then(YamlValue::as_mapping),
                value.as_mapping(),
            )
        {
            merged.insert(
                key.clone(),
                YamlValue::Mapping(merge_default_mappings(default_map, task_map)),
            );
            continue;
        }
        merged.insert(key.clone(), value.clone());
    }
    Ok(YamlValue::Mapping(merged))
}

fn parse_tasks_with_defaults<'a>(
    tasks: &[YamlValue],
    defaults: Option<&YamlValue>,
    global_params: &'a GlobalParams<'a>,
) -> Result<Tasks<'a>> {
    tasks
        .iter()
        .map(|task| {
            let effective = match defaults {
                Some(defaults) => apply_task_defaults(task, defaults)?,
                None => task.clone(),
            };
            Task::new(&effective, global_params)
        })
        .collect()
}

pub fn parse_file<'a>(
    file_content: &str,
    global_params: &'a GlobalParams<'a>,
) -> Result<Tasks<'a>> {
    let yaml: YamlValue = serde_norway::from_str(file_content)?;
    match yaml {
        YamlValue::Sequence(tasks) => parse_tasks_with_defaults(&tasks, None, global_params),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected a YAML sequence of tasks, got: {yaml:?}"),
        )),
    }
}

#[derive(Debug)]
pub struct ParsedFile<'a> {
    pub tasks: Tasks<'a>,
    pub handlers: Option<Handlers<'a>>,
}

pub fn parse_file_with_handlers<'a>(
    file_content: &str,
    global_params: &'a GlobalParams<'a>,
) -> Result<ParsedFile<'a>> {
    let yaml: YamlValue = serde_norway::from_str(file_content)?;
    let mapping = yaml.as_mapping().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            "Expected a YAML mapping with tasks (and optional handlers/defaults)",
        )
    })?;

    for key in mapping.keys() {
        if !matches!(key.as_str(), Some("tasks" | "handlers" | "defaults")) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Unknown top-level script key: {key:?}"),
            ));
        }
    }

    let tasks_yaml = mapping
        .get(YamlValue::String("tasks".to_owned()))
        .and_then(YamlValue::as_sequence)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "tasks must be a YAML sequence"))?;
    let defaults = mapping.get(YamlValue::String("defaults".to_owned()));
    let tasks = parse_tasks_with_defaults(tasks_yaml, defaults, global_params)?;

    let handlers = match mapping.get(YamlValue::String("handlers".to_owned())) {
        Some(value) => {
            let handlers = value.as_sequence().ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "handlers must be a YAML sequence")
            })?;
            let effective: Vec<YamlValue> = handlers
                .iter()
                .map(|handler| match defaults {
                    Some(defaults) => apply_task_defaults(handler, defaults),
                    None => Ok(handler.clone()),
                })
                .collect::<Result<_>>()?;
            Some(Handlers::from_yaml(&effective, global_params)?)
        }
        None => None,
    };

    Ok(ParsedFile { tasks, handlers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::context;

    #[test]
    fn failed_when_false_keeps_nonzero_command_as_data() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "exit 7"]
            register: command_result
            failed_when: false
            changed_when: false
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let result = task.exec(context! {}).unwrap();
        assert!(!result.get_failed());
        assert!(!result.get_changed());
        let vars = result.get_vars().unwrap();
        let registered = vars.get_attr("command_result").unwrap();
        assert_eq!(registered.get_attr("rc").unwrap().as_i64(), Some(7));
        assert!(!registered.get_attr("failed").unwrap().is_true());
    }

    #[test]
    fn ignored_failure_is_registered_and_marked_failed() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "echo boom >&2; exit 3"]
            register: command_result
            ignore_errors: true
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let result = task.exec(context! {}).unwrap();
        assert!(result.get_failed());
        let registered = result
            .get_vars()
            .unwrap()
            .get_attr("command_result")
            .unwrap();
        assert_eq!(registered.get_attr("rc").unwrap().as_i64(), Some(3));
        assert!(registered.get_attr("failed").unwrap().is_true());
        assert!(
            registered
                .get_attr("stderr")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("boom")
        );
    }

    #[test]
    fn semantic_failure_triggers_rescue() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "exit 2"]
            rescue:
              - set_vars:
                  rescued: true
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let result = task.exec(context! {}).unwrap();
        assert!(
            result
                .get_vars()
                .unwrap()
                .get_attr("rescued")
                .unwrap()
                .is_true()
        );
    }

    #[test]
    fn module_extra_is_flattened_but_preserved() {
        let extra: YamlValue = serde_norway::from_str("rc: 4\nstderr: nope").unwrap();
        let module_result = ModuleResult::new(true, Some(extra), Some("out".into()));
        let value = Task::result_value(Some(&module_result), true, false, None);
        assert_eq!(value.get_attr("rc").unwrap().as_i64(), Some(4));
        assert_eq!(value.get_attr("stdout").unwrap().as_str(), Some("out"));
        assert_eq!(
            value
                .get_attr("extra")
                .unwrap()
                .get_attr("rc")
                .unwrap()
                .as_i64(),
            Some(4)
        );
    }

    #[test]
    fn script_defaults_merge_environment() {
        let file = r#"
        defaults:
          environment:
            A: one
            B: default
          changed_when: false
        tasks:
          - command: echo hi
            environment:
              B: task
        "#;
        let params = GlobalParams::default();
        let parsed = parse_file_with_handlers(file, &params).unwrap();
        assert_eq!(parsed.tasks.len(), 1);
        let env = parsed.tasks[0].environment.as_ref().unwrap();
        assert_eq!(env["A"].as_str(), Some("one"));
        assert_eq!(env["B"].as_str(), Some("task"));
    }

    #[test]
    fn explicit_exit_runs_always_but_not_rescue_and_ignores_ignore_errors() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            meta:
              action: exit
              code: 17
            ignore_errors: true
            rescue:
              - fail:
                  msg: rescue must not run
            always:
              - debug:
                  msg: cleanup
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let error = task.exec(context! {}).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ExplicitExit);
        assert_eq!(error.raw_os_error(), Some(17));
    }

    #[test]
    fn sequential_loop_keeps_vars_from_the_last_item() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            set_vars:
              from_loop: "{{ item }}"
            loop: [first, last]
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let result = task.exec(context! {}).unwrap();
        let vars = result.get_vars().unwrap();
        assert_eq!(vars.get_attr("from_loop").unwrap().as_str(), Some("last"));
    }

    #[test]
    fn failed_when_sees_current_result_over_stale_var() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "exit 0"]
            register: previous
            failed_when: result.rc != 0 or previous.rc != 0
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let stale = context! {
            result => context! {rc => 1},
            previous => context! {rc => 1},
        };
        assert!(!task.exec(stale).unwrap().get_failed());
    }

    #[test]
    fn rescue_sees_vars_registered_again_by_earlier_rescue_tasks() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            fail:
              msg: boom
            rescue:
              - set_vars:
                  stage: rescued
              - assert:
                  that:
                    - stage == "rescued"
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let result = task.exec(context! {stage => "initial"}).unwrap();
        let vars = result.get_vars().unwrap();
        assert_eq!(vars.get_attr("stage").unwrap().as_str(), Some("rescued"));
    }

    #[test]
    fn no_log_failure_error_is_redacted() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "echo hidden >&2; exit 1"]
            no_log: true
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let task = Task::new(&yaml, &global_params).unwrap();
        let error = task.exec(context! {}).unwrap_err();
        assert_eq!(error.to_string(), NO_LOG_MESSAGE);
    }

    #[test]
    fn invalid_task_attribute_is_rejected() {
        let yaml: YamlValue =
            serde_norway::from_str("command: echo hi\ninvalid_attr: true").unwrap();
        assert!(Task::new(&yaml, &GlobalParams::default()).is_err());
    }
}
