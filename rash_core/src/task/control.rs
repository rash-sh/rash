//! Task control flow: retries (`until`), loops and `rescue`/`always` sections.
use crate::error::{Error, ErrorKind, Result};
use crate::jinja::merge_option;
use crate::task::Task;
use crate::task::result::{TaskExecResult, non_empty};

use std::thread;
use std::time::Duration;

use minijinja::{Value, context};
use serde_norway::Value as YamlValue;

/// Results of several executions (loop items, section tasks) folded into one: vars of
/// later executions win, `failed` and `changed` if any execution did, the first error.
#[derive(Default)]
pub(super) struct Accumulated {
    changed: bool,
    failed: bool,
    error: Option<String>,
    vars: Option<Value>,
    flush_handlers: bool,
    /// Registered results of the executed loop items, in loop order.
    item_results: Vec<Value>,
}

impl Accumulated {
    fn add(&mut self, result: TaskExecResult) {
        self.changed |= result.get_changed();
        self.failed |= result.get_failed();
        if self.error.is_none() {
            self.error = result.get_error().map(str::to_owned);
        }
        self.flush_handlers |= result.is_flush_handlers();
        if let Some(new_vars) = result.take_vars() {
            self.vars = Some(merge_vars(self.vars.take(), new_vars));
        }
    }

    /// Add the result of a loop item, keeping what it registered (with its `item`) for the
    /// `results` of the loop.
    pub(super) fn add_item(&mut self, task: &Task, item: &YamlValue, result: TaskExecResult) {
        if let Some(registered) = task.registered_value(result.get_vars()) {
            self.item_results
                .push(context! {item => item, ..registered});
        }
        self.add(result);
    }

    /// The result of a loop: the vars and registered result of the last executed item, where
    /// `failed`, `error` and `changed` summarize every item and `results` lists the
    /// registered result of each executed item.
    pub(super) fn into_loop_result(mut self, task: &Task) -> TaskExecResult {
        if let Some(last) = self.item_results.last() {
            let summary = context! {
                changed => self.changed,
                failed => self.failed,
                error => self.error.as_deref(),
                results => &self.item_results,
                ..last.clone()
            };
            if let Some(registered) = task.register_vars(summary) {
                let vars = self.vars.take().unwrap_or(context! {});
                self.vars = Some(context! {..registered, ..vars});
            }
        }
        self.into_result("loop item failed")
    }

    fn into_result(self, default_error: &str) -> TaskExecResult {
        let result = if self.failed {
            let error = self.error.unwrap_or_else(|| default_error.to_owned());
            TaskExecResult::failed(self.changed, self.vars, error)
        } else {
            TaskExecResult::new(self.changed, self.vars)
        };
        if self.flush_handlers {
            result.with_flush_handlers()
        } else {
            result
        }
    }
}

/// `newer` vars on top of `older` ones.
fn merge_vars(older: Option<Value>, newer: Value) -> Value {
    match older {
        Some(older) => context! {..newer, ..older},
        None => newer,
    }
}

/// What happened in the main task or the rescue section of a task with rescue/always.
#[derive(Default)]
struct Section {
    changed: bool,
    vars: Option<Value>,
    /// Failure, ignored by `ignore_errors` or not.
    failed: bool,
    error: Option<String>,
    /// Non-termination error returned instead of a result.
    hard_error: Option<Error>,
    /// Explicit exit or interrupt: only `always` still runs.
    exit: Option<Error>,
}

impl Section {
    fn from_execution(execution: Result<TaskExecResult>) -> Self {
        match execution {
            Ok(result) => Self {
                changed: result.get_changed(),
                failed: result.get_failed(),
                error: result.get_error().map(str::to_owned),
                vars: result.take_vars(),
                ..Self::default()
            },
            Err(error) if error.is_termination() => Self {
                exit: Some(error),
                ..Self::default()
            },
            Err(error) => Self {
                failed: true,
                error: Some(error.to_string()),
                hard_error: Some(error),
                ..Self::default()
            },
        }
    }
}

