use serde_json::json;

use crate::{HELP, INVALID, check, error_message, with};

#[test]
fn commands_with_repeatable_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
"#;
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
                &["install", "foo", "bar"],
                Ok(json!({
                    "help": false,
                    "install": true,
                    "package_filters": ["foo", "bar"],
                    "update": false,
                })),
            ),
            (
                &["update", "foo"],
                Ok(json!({
                    "help": false,
                    "install": false,
                    "package_filters": ["foo"],
                    "update": true,
                })),
            ),
            (&["install"], Err(INVALID)),
            (&[], Err(INVALID)),
            (&["other", "foo"], Err(INVALID)),
            (&["help", "foo"], Err(HELP)),
        ],
    );
}

#[test]
fn dashed_positional_uses_underscore_key() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package-filters>...
#
"#;
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
                &["install", "foo", "boo"],
                Ok(json!({
                    "help": false,
                    "install": true,
                    "package_filters": ["foo", "boo"],
                    "update": false,
                })),
            ),
        ],
    );
}

#[test]
fn dashed_commands_use_underscore_keys() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./systemctl (daemon-reload|daemon-reexec|help)
#
"#;
    check(
        file,
        &[
            (
                &["daemon-reload"],
                Ok(json!({"daemon_reexec": false, "daemon_reload": true, "help": false})),
            ),
            (
                &["daemon-reexec"],
                Ok(json!({"daemon_reexec": true, "daemon_reload": false, "help": false})),
            ),
            (&["daemon_reload"], Err(INVALID)),
            (&["help"], Err(HELP)),
        ],
    );
}

#[test]
fn dashed_commands_and_positionals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool (daemon-reload|daemon-reexec) <unit-name>
#
"#;
    check(
        file,
        &[
            (
                &["daemon-reload", "foo.service"],
                Ok(json!({
                    "daemon_reexec": false,
                    "daemon_reload": true,
                    "unit_name": "foo.service",
                })),
            ),
            (
                &["daemon-reexec", "foo.service"],
                Ok(json!({
                    "daemon_reexec": true,
                    "daemon_reload": false,
                    "unit_name": "foo.service",
                })),
            ),
        ],
    );
}

#[test]
fn uppercase_positionals_use_lowercase_keys() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool SOURCE DEST
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
fn optional_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({}))),
            (&["x"], Ok(json!({"d": "x"}))),
            (&["x", "y"], Err(INVALID)),
        ],
    );
}

#[test]
fn repeatable_positional_inside_optional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>...]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({}))),
            (&["x"], Ok(json!({"d": ["x"]}))),
            (&["x", "y"], Ok(json!({"d": ["x", "y"]}))),
        ],
    );
}

#[test]
fn repeatable_optional_positional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]...
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({}))),
            (&["x"], Ok(json!({"d": ["x"]}))),
            (&["x", "y"], Ok(json!({"d": ["x", "y"]}))),
        ],
    );
}

#[test]
fn repeated_group_rejects_incomplete_tail() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo (<a> <b>)...
#
"#;
    check(
        file,
        &[
            (&["1", "2"], Ok(json!({"a": ["1"], "b": ["2"]}))),
            (
                &["a", "b", "c", "d"],
                Ok(json!({"a": ["a", "c"], "b": ["b", "d"]})),
            ),
            (&["a", "b", "c"], Err(INVALID)),
            (&[], Err(INVALID)),
        ],
    );
}

#[test]
fn flat_optional_sequence_is_independently_optional() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [alpha beta]
#
"#;

    // Flat `[a b]` makes each element independently optional.
    check(
        file,
        &[
            (&[], Ok(json!({"alpha": false, "beta": false}))),
            (&["alpha"], Ok(json!({"alpha": true, "beta": false}))),
            (&["beta"], Ok(json!({"alpha": false, "beta": true}))),
            (&["alpha", "beta"], Ok(json!({"alpha": true, "beta": true}))),
            (&["beta", "alpha"], Err(INVALID)),
        ],
    );
}

#[test]
fn grouped_optional_sequence_is_atomic() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [(alpha beta)]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"alpha": false, "beta": false}))),
            (&["alpha", "beta"], Ok(json!({"alpha": true, "beta": true}))),
            (&["alpha"], Err(INVALID)),
            (&["beta"], Err(INVALID)),
        ],
    );
}

