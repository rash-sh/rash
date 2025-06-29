mod assert;
mod block;
mod command;
mod copy;
mod debug;
mod file;
pub mod find;
mod get_url;
mod include;
mod lineinfile;
mod pacman;
mod set_vars;
mod setup;
mod systemd;
mod template;
mod uri;

use crate::context::GlobalParams;
use crate::error::{Error, ErrorKind, Result};
use crate::modules::assert::Assert;
use crate::modules::block::Block;
use crate::modules::command::Command;
use crate::modules::copy::Copy;
use crate::modules::debug::Debug;
use crate::modules::file::File;
use crate::modules::find::Find;
use crate::modules::get_url::GetUrl;
use crate::modules::include::Include;
use crate::modules::lineinfile::Lineinfile;
use crate::modules::pacman::Pacman;
use crate::modules::set_vars::SetVars;
use crate::modules::setup::Setup;
use crate::modules::systemd::Systemd;
use crate::modules::template::Template;
use crate::modules::uri::Uri;

use std::collections::HashMap;
use std::sync::LazyLock;

use minijinja::Value;
#[cfg(feature = "docs")]
use schemars::Schema;
use serde::{Deserialize, Serialize};
use serde_norway::Value as YamlValue;

/// Return values of a [`Module`] execution.
///
/// [`Module`]: trait.Module.html
#[derive(Clone, Debug, PartialEq, Serialize)]
// ANCHOR: module_result
pub struct ModuleResult {
    /// True when the executed module changed something.
    changed: bool,
    /// The Output value will appear in logs when module is executed.
    output: Option<String>,
    /// Modules store the data they return in the Extra field.
    extra: Option<YamlValue>,
}
// ANCHOR_END: module_result

impl ModuleResult {
    pub fn new(changed: bool, extra: Option<YamlValue>, output: Option<String>) -> Self {
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
    pub fn get_extra(&self) -> Option<YamlValue> {
        self.extra.clone()
    }

    /// Return output which is printed in log.
    pub fn get_output(&self) -> Option<String> {
        self.output.clone()
    }
}

pub trait Module: Send + Sync + std::fmt::Debug {
    /// Returns the name of the module.
    fn get_name(&self) -> &str;

    /// Executes the module's functionality with the provided parameters.
    ///
    /// This method is responsible for performing the module's core logic.
    /// It accepts a set of YAML parameters and additional variables, then
    /// runs the module's functionality. The result includes both the outcome
    /// of the execution and any potential changes made to the variables.
    fn exec(
        &self,
        global_params: &GlobalParams,
        params: YamlValue,
        vars: &Value,
        check_mode: bool,
    ) -> Result<(ModuleResult, Option<Value>)>;

    /// Determines if the module requires its parameters to be treated as strings.
    ///
    /// By default, this returns `true`, meaning the module will force all parameters
    /// to be interpreted as strings. Override this method if the module should
    /// accept other types.
    fn force_string_on_params(&self) -> bool {
        true
    }

    #[cfg(feature = "docs")]
    fn get_json_schema(&self) -> Option<Schema>;
}

pub static MODULES: LazyLock<HashMap<&'static str, Box<dyn Module>>> = LazyLock::new(|| {
    vec![
        (Assert.get_name(), Box::new(Assert) as Box<dyn Module>),
        (Block.get_name(), Box::new(Block) as Box<dyn Module>),
        (Command.get_name(), Box::new(Command) as Box<dyn Module>),
        (Copy.get_name(), Box::new(Copy) as Box<dyn Module>),
        (Debug.get_name(), Box::new(Debug) as Box<dyn Module>),
        (File.get_name(), Box::new(File) as Box<dyn Module>),
        (Find.get_name(), Box::new(Find) as Box<dyn Module>),
        (GetUrl.get_name(), Box::new(GetUrl) as Box<dyn Module>),
        (Include.get_name(), Box::new(Include) as Box<dyn Module>),
        (
            Lineinfile.get_name(),
            Box::new(Lineinfile) as Box<dyn Module>,
        ),
        (Pacman.get_name(), Box::new(Pacman) as Box<dyn Module>),
        (SetVars.get_name(), Box::new(SetVars) as Box<dyn Module>),
        (Setup.get_name(), Box::new(Setup) as Box<dyn Module>),
        (Systemd.get_name(), Box::new(Systemd) as Box<dyn Module>),
        (Template.get_name(), Box::new(Template) as Box<dyn Module>),
        (Uri.get_name(), Box::new(Uri) as Box<dyn Module>),
    ]
    .into_iter()
    .collect()
});

#[inline(always)]
pub fn is_module(module: &str) -> bool {
    MODULES.get(module).is_some()
}

#[inline(always)]
pub fn parse_params<P>(yaml: YamlValue) -> Result<P>
where
    for<'a> P: Deserialize<'a>,
{
    trace!("parse params: {yaml:?}");
    serde_norway::from_value(yaml).map_err(|e| Error::new(ErrorKind::InvalidData, e))
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
            r#"["yea", "foo", "boo"]"#.to_owned(),
            r#"["fuu", "buu"]"#.to_owned(),
            "yuu".to_owned(),
        ]);
        assert_eq!(
            vec_string,
            vec![
                "yea".to_owned(),
                "foo".to_owned(),
                "boo".to_owned(),
                "fuu".to_owned(),
                "buu".to_owned(),
                "yuu".to_owned()
            ]
        )
    }
}
