use yaml_rust::Yaml;

use crate::modules::ModuleResult;

pub fn exec(optional_parameters: Option<Yaml>) -> ModuleResult {
    ModuleResult {
        changed: true,
        extra: None,
    }
}
