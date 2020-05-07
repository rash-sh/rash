mod command;

use crate::context::Context;

use std::collections::HashMap;

use yaml_rust::Yaml;

pub struct ModuleResult {
    changed: bool,
    extra: Option<Yaml>,
}

/// Module definition with exec function and params received
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    exec: fn(Option<Yaml>) -> ModuleResult,
    params: Option<Yaml>,
}

#[cfg(test)]
impl Module {
    pub fn test_example() -> Self {
        Module {
            exec: |_: Option<Yaml>| ModuleResult {
                changed: true,
                extra: None,
            },
            params: None,
        }
    }
}

lazy_static! {
    pub static ref MODULES: HashMap<&'static str, Module> = {
        let mut m = HashMap::new();
        m.insert(
            "command",
            Module {
                exec: command::exec,
                params: None,
            },
        );
        m
    };
}

/// Module with rendered params ready to be executed
#[derive(Debug)]
pub struct ModuleExec {
    // until unnamed field: https://gcc.gnu.org/onlinedocs/gcc/Unnamed-Fields.html
    module: Module,
    rendered_params: Option<Yaml>,
}

/// Render string params with Jinja2 and context substitution
fn render_params(_context: Context, args: Option<Yaml>) -> Option<Yaml> {
    // TODO jinja2 on strings with context
    Some(args?)
}

/// Verify input args are valid
fn verify_params(_params: Yaml, _args: Yaml) -> bool {
    // TODO
    true
}

impl ModuleExec {
    pub fn new(module: Module, context: Context, args: Option<Yaml>) -> Self {
        ModuleExec {
            module: module,
            rendered_params: render_params(context, args),
        }
    }

    pub fn exec(&self) -> ModuleResult {
        let exec_fn = self.module.exec;
        exec_fn(self.rendered_params.clone())
    }

    #[cfg(test)]
    pub fn test_example() -> Self {
        ModuleExec {
            module: MODULES.get("command").unwrap().clone(),
            rendered_params: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moduleexec_new() {
        let module_exec = ModuleExec::new(Module::test_example(), Context::test_example(), None);
        assert_eq!(module_exec.module, Module::test_example());
        assert_eq!(module_exec.rendered_params, None);
    }
}
