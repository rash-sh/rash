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
fn mixed_case_command_is_not_silently_added_to_the_language() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool Run
#
"#;
    assert_parity(file, &["Run"]);
}

#[test]
fn numeric_command_suffix_is_not_silently_added_to_the_language() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool run2
#
"#;
    assert_parity(file, &["run2"]);
}

#[test]
fn uppercase_angle_positional_is_not_silently_added_to_the_language() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <FILE>
#
"#;
    assert_parity(file, &["value"]);
}

#[test]
fn numeric_angle_positional_is_not_silently_added_to_the_language() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <file2>
#
"#;
    assert_parity(file, &["value"]);
}

#[test]
fn punctuation_command_is_not_silently_added_to_the_language() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool foo.bar
#
"#;
    assert_parity(file, &["foo.bar"]);
}
