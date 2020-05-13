mod command;

use crate::error::Result;

use std::collections::HashMap;

use serde_json::Value;
use yaml_rust::Yaml;

pub struct ModuleResult {
    changed: bool,
    extra: Option<Value>,
    output: Option<String>,
}

impl ModuleResult {
    pub fn get_changed(&self) -> bool {
        self.changed
    }

    pub fn get_extra(&self) -> Option<Value> {
        self.extra.clone()
    }

    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }
}

/// Module definition with exec function and input parameters
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    name: &'static str,
    exec_fn: fn(Yaml) -> Result<ModuleResult>,
}

impl Module {
    pub fn get_name(&self) -> &str {
        self.name
    }

    pub fn exec(&self, params: Yaml) -> Result<ModuleResult> {
        (self.exec_fn)(params.clone())
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Module {
            name: "test",
            exec_fn: |_: Yaml| {
                Ok(ModuleResult {
                    changed: true,
                    extra: None,
                    output: None,
                })
            },
        }
    }
}

lazy_static! {
    pub static ref MODULES: HashMap<&'static str, Module> = {
        let mut m = HashMap::new();
        m.insert(
            "command",
            Module {
                name: "command",
                exec_fn: command::exec,
            },
        );
        m
    };
}
