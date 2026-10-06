use serde_json::json;

use rash_core::script_cli;

use crate::{INVALID, check, with};

#[test]
fn naval_fate() {
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
            (&[], Err(INVALID)),
            (
                &["mine", "set", "10", "50", "--drifting"],
                Ok(with(
                    &defaults,
                    json!({
                        "mine": true,
                        "options": {"drifting": true},
                        "set": true,
                        "x": "10",
                        "y": "50",
                    }),
                )),
            ),
            (&["mine", "set", "10", "50", "--speed=50"], Err(INVALID)),
            (
                &["ship", "foo", "move", "2", "3", "-s", "20"],
                Ok(with(
                    &defaults,
                    json!({
                        "move": true,
                        "name": ["foo"],
                        "options": {"speed": "20"},
                        "ship": true,
                        "x": "2",
                        "y": "3",
                    }),
                )),
            ),
            (
                &["ship", "foo", "move", "2", "3", "-s20"],
                Ok(with(
                    &defaults,
                    json!({
                        "move": true,
                        "name": ["foo"],
                        "options": {"speed": "20"},
                        "ship": true,
                        "x": "2",
                        "y": "3",
                    }),
                )),
            ),
            (
                &["ship", "foo", "move", "2", "3", "-s=20"],
                Ok(with(
                    &defaults,
                    json!({
                        "move": true,
                        "name": ["foo"],
                        "options": {"speed": "20"},
                        "ship": true,
                        "x": "2",
                        "y": "3",
                    }),
                )),
            ),
            (
                &["ship", "foo", "move", "2", "3", "-s20", "-x"],
                Err(INVALID),
            ),
            (
                &["ship", "new", "a", "b", "c"],
                Ok(with(
                    &defaults,
                    json!({"name": ["a", "b", "c"], "new": true, "ship": true}),
                )),
            ),
            (
                &["ship", "shoot", "1", "2"],
                Ok(with(
                    &defaults,
                    json!({"ship": true, "shoot": true, "x": "1", "y": "2"}),
                )),
            ),
        ],
    );
}

#[test]
fn overlapping_usages_with_identical_bindings() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

    // Several successful paths with identical bindings are not ambiguous.
    check(
        file,
        &[
            (
                &["foo", "/tmp"],
                Ok(json!({"dest": "/tmp", "source": ["foo"]})),
            ),
            (
                &["foo", "bar", "/tmp"],
                Ok(json!({"dest": "/tmp", "source": ["foo", "bar"]})),
            ),
            (
                &["foo", "bar", "baz", "/tmp"],
                Ok(json!({"dest": "/tmp", "source": ["foo", "bar", "baz"]})),
            ),
            (&["foo"], Err(INVALID)),
        ],
    );
}

#[test]
fn optional_options_around_command_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool [--verbose] (start|stop) [--force]
#
# Options:
#   --verbose  Show detailed output
#   --force    Force the operation
#
"#;

    // Options are matched at their declared position (`--force start` is rejected).
    // Intentional difference 6: a bare `start` is accepted.
    check(
        file,
        &[
            (
                &["--verbose", "start", "--force"],
                Ok(json!({
                    "options": {"force": true, "verbose": true},
                    "start": true,
                    "stop": false,
                })),
            ),
            (
                &["--verbose", "start"],
                Ok(json!({
                    "options": {"force": false, "verbose": true},
                    "start": true,
                    "stop": false,
                })),
            ),
            (
                &["stop", "--force"],
                Ok(json!({
                    "options": {"force": true, "verbose": false},
                    "start": false,
                    "stop": true,
                })),
            ),
            (&["--force", "--verbose", "start"], Err(INVALID)),
            (
                &["start"],
                Ok(json!({
                    "options": {"force": false, "verbose": false},
                    "start": true,
                    "stop": false,
                })),
            ),
            (&["--force", "start"], Err(INVALID)),
            (&["start", "--verbose"], Err(INVALID)),
        ],
    );
}

#[test]
fn optional_option_between_positionals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool <input> [--verbose] <output>
#
# Options:
#   --verbose  Show detailed output
#
"#;
    check(
        file,
        &[
            (
                &["input.txt", "--verbose", "output.txt"],
                Ok(json!({
                    "input": "input.txt",
                    "options": {"verbose": true},
                    "output": "output.txt",
                })),
            ),
            (
                &["input.txt", "output.txt"],
                Ok(json!({
                    "input": "input.txt",
                    "options": {"verbose": false},
                    "output": "output.txt",
                })),
            ),
            (&["--verbose", "input.txt", "output.txt"], Err(INVALID)),
        ],
    );
}

#[test]
fn nested_groups_with_repeatable_positionals() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool [options] (add [<item>...] | (remove|delete) <id>)
#
# Options:
#   -f, --force    Force operation
#
"#;
    check(
        file,
        &[
            (
                &["--force", "add", "item1", "item2", "item3"],
                Ok(json!({
                    "add": true,
                    "delete": false,
                    "item": ["item1", "item2", "item3"],
                    "options": {"force": true},
                    "remove": false,
                })),
            ),
            (
                &["add"],
                Ok(json!({
                    "add": true,
                    "delete": false,
                    "options": {"force": false},
                    "remove": false,
                })),
            ),
            (
                &["remove", "12345"],
                Ok(json!({
                    "add": false,
                    "delete": false,
                    "id": "12345",
                    "options": {"force": false},
                    "remove": true,
                })),
            ),
            (
                &["delete", "12345"],
                Ok(json!({
                    "add": false,
                    "delete": true,
                    "id": "12345",
                    "options": {"force": false},
                    "remove": false,
                })),
            ),
            (&["remove"], Err(INVALID)),
        ],
    );
}

