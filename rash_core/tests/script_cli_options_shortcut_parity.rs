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
fn one_option_shortcut_rejects_duplicate_non_repeatable_option() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -a --alpha  alpha
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["--alpha"]);
    assert_parity(file, &["--alpha", "--alpha"]);
}

#[test]
fn multi_option_shortcut_preserves_legacy_repeated_group_behavior() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#
"#;

    assert_parity(file, &["--alpha", "--beta"]);
    assert_parity(file, &["--beta", "--alpha"]);
    assert_parity(file, &["--alpha", "--alpha"]);
    assert_parity(file, &["--alpha", "--beta", "--alpha"]);
}
