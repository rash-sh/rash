/// Context
///
/// Preserve state between executions
use crate::error::{Error, ErrorKind, Result};
use crate::task::Tasks;
use crate::vars::Vars;

/// Main data structure in `rash`.
/// It contents all [`task::Tasks`] with their [`vars::Vars`] to be executed
///
/// [`task::Tasks`]: ../task/type.Tasks.html
/// [`vars::Vars`]: ../vars/type.Vars.html
#[derive(Debug)]
pub struct Context {
    tasks: Tasks,
    vars: Vars,
}

impl Context {
    /// Create a new context from [`task::Tasks`] and [`vars::Vars`].Error
    ///
    /// [`task::Tasks`]: ../task/type.Tasks.html
    /// [`vars::Vars`]: ../vars/type.Vars.html
    pub fn new(tasks: Tasks, vars: Vars) -> Self {
        Context { tasks, vars }
    }

    /// Execute the first [`task::Task`] and return a new context without the one that executed [`task::Task`]
    ///
    /// [`task::Task`]: ../task/struct.Task.html
    pub fn exec_task(&self) -> Result<Self> {
        if self.tasks.is_empty() {
            return Err(Error::new(
                ErrorKind::EmptyTaskStack,
                format!("No more tasks in context stack: {self:?}"),
            ));
        }

        let mut next_tasks = self.tasks.clone();
        let next_task = next_tasks.remove(0);
        info!(target: "task",
            "[{}] - {} to go - ",
            next_task.get_rendered_name(self.vars.clone())
                .unwrap_or_else(|_| next_task.get_module().get_name().to_string()),
            self.tasks.len(),
        );
        let vars = next_task.exec(self.vars.clone())?;
        Ok(Self {
            tasks: next_tasks,
            vars,
        })
    }

    /// Execute all Tasks in Context until empty.
    ///
    /// If this finishes correctly, it will return an [`error::Error`] with [`ErrorKind::EmptyTaskStack`]
    ///
    /// [`error::Error`]: ../error/struct.Error.html
    /// [`ErrorKind::EmptyTaskStack`]: ../error/enum.ErrorKind.html
    pub fn exec(context: Self) -> Result<Self> {
        // https://prev.rust-lang.org/en-US/faq.html#does-rust-do-tail-call-optimization
        Self::exec(context.exec_task()?)
    }
}
