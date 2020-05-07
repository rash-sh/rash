/// Context
///
/// Preserve state between executions
use crate::executor::task::Task;
use crate::plugins::inventory::env::Inventory;

use std::error;
use std::fs;
use std::path::PathBuf;

use yaml_rust::YamlLoader;

#[derive(Debug)]
pub struct Context {
    tasks: Box<[Task]>,
    inventory: Inventory,
}

impl Context {
    pub fn new(
        tasks_file_path: PathBuf,
        inventory: Inventory,
    ) -> Result<Self, Box<dyn error::Error>> {
        let tasks_file =
            fs::read_to_string(tasks_file_path).expect("Something went wrong reading the file");
        let docs = YamlLoader::load_from_str(&tasks_file)?;
        let yaml = docs.first().unwrap();
        let tasks: Result<Box<[Task]>, _> = yaml
            .clone()
            .into_iter()
            .map(|task| Task::from(&task))
            .collect();
        Ok(Context {
            tasks: tasks?,
            inventory: inventory,
        })
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Context {
            tasks: vec![Task::test_example()].into_boxed_slice(),
            inventory: Inventory::test_example(),
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

        let context = Context::new(file_path, Inventory::new(env::vars())).unwrap();
        assert_eq!(context.tasks.len(), 2);
    }
}
