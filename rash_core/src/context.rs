/// Context
///
/// Preserve state between executions
use crate::error::Result;
use crate::plugins::facts::Facts;
use crate::task::Task;

#[cfg(test)]
use crate::plugins::facts::test_example as facts_text_example;

use std::fs;
use std::path::PathBuf;

use yaml_rust::YamlLoader;

#[derive(Debug)]
pub struct Context {
    tasks: Vec<Task>,
    facts: Facts,
}

impl Context {
    fn read_tasks(tasks_file_path: PathBuf) -> Result<Vec<Task>> {
        let tasks_file = fs::read_to_string(tasks_file_path)?;
        let docs = YamlLoader::load_from_str(&tasks_file)?;
        let yaml = docs.first().unwrap();
        yaml.clone()
            .into_iter()
            .map(|task| Task::new(&task))
            .collect::<Result<Vec<Task>>>()
    }

    pub fn new(tasks_file_path: PathBuf, facts: Facts) -> Result<Self> {
        Ok(Context {
            tasks: Context::read_tasks(tasks_file_path)?,
            facts: facts,
        })
    }

    /// Execute task using inventory
    pub fn execute_task(&self) -> Result<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_context_new() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("entrypoint.rh");
        let mut file = File::create(file_path.clone()).unwrap();
        writeln!(
            file,
            r#"
        #!/bin/rash
        - name: task 1
          command:
            foo: boo

        - name: task 2
          command: boo
        "#
        )
        .unwrap();

        let context = Context::new(file_path, facts_text_example()).unwrap();
        assert_eq!(context.tasks.len(), 2);
    }
}