#[test]
fn multiple_patterns_with_commands_and_options() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool deploy [--env=<environment>] [--dry-run] [<service>...]
#   ./tool rollback [--force] <version>
#   ./tool (start|stop|restart) [(--all | <service>...)]
#
# Options:
#   --env=<environment>  Target environment [default: dev]
#   --dry-run            Don't actually deploy
#   --force              Force the operation
#   --all                Apply to all services
#
"#;
    let defaults = json!({
        "deploy": false,
        "options": {"all": false, "dry_run": false, "env": "dev", "force": false},
        "restart": false,
        "rollback": false,
        "start": false,
        "stop": false,
    });
    check(
        file,
        &[
            (
                &["deploy", "--env=prod", "--dry-run", "web", "api", "db"],
                Ok(with(
                    &defaults,
                    json!({
                        "deploy": true,
                        "options": {"dry_run": true, "env": "prod"},
                        "service": ["web", "api", "db"],
                    }),
                )),
            ),
            (&["deploy"], Ok(with(&defaults, json!({"deploy": true})))),
            (
                &["rollback", "--force", "v1.2.3"],
                Ok(with(
                    &defaults,
                    json!({"options": {"force": true}, "rollback": true, "version": "v1.2.3"}),
                )),
            ),
            (
                &["start", "web", "api"],
                Ok(with(
                    &defaults,
                    json!({"service": ["web", "api"], "start": true}),
                )),
            ),
            (
                &["start", "--all"],
                Ok(with(
                    &defaults,
                    json!({"options": {"all": true}, "start": true}),
                )),
            ),
            (&["restart"], Ok(with(&defaults, json!({"restart": true})))),
            (&["rollback"], Err(INVALID)),
        ],
    );
}

#[test]
fn required_group_and_option_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool sync (<source> <dest>) [--delete]
#   ./tool query [--format=<format>] (<key> | --all)
#
# Options:
#   --delete           Delete files missing from source
#   --format=<format>  Output format [default: text]
#   --all              Query all values
#
"#;

    // The legacy parser also exposed a malformed `options["all)"]` key scanned from `--all)`.
    check(
        file,
        &[
            (
                &["sync", "src", "dst"],
                Ok(json!({
                    "dest": "dst",
                    "options": {"all": false, "delete": false, "format": "text"},
                    "query": false,
                    "source": "src",
                    "sync": true,
                })),
            ),
            (
                &["sync", "src", "dst", "--delete"],
                Ok(json!({
                    "dest": "dst",
                    "options": {"all": false, "delete": true, "format": "text"},
                    "query": false,
                    "source": "src",
                    "sync": true,
                })),
            ),
            (
                &["query", "foo"],
                Ok(json!({
                    "key": "foo",
                    "options": {"all": false, "delete": false, "format": "text"},
                    "query": true,
                    "sync": false,
                })),
            ),
            (
                &["query", "--format=json", "foo"],
                Ok(json!({
                    "key": "foo",
                    "options": {"all": false, "delete": false, "format": "json"},
                    "query": true,
                    "sync": false,
                })),
            ),
            (
                &["query", "--all"],
                Ok(json!({
                    "options": {"all": true, "delete": false, "format": "text"},
                    "query": true,
                    "sync": false,
                })),
            ),
        ],
    );
}

#[test]
fn pacman_fixture() {
    let file = include_str!("../mocks/pacman.rh");
    let defaults = json!({
        "options": {
            "arch": null,
            "asdeps": false,
            "asexplicit": false,
            "assume_installed": null,
            "cachedir": null,
            "clean": false,
            "color": null,
            "config": null,
            "confirm": false,
            "database": false,
            "dbonly": false,
            "dbpath": null,
            "debug": false,
            "deps": false,
            "deptest": false,
            "disable_download_timeout": false,
            "downloadonly": false,
            "explicit": false,
            "files": false,
            "gpgdir": null,
            "groups": false,
            "help": false,
            "hookdir": null,
            "ignore": null,
            "ignoregroup": null,
            "info": false,
            "list": null,
            "logfile": null,
            "needed": false,
            "noconfirm": false,
            "nodeps": false,
            "noprogressbar": false,
            "noscriptlet": false,
            "overwrite": null,
            "print": false,
            "print_format": null,
            "query": false,
            "quiet": false,
            "refresh": false,
            "remove": false,
            "root": null,
            "search": null,
            "sync": false,
            "sysroot": false,
            "sysupgrade": false,
            "upgrade": false,
            "upgrades": false,
            "verbose": false,
            "version": false,
        },
    });
    check(
        file,
        &[
            (
                &["--sysupgrade"],
                Ok(with(&defaults, json!({"options": {"sysupgrade": true}}))),
            ),
            (
                &["--upgrades"],
                Ok(with(&defaults, json!({"options": {"upgrades": true}}))),
            ),
            (
                &["--sync", "--noconfirm", "rash"],
                Ok(with(
                    &defaults,
                    json!({"options": {"noconfirm": true, "sync": true}, "targets": ["rash"]}),
                )),
            ),
        ],
    );
}

#[test]
fn different_bindings_are_rejected_as_ambiguous() {
    // Intentional difference 2: no result is picked by iteration order.
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool <source> <dest>
#   tool <input> <output>
#
"#;
    let error = script_cli::parse(file, &["a", "b"]).unwrap_err();
    assert_eq!(error.kind(), INVALID);
    assert!(
        error
            .to_string()
            .starts_with("Ambiguous usage declaration.")
    );
}

#[test]
fn ten_thousand_repeatable_arguments() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <file>...
#
"#;
    let args = (0..10_000)
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    check(file, &[(&args, Ok(json!({"file": args})))]);
}