impl Task {
    /// Repeat the task until `until` holds. A skipped task (`when` false) is not retried and
    /// `until` is not evaluated. `until` sees the task vars, the registered result of the
    /// attempt and the attempt number as `retries`.
    fn exec_with_retry(&self, vars: Value) -> Result<TaskExecResult> {
        if !self.is_exec(&vars)? {
            debug!("skipping");
            return Ok(TaskExecResult::new(false, None));
        }
        let max_retries = self.retries.unwrap_or(3);
        let delay = self.delay.unwrap_or(0);
        let extended = self.extend_vars(vars.clone())?;
        let mut last_result = TaskExecResult::new(false, None);

        for attempt in 0..=max_retries {
            let result = self.exec_module(vars.clone())?;
            let result_vars = result.get_vars().cloned().unwrap_or(context! {});
            let merged = context! {..result_vars, ..extended.clone()};
            let check_vars = context! {retries => attempt, ..merged};
            if self.is_until_satisfied(&check_vars)? {
                return Ok(result);
            }
            last_result = result;
            if attempt < max_retries && delay > 0 {
                thread::sleep(Duration::from_secs(delay));
            }
        }

        let error = format!("until condition not satisfied after {max_retries} retries");
        let vars = self.exhausted_retry_vars(last_result.take_vars(), &error);
        Ok(TaskExecResult::failed(false, vars, error))
    }

    /// Vars of the last attempt, with its registered result marked as failed like the task.
    fn exhausted_retry_vars(&self, vars: Option<Value>, error: &str) -> Option<Value> {
        let registered = self.registered_value(vars.as_ref());
        match (vars, registered) {
            (Some(vars), Some(registered)) => {
                let failed = context! {failed => true, error => error, ..registered};
                Some(context! {..self.register_vars(failed)?, ..vars})
            }
            (vars, _) => vars,
        }
    }

    /// Run `exec_item` for each loop item, stopping at the first failure unless ignored.
    fn exec_loop(
        &self,
        vars: Value,
        exec_item: fn(&Self, Value) -> Result<TaskExecResult>,
    ) -> Result<TaskExecResult> {
        let mut accumulated = Accumulated::default();
        for item in self.render_iterator(vars.clone())? {
            let item_vars = context! {item => &item, ..vars.clone()};
            accumulated.add_item(self, &item, exec_item(self, item_vars)?);
            if accumulated.failed && !self.ignore_errors.unwrap_or(false) {
                break;
            }
        }
        Ok(accumulated.into_loop_result(self))
    }

    pub(super) fn exec_main_task(&self, vars: Value) -> Result<TaskExecResult> {
        match (
            self.r#loop.is_some(),
            self.r#async.is_some(),
            self.until.is_some(),
        ) {
            (true, true, _) => self.exec_parallel_loop(vars),
            (true, false, true) => self.exec_loop(vars, Self::exec_with_retry),
            (true, false, false) => self.exec_loop(vars, Self::exec_module),
            (false, true, _) => self.exec_async_single(vars),
            (false, false, true) => self.exec_with_retry(vars),
            (false, false, false) => self.exec_module(vars),
        }
    }

