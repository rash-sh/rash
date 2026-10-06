mod asynchronous;
mod control;
mod handler;
mod new;
mod privilege;
mod result;
mod valid;

pub use handler::{Handlers, PendingHandlers, parse_notify_value};
pub use privilege::{
    BecomeOutcome, BecomeUser, InternalTaskData, RASH_INTERNAL_OUTPUT_ENV,
    RASH_INTERNAL_RESULT_ENV, RASH_INTERNAL_TASK_ENV, RASH_INTERNAL_TASK_FLAG,
    execute_internal_task, get_internal_output, get_internal_result_path, is_internal_execution,
    is_internal_task_execution,
};
pub use result::TaskExecResult;

use crate::context::{BecomeMethod, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::{is_render_string, render, render_force_string, render_map, render_string};
use crate::logger::suppress_logs;
use crate::modules::Module;
use crate::task::new::TaskNew;

use rash_derive::FieldNames;

use std::collections::HashMap;
use std::env;

use minijinja::{Value, context};
use serde_norway::Value as YamlValue;

/// Failure message reported instead of the real one for `no_log` tasks (as Ansible does).
pub const NO_LOG_MESSAGE: &str =
    "the output has been hidden due to the fact that 'no_log: true' was specified for this result";

#[derive(Debug, Clone, FieldNames)]
// ANCHOR: task
pub struct Task {
    r#become: bool,
    become_user: String,
    become_method: BecomeMethod,
    become_exe: String,
    become_password: Option<String>,
    check_mode: bool,
    #[field_names(skip)]
    module: &'static dyn Module,
    #[field_names(skip)]
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
}
// ANCHOR_END: task

pub type Tasks = Vec<Task>;

impl Task {
    pub fn new(yaml: &YamlValue, global_params: &GlobalParams) -> Result<Self> {
        trace!("new task: {yaml:?}");
        TaskNew::from(yaml)
            .validate_attrs()?
            .get_task(global_params)
    }

    fn is_attr(attr: &str) -> bool {
        Self::FIELD_NAMES.contains(&attr)
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

    /// Run the task in a become child process: a failure is reported as a failed result,
    /// without applying `ignore_errors` or logging it, since the parent task does both.
    pub(crate) fn exec_in_become_child(&self, vars: Value) -> Result<TaskExecResult> {
        let _no_log_guard = self.no_log.then(suppress_logs);
        match self.exec_main_task(vars) {
            Err(error) if !error.is_termination() => Ok(self.module_error_result(error)),
            execution => execution,
        }
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
impl From<YamlValue> for Task {
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

/// Merge script or block `defaults` into a task: task values win, `vars` and `environment`
/// maps are merged key by key.
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

pub(crate) fn parse_tasks_with_defaults(
    tasks: &[YamlValue],
    defaults: Option<&YamlValue>,
    global_params: &GlobalParams,
) -> Result<Tasks> {
    tasks
        .iter()
        .enumerate()
        .map(|(index, task)| {
            let effective = match defaults {
                Some(defaults) => apply_task_defaults(task, defaults),
                None => Ok(task.clone()),
            };
            effective
                .and_then(|effective| Task::new(&effective, global_params))
                .map_err(|e| {
                    Error::new(
                        e.kind(),
                        format!("Failed to parse task at index {index}: {e}"),
                    )
                })
        })
        .collect()
}

pub fn parse_file(file_content: &str, global_params: &GlobalParams) -> Result<Tasks> {
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
pub struct ParsedFile {
    pub tasks: Tasks,
    pub handlers: Option<Handlers>,
}

pub fn parse_file_with_handlers(
    file_content: &str,
    global_params: &GlobalParams,
) -> Result<ParsedFile> {
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

/// Whether a script is written in the task sequence form (instead of the mapping form).
pub fn is_task_sequence(script: &str) -> bool {
    serde_norway::from_str::<YamlValue>(script).is_ok_and(|yaml| yaml.is_sequence())
}

/// Parse a script in either form: a sequence of tasks or a mapping with `tasks` (and
/// optional `handlers` and `defaults`). Errors are those of the form the script uses.
pub fn parse_script(script: &str, global_params: &GlobalParams) -> Result<ParsedFile> {
    if is_task_sequence(script) {
        return Ok(ParsedFile {
            tasks: parse_file(script, global_params)?,
            handlers: None,
        });
    }
    parse_file_with_handlers(script, global_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ModuleResult;
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
    fn task_values_override_defaults_and_maps_merge() {
        let task: YamlValue = serde_norway::from_str(
            r#"
            command: echo hi
            become: false
            environment:
              B: task
            "#,
        )
        .unwrap();
        let defaults: YamlValue = serde_norway::from_str(
            r#"
            become: true
            environment:
              A: default
              B: default
            "#,
        )
        .unwrap();
        let merged = apply_task_defaults(&task, &defaults).unwrap();
        assert_eq!(merged["become"].as_bool(), Some(false));
        assert_eq!(merged["environment"]["A"].as_str(), Some("default"));
        assert_eq!(merged["environment"]["B"].as_str(), Some("task"));
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
    fn failed_async_job_error_is_the_task_error() {
        let global_params = GlobalParams::default();
        let start: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [sh, -c, "echo boom >&2; exit 3"]
            async: 60
            poll: 0
            register: job
            "#,
        )
        .unwrap();
        let started = Task::new(&start, &global_params)
            .unwrap()
            .exec(context! {})
            .unwrap();
        let vars = started.get_vars().unwrap();
        let jid = vars
            .get_attr("job")
            .unwrap()
            .get_attr("rash_job_id")
            .unwrap()
            .as_i64()
            .unwrap() as u64;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while crate::job::get_job(jid) == Some(crate::job::JobStatus::Running) {
            assert!(
                std::time::Instant::now() < deadline,
                "job {jid} still running"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let status: YamlValue =
            serde_norway::from_str(&format!("async_status:\n  jid: {jid}")).unwrap();
        let error = Task::new(&status, &global_params)
            .unwrap()
            .exec(vars.clone())
            .unwrap_err();
        assert!(error.to_string().contains("code 3: boom"), "{error}");
    }

    #[test]
    fn exhausted_until_registers_a_failed_result() {
        let yaml: YamlValue = serde_norway::from_str(
            r#"
            command:
              argv: [echo, never]
            register: probe
            until: probe.stdout == "done"
            retries: 1
            ignore_errors: true
            "#,
        )
        .unwrap();
        let global_params = GlobalParams::default();
        let result = Task::new(&yaml, &global_params)
            .unwrap()
            .exec(context! {})
            .unwrap();
        assert!(result.get_failed());
        let probe = result.get_vars().unwrap().get_attr("probe").unwrap();
        assert!(probe.get_attr("failed").unwrap().is_true());
        assert_eq!(probe.get_attr("rc").unwrap().as_i64(), Some(0));
        assert!(
            probe
                .get_attr("error")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("until condition")
        );
    }

    #[test]
    fn task_attributes_are_the_task_fields_without_internals() {
        for attr in ["become", "loop", "async", "no_log", "rescue", "poll"] {
            assert!(Task::is_attr(attr), "{attr}");
        }
        for internal in ["module", "params", "global_params"] {
            assert!(!Task::is_attr(internal), "{internal}");
        }
    }

    #[test]
    fn invalid_task_attribute_is_rejected() {
        let yaml: YamlValue =
            serde_norway::from_str("command: echo hi\ninvalid_attr: true").unwrap();
        assert!(Task::new(&yaml, &GlobalParams::default()).is_err());
    }
}
