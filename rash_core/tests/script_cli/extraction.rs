use serde_json::json;

use rash_core::script_cli;

use crate::{HELP, INVALID, check};

const DOTS: &str = r#"
#!/usr/bin/env -S rash --diff
#
# dots easy manage of your dotfiles.
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
# Arguments:
#   package_filters   List of regex matching packages wanted.
#
# Examples:
#   ./dots install '.*zsh.*'
#
# Subcommands:
#   install   Copy files to host.
#   update    Get files from host.
#   help      Show this screen.
#
doe: "a deer, a female deer"
# comment example
xmas-fifth-day:
  # yep, another comment example
  calling-birds: four
"#;

const NOTE: &str =
    "Note: Options must be preceded by `--`. If not, you are passing options directly to rash.
For more information check rash options with `rash --help`.
";

#[test]
fn no_usage_returns_empty_context() {
    let file = r#"
#!/usr/bin/env rash
# No CLI declaration here.
- debug:
    msg: hi
"#;
    check(
        file,
        &[
            (&[], Ok(json!({}))),
            (&["anything", "--flag"], Ok(json!({}))),
        ],
    );
}

#[test]
fn standard_one_line_usage_is_active() {
    let file = r#"
#!/usr/bin/env rash
# Usage: tool <value>
#
"#;
    check(
        file,
        &[
            (&["x"], Ok(json!({"value": "x"}))),
            (&[], Err(INVALID)),
            (&["x", "y"], Err(INVALID)),
        ],
    );
}

#[test]
fn one_line_usage_tolerates_extra_spacing() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:  cp <source> <dest>
#
"#;
    check(
        file,
        &[
            (&["a", "b"], Ok(json!({"dest": "b", "source": "a"}))),
            (&["a"], Err(INVALID)),
        ],
    );
}

#[test]
fn usage_without_comment_spacing_is_inactive() {
    let file = r#"
#!/usr/bin/env rash
#Usage: tool <value>
#
"#;
    check(file, &[(&["x"], Ok(json!({}))), (&[], Ok(json!({})))]);
}

#[test]
fn usage_without_colon_spacing_is_inactive() {
    let file = r#"
#!/usr/bin/env rash
# Usage:tool <value>
#
"#;
    check(file, &[(&["x"], Ok(json!({}))), (&[], Ok(json!({})))]);
}

#[test]
fn multiline_usage_reads_indented_patterns() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool get <value>
#   tool set <value>
#
"#;
    check(
        file,
        &[
            (
                &["get", "x"],
                Ok(json!({"get": true, "set": false, "value": "x"})),
            ),
            (
                &["set", "x"],
                Ok(json!({"get": false, "set": true, "value": "x"})),
            ),
            (&["x"], Err(INVALID)),
        ],
    );
}

#[test]
fn multiline_usage_stops_at_next_section() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
# Foo:
#   buu
#   fuu
"#;
    check(
        file,
        &[
            (&["a", "b"], Ok(json!({"dest": "b", "source": ["a"]}))),
            (
                &["a", "b", "c"],
                Ok(json!({"dest": "c", "source": ["a", "b"]})),
            ),
            (&["buu"], Err(INVALID)),
        ],
    );
}

#[test]
fn usage_block_ends_with_comment_block() {
    let file = DOTS;
    check(
        file,
        &[
            (
                &["install", "foo"],
                Ok(json!({
                    "help": false,
                    "install": true,
                    "package_filters": ["foo"],
                    "update": false,
                })),
            ),
            (
                &["update", "a", "b"],
                Ok(json!({
                    "help": false,
                    "install": false,
                    "package_filters": ["a", "b"],
                    "update": true,
                })),
            ),
            (&["help", "x"], Err(HELP)),
        ],
    );
}

#[test]
fn help_text_is_the_leading_comment_block() {
    // The shebang is skipped, the first space of each comment line is removed and the block ends
    // at the first non-comment line.
    let error = script_cli::parse(DOTS, &["help", "x"]).unwrap_err();
    assert_eq!(error.kind(), HELP);
    assert_eq!(
        error.to_string(),
        format!(
            r#"
dots easy manage of your dotfiles.

Usage:
  ./dots (install|update|help) <package_filters>...

Arguments:
  package_filters   List of regex matching packages wanted.

Examples:
  ./dots install '.*zsh.*'

Subcommands:
  install   Copy files to host.
  update    Get files from host.
  help      Show this screen.

{NOTE}"#
        )
    );
}

#[test]
fn unmatched_argv_reports_the_help_text() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;
    let error = script_cli::parse(file, &[]).unwrap_err();
    assert_eq!(error.kind(), INVALID);
    assert_eq!(
        error.to_string(),
        format!("\nUsage:\n  cp <source> <dest>\n  cp <source>... <dest>\n\n{NOTE}")
    );
}

#[test]
fn usage_patterns_tolerate_trailing_whitespace_and_tab_indentation() {
    let file = "\n#!/usr/bin/env rash\n# Usage:\n# \ttool <a>  \n#   tool <a> <b>\t\n#\n";
    check(
        file,
        &[
            (&["x"], Ok(json!({"a": "x"}))),
            (&["x", "y"], Ok(json!({"a": "x", "b": "y"}))),
            (&["x", "y", "z"], Err(INVALID)),
        ],
    );
}

#[test]
fn whitespace_only_line_in_usage_block_is_skipped() {
    // Legacy read such a line as an empty pattern, so it also accepted an empty argv. Like
    // docopt 0.6.2, the line is skipped and the patterns after it still belong to the block.
    for blank in ["  ", "\t", " \t "] {
        let file = format!(
            "\n#!/usr/bin/env rash\n# Usage:\n#   tool <a>\n#{blank}\n#   tool <a> <b>\n#\n"
        );
        check(
            &file,
            &[
                (&["x"], Ok(json!({"a": "x"}))),
                (&["x", "y"], Ok(json!({"a": "x", "b": "y"}))),
                (&[], Err(INVALID)),
            ],
        );
    }
}

#[test]
fn whitespace_only_line_before_next_section_is_skipped() {
    let file = "\n#!/usr/bin/env rash\n# Usage:\n#   tool [options] <a>\n#  \n# Options:\n#   -v  verbose\n";
    check(
        file,
        &[
            (&["x"], Ok(json!({"a": "x", "options": {"v": false}}))),
            (&["-v", "x"], Ok(json!({"a": "x", "options": {"v": true}}))),
            (&[], Err(INVALID)),
        ],
    );
}
