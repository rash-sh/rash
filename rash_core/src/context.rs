/// Context
///
/// Preserve state between executions
use crate::error::{Error, ErrorKind, Result};
use crate::facts::Facts;
use crate::task::Tasks;

#[cfg(test)]
use crate::task::Task;

#[cfg(test)]
use crate::facts::test_example as facts_text_example;

#[derive(Debug)]
pub struct Context {
    tasks: Tasks,
    facts: Facts,
}

impl Context {
    pub fn new(tasks: Tasks, facts: Facts) -> Self {
        Context {
            tasks: tasks,
            facts: facts,
        }
    }

    /// Execute task using inventory
    pub fn exec_task(&self) -> Result<Self> {
        let mut next_tasks = self.tasks.clone();
        if next_tasks.len() == 0 {
            return Err(Error::new(
                ErrorKind::EmptyTaskStack,
                format!("No more tasks in context stack: {:?}", self),
            ));
        }

        let next_task = next_tasks.remove(0);
        info!(target: "task",
            "[{}] - {} to go - ",
            next_task.get_rendered_name(self.facts.clone())
                .clone()
                .unwrap_or_else(|| next_task.get_module().get_name().to_string()),
            self.tasks.len(),
        );
        let facts = next_task.exec(self.facts.clone())?;
        Ok(Self {
            tasks: next_tasks.clone(),
            facts: facts,
        })
    }

    pub fn exec(context: Self) -> Result<Self> {
        // https://prev.rust-lang.org/en-US/faq.html#does-rust-do-tail-call-optimization
        Self::exec(context.exec_task()?)
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Context {
            tasks: vec![Task::test_example()],
            facts: facts_text_example(),
        }
    }
}
