/// Context
///
/// Preserve state between executions
use crate::error::{Error, ErrorKind, Result};
use crate::task::Tasks;
use minijinja::Value;

/// Main data structure in `rash`.
/// It contents all [`task::Tasks`] with their [`vars::Vars`] to be executed
///
/// [`task::Tasks`]: ../task/type.Tasks.html
/// [`vars::Vars`]: ../vars/type.Vars.html
#[derive(Debug, Clone)]
pub struct Context<'a> {
    pub tasks: Tasks<'a>,
    vars: Value,
}

impl<'a> Context<'a> {
    /// Create a new context from [`task::Tasks`] and [`vars::Vars`].Error
    ///
    /// [`task::Tasks`]: ../task/type.Tasks.html
    /// [`vars::Vars`]: ../vars/type.Vars.html
    pub fn new(tasks: Tasks<'a>, vars: Value) -> Self {
        Context { tasks, vars }
    }

    /// Execute all Tasks in Context until empty.
    ///
    /// Returns the new variables that were added during execution.
    /// If this finishes correctly, it will return an [`error::Error`] with [`ErrorKind::EmptyTaskStack`].
    ///
    /// [`error::Error`]: ../error/struct.Error.html
    /// [`ErrorKind::EmptyTaskStack`]: ../error/enum.ErrorKind.html
    pub fn exec(&self) -> Result<Option<Value>> {
        let mut context = self.clone();
        let original_vars = self.vars.clone();

        while !context.tasks.is_empty() {
            let mut next_tasks = context.tasks.clone();
            let next_task = next_tasks.remove(0);

            info!(target: "task",
                "[{}:{}] - {} to go - ",
                context.vars.get_attr("rash")?.get_attr("path")?,
                next_task.get_rendered_name(context.vars.clone())
                    .unwrap_or_else(|_| next_task.get_module().get_name().to_owned()),
                context.tasks.len(),
            );

            let vars = next_task.exec(context.vars.clone())?;
            context = Self {
                tasks: next_tasks,
                vars,
            };
        }

        // Calculate what new variables were added
        Self::calculate_new_variables(&original_vars, &context.vars)
    }

    /// Calculate the difference between original and new variables
    /// Returns only the new variables that were added
    fn calculate_new_variables(original: &Value, updated: &Value) -> Result<Option<Value>> {
        // Convert both values to JSON for easier comparison
        let original_json: serde_json::Value = serde_json::to_value(original).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Serialization error: {}", e),
            )
        })?;
        let updated_json: serde_json::Value = serde_json::to_value(updated).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Serialization error: {}", e),
            )
        })?;

        // If they're the same, no new variables
        if original_json == updated_json {
            return Ok(None);
        }

        match (original_json, updated_json) {
            (serde_json::Value::Object(orig_map), serde_json::Value::Object(upd_map)) => {
                let mut new_vars = serde_json::Map::new();

                for (key, value) in upd_map {
                    // Only include variables that are new (not present in original)
                    // or have a different value
                    if !orig_map.contains_key(&key) {
                        new_vars.insert(key, value);
                    }
                }

                if new_vars.is_empty() {
                    Ok(None)
                } else {
                    let new_value = Value::from_serialize(serde_json::Value::Object(new_vars));
                    Ok(Some(new_value))
                }
            }
            // If structure changed completely, might be hard to compare
            _ => Ok(None),
        }
    }

    /// Get a reference to the variables
    pub fn get_vars(&self) -> &Value {
        &self.vars
    }
}

/// [`task::Task`] parameters that can be set globally
#[derive(Debug)]
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

#[cfg(test)]
use std::sync::LazyLock;

#[cfg(test)]
pub static GLOBAL_PARAMS: LazyLock<GlobalParams> = LazyLock::new(GlobalParams::default);