#[test]
fn groups_inside_brackets_are_independently_optional() {
    // Unlike legacy (as in docopt 0.6.2): each group inside `[...]` is optional on its own; legacy
    // treated the whole bracket as one all-or-nothing group.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [(a | b) (c | d)]
#
"#;
    let defaults = json!({"a": false, "b": false, "c": false, "d": false});
    check(
        file,
        &[
            (&[], Ok(defaults.clone())),
            (&["a"], Ok(with(&defaults, json!({"a": true})))),
            (&["c"], Ok(with(&defaults, json!({"c": true})))),
            (
                &["b", "d"],
                Ok(with(&defaults, json!({"b": true, "d": true}))),
            ),
            (&["c", "a"], Err(INVALID)),
        ],
    );

    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [(a b) c]
#
"#;
    let defaults = json!({"a": false, "b": false, "c": false});
    check(
        file,
        &[
            (&[], Ok(defaults.clone())),
            (&["c"], Ok(with(&defaults, json!({"c": true})))),
            (
                &["a", "b"],
                Ok(with(&defaults, json!({"a": true, "b": true}))),
            ),
            (
                &["a", "b", "c"],
                Ok(json!({"a": true, "b": true, "c": true})),
            ),
            (&["a"], Err(INVALID)),
            (&["b"], Err(INVALID)),
        ],
    );

    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [(--aa | --bb) (--cc | --dd)]
#
"#;
    let defaults = json!({"options": {"aa": false, "bb": false, "cc": false, "dd": false}});
    check(
        file,
        &[
            (&[], Ok(defaults.clone())),
            (
                &["--aa"],
                Ok(with(&defaults, json!({"options": {"aa": true}}))),
            ),
            (
                &["--cc"],
                Ok(with(&defaults, json!({"options": {"cc": true}}))),
            ),
            (
                &["--bb", "--dd"],
                Ok(with(
                    &defaults,
                    json!({"options": {"bb": true, "dd": true}}),
                )),
            ),
        ],
    );
}

#[test]
fn option_opening_a_group_is_recognized() {
    // Unlike legacy (as in docopt 0.6.2): an option right after `(` or `[` is an option; legacy
    // reported it as unknown and rejected every argv for these declarations.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program (--either-this <and-that> | <or-this>)
#
"#;
    check(
        file,
        &[
            (
                &["--either-this", "x"],
                Ok(json!({"and_that": "x", "options": {"either_this": true}})),
            ),
            (
                &["y"],
                Ok(json!({"options": {"either_this": false}, "or_this": "y"})),
            ),
            (&[], Err(INVALID)),
            (&["--either-this"], Err(INVALID)),
            (&["x", "y"], Err(INVALID)),
        ],
    );

    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program [--either-this <and-that> | <or-this>]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"either_this": false}}))),
            (
                &["--either-this", "x"],
                Ok(json!({"and_that": "x", "options": {"either_this": true}})),
            ),
            (
                &["y"],
                Ok(json!({"options": {"either_this": false}, "or_this": "y"})),
            ),
        ],
    );
}

#[test]
fn nested_option_requires_outer_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [command [--force]]
#
# Options:
#   --force  force
#
"#;

    // A nested optional depends on the outer optional element.
    check(
        file,
        &[
            (
                &[],
                Ok(json!({"command": false, "options": {"force": false}})),
            ),
            (
                &["command"],
                Ok(json!({"command": true, "options": {"force": false}})),
            ),
            (
                &["command", "--force"],
                Ok(json!({"command": true, "options": {"force": true}})),
            ),
            (&["--force"], Err(INVALID)),
        ],
    );
}

#[test]
fn nested_positional_requires_outer_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [command [<value>]]
#
"#;

    // A nested optional depends on the outer optional element.
    check(
        file,
        &[
            (&[], Ok(json!({"command": false}))),
            (&["command"], Ok(json!({"command": true}))),
            (
                &["command", "value"],
                Ok(json!({"command": true, "value": "value"})),
            ),
            (&["value"], Err(INVALID)),
        ],
    );
}

#[test]
fn optional_command_inside_sequence() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo a [b] c
#
"#;
    check(
        file,
        &[
            (&["a", "c"], Ok(json!({"a": true, "b": false, "c": true}))),
            (
                &["a", "b", "c"],
                Ok(json!({"a": true, "b": true, "c": true})),
            ),
            (&["a", "b"], Err(INVALID)),
            (&["b", "c"], Err(INVALID)),
        ],
    );
}

