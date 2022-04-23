mod assert;
mod command;
mod copy;
mod file;
mod find;
mod set_vars;
mod template;

use crate::error::{Error, ErrorKind, Result};
use crate::utils::get_string;
use crate::vars::Vars;

use std::collections::HashMap;

#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
use serde::{Deserialize, Serialize};
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
    exec_fn: fn(Yaml, Vars, bool) -> Result<(ModuleResult, Vars)>,
    #[cfg(feature = "docs")]
    get_json_schema_fn: Option<fn() -> RootSchema>,
}

impl Module {
    /// Return name.
    pub fn get_name(&self) -> &str {
        self.name
    }

    /// Execute `self.exec_fn`.
    pub fn exec(&self, params: Yaml, vars: Vars, check_mode: bool) -> Result<(ModuleResult, Vars)> {
        (self.exec_fn)(params, vars, check_mode)
    }

    #[cfg(feature = "docs")]
    pub fn get_json_schema(&self) -> Option<RootSchema> {
        self.get_json_schema_fn.map(|f| (f)())
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        Module {
            name: "test",
            exec_fn: |_, _, _| {
                Ok((
                    ModuleResult {
                        changed: true,
                        extra: None,
                        output: None,
                    },
                    Vars::new(),
                ))
            },
            #[cfg(feature = "docs")]
            get_json_schema_fn: None,
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
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(assert::Params::get_json_schema),
                },
            ),
            (
                "command",
                Module {
                    name: "command",
                    exec_fn: command::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(command::Params::get_json_schema),
                },
            ),
            (
                "copy",
                Module {
                    name: "copy",
                    exec_fn: copy::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(copy::Params::get_json_schema),
                },
            ),
            (
                "file",
                Module {
                    name: "file",
                    exec_fn: file::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(file::Params::get_json_schema),
                },
            ),
            (
                "find",
                Module {
                    name: "find",
                    exec_fn: find::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(find::Params::get_json_schema),
                },
            ),
            (
                "set_vars",
                Module {
                    name: "set_vars",
                    exec_fn: set_vars::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: None,
                },
            ),
            (
                "template",
                Module {
                    name: "template",
                    exec_fn: template::exec,
                    #[cfg(feature = "docs")]
                    get_json_schema_fn: Some(template::Params::get_json_schema),
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

#[inline(always)]
pub fn parse_params<P>(yaml: Yaml) -> Result<P>
where
    for<'a> P: Deserialize<'a>,
{
    trace!("parse params: {:?}", yaml);
    serde_yaml::from_str(&get_string(yaml)?).map_err(|e| Error::new(ErrorKind::InvalidData, e))
}

#[inline(always)]
pub fn parse_if_json(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .flat_map(|s| serde_json::from_str(&s).unwrap_or_else(|_| vec![s]))
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_if_json() {
        let vec_string = parse_if_json(vec![
            r#"["yea", "foo", "boo"]"#.to_string(),
            r#"["fuu", "buu"]"#.to_string(),
            "yuu".to_string(),
        ]);
        assert_eq!(
            vec_string,
            vec![
                "yea".to_string(),
                "foo".to_string(),
                "boo".to_string(),
                "fuu".to_string(),
                "buu".to_string(),
                "yuu".to_string()
            ]
        )
    }
}
