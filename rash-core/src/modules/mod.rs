mod command;

use std::collections::HashMap;

use yaml_rust::Yaml;

pub type ModuleResult = Yaml;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    exec: fn(Yaml) -> ModuleResult,
    parameters: Option<Yaml>,
}


lazy_static! {
    pub static ref MODULES: HashMap<&'static str, Module> = {
        let mut m = HashMap::new();
        m.insert("command", Module { exec: command::exec, parameters: None });
        m
    };
}
