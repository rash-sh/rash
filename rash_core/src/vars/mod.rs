// TODO: remove this file
pub mod builtin;
pub mod env;

use minijinja::Value;

/// Variables stored and accessible during execution, based on [`minijinja::Value`]
///
/// [`minijinja::Value`]: ../../minijinja/macro.context.html
pub type Vars = Value;