    /// Run a `rescue` or `always` section: like the main task, its tasks see the task vars
    /// and inherit its become and check mode settings.
    fn execute_task_sequence(&self, tasks_yaml: &YamlValue, vars: Value) -> Result<TaskExecResult> {
        let tasks = tasks_yaml.as_sequence().ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "task sequence must be a YAML array")
        })?;
        let global_params = self.effective_global_params();
        let mut current_vars = self.extend_vars(vars)?;
        let mut accumulated = Accumulated::default();
        for (index, task_yaml) in tasks.iter().enumerate() {
            let task = Task::new(task_yaml, &global_params).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid task at index {index}: {e}"),
                )
            })?;
            let result = task.exec(current_vars.clone())?;
            if let Some(vars) = result.get_vars() {
                current_vars = merge_vars(Some(current_vars), vars.clone());
            }
            accumulated.add(result);
        }
        // Failed tasks returned an error above: what is left are ignored failures.
        accumulated.failed = false;
        Ok(accumulated.into_result("task failed"))
    }

    /// Run the rescue section after a failure of the main task.
    fn run_rescue(&self, vars: Value) -> Option<Section> {
        let rescue_tasks = self.rescue.as_ref()?;
        Some(Section::from_execution(
            self.execute_task_sequence(rescue_tasks, vars),
        ))
    }

    /// `always` is a true finally section: it runs after main failures, rescue failures,
    /// explicit exits and interrupts. A failure or exit in `always` itself takes precedence.
    pub(super) fn exec_with_rescue_always(&self, vars: Value) -> Result<TaskExecResult> {
        let ignore_errors = self.ignore_errors.unwrap_or(false);
        let main = Section::from_execution(self.exec_main_task(vars.clone()));
        let post_main_vars = merge_option(vars, main.vars.clone());

        // An ignored failure is not rescued: the caller reports it as ignored.
        let rescue = if main.failed && !ignore_errors && main.exit.is_none() {
            self.run_rescue(post_main_vars.clone())
        } else {
            None
        };
        let recovered = !main.failed || rescue.as_ref().is_some_and(|r| r.hard_error.is_none());
        let rescue = rescue.unwrap_or_default();
        let post_rescue_vars = merge_option(post_main_vars, rescue.vars.clone());

        let always = match &self.always {
            Some(always_tasks) => self.execute_task_sequence(always_tasks, post_rescue_vars)?,
            None => TaskExecResult::new(false, None),
        };
        if let Some(exit) = main.exit.or(rescue.exit) {
            return Err(exit);
        }
        if let Some(error) = rescue.hard_error {
            return Err(error);
        }

        let changed = main.changed || rescue.changed || always.get_changed();
        let all_vars = non_empty(
            [main.vars, rescue.vars, always.take_vars()]
                .into_iter()
                .fold(context! {}, merge_option),
        );
        if main.failed && !recovered {
            let message = main.error.unwrap_or_else(|| "task failed".to_owned());
            return Ok(TaskExecResult::failed(changed, all_vars, message));
        }
        Ok(TaskExecResult::new(changed, all_vars))
    }
}

#[cfg(test)]
mod tests {
    use crate::context::GlobalParams;
    use crate::jinja::is_render_string;
    use crate::task::{Task, TaskExecResult};

    use minijinja::{Value, context};
    use serde_norway::Value as YamlValue;

    fn exec(yaml: &str) -> crate::error::Result<TaskExecResult> {
        let yaml: YamlValue = serde_norway::from_str(yaml).unwrap();
        let global_params = GlobalParams::default();
        Task::new(&yaml, &global_params)?.exec(context! {})
    }

    fn registered(result: &TaskExecResult, name: &str) -> Value {
        result.get_vars().unwrap().get_attr(name).unwrap()
    }

    fn items(value: &Value, attr: &str) -> Vec<String> {
        value
            .get_attr("results")
            .unwrap()
            .try_iter()
            .unwrap()
            .map(|entry| entry.get_attr(attr).unwrap().to_string())
            .collect()
    }

    fn flags(value: &Value, attr: &str) -> Vec<bool> {
        value
            .get_attr("results")
            .unwrap()
            .try_iter()
            .unwrap()
            .map(|entry| entry.get_attr(attr).unwrap().is_true())
            .collect()
    }

    #[test]
    fn until_sees_task_vars_and_the_registered_result() {
        let result = exec(
            r#"
            command: echo hi
            vars: {want: 0}
            register: r
            until: r.rc == want
            retries: 0
            "#,
        )
        .unwrap();
        assert!(!result.get_failed());
        assert_eq!(
            registered(&result, "r").get_attr("rc").unwrap().as_i64(),
            Some(0)
        );
    }

