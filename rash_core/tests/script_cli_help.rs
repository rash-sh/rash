use rash_core::{docopt, error::ErrorKind, script_cli};

#[test]
fn help_option_short_circuits_missing_required_arguments() {
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
