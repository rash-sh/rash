use rash_core::{docopt, error::ErrorKind, script_cli};

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
