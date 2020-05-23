mod command;
mod copy;

use crate::error::{Error, ErrorKind, Result};

use std::collections::HashMap;

use serde_json::Value;
use yaml_rust::Yaml;

/// Return values by [`Module`] execution.
///
/// [`Module`]: struct.Module.html
#[derive(PartialEq, Debug)]
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
        vec![
            (
                "command",
                Module {
                    name: "command",
                    exec_fn: command::exec,
                },
            ),
            (
                "copy",
                Module {
                    name: "copy",
                    exec_fn: copy::exec,
                },
            ),
        ]
        .into_iter()
        .collect::<HashMap<&'static str, Module>>()
    };
}

#[inline]
pub fn get_param(yaml: &Yaml, key: &str) -> Result<String> {
    if yaml[key].is_badvalue() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("param {} not found in: {:?}", key, yaml),
        ));
    };

    match yaml[key].as_str() {
        Some(s) => Ok(s.to_string()),
        None => Err(Error::new(
            ErrorKind::InvalidData,
            format!("param '{}' not valid string in: {:?}", key, yaml),
        )),
    }
}
