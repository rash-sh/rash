use crate::error::{Error, ErrorKind, Result};
use crate::task::{Task, Tasks};

use std::fs;
use std::path::PathBuf;

use yaml_rust::YamlLoader;

pub fn read_file(tasks_file_path: PathBuf) -> Result<Tasks> {
    let tasks_file = fs::read_to_string(tasks_file_path)
        .or_else(|e| Err(Error::new(ErrorKind::InvalidData, e)))?;

    let docs = YamlLoader::load_from_str(&tasks_file)?;
    let yaml = docs.first().ok_or(Error::new(
        ErrorKind::InvalidData,
        format!("Docs not contain yaml: {:?}", docs),
    ))?;

    yaml.clone()
        .into_iter()
        .map(|task| Task::new(&task))
        .collect::<Result<Tasks>>()
}