#[test]
fn nested_alternatives() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo ((a | b) (c | d))
#
"#;
    check(
        file,
        &[
            (
                &["a", "c"],
                Ok(json!({"a": true, "b": false, "c": true, "d": false})),
            ),
            (
                &["a", "d"],
                Ok(json!({"a": true, "b": false, "c": false, "d": true})),
            ),
            (
                &["b", "c"],
                Ok(json!({"a": false, "b": true, "c": true, "d": false})),
            ),
            (
                &["b", "d"],
                Ok(json!({"a": false, "b": true, "c": false, "d": true})),
            ),
            (&["a"], Err(INVALID)),
            (&["c", "a"], Err(INVALID)),
        ],
    );
}

#[test]
fn three_way_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo (a | b | c)
#
"#;
    check(
        file,
        &[
            (&["a"], Ok(json!({"a": true, "b": false, "c": false}))),
            (&["b"], Ok(json!({"a": false, "b": true, "c": false}))),
            (&["c"], Ok(json!({"a": false, "b": false, "c": true}))),
            (&["d"], Err(INVALID)),
            (&[], Err(INVALID)),
        ],
    );
}

#[test]
fn alternative_branches_with_positionals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo (a <b> | c <d>)
#
"#;
    check(
        file,
        &[
            (&["a", "x"], Ok(json!({"a": true, "b": "x", "c": false}))),
            (&["c", "y"], Ok(json!({"a": false, "c": true, "d": "y"}))),
            (&["a"], Err(INVALID)),
            (&["x", "y"], Err(INVALID)),
        ],
    );
}

#[test]
fn repeated_commands_are_counters() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [(a | b)] [(a | b)]
#
"#;

    // Unlike legacy (as in docopt 0.6.2): a command that can occur more than once is a counter,
    // even when matched once.
    check(
        file,
        &[
            (&[], Ok(json!({"a": 0, "b": 0}))),
            (&["a"], Ok(json!({"a": 1, "b": 0}))),
            (&["b"], Ok(json!({"a": 0, "b": 1}))),
            (&["a", "a"], Ok(json!({"a": 2, "b": 0}))),
            (&["a", "b"], Ok(json!({"a": 1, "b": 1}))),
            (&["b", "b"], Ok(json!({"a": 0, "b": 2}))),
            (&["a", "b", "a"], Err(INVALID)),
        ],
    );
}

#[test]
fn command_declared_twice_is_a_counter() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool a [a]
#
"#;

    // Unlike legacy (as in docopt 0.6.2): a command that can occur more than once is a counter.
    check(
        file,
        &[
            (&["a"], Ok(json!({"a": 1}))),
            (&["a", "a"], Ok(json!({"a": 2}))),
            (&[], Err(INVALID)),
            (&["a", "a", "a"], Err(INVALID)),
        ],
    );
}

#[test]
fn mixed_case_command_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool Run
#
"#;

    // Command and positional identifiers are lowercase (or uppercase positional) ASCII words.
    check(file, &[(&["Run"], Err(INVALID))]);
    // Unlike legacy: the error names the offending identifier.
    assert_eq!(
        error_message(file, &["Run"]),
        "Invalid usage identifier: Run"
    );
}

#[test]
fn numeric_command_suffix_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool run2
#
"#;
    check(file, &[(&["run2"], Err(INVALID))]);
}

#[test]
fn uppercase_angle_positional_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <FILE>
#
"#;
    check(file, &[(&["value"], Err(INVALID))]);
}

#[test]
fn numeric_angle_positional_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <file2>
#
"#;
    check(file, &[(&["value"], Err(INVALID))]);
}

#[test]
fn punctuation_command_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool foo.bar
#
"#;
    check(file, &[(&["foo.bar"], Err(INVALID))]);
}

#[test]
fn mixed_case_long_option_is_accepted() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--Foo]
#
"#;

    // Option identifiers keep the permissive legacy spelling rules.
    check(
        file,
        &[
            (&["--Foo"], Ok(json!({"options": {"Foo": true}}))),
            (&[], Ok(json!({"options": {"Foo": false}}))),
        ],
    );
}

#[test]
fn numeric_long_option_suffix_is_accepted() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--foo2]
#
"#;
    check(
        file,
        &[(&["--foo2"], Ok(json!({"options": {"foo2": true}})))],
    );
}

