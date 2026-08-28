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
fn standard_one_line_usage_remains_active() {
    let file = r#"
#!/usr/bin/env rash
# Usage: tool <value>
#
"#;
    assert_parity(file, &["x"]);
}

#[test]
fn usage_without_comment_spacing_keeps_legacy_inactive_behavior() {
    let file = r#"
#!/usr/bin/env rash
#Usage: tool <value>
#
"#;
    assert_parity(file, &["x"]);
}

#[test]
fn usage_without_colon_spacing_keeps_legacy_inactive_behavior() {
    let file = r#"
#!/usr/bin/env rash
# Usage:tool <value>
#
"#;
    assert_parity(file, &["x"]);
}

#[test]
fn multiline_usage_preserves_legacy_indentation_rules() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool get <value>
#   tool set <value>
#
"#;
    assert_parity(file, &["get", "x"]);
    assert_parity(file, &["set", "x"]);
}
