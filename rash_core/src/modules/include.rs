/// ANCHOR: module
/// # include
///
/// This module include tasks to be executed from another file.
///
/// ## Attributes
///
/// ```yaml
/// check_mode:
///   support: none
/// ```
/// ANCHOR_END: module
/// ANCHOR: parameters
/// | Parameter | Required | Type   | Values | Description                                                 |
/// | --------- | -------- | ------ | ------ | ----------------------------------------------------------- |
/// | file      | true     | string |        | Parse target file and execute tasks in the current context. |
///
/// ANCHOR_END: parameters
///
/// ANCHOR: examples
/// ## Example
///
/// ```yaml
/// - include: foo.rh
///
/// - include: "{{ rash.dir }}/bar.rh"
///
/// - include: "{{ env.HOSTNAME }}.rh"
/// ```
/// ANCHOR_END: examples
use crate::context::{Context, GlobalParams};
use crate::error::{Error, ErrorKind, Result};
use crate::modules::{Module, ModuleResult};
use crate::task::parse_file;
use crate::vars::builtin::Builtins;

use std::fs::read_to_string;
use std::path::Path;

use minijinja::{Value, context};
#[cfg(feature = "docs")]
use schemars::schema::RootSchema;
use serde::Deserialize;
use serde_yaml::Value as YamlValue;

#[derive(Debug)]
pub struct Include;

impl Module for Include {
    fn get_name(&self) -> &str {
        "include"
    }

    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: Value,
        _check_mode: bool,
    ) -> Result<(ModuleResult, Value)> {
        match params {
            YamlValue::String(script_file) => {
                let script_path = Path::new(&script_file);

                trace!("reading tasks from: {script_path:?}");

                let main_file = read_to_string(script_path).map_err(|e| {
                    Error::new(
                        ErrorKind::InvalidData,
                        format!("Error reading file: {:?}", e),
                    )
                })?;

                let tasks = parse_file(&main_file, global_params)?;
                let builtins = Builtins::deserialize(vars.get_attr("rash")?)?;
                let include_builtins = builtins.update(script_path)?;
                let include_vars = context! {rash => &include_builtins, ..vars.clone()};

                trace!("Vars: {include_vars}");
                Context::new(tasks, include_vars.clone()).exec()?;

                Ok((ModuleResult::new(false, None, None), vars))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "file parameter must be a string",
            )),
        }
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<RootSchema> {
        None
    }
}
