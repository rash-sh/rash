/// Context
///
/// Preserve state between executions
use crate::executor::task::Task;
use crate::plugins::inventory::env::Inventory;

use std::fs;

extern crate yaml_rust;
use yaml_rust::YamlLoader;

#[derive(Debug)]
pub struct Context {
    tasks: Box<[Task]>,
    inventory: Inventory,
}

impl Context {
    pub fn new(tasks_file_path: &str, inventory: Inventory) -> Self {
        let tasks_file =
            fs::read_to_string(tasks_file_path).expect("Something went wrong reading the file");
        let docs = YamlLoader::load_from_str(&tasks_file).unwrap();
        let tasks: Box<[Task]> = docs.iter().map(Task::from).collect();

        Context {
            tasks: tasks,
            inventory: inventory,
        }
    }
}
