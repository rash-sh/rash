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
fn duplicate_non_repeatable_explicit_option_matches_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-b]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#
"#;

    assert_parity(file, &["-a", "-a"]);
    assert_parity(file, &["-b", "-b"]);
    assert_parity(file, &["-a", "-b", "-a"]);
}

#[test]
fn duplicate_non_repeatable_options_shortcut_matches_legacy() {
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

    assert_parity(file, &["-a", "-a"]);
    assert_parity(file, &["--alpha", "--alpha"]);
    assert_parity(file, &["-a", "-b", "-a"]);
}

#[test]
fn explicitly_repeatable_option_matches_legacy() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a]...
#
# Options:
#   -a --alpha  alpha
#
"#;

    assert_parity(file, &[]);
    assert_parity(file, &["-a"]);
    assert_parity(file, &["-aa"]);
    assert_parity(file, &["--alpha", "--alpha", "--alpha"]);
}
