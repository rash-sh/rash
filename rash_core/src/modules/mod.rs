mod command;

use crate::error::Result;

use std::collections::HashMap;

use serde_json::Value;
use yaml_rust::Yaml;

/// Return values by [`Module`] execution.
///
/// [`Module`]: struct.Module.html
pub struct ModuleResult {
    changed: bool,
    extra: Option<Value>,
    output: Option<String>,
}

impl ModuleResult {
    /// Return changed
    pub fn get_changed(&self) -> bool {
        self.changed
    }

    /// Return extra
    pub fn get_extra(&self) -> Option<Value> {
        self.extra.clone()
    }

    /// Return output which is printed in log
    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }
}

/// Basic execution structure. Build with module name and module exec function
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    name: &'static str,
    exec_fn: fn(Yaml) -> Result<ModuleResult>,
}

impl Module {
    /// Return name
    pub fn get_name(&self) -> &str {
        self.name
    }

    /// Execute `self.exec_fn`
    pub fn exec(&self, params: Yaml) -> Result<ModuleResult> {
        (self.exec_fn)(params)
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
