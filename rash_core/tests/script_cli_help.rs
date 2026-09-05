use rash_core::{docopt, error::ErrorKind, script_cli};

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

fn assert_error_parity(file: &str, args: &[&str]) {
    let legacy = docopt::parse(file, args).unwrap_err();
    let compiled = script_cli::parse(file, args).unwrap_err();
    assert_eq!(compiled.kind(), legacy.kind(), "args={args:?}");
}

#[test]
fn help_option_can_satisfy_a_required_positional_slot() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--help] <required>
#
# Options:
#   -h --help  show this help
#
"#;

    for args in [vec!["--help"], vec!["-h"]] {
        let legacy = docopt::parse(file, &args).unwrap_err();
        let compiled = script_cli::parse(file, &args).unwrap_err();
        assert_eq!(legacy.kind(), ErrorKind::GracefulExit);
        assert_eq!(compiled.kind(), legacy.kind());
    }
}

#[test]
fn help_option_can_replace_a_positional_after_a_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool run <target>
#   tool --help
#
# Options:
#   -h --help  show this help
#
"#;

    assert_error_parity(file, &["run", "--help"]);
    assert_error_parity(file, &["run", "-h"]);
}

#[test]
fn impossible_extra_help_is_not_globally_accepted() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool run
#   tool --help
#
# Options:
#   -h --help  show this help
#
"#;

    assert_error_parity(file, &["run", "--help"]);
    assert_error_parity(file, &["run", "-h"]);
}

#[test]
fn short_only_h_can_fill_a_positional_without_graceful_exit() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <required>
#
# Options:
#   -h  ordinary h flag
#
"#;

    assert_parity(file, &["-h"]);
}

#[test]
fn h_with_a_non_help_long_alias_does_not_fill_a_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <required>
#
# Options:
#   -h --host  host flag
#
"#;

    assert_parity(file, &["-h"]);
}