#[test]
fn uppercase_short_option_is_accepted() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-X]
#
"#;
    check(file, &[(&["-X"], Ok(json!({"options": {"X": true}})))]);
}

#[test]
fn bracket_contents_are_independently_optional() {
    let defaults = json!({
        "boo": false,
        "fuu": false,
        "zuu": false,
        "options": {"a": false, "b": false, "c": false, "d": false},
    });
    let cases: &[(&[&str], _)] = &[
        (&[], Ok(defaults.clone())),
        (&["fuu"], Ok(with(&defaults, json!({"fuu": true})))),
        (
            &["boo", "fuu"],
            Ok(with(&defaults, json!({"boo": true, "fuu": true}))),
        ),
        (
            &["-c", "-a"],
            Ok(with(&defaults, json!({"options": {"a": true, "c": true}}))),
        ),
        (
            &["-d"],
            Ok(with(&defaults, json!({"options": {"d": true}}))),
        ),
        (
            &["boo", "-b", "zuu", "-d"],
            Ok(with(
                &defaults,
                json!({"boo": true, "zuu": true, "options": {"b": true, "d": true}}),
            )),
        ),
        (&["fuu", "boo"], Err(INVALID)),
        (&["-d", "zuu"], Err(INVALID)),
    ];
    for usage in [
        "foo [boo fuu] [-a -b -c] [zuu -d]",
        "foo [ boo fuu ] [ -a -b -c ] [ zuu -d ]",
    ] {
        let file = format!(
            "\n#!/usr/bin/env rash\n#\n# Usage: {usage}\n#\n# Options:\n#   -a  a\n#   -b  b\n#   -c  c\n#   -d  d\n#\n"
        );
        check(&file, cases);
    }
}

#[test]
fn repeated_command_alternatives_without_parentheses_are_counters() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [a | b] [a | b]
#
"#;
    // Unlike legacy (as in docopt 0.6.2): a command that can occur more than once is a counter.
    check(
        file,
        &[
            (&[], Ok(json!({"a": 0, "b": 0}))),
            (&["a"], Ok(json!({"a": 1, "b": 0}))),
            (&["a", "b"], Ok(json!({"a": 1, "b": 1}))),
            (&["a", "a"], Ok(json!({"a": 2, "b": 0}))),
            (&["b", "a", "a"], Err(INVALID)),
        ],
    );
}

#[test]
fn dash_and_double_dash_are_commands() {
    // As in docopt 0.6.2, `-` (stdin/stdout by convention) and `--` are commands; their keys
    // follow the `-` to `_` rule of every command key. Legacy rejected `[-]` as an invalid usage.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-] <file>
#
"#;
    check(
        file,
        &[
            (&["-", "x"], Ok(json!({"_": true, "file": "x"}))),
            (&["x"], Ok(json!({"_": false, "file": "x"}))),
            // A positional also accepts `-`.
            (&["-"], Ok(json!({"_": false, "file": "-"}))),
        ],
    );
}

#[test]
fn command_named_options_is_reserved_when_options_are_declared() {
    let with_options = r#"
#!/usr/bin/env rash
#
# Usage: tool options [--force]
#
"#;
    assert_eq!(
        error_message(with_options, &["options"]),
        "`options` is a reserved name when the usage declares options: rename the `options` \
         command or positional"
    );

    let positional = r#"
#!/usr/bin/env rash
#
# Usage: tool <options>
#
# Options:
#   -v  verbose
#
"#;
    check(positional, &[(&["x"], Err(INVALID))]);

    let without_options = r#"
#!/usr/bin/env rash
#
# Usage: tool options <options-file>
#
"#;
    check(
        without_options,
        &[(
            &["options", "x"],
            Ok(json!({"options": true, "options_file": "x"})),
        )],
    );
}

#[test]
fn deep_group_nesting_is_rejected() {
    let nested = |depth: usize| {
        format!(
            "\n#\n# Usage: tool {}<x>{}\n#\n",
            "(".repeat(depth),
            ")".repeat(depth)
        )
    };
    check(&nested(64), &[(&["v"], Ok(json!({"x": "v"})))]);
    assert_eq!(
        error_message(&nested(65), &["v"]),
        "Invalid usage grammar at token 65: groups nested deeper than 64 levels"
    );
    // Far beyond the limit, the error is still reported instead of overflowing the stack.
    check(&nested(100_000), &[(&["v"], Err(INVALID))]);
}
