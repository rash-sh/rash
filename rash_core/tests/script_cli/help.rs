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
fn help_option_exits_even_where_no_pattern_accepts_it() {
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
            // As in docopt 0.6.2, a help option anywhere in argv shows the help.
            (&["run", "--help"], Err(HELP)),
            (&["run", "-h"], Err(HELP)),
            (&["-h"], Err(HELP)),
        ],
    );
}

#[test]
fn short_only_h_flag_is_a_help_option() {
    // Unlike legacy (as in docopt 0.6.2): a short-only `-h` flag requests help. Legacy let it fill
    // the `<x>` slot and bound `options.h`.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x>
#
# Options:
#   -h  short h only
#   -v  verbose
#
"#;
    check(
        file,
        &[
            (&["-h"], Err(HELP)),
            (&["x", "-h"], Err(HELP)),
            (&["-vh", "x"], Err(HELP)),
            (
                &["-v", "x"],
                Ok(json!({"options": {"h": false, "v": true}, "x": "x"})),
            ),
        ],
    );

    let required = r#"
#!/usr/bin/env rash
#
# Usage: tool <required>
#
# Options:
#   -h  short h only
#
"#;
    check(required, &[(&["-h"], Err(HELP))]);
}

#[test]
fn h_value_option_is_not_a_help_option() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x>
#
# Options:
#   -h <host>  host
#
"#;
    check(
        file,
        &[
            (
                &["-h", "example.com", "x"],
                Ok(json!({"options": {"h": "example.com"}, "x": "x"})),
            ),
            (&["-h", "x"], Err(INVALID)),
        ],
    );
}

#[test]
fn h_with_non_help_long_alias_is_not_a_help_option() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x>
#
# Options:
#   -h --human  human readable
#
"#;
    check(
        file,
        &[
            (
                &["-h", "x"],
                Ok(json!({"options": {"human": true}, "x": "x"})),
            ),
            (&["-h"], Err(INVALID)),
        ],
    );
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

#[test]
fn help_option_exits_with_required_positionals_missing() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x> <y>
#
# Options:
#   -h --help    show this help
#   --mode MODE  mode
#
"#;
    check(
        file,
        &[
            (&["--help"], Err(HELP)),
            (&["x", "-h"], Err(HELP)),
            (&["--help", "x", "y", "z"], Err(HELP)),
            (&[], Err(INVALID)),
            // An option value is not a help option.
            (
                &["--mode", "--help", "x", "y"],
                Ok(json!({
                    "options": {"help": false, "mode": "--help"},
                    "x": "x",
                    "y": "y",
                })),
            ),
        ],
    );
}

#[test]
fn help_option_after_separator_is_a_word() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] [--] <args>...
#
# Options:
#   -h --help  show this help
#
"#;
    check(
        file,
        &[
            (&["--help", "--", "x"], Err(HELP)),
            (
                &["--", "--help"],
                Ok(json!({"__": true, "args": ["--help"], "options": {"help": false}})),
            ),
        ],
    );
}
