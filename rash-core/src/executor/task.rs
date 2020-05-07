use crate::modules::{Module, MODULES};

use std::error;
use std::fmt;

use yaml_rust::Yaml;

#[derive(Debug, Clone)]
struct ModuleNotFound;

impl fmt::Display for ModuleNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid first item to double")
    }
}

impl error::Error for ModuleNotFound {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[derive(Debug)]
pub struct Task {
    module: Module,
    name: Option<String>,
}

fn is_module(module: &(&Yaml, &Yaml)) -> bool {
    match MODULES.get(module.0.as_str().expect("Key is not string")) {
        Some(_) => true,
        None => false,
    }
}

#[inline(always)]
fn find_module(task: &Yaml) -> Option<&Module> {
    println!("{:?}", task);
    task.clone()
        .into_hash()
        .unwrap()
        .iter()
        .filter(|key| is_module(key))
        .map(|(key, _)| key.as_str().expect("Key is not string"))
        .map(|s| MODULES.get(s))
        .next()?
}

impl Task {
    pub fn from(task: &Yaml) -> Result<Self, Box<dyn error::Error>> {
        let module = find_module(task).ok_or(ModuleNotFound)?;
        Ok(Task {
            module: module.clone(),
            name: task["name"].as_str().map(String::from),
        })
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Task {
            module: Module::test_example(),
            name: None,
        }
    }
}

// execute tasks requires contexts and replace Jinja

#[cfg(test)]
mod tests {
    use super::*;

    use yaml_rust::YamlLoader;

    #[test]
    fn test_from_yaml() {
        let s: String = r#"
        name: 'Test task'
        command: 'example'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(&yaml).unwrap();
        println!("{:?}", task);
        assert_eq!(task.name.unwrap(), "Test task");
        assert_eq!(&task.module, MODULES.get("command").unwrap());
    }

    #[test]
    fn test_from_yaml_no_module() {
        let s: String = r#"
        name: 'Test task'
        no_module: 'example'
        "#
        .to_owned();
        let out = YamlLoader::load_from_str(&s).unwrap();
        let yaml = out.first().unwrap();
        let task = Task::from(&yaml);
        assert!(task.is_err());
    }
}