    #[test]
    fn skipped_task_neither_evaluates_until_nor_retries() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("attempts");
        let result = exec(&format!(
            r#"
            command:
              argv: [sh, -c, "echo ran >> {}"]
            when: false
            register: r
            until: r.rc == 0
            retries: 2
            "#,
            marker.display()
        ))
        .unwrap();
        assert!(!result.get_failed());
        assert!(result.get_vars().is_none());
        assert!(!marker.exists());
    }

    #[test]
    fn loop_ignored_failure_registers_failed_summary_and_last_item() {
        let result = exec(
            r#"
            command:
              argv: [sh, -c, "echo {{ item }}; test {{ item }} != b"]
            loop: [a, b, c]
            register: r
            ignore_errors: true
            "#,
        )
        .unwrap();
        assert!(result.get_failed());
        let vars = result.get_vars().unwrap();
        let r = registered(&result, "r");
        assert!(r.get_attr("failed").unwrap().is_true());
        assert!(r.get_attr("changed").unwrap().is_true());
        assert_eq!(
            r.get_attr("error").unwrap().as_str(),
            Some("command exited with code 1")
        );
        assert_eq!(r.get_attr("stdout").unwrap().as_str(), Some("c\n"));
        assert_eq!(r.get_attr("rc").unwrap().as_i64(), Some(0));
        assert_eq!(r.get_attr("item").unwrap().as_str(), Some("c"));
        assert_eq!(items(&r, "item"), vec!["a", "b", "c"]);
        assert_eq!(flags(&r, "failed"), vec![false, true, false]);
        assert_eq!(items(&r, "stdout"), vec!["a\n", "b\n", "c\n"]);
        assert!(is_render_string("r is failed", vars).unwrap());
        assert!(is_render_string("r.results | length == 3", vars).unwrap());
        assert!(is_render_string("r.results[1] is failed", vars).unwrap());
    }

    #[test]
    fn loop_without_failures_registers_successful_summary() {
        let result = exec(
            r#"
            command:
              argv: [echo, "{{ item }}"]
            loop: [one, two]
            register: r
            changed_when: item == "one"
            "#,
        )
        .unwrap();
        let vars = result.get_vars().unwrap();
        let r = registered(&result, "r");
        assert!(!r.get_attr("failed").unwrap().is_true());
        assert!(r.get_attr("error").unwrap().is_none());
        // Any item changed: the last one did not, the first one did.
        assert!(r.get_attr("changed").unwrap().is_true());
        assert_eq!(flags(&r, "changed"), vec![true, false]);
        assert!(is_render_string("r is succeeded", vars).unwrap());
        assert!(is_render_string("r.results | length == 2", vars).unwrap());
    }

    #[test]
    fn loop_with_until_registers_exhausted_item_as_failed() {
        let result = exec(
            r#"
            command:
              argv: [sh, -c, "echo {{ item }}; test {{ item }} = b"]
            loop: [a, b]
            register: r
            until: r.rc == 0
            retries: 1
            ignore_errors: true
            "#,
        )
        .unwrap();
        assert!(result.get_failed());
        let r = registered(&result, "r");
        assert!(r.get_attr("failed").unwrap().is_true());
        assert!(
            r.get_attr("error")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("until condition not satisfied")
        );
        assert_eq!(r.get_attr("stdout").unwrap().as_str(), Some("b\n"));
        assert_eq!(flags(&r, "failed"), vec![true, false]);
    }

    #[test]
    fn loop_failure_without_ignore_stops_and_registers_results_so_far() {
        let result = exec(
            r#"
            command:
              argv: [sh, -c, "test {{ item }} != b"]
            loop: [a, b, c]
            register: r
            rescue:
              - set_vars:
                  seen: "{{ r.results | length }}"
            "#,
        )
        .unwrap();
        assert!(!result.get_failed());
        let vars = result.get_vars().unwrap();
        assert_eq!(vars.get_attr("seen").unwrap().to_string(), "2");
        let r = registered(&result, "r");
        assert!(r.get_attr("failed").unwrap().is_true());
        assert_eq!(r.get_attr("item").unwrap().as_str(), Some("b"));
    }

    #[test]
    fn async_loop_registers_the_same_shape_as_a_sequential_loop() {
        let result = exec(
            r#"
            command:
              argv: [sh, -c, "echo {{ item }}; test {{ item }} != b"]
            loop: [a, b, c]
            async: 10
            poll: 1
            register: r
            ignore_errors: true
            "#,
        )
        .unwrap();
        assert!(result.get_failed());
        let vars = result.get_vars().unwrap();
        let r = registered(&result, "r");
        assert!(r.get_attr("failed").unwrap().is_true());
        assert_eq!(
            r.get_attr("error").unwrap().as_str(),
            Some("command exited with code 1")
        );
        assert_eq!(r.get_attr("stdout").unwrap().as_str(), Some("c\n"));
        assert_eq!(r.get_attr("item").unwrap().as_str(), Some("c"));
        assert_eq!(items(&r, "item"), vec!["a", "b", "c"]);
        assert_eq!(flags(&r, "failed"), vec![false, true, false]);
        assert!(is_render_string("r is failed", vars).unwrap());
        assert!(is_render_string("r.results | length == 3", vars).unwrap());
    }
}
