//! Behavioural tests for the compiled script CLI parser (`rash_core::script_cli`).
//!
//! Every case states the expected template variables, or the expected error kind, explicitly.
//! Cases where the parser intentionally differs from the former Rash parser cite the numbered
//! entries in `rash_core/SCRIPT_CLI_PARSER_REFACTOR.md` ("Intentional differences").
//!
//! - [`extraction`]: which comment lines become the usage declaration and the help text.
//! - [`grammar`]: commands, positionals, groups, optionals, repetition and identifier rules.
//! - [`options`]: option declarations, aliases, clusters, values, defaults and `[options]`.
//! - [`help`]: `help` commands and help options.
//! - [`matching`]: multi-pattern declarations, option positions and real-world interfaces.
//! - [`exhaustive`]: every argv over a small alphabet checked against a reference model.

// The large `json!` literals of the real-world fixtures exceed the default macro recursion limit.
#![recursion_limit = "256"]

mod exhaustive;
mod extraction;
mod grammar;
mod help;
mod matching;
mod options;

use rash_core::{error::ErrorKind, script_cli};
use serde_json::Value;

/// The argv does not match the declaration, or the declaration itself is invalid.
pub const INVALID: ErrorKind = ErrorKind::InvalidData;
/// Help was requested: the parser stops with the help text.
pub const HELP: ErrorKind = ErrorKind::GracefulExit;

/// Template variables on success, or the error kind on failure.
pub type Expected = Result<Value, ErrorKind>;

pub fn parse(file: &str, args: &[&str]) -> Expected {
    script_cli::parse(file, args).map_err(|error| error.kind())
}

/// Assert every `(argv, expected)` case against `file`.
#[track_caller]
pub fn check(file: &str, cases: &[(&[&str], Expected)]) {
    for (args, expected) in cases {
        assert_eq!(&parse(file, args), expected, "args={args:?}");
    }
}

/// Return `defaults` with `overrides` merged in; nested objects (e.g. `options`) merge per key.
pub fn with(defaults: &Value, overrides: Value) -> Value {
    let mut merged = defaults.clone();
    merge(&mut merged, overrides);
    merged
}

fn merge(target: &mut Value, source: Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source,
    }
}
