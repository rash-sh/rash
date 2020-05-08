/// Context
///
/// Preserve state between executions
use crate::executor::task::Task;
use crate::plugins::inventory::{Facts, Inventory};

use std::error;
use std::fs;
use std::path::PathBuf;

use yaml_rust::YamlLoader;

#[derive(Debug)]
pub struct Context {
    tasks: Box<[Task]>,
    facts: Facts,
}

fn read_tasks(tasks_file_path: PathBuf) -> Result<Box<[Task]>, Box<dyn error::Error>> {
    let tasks_file =
        fs::read_to_string(tasks_file_path).expect("Something went wrong reading the file");
    let docs = YamlLoader::load_from_str(&tasks_file)?;
    let yaml = docs.first().unwrap();
    yaml.clone()
        .into_iter()
        .map(|task| Task::from(&task))
        .collect::<Result<Box<[Task]>, _>>()
}

impl Context {
    pub fn new(tasks_file_path: PathBuf, facts: Facts) -> Result<Self, Box<dyn error::Error>> {
        Ok(Context {
            tasks: read_tasks(tasks_file_path)?,
            facts: facts,
        })
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Context {
            tasks: vec![Task::test_example()].into_boxed_slice(),
            facts: Inventory::test_example().load(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env;

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

        let context = Context::new(file_path, Inventory::test_example().load()).unwrap();
        assert_eq!(context.tasks.len(), 2);
    }
}
