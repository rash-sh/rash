mod command;
mod copy;
mod template;

use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use std::collections::HashMap;

use serde_json::Value;
use yaml_rust::Yaml;

/// Return values by [`Module`] execution.
///
/// [`Module`]: struct.Module.html
#[derive(Debug, PartialEq)]
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
    exec_fn: fn(Yaml, Vars) -> Result<ModuleResult>,
}

impl Module {
    /// Return name
    pub fn get_name(&self) -> &str {
        self.name
    }

    /// Execute `self.exec_fn`
    pub fn exec(&self, params: Yaml, vars: Vars) -> Result<ModuleResult> {
        (self.exec_fn)(params, vars)
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Module {
            name: "test",
            exec_fn: |_, _| {
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
            (
                "template",
                Module {
                    name: "template",
                    exec_fn: template::exec,
                },
            ),
        ]
        .into_iter()
        .collect::<HashMap<&'static str, Module>>()
    };
}

#[inline]
fn get_key(yaml: &Yaml, key: &str) -> Result<Yaml> {
    if yaml[key].is_badvalue() {
        Err(Error::new(
            ErrorKind::NotFound,
            format!("param {} not found in: {:?}", key, yaml),
        ))
    } else {
        Ok(yaml[key].clone())
    }
}

/// Get param from [`Yaml`] with `rash` [`Error`] wrappers.
///
/// # Example
/// ```ignore
/// let param = get_param(&yaml, "foo").unwrap();
/// assert_eq!(param, "boo");
/// ```
/// [`Yaml`]: ../../yaml_rust/struct.Yaml.
/// [`Error`]: ../error/struct.Error.html
#[inline]
pub fn get_param(yaml: &Yaml, key: &str) -> Result<String> {
    match get_key(yaml, key)?.as_str() {
        Some(s) => Ok(s.to_string()),
        None => Err(Error::new(
            ErrorKind::InvalidData,
            format!("param '{}' not valid string in: {:?}", key, yaml),
        )),
    }
}

/// Get param from [`Yaml`] with `rash` [`Error`] wrappers.
///
/// # Example
/// ```ignore
/// let param = get_param_bool(&yaml, "foo").unwrap();
/// assert_eq!(param, true);
/// ```
/// [`Yaml`]: ../../yaml_rust/struct.Yaml.
/// [`Error`]: ../error/struct.Error.html
#[inline]
pub fn get_param_bool(yaml: &Yaml, key: &str) -> Result<bool> {
    match get_key(yaml, key)?.as_bool() {
        Some(x) => Ok(x),
        None => Err(Error::new(
            ErrorKind::InvalidData,
            format!("param '{}' not valid boolean in: {:?}", key, yaml),
        )),
    }
}
