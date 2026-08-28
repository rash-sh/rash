use rash_core::{docopt, error::ErrorKind, script_cli};
use serde_json::Value;

fn assert_parity(file: &str, args: &[&str]) {
    let legacy = docopt::parse(file, args);
    let compiled = script_cli::parse(file, args);
    match (legacy, compiled) {
        (Ok(legacy), Ok(compiled)) => assert_eq!(compiled, legacy, "args={args:?}"),
        (Err(legacy), Err(compiled)) => {
            assert_eq!(compiled.kind(), legacy.kind(), "args={args:?}")
        }
        (legacy, compiled) => {
            panic!("parser mismatch for args={args:?}: legacy={legacy:?} compiled={compiled:?}")
        }
    }
}

#[test]
fn optional_sequence_is_independent_like_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [alpha beta]
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["alpha"]);
    assert_parity(file, &["beta"]);
    assert_parity(file, &["alpha", "beta"]);
}

#[test]
fn grouped_optional_sequence_remains_atomic() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [(alpha beta)]
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["alpha", "beta"]);
    assert_parity(file, &["alpha"]);
    assert_parity(file, &["beta"]);
}

#[test]
fn optional_options_are_order_independent() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-b] [-c]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#   -c --charlie  charlie
#
"#;

    for args in [
        vec![],
        vec!["-a"],
        vec!["-b", "-a"],
        vec!["--charlie", "--alpha", "--beta"],
        vec!["-cba"],
    ] {
        assert_parity(file, &args);
    }
}

#[test]
fn stacked_short_options_with_value_keep_legacy_semantics() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-vfo FILE] [INPUT ...]
#
# Options:
#   -v --verbose  verbose
#   -f --force    force
#   -o FILE       output [default: stdout]
#
"#;

    for args in [
        vec!["-v", "input"],
        vec!["-o", "result", "input"],
        vec!["-voresult", "input"],
        vec!["-fvo=result", "input"],
    ] {
        assert_parity(file, &args);
    }
}

#[test]
fn dashed_commands_and_positionals_keep_context_names() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool (daemon-reload|daemon-reexec) <unit-name>
#
"#;

    assert_parity(file, &["daemon-reload", "foo.service"]);
    assert_parity(file, &["daemon-reexec", "foo.service"]);
}

#[test]
fn uppercase_positionals_keep_legacy_shape() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool SOURCE DEST
#
"#;

    assert_parity(file, &["a", "b"]);
}

#[test]
fn option_aliases_share_one_output_key() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--dry-run]
#
# Options:
#   -d --dry-run  dry run
#
"#;

    assert_parity(file, &["-d"]);
    assert_parity(file, &["--dry-run"]);
}

#[test]
fn option_values_can_contain_equals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--env=<key=value>]
#
"#;

    assert_parity(file, &["--env=FOO=BAR"]);
    assert_parity(file, &["--env", "FOO=BAR"]);
}

#[test]
fn unknown_option_matches_legacy_error_kind() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--known]
#
"#;

    let legacy = docopt::parse(file, &["--unknown"]).unwrap_err();
    let compiled = script_cli::parse(file, &["--unknown"]).unwrap_err();
    assert_eq!(legacy.kind(), ErrorKind::InvalidData);
    assert_eq!(compiled.kind(), legacy.kind());
}

#[test]
fn help_command_and_option_match_legacy_exit_kind() {
    let command = r#"
#!/usr/bin/env rash
#
# Usage: tool (run|help)
#
"#;
    let option = r#"
#!/usr/bin/env rash
#
# Usage: tool [-h]
#
# Options:
#   -h --help  help
#
"#;

    assert_parity(command, &["help"]);
    assert_parity(option, &["-h"]);
    assert_parity(option, &["--help"]);
}

#[test]
fn no_usage_returns_empty_context() {
    let file = r#"
#!/usr/bin/env rash
# No CLI declaration here.
- debug:
    msg: hi
"#;

    assert_eq!(script_cli::parse(file, &[]).unwrap(), Value::Object(Default::default()));
    assert_parity(file, &[]);
}

#[test]
fn identical_bindings_from_overlapping_patterns_are_not_ambiguous() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

    assert_parity(file, &["one", "dest"]);
    assert_parity(file, &["one", "two", "dest"]);
}

#[test]
fn repeated_group_rejects_incomplete_tail() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool (<a> <b>)...
#
"#;

    assert_parity(file, &["1", "2"]);
    assert_parity(file, &["1", "2", "3", "4"]);
    assert_parity(file, &["1", "2", "3"]);
}

#[test]
fn alternatives_and_optional_options_can_mix_positions() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--verbose] (start|stop) [--force]
#
# Options:
#   --verbose  verbose
#   --force    force
#
"#;

    for args in [
        vec!["start"],
        vec!["--verbose", "start"],
        vec!["stop", "--force"],
        vec!["--force", "--verbose", "start"],
    ] {
        assert_parity(file, &args);
    }
}
