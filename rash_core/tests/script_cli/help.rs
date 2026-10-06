use serde_json::json;

use crate::{HELP, INVALID, check, with};

#[test]
fn help_command_exits_gracefully() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool (run|help)
#
"#;
    check(
        file,
        &[
            (&["help"], Err(HELP)),
            (&["run"], Ok(json!({"help": false, "run": true}))),
        ],
    );
}

#[test]
fn help_command_with_optional_positionals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) [<package_filters>...]
#
"#;
    check(
        file,
        &[
            (&["help"], Err(HELP)),
            (&["help", "x"], Err(HELP)),
            (
                &["install"],
                Ok(json!({"help": false, "install": true, "update": false})),
            ),
        ],
    );
}

#[test]
fn help_option_exits_gracefully() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-h]
#
# Options:
#   -h --help  help
#
"#;
    check(
        file,
        &[
            (&["-h"], Err(HELP)),
            (&["--help"], Err(HELP)),
            (&[], Ok(json!({"options": {"help": false}}))),
        ],
    );
}

#[test]
fn undocumented_help_option_exits_gracefully() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots [--help]
#
"#;
    check(
        file,
        &[
            (&["--help"], Err(HELP)),
            (&[], Ok(json!({"options": {"help": false}}))),
        ],
    );
}

#[test]
fn undocumented_help_option_satisfies_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots [--help] <foo>
#
"#;
    check(
        file,
        &[
            (&["--help"], Err(HELP)),
            (&["x"], Ok(json!({"foo": "x", "options": {"help": false}}))),
            (&["--help", "x"], Err(HELP)),
        ],
    );
}

#[test]
fn help_option_satisfies_required_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--help] <required>
#
# Options:
#   -h --help  show this help
#
"#;
    check(file, &[(&["--help"], Err(HELP)), (&["-h"], Err(HELP))]);
}

#[test]
fn help_option_replaces_positional_after_command() {
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
    check(
        file,
        &[
            (&["run", "--help"], Err(HELP)),
            (&["run", "-h"], Err(HELP)),
            (&["--help"], Err(HELP)),
            (
                &["run", "x"],
                Ok(json!({"options": {"help": false}, "run": true, "target": "x"})),
            ),
        ],
    );
}

#[test]
fn extra_help_option_is_not_globally_accepted() {
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
    check(
        file,
        &[
            (&["run", "--help"], Err(INVALID)),
            (&["run", "-h"], Err(INVALID)),
            (&["-h"], Err(HELP)),
        ],
    );
}

#[test]
fn short_only_h_fills_positional_without_exit() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <required>
#
# Options:
#   -h  ordinary h flag
#
"#;
    check(file, &[(&["-h"], Ok(json!({"options": {"h": true}})))]);
}

#[test]
fn h_with_non_help_long_alias_does_not_fill_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <required>
#
# Options:
#   -h --host  host flag
#
"#;
    check(file, &[(&["-h"], Err(INVALID))]);
}

#[test]
fn help_alternative_in_multi_pattern_usage() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   naval_fate.rh ship new <name>...
#   naval_fate.rh ship <name> move <x> <y> [--speed=<kn>]
#   naval_fate.rh ship shoot <x> <y>
#   naval_fate.rh mine (set|remove) <x> <y> [--moored|--drifting]
#   naval_fate.rh -h | --help
#   naval_fate.rh --version
#
# Options:
#   -h --help        Show this screen.
#   -v --version     Show version.
#   -s --speed=<kn>  Speed in knots [default: 10].
#   --moored         Moored (anchored) mine.
#   --drifting       Drifting mine.
#
"#;
    let defaults = json!({
        "mine": false,
        "move": false,
        "new": false,
        "options": {
            "drifting": false,
            "help": false,
            "moored": false,
            "speed": "10",
            "version": false,
        },
        "remove": false,
        "set": false,
        "ship": false,
        "shoot": false,
    });
    check(
        file,
        &[
            (&["-h"], Err(HELP)),
            (&["--help"], Err(HELP)),
            (
                &["--version"],
                Ok(with(&defaults, json!({"options": {"version": true}}))),
            ),
        ],
    );
}
