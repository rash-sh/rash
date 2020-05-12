/// Context
///
/// Preserve state between executions
use crate::error::Result;
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
    pub fn exec(&self) -> Result<Self> {
        let mut next_tasks = self.tasks.clone();
        let next_task = next_tasks.remove(0);
        let facts = next_task.execute(self.facts.clone())?;
        Ok(Self {
            tasks: self.tasks.clone(),
            facts: facts,
        })
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Context {
            tasks: vec![Task::test_example()],
            facts: facts_text_example(),
        }
    }
}
