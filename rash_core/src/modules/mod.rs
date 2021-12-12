mod assert;
mod command;
mod copy;
mod file;
mod set_vars;
mod template;

use crate::error::{Error, ErrorKind, Result};
use crate::vars::Vars;

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use yaml_rust::Yaml;

/// Return values of a [`Module`] execution.
///
/// [`Module`]: struct.Module.html
#[derive(Clone, Debug, PartialEq, Serialize)]
// ANCHOR: module_result
pub struct ModuleResult {
    /// True when the executed module changed something.
    changed: bool,
    /// The Output value will appear in logs when module is executed.
    output: Option<String>,
    /// Modules store the data they return in the Extra field.
    extra: Option<Value>,
}
// ANCHOR_END: module_result

impl ModuleResult {
    pub fn new(changed: bool, extra: Option<Value>, output: Option<String>) -> Self {
        Self {
            changed,
            extra,
            output,
        }
    }

    /// Return changed.
    pub fn get_changed(&self) -> bool {
        self.changed
    }

    /// Return extra.
    pub fn get_extra(&self) -> Option<Value> {
        self.extra.clone()
    }

    /// Return output which is printed in log.
    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }
}

/// Basic execution structure. Build with module name and module exec function.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    name: &'static str,
    exec_fn: fn(Yaml, Vars) -> Result<(ModuleResult, Vars)>,
}

impl Module {
    /// Return name.
    pub fn get_name(&self) -> &str {
        self.name
    }

    /// Execute `self.exec_fn`.
    pub fn exec(&self, params: Yaml, vars: Vars) -> Result<(ModuleResult, Vars)> {
        (self.exec_fn)(params, vars)
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Module {
            name: "test",
            exec_fn: |_, _| {
                Ok((
                    ModuleResult {
                        changed: true,
                        extra: None,
                        output: None,
                    },
                    Vars::new(),
                ))
            },
        }
    }
}

lazy_static! {
    pub static ref MODULES: HashMap<&'static str, Module> = {
        vec![
            (
                "assert",
                Module {
                    name: "assert",
                    exec_fn: assert::exec,
                },
            ),
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
                "file",
                Module {
                    name: "file",
                    exec_fn: file::exec,
                },
            ),
            (
                "set_vars",
                Module {
                    name: "set_vars",
                    exec_fn: set_vars::exec,
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

#[inline(always)]
pub fn is_module(module: &str) -> bool {
    MODULES.get(module).is_some()
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

/// Get param from [`Yaml`] with `rash` [`Error`] wrappers.
///
/// # Example
/// ```ignore
/// let param = get_param_list(&yaml, "foo").unwrap();
/// assert_eq!(param, vec!["1 == 1"]);
/// ```
/// [`Yaml`]: ../../yaml_rust/struct.Yaml.
/// [`Error`]: ../error/struct.Error.html
#[inline]
pub fn get_param_list(yaml: &Yaml, key: &str) -> Result<Vec<String>> {
    match get_key(yaml, key)?.as_vec() {
        Some(x) => Ok(x
            .iter()
            .map(|yaml| match yaml.as_str() {
                Some(s) => Ok(s.to_string()),
                None => Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("{:?} is not a valid string", &yaml),
                )),
            })
            .collect::<Result<Vec<String>>>()?),
        None => Err(Error::new(
            ErrorKind::InvalidData,
            format!("param '{}' not valid boolean in: {:?}", key, yaml),
        )),
    }
}
