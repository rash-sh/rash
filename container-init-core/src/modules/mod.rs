mod command;

use std::collections::HashMap;

use yaml_rust::Yaml;

pub type Module = fn(Yaml);

lazy_static! {
    pub static ref MODULES: HashMap<&'static str, Module> = {
        let mut m = HashMap::new();
        m.insert("command", command::command as fn(_));
        m
    };
}
