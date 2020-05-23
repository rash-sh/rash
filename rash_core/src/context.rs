/// Context
///
/// Preserve state between executions
use crate::error::{Error, ErrorKind, Result};
use crate::facts::Facts;
use crate::task::Tasks;

#[cfg(test)]
use crate::task::Task;

#[cfg(test)]
use crate::facts;

/// Main data structure in `rash`.
/// It contents all [`task::Tasks`] with its [`facts::Facts`] to be executed
///
/// [`task::Tasks`]: ../task/type.Tasks.html
/// [`facts::Facts`]: ../facts/type.Facts.html
#[derive(Debug)]
pub struct Context {
    tasks: Tasks,
    facts: Facts,
}

impl Context {
    /// Create a new context from [`task::Tasks`] and [`facts::Facts`].Error
    ///
    /// [`task::Tasks`]: ../task/type.Tasks.html
    /// [`facts::Facts`]: ../facts/type.Facts.html
    pub fn new(tasks: Tasks, facts: Facts) -> Self {
        Context { tasks, facts }
    }

    /// Execute first [`task::Task`] and return a new context without that executed [`task::Task`]
    ///
    /// [`task::Task`]: ../task/struct.Task.html
    pub fn exec_task(&self) -> Result<Self> {
        if self.tasks.is_empty() {
            return Err(Error::new(
                ErrorKind::EmptyTaskStack,
                format!("No more tasks in context stack: {:?}", self),
            ));
        }

        let mut next_tasks = self.tasks.clone();
        let next_task = next_tasks.remove(0);
        info!(target: "task",
            "[{}] - {} to go - ",
            next_task.get_rendered_name(self.facts.clone())
                .unwrap_or_else(|_| next_task.get_module().get_name().to_string()),
            self.tasks.len(),
        );
        let facts = next_task.exec(self.facts.clone())?;
        Ok(Self {
            tasks: next_tasks,
            facts,
        })
    }

    /// Execute all Tasks in Context until empty.
    ///
    /// If it finish correctly it will return an [`error::Error`] with [`ErrorKind::EmptyTaskStack`]
    ///
    /// [`error::Error`]: ../error/struct.Error.html
    /// [`ErrorKind::EmptyTaskStack`]: ../error/enum.ErrorKind.html
    pub fn exec(context: Self) -> Result<Self> {
        // https://prev.rust-lang.org/en-US/faq.html#does-rust-do-tail-call-optimization
        Self::exec(context.exec_task()?)
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Context {
            tasks: vec![Task::test_example()],
            facts: facts::from_iter(vec![].into_iter()),
        }
    }
}
