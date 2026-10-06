//! Task results: module results turned into task results (`changed_when`, `failed_when`,
//! `register`) and their logging.
use crate::error::{Error, Result};
use crate::jinja::{is_render_string, merge_option};
use crate::logger::is_json_output;
use crate::modules::ModuleResult;
use crate::task::Task;

use minijinja::{Value, context};
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;

/// `Some(vars)` unless `vars` is empty.
pub(super) fn non_empty(vars: Value) -> Option<Value> {
    (vars != context! {}).then_some(vars)
}

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

    pub(super) fn failed(changed: bool, vars: Option<Value>, error: impl Into<String>) -> Self {
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

impl Task<'_> {
    fn module_default_failed(result: &ModuleResult) -> bool {
        result
            .get_extra()
            .and_then(|extra| extra.get("failed").and_then(YamlValue::as_bool))
            .unwrap_or(false)
    }

    pub(super) fn result_value(
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
        let additions = merge_option(result_binding, self.register_vars(result));
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
            // E.g. `async_status` of a failed job.
            if let Some(error) = extra
                .get("error")
                .and_then(YamlValue::as_str)
                .filter(|value| !value.is_empty())
            {
                return format!("{}: {error}", self.module.get_name());
            }
        }
        format!(
            "Task '{}' failed",
            self.name.as_deref().unwrap_or(self.module.get_name())
        )
    }

    /// Changed and failed status after applying `changed_when` and `failed_when`, which see
    /// the module result as `result` and as the registered var.
    fn evaluate_conditions(&self, result: &ModuleResult, vars: &Value) -> Result<(bool, bool)> {
        let default_changed = result.get_changed();
        let default_failed = Self::module_default_failed(result);
        let error = default_failed.then(|| self.failure_message(result));
        let preliminary = Self::result_value(
            Some(result),
            default_changed,
            default_failed,
            error.as_deref(),
        );
        let expression_vars = self.expression_vars(vars, preliminary);
        let evaluate = |expression: &Option<String>, default: bool| match expression {
            Some(expression) => is_render_string(expression, &expression_vars),
            None => Ok(default),
        };
        Ok((
            evaluate(&self.changed_when, default_changed)?,
            evaluate(&self.failed_when, default_failed)?,
        ))
    }

    /// Vars binding the `register` name (if any) to `value`.
    fn register_vars(&self, value: Value) -> Option<Value> {
        self.register
            .as_ref()
            .map(|register| [(register.as_str(), value)].into_iter().collect::<Value>())
    }

    fn requests_handler_flush(&self, result: &ModuleResult) -> bool {
        self.module.get_name() == "meta"
            && result.get_extra().as_ref().and_then(YamlValue::as_str) == Some("flush_handlers")
    }

    /// `hide_output`: never log the module output (it is still registered).
    pub(super) fn finalize_module_result(
        &self,
        result: ModuleResult,
        result_vars: Option<Value>,
        vars: &Value,
        hide_output: bool,
    ) -> Result<TaskExecResult> {
        let (changed, failed) = self.evaluate_conditions(&result, vars)?;
        let error = failed.then(|| self.failure_message(&result));
        let final_value = Self::result_value(Some(&result), changed, failed, error.as_deref());
        let new_vars = non_empty(
            [result_vars, self.register_vars(final_value)]
                .into_iter()
                .fold(context! {}, merge_option),
        );

        if !self.quiet && !matches!(self.module.get_name(), "include" | "block" | "meta") {
            log_module_result(changed, failed, &result, hide_output);
        }

        let exec_result = match error {
            Some(error) => TaskExecResult::failed(changed, new_vars, error),
            None => TaskExecResult::new(changed, new_vars),
        };
        Ok(if self.requests_handler_flush(&result) {
            exec_result.with_flush_handlers()
        } else {
            exec_result
        })
    }

    pub(super) fn module_error_result(&self, error: Error) -> TaskExecResult {
        let message = error.to_string();
        let value = Self::result_value(None, false, true, Some(&message));
        TaskExecResult::failed(false, self.register_vars(value), message)
    }
}
