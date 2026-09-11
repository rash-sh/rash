use rash_core::{docopt, script_cli};

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
fn nested_option_requires_outer_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [command [--force]]
#
# Options:
#   --force  force
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["command"]);
    assert_parity(file, &["command", "--force"]);
    assert_parity(file, &["--force"]);
}

#[test]
fn nested_positional_requires_outer_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [command [<value>]]
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["command"]);
    assert_parity(file, &["command", "value"]);
    assert_parity(file, &["value"]);
}

#[test]
fn flat_optional_sequence_stays_independent() {
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
