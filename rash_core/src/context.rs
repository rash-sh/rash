/// Context
///
/// Preserve state between executions
use crate::error::Result;
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
    /// If this finishes correctly, it will return an [`error::Error`] with [`ErrorKind::EmptyTaskStack`].
    ///
    /// [`error::Error`]: ../error/struct.Error.html
    /// [`ErrorKind::EmptyTaskStack`]: ../error/enum.ErrorKind.html
    pub fn exec(&self) -> Result<Self> {
        let mut context = self.clone();

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

        Ok(context)
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
