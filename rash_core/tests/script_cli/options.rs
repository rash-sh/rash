use serde_json::json;

use rash_core::script_cli;

use crate::{HELP, INVALID, check, error_message, with};

#[test]
fn short_and_long_aliases_share_one_key() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--dry-run]
#
# Options:
#   -d --dry-run  dry run
#
"#;
    check(
        file,
        &[
            (&["-d"], Ok(json!({"options": {"dry_run": true}}))),
            (&["--dry-run"], Ok(json!({"options": {"dry_run": true}}))),
            (&[], Ok(json!({"options": {"dry_run": false}}))),
        ],
    );
}

#[test]
fn undocumented_short_option() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [-d]
#
"#;
    check(
        file,
        &[
            (&["-d"], Ok(json!({"options": {"d": true}}))),
            (&[], Ok(json!({"options": {"d": false}}))),
            (&["-a"], Err(INVALID)),
            (&["-ad"], Err(INVALID)),
            (&["-a", "-d"], Err(INVALID)),
        ],
    );
}

#[test]
fn unknown_long_option_is_rejected() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--known]
#
"#;
    check(
        file,
        &[
            (&["--unknown"], Err(INVALID)),
            (&["--known"], Ok(json!({"options": {"known": true}}))),
            (&[], Ok(json!({"options": {"known": false}}))),
            (&["--known=value"], Err(INVALID)),
        ],
    );
    // Unlike legacy: the error names the option instead of being empty.
    assert_eq!(
        error_message(file, &["--known=value"]),
        "Option --known does not take a value"
    );
}

#[test]
fn option_value_with_equals_sign() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [--env=<key>]
#
"#;
    check(
        file,
        &[
            (&["--env=FOO"], Ok(json!({"options": {"env": "FOO"}}))),
            (&["--env", "FOO"], Ok(json!({"options": {"env": "FOO"}}))),
            (&[], Ok(json!({"options": {"env": null}}))),
        ],
    );
}

#[test]
fn option_value_containing_equals_sign() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [--env=<key=value>]
#
"#;
    check(
        file,
        &[
            (
                &["--env=FOO=BAR"],
                Ok(json!({"options": {"env": "FOO=BAR"}})),
            ),
            (
                &["--env", "FOO=BAR"],
                Ok(json!({"options": {"env": "FOO=BAR"}})),
            ),
        ],
    );
}

#[test]
fn stacked_short_options_with_value() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-vfo FILE] [INPUT ...]
#
# Options:
#   -v --verbose  verbose
#   -f --force    force
#   -o FILE       output [default: stdout]
#
"#;

    // The last option of a short cluster may take the rest of the word, or the next one, as value.
    check(
        file,
        &[
            (
                &["-v", "input"],
                Ok(json!({
                    "input": ["input"],
                    "options": {"force": false, "o": "stdout", "verbose": true},
                })),
            ),
            (
                &["-o", "result", "input"],
                Ok(json!({
                    "input": ["input"],
                    "options": {"force": false, "o": "result", "verbose": false},
                })),
            ),
            (
                &["-voresult", "input"],
                Ok(json!({
                    "input": ["input"],
                    "options": {"force": false, "o": "result", "verbose": true},
                })),
            ),
            (
                &["-fvo=result", "input"],
                Ok(json!({
                    "input": ["input"],
                    "options": {"force": true, "o": "result", "verbose": true},
                })),
            ),
            (
                &[],
                Ok(json!({"options": {"force": false, "o": "stdout", "verbose": false}})),
            ),
        ],
    );
}

#[test]
fn options_without_section_header() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [-hso FILE] [--quiet | --verbose] [INPUT ...]
#
# -h --help    show this
# -s --sorted  sorted output
# -o FILE      specify output file [default: ./test.txt]
# --quiet      print less text
# --verbose    print more text
# --dry-run    run without modifications
#
"#;
    check(
        file,
        &[
            (
                &["-o", "yea", "--sorted"],
                Ok(json!({
                    "options": {
                        "dry_run": false,
                        "help": false,
                        "o": "yea",
                        "quiet": false,
                        "sorted": true,
                        "verbose": false,
                    },
                })),
            ),
            (
                &[],
                Ok(json!({
                    "options": {
                        "dry_run": false,
                        "help": false,
                        "o": "./test.txt",
                        "quiet": false,
                        "sorted": false,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["--quiet", "a", "b"],
                Ok(json!({
                    "input": ["a", "b"],
                    "options": {
                        "dry_run": false,
                        "help": false,
                        "o": "./test.txt",
                        "quiet": true,
                        "sorted": false,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-so", "x", "in"],
                Ok(json!({
                    "input": ["in"],
                    "options": {
                        "dry_run": false,
                        "help": false,
                        "o": "x",
                        "quiet": false,
                        "sorted": true,
                        "verbose": false,
                    },
                })),
            ),
            (&["--quiet", "--verbose"], Err(INVALID)),
            (&["--dry-run"], Err(INVALID)),
            (&["-h"], Err(HELP)),
        ],
    );
}

#[test]
fn options_header_without_colon() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: multi_word.rh [options]
#
# Options
#   -h --help    show this
#   --dry-run    run without modifications
#   --fast-run   run using max CPU cores
#
"#;
    check(
        file,
        &[
            (
                &["--fast-run"],
                Ok(json!({"options": {"dry_run": false, "fast_run": true, "help": false}})),
            ),
            (
                &["--dry-run", "--fast-run"],
                Ok(json!({"options": {"dry_run": true, "fast_run": true, "help": false}})),
            ),
        ],
    );
}

#[test]
fn value_placeholders_in_descriptions() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [options] <port>
#
# Options:
#   -h --help                show this help message and exit
#   --version                show version and exit
#   -n, --number N           use N as a number
#   -t, --timeout TIMEOUT    set timeout TIMEOUT seconds
#   --apply                  apply changes to database
#   -q                       operate in quiet mode
#
"#;
    check(
        file,
        &[
            (
                &["-qn", "10", "443"],
                Ok(json!({
                    "options": {
                        "apply": false,
                        "help": false,
                        "number": "10",
                        "q": true,
                        "timeout": null,
                        "version": false,
                    },
                    "port": "443",
                })),
            ),
            (
                &["-q", "-n10", "443"],
                Ok(json!({
                    "options": {
                        "apply": false,
                        "help": false,
                        "number": "10",
                        "q": true,
                        "timeout": null,
                        "version": false,
                    },
                    "port": "443",
                })),
            ),
            (
                &["--number=10", "443"],
                Ok(json!({
                    "options": {
                        "apply": false,
                        "help": false,
                        "number": "10",
                        "q": false,
                        "timeout": null,
                        "version": false,
                    },
                    "port": "443",
                })),
            ),
            (
                &["--timeout", "5", "443"],
                Ok(json!({
                    "options": {
                        "apply": false,
                        "help": false,
                        "number": null,
                        "q": false,
                        "timeout": "5",
                        "version": false,
                    },
                    "port": "443",
                })),
            ),
            (
                &["443"],
                Ok(json!({
                    "options": {
                        "apply": false,
                        "help": false,
                        "number": null,
                        "q": false,
                        "timeout": null,
                        "version": false,
                    },
                    "port": "443",
                })),
            ),
            (&["-n"], Err(INVALID)),
        ],
    );
    // Unlike legacy (as in docopt 0.6.2): a value option without its value is an error (legacy
    // bound the option's own spelling, `"-n"`, as its value). The message names the option.
    assert_eq!(error_message(file, &["-n"]), "Option -n requires a value");
}

#[test]
fn options_shortcut_with_default_value() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./systemctl [options] (daemon-reload|daemon-reexec|help)
#
# Options:
#   --failed                   Show only failed units
#   -t, --type=TYPE           List units of a particular type [default: service]
#
"#;
    check(
        file,
        &[
            (
                &["daemon-reload"],
                Ok(json!({
                    "daemon_reexec": false,
                    "daemon_reload": true,
                    "help": false,
                    "options": {"failed": false, "type": "service"},
                })),
            ),
            (
                &["--type=timer", "daemon-reexec"],
                Ok(json!({
                    "daemon_reexec": true,
                    "daemon_reload": false,
                    "help": false,
                    "options": {"failed": false, "type": "timer"},
                })),
            ),
            (
                &["-t", "timer", "daemon-reload"],
                Ok(json!({
                    "daemon_reexec": false,
                    "daemon_reload": true,
                    "help": false,
                    "options": {"failed": false, "type": "timer"},
                })),
            ),
            (
                &["--failed", "--type", "socket", "daemon-reload"],
                Ok(json!({
                    "daemon_reexec": false,
                    "daemon_reload": true,
                    "help": false,
                    "options": {"failed": true, "type": "socket"},
                })),
            ),
        ],
    );
}

#[test]
fn value_arity_inferred_from_description() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./systemctl [--failed | --type ] (daemon-reload|daemon-reexec|help)
#
# Options:
#   --failed                   Show only failed units
#   -t, --type=TYPE           List units of a particular type [default: service]
#
"#;

    // The description declares the value, so `[--type ]` in the usage takes one.
    check(
        file,
        &[
            (
                &["daemon-reload"],
                Ok(json!({
                    "daemon_reexec": false,
                    "daemon_reload": true,
                    "help": false,
                    "options": {"failed": false, "type": "service"},
                })),
            ),
            (
                &["--type=timer", "daemon-reexec"],
                Ok(json!({
                    "daemon_reexec": true,
                    "daemon_reload": false,
                    "help": false,
                    "options": {"failed": false, "type": "timer"},
                })),
            ),
            (
                &["--type", "timer", "daemon-reexec"],
                Ok(json!({
                    "daemon_reexec": true,
                    "daemon_reload": false,
                    "help": false,
                    "options": {"failed": false, "type": "timer"},
                })),
            ),
            (&["--failed", "--type=timer", "daemon-reload"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_flag_combinations() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <target>
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#   -c --gamma  gamma
#
"#;
    check(
        file,
        &[
            (
                &["target"],
                Ok(json!({
                    "options": {"alpha": false, "beta": false, "gamma": false},
                    "target": "target",
                })),
            ),
            (
                &["--alpha", "target"],
                Ok(json!({
                    "options": {"alpha": true, "beta": false, "gamma": false},
                    "target": "target",
                })),
            ),
            (
                &["--beta", "target"],
                Ok(json!({
                    "options": {"alpha": false, "beta": true, "gamma": false},
                    "target": "target",
                })),
            ),
            (
                &["--alpha", "--beta", "target"],
                Ok(json!({
                    "options": {"alpha": true, "beta": true, "gamma": false},
                    "target": "target",
                })),
            ),
            (
                &["--gamma", "target"],
                Ok(json!({
                    "options": {"alpha": false, "beta": false, "gamma": true},
                    "target": "target",
                })),
            ),
            (
                &["--alpha", "--gamma", "target"],
                Ok(json!({
                    "options": {"alpha": true, "beta": false, "gamma": true},
                    "target": "target",
                })),
            ),
            (
                &["--beta", "--gamma", "target"],
                Ok(json!({
                    "options": {"alpha": false, "beta": true, "gamma": true},
                    "target": "target",
                })),
            ),
            (
                &["--alpha", "--beta", "--gamma", "target"],
                Ok(json!({
                    "options": {"alpha": true, "beta": true, "gamma": true},
                    "target": "target",
                })),
            ),
            (&["target", "--alpha"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_excludes_options_of_every_usage_pattern() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool get [options]
#   tool set [--force]
#
# Options:
#   --force    force
#   --verbose  verbose
#
"#;

    // As in legacy and docopt 0.6.2, `[options]` stands for the described options that no usage
    // pattern references, so `--force` is only accepted where it is written.
    check(
        file,
        &[
            (&["get", "--force"], Err(INVALID)),
            (
                &["get", "--verbose"],
                Ok(json!({
                    "get": true,
                    "options": {"force": false, "verbose": true},
                    "set": false,
                })),
            ),
            (
                &["set", "--force"],
                Ok(json!({
                    "get": false,
                    "options": {"force": true, "verbose": false},
                    "set": true,
                })),
            ),
            (&["set", "--verbose"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_clusters_and_defaults() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool [options] <file>
#
# Options:
#   -v, --verbose            Show detailed output
#   -f, --format=<format>    Output format [default: json]
#   -r, --repeat=<n>         Repeat operation n times [default: 1]
#   -q, --quiet              Suppress output
#
"#;
    check(
        file,
        &[
            (
                &["-vfyaml", "-r5", "-q", "data.txt"],
                Ok(json!({
                    "file": "data.txt",
                    "options": {
                        "format": "yaml",
                        "quiet": true,
                        "repeat": "5",
                        "verbose": true,
                    },
                })),
            ),
            (
                &["--format=xml", "--repeat=3", "data.txt"],
                Ok(json!({
                    "file": "data.txt",
                    "options": {
                        "format": "xml",
                        "quiet": false,
                        "repeat": "3",
                        "verbose": false,
                    },
                })),
            ),
            (
                &["--format", "yaml", "data.txt"],
                Ok(json!({
                    "file": "data.txt",
                    "options": {
                        "format": "yaml",
                        "quiet": false,
                        "repeat": "1",
                        "verbose": false,
                    },
                })),
            ),
        ],
    );
}

#[test]
fn adjacent_optional_options_are_order_independent() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-b] [-c]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#   -c --charlie  charlie
#
"#;
    check(
        file,
        &[
            (
                &[],
                Ok(json!({"options": {"alpha": false, "beta": false, "charlie": false}})),
            ),
            (
                &["-a"],
                Ok(json!({"options": {"alpha": true, "beta": false, "charlie": false}})),
            ),
            (
                &["-b", "-a"],
                Ok(json!({"options": {"alpha": true, "beta": true, "charlie": false}})),
            ),
            (
                &["--charlie", "--alpha", "--beta"],
                Ok(json!({"options": {"alpha": true, "beta": true, "charlie": true}})),
            ),
            (
                &["-cba"],
                Ok(json!({"options": {"alpha": true, "beta": true, "charlie": true}})),
            ),
        ],
    );
}

#[test]
fn explicit_option_is_bounded_by_its_declarations() {
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

    // Unlike legacy (as in docopt 0.6.2): an option matched more often than declared is rejected.
    check(
        file,
        &[
            (&["-a", "-a"], Err(INVALID)),
            (&["-b", "-b"], Err(INVALID)),
            (&["-a", "-b", "-a"], Err(INVALID)),
            (&["--alpha", "-a"], Err(INVALID)),
            (
                &["-a", "-b"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
        ],
    );
}

#[test]
fn explicit_option_after_options_shortcut_is_bounded() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] [-a]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#
"#;

    // Unlike legacy (as in docopt 0.6.2): an option matched more often than declared is rejected.
    check(
        file,
        &[
            (
                &["-a"],
                Ok(json!({"options": {"alpha": true, "beta": false}})),
            ),
            (&["-a", "-a"], Err(INVALID)),
            (
                &["-b", "-a"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
        ],
    );
}

#[test]
fn options_shortcut_with_one_option_is_not_repeatable() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -a --alpha  alpha
#
"#;

    // A bare `[options]` with a single option matches it at most once.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"alpha": false}}))),
            (&["--alpha"], Ok(json!({"options": {"alpha": true}}))),
            (&["--alpha", "--alpha"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_with_several_options_accepts_repeats() {
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

    // A bare `[options]` with two or more options accepts repeated flags.
    check(
        file,
        &[
            (
                &["--alpha", "--beta"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
            (
                &["--beta", "--alpha"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
            (
                &["-a", "-a"],
                Ok(json!({"options": {"alpha": true, "beta": false}})),
            ),
            (
                &["--alpha", "--alpha"],
                Ok(json!({"options": {"alpha": true, "beta": false}})),
            ),
            (
                &["-a", "-b", "-a"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
            (
                &["--alpha", "--beta", "--alpha"],
                Ok(json!({"options": {"alpha": true, "beta": true}})),
            ),
        ],
    );
}

#[test]
fn repeatable_flag_is_a_counter() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [-d]...
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"d": 0}}))),
            (&["-d"], Ok(json!({"options": {"d": 1}}))),
            (&["-dd"], Ok(json!({"options": {"d": 2}}))),
            (&["-d", "-d"], Ok(json!({"options": {"d": 2}}))),
        ],
    );
}

#[test]
fn repeatable_documented_flag_is_a_counter() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a]...
#
# Options:
#   -a --alpha  alpha
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"alpha": 0}}))),
            (&["-a"], Ok(json!({"options": {"alpha": 1}}))),
            (&["-aa"], Ok(json!({"options": {"alpha": 2}}))),
            (
                &["--alpha", "--alpha", "--alpha"],
                Ok(json!({"options": {"alpha": 3}})),
            ),
        ],
    );
}

#[test]
fn flag_with_ellipsis_inside_brackets_is_a_counter() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a...]
#
# Options:
#   -a --alpha  alpha
#
"#;

    // Unlike legacy (as in docopt 0.6.2): a repeatable flag is a counter; legacy rejected every
    // argv.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"alpha": 0}}))),
            (&["-a"], Ok(json!({"options": {"alpha": 1}}))),
            (&["-a", "-a"], Ok(json!({"options": {"alpha": 2}}))),
            (&["-aaa"], Ok(json!({"options": {"alpha": 3}}))),
        ],
    );
}

#[test]
fn flag_declared_twice_is_a_counter() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-a]
#
# Options:
#   -a --alpha  alpha
#
"#;

    // Unlike legacy (as in docopt 0.6.2): an option declared twice is a counter bounded by its
    // declarations.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"alpha": 0}}))),
            (&["-a"], Ok(json!({"options": {"alpha": 1}}))),
            (&["-a", "-a"], Ok(json!({"options": {"alpha": 2}}))),
            (&["-a", "-a", "-a"], Err(INVALID)),
        ],
    );
}

#[test]
fn repeatable_value_option_keeps_last_value() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--tag=<value>]...
#
"#;

    // Repeated value options keep the last value as a scalar.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"tag": null}}))),
            (&["--tag=one"], Ok(json!({"options": {"tag": "one"}}))),
            (
                &["--tag=one", "--tag=two"],
                Ok(json!({"options": {"tag": "two"}})),
            ),
        ],
    );
}

#[test]
fn documented_repeatable_value_option_keeps_last_value() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--tag]...
#
# Options:
#   --tag=VALUE  tag value
#
"#;

    // Legacy failed with "Not mergeable options" and docopt 0.6.2 rejects the declaration; the
    // compiled parser applies the same scalar last-value semantics as `[--tag=<value>]...`.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"tag": null}}))),
            (&["--tag", "one"], Ok(json!({"options": {"tag": "one"}}))),
            (
                &["--tag=one", "--tag=two"],
                Ok(json!({"options": {"tag": "two"}})),
            ),
        ],
    );
}

#[test]
fn shared_short_alias_keeps_long_options_usable() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -u --sysupgrade  upgrade
#   -u --upgrades    list upgrades
#
"#;

    // Unlike legacy (as in docopt 0.6.2): a short alias shared by two options is ambiguous and
    // rejected (legacy silently picked one); the long aliases still work.
    check(
        file,
        &[
            (
                &["--sysupgrade"],
                Ok(json!({"options": {"sysupgrade": true, "upgrades": false}})),
            ),
            (
                &["--upgrades"],
                Ok(json!({"options": {"sysupgrade": false, "upgrades": true}})),
            ),
            (&["-u"], Err(INVALID)),
        ],
    );
}

#[test]
fn long_option_list() {
    let file = r#"
# Pacman binary mock for Pacman module tests.
#
# Usage:
#   ./pacman.rh [options] [<packages>]...
#
# Options:
#  -b, --dbpath <path>  set an alternate database location
#  -c, --clean          remove old packages from cache directory (-cc for all)
#  -d, --nodeps         skip dependency version checks (-dd to skip all checks)
#  -g, --groups         view all members of a package group
#                       (-gg to view all groups and members)
#  -i, --info           view package information (-ii for extended information)
#  -l, --list <repo>    view a list of packages in a repo
#  -p, --print          print the targets instead of performing the operation
#  -q, --quiet          show less information for query and search
#  -r, --root <path>    set an alternate installation root
#  -s, --search <regex> search remote repositories for matching strings
#  -u, --sysupgrade     upgrade installed packages (-uu enables downgrades)
#  -v, --verbose        be verbose
#  -w, --downloadonly   download packages but do not install/upgrade anything
#  -y, --refresh        download fresh package databases from the server
#                       (-yy to force a refresh even if up to date)
#      --arch <arch>    set an alternate architecture
#      --asdeps         install packages as non-explicitly installed
#      --asexplicit     install packages as explicitly installed
#      --assume-installed <package=version>
#                       add a virtual package to satisfy dependencies
#      --cachedir <dir> set an alternate package cache location
#      --color <when>   colourise the output
#      --config <path>  set an alternate configuration file
#      --confirm        always ask for confirmation
#      --dbonly         only modify database entries, not package files
#      --debug          display debug messages
#      --disable-download-timeout
#                       use relaxed timeouts for download
#      --gpgdir <path>  set an alternate home directory for GnuPG
#      --hookdir <dir>  set an alternate hook location
#      --ignore <pkg>   ignore a package upgrade (can be used more than once)
#      --ignoregroup <grp>
#                       ignore a group upgrade (can be used more than once)
#      --logfile <path> set an alternate log file
#      --needed         do not reinstall up to date packages
#      --noconfirm      do not ask for any confirmation
#      --noprogressbar  do not show a progress bar when downloading files
#      --noscriptlet    do not execute the install scriptlet if one exists
#      --overwrite <glob>
#                       overwrite conflicting files (can be used more than once)
#      --print-format <string>
#                       specify how the targets should be printed
#      --sysroot        operate on a mounted guest system (root-only)
#      --help
"#;
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
            "dbonly": false,
            "dbpath": null,
            "debug": false,
            "disable_download_timeout": false,
            "downloadonly": false,
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
            "quiet": false,
            "refresh": false,
            "root": null,
            "search": null,
            "sysroot": false,
            "sysupgrade": false,
            "verbose": false,
        },
    });
    check(
        file,
        &[
            (
                &[
                    "-b",
                    "yea",
                    "-cdgi",
                    "-l",
                    "boo",
                    "-p",
                    "-q",
                    "-r",
                    "yea",
                    "-s",
                    "boo",
                    "-yvwy",
                    "--arch",
                    "yea",
                    "--asdeps",
                    "--asexplicit",
                    "--assume-installed",
                    "yea",
                    "--cachedir=boo",
                    "--color",
                    "yea",
                    "--config",
                    "ye",
                    "--confirm",
                    "--dbonly",
                    "--debug",
                    "--disable-download-timeout",
                    "--gpgdir",
                    "gooo",
                    "--hookdir",
                    "assa",
                    "--ignore",
                    "yea",
                    "--ignoregroup=yea",
                    "--logfile=boo",
                    "--needed",
                    "--noconfirm",
                    "--noprogressbar",
                    "--noscriptlet",
                    "--overwrite",
                    "yea",
                    "--print-format",
                    "yea",
                    "--sysroot",
                ],
                Ok(with(
                    &defaults,
                    json!({
                        "options": {
                            "arch": "yea",
                            "asdeps": true,
                            "asexplicit": true,
                            "assume_installed": "yea",
                            "cachedir": "boo",
                            "clean": true,
                            "color": "yea",
                            "config": "ye",
                            "confirm": true,
                            "dbonly": true,
                            "dbpath": "yea",
                            "debug": true,
                            "disable_download_timeout": true,
                            "downloadonly": true,
                            "gpgdir": "gooo",
                            "groups": true,
                            "hookdir": "assa",
                            "ignore": "yea",
                            "ignoregroup": "yea",
                            "info": true,
                            "list": "boo",
                            "logfile": "boo",
                            "needed": true,
                            "noconfirm": true,
                            "nodeps": true,
                            "noprogressbar": true,
                            "noscriptlet": true,
                            "overwrite": "yea",
                            "print": true,
                            "print_format": "yea",
                            "quiet": true,
                            "refresh": true,
                            "root": "yea",
                            "search": "boo",
                            "sysroot": true,
                            "verbose": true,
                        },
                    }),
                )),
            ),
            (
                &["pkg1", "pkg2"],
                Ok(with(&defaults, json!({"packages": ["pkg1", "pkg2"]}))),
            ),
            (&["--help"], Err(HELP)),
        ],
    );
}

#[test]
fn short_option_value_normalization() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -o FILE       output [default: ./test.txt]
#   -s --sorted   sorted
#   -q --quiet    quiet
#
"#;
    check(
        file,
        &[
            (
                &[],
                Ok(json!({"options": {"o": "./test.txt", "quiet": false, "sorted": false}})),
            ),
            (
                &["-qo", "yea", "--sorted"],
                Ok(json!({"options": {"o": "yea", "quiet": true, "sorted": true}})),
            ),
            (
                &["-qo", "yea", "-s"],
                Ok(json!({"options": {"o": "yea", "quiet": true, "sorted": true}})),
            ),
            (
                &["-qoyea", "-s"],
                Ok(json!({"options": {"o": "yea", "quiet": true, "sorted": true}})),
            ),
            (
                &["-sq"],
                Ok(json!({"options": {"o": "./test.txt", "quiet": true, "sorted": true}})),
            ),
            (
                &["-o=yea"],
                Ok(json!({"options": {"o": "yea", "quiet": false, "sorted": false}})),
            ),
            (
                &["-oyeo"],
                Ok(json!({"options": {"o": "yeo", "quiet": false, "sorted": false}})),
            ),
            (
                &["-o=FOO=yea"],
                Ok(json!({"options": {"o": "FOO=yea", "quiet": false, "sorted": false}})),
            ),
        ],
    );
}

#[test]
fn long_option_value_normalization() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -t --test=<test>  test selection [default: all]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"test": "all"}}))),
            (
                &["-tall-except-one"],
                Ok(json!({"options": {"test": "all-except-one"}})),
            ),
            (&["--test=none"], Ok(json!({"options": {"test": "none"}}))),
            (
                &["--test", "none"],
                Ok(json!({"options": {"test": "none"}})),
            ),
            (
                &["--test", "ENV=FOO"],
                Ok(json!({"options": {"test": "ENV=FOO"}})),
            ),
        ],
    );
}

#[test]
fn options_registry_from_descriptions() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [-hsoFILE] [--quiet | --verbose] [INPUT ...]
#
# -h --help        show this
# -s --sorted      sorted output
# -o FILE          specify output file [default: ./test.txt]
# -r --repeatable  can be repeated. E.g.: -rr
# --quiet          print less text
# --verbose        print more text
#
"#;
    check(
        file,
        &[
            (
                &[],
                Ok(json!({
                    "options": {
                        "help": false,
                        "o": "./test.txt",
                        "quiet": false,
                        "repeatable": false,
                        "sorted": false,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-oout", "in"],
                Ok(json!({
                    "input": ["in"],
                    "options": {
                        "help": false,
                        "o": "out",
                        "quiet": false,
                        "repeatable": false,
                        "sorted": false,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-s", "--quiet"],
                Ok(json!({
                    "options": {
                        "help": false,
                        "o": "./test.txt",
                        "quiet": true,
                        "repeatable": false,
                        "sorted": true,
                        "verbose": false,
                    },
                })),
            ),
            (&["-r"], Err(INVALID)),
            (&["-h"], Err(HELP)),
            // As in docopt 0.6.2; legacy bound `"-o"` (or `"-o=--sorted"` as INPUT).
            (&["-o"], Err(INVALID)),
            (&["-s", "-o"], Err(INVALID)),
        ],
    );
    assert_eq!(error_message(file, &["-o"]), "Option -o requires a value");
}

#[test]
fn options_registry_with_repeatable_option() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [-hsoFILE] [--repeatable]... [--quiet | --verbose] [INPUT ...]
#
# -h --help        show this
# -s --sorted      sorted output
# -o FILE          specify output file [default: ./test.txt]
# -r --repeatable  can be repeated. E.g.: -rr
# --quiet          print less text
# --verbose        print more text
#
"#;
    let defaults = json!({
        "options": {
            "help": false,
            "o": "./test.txt",
            "quiet": false,
            "repeatable": 0,
            "sorted": false,
            "verbose": false,
        },
    });

    // Legacy rejected every argv for this declaration.
    check(
        file,
        &[
            (&[], Ok(defaults.clone())),
            (
                &["-rr"],
                Ok(with(&defaults, json!({"options": {"repeatable": 2}}))),
            ),
            (
                &["-s", "--quiet", "in"],
                Ok(with(
                    &defaults,
                    json!({"input": ["in"], "options": {"quiet": true, "sorted": true}}),
                )),
            ),
            (
                &["-oout", "-r", "a", "b"],
                Ok(with(
                    &defaults,
                    json!({"input": ["a", "b"], "options": {"o": "out", "repeatable": 1}}),
                )),
            ),
            (
                &["--repeatable", "--repeatable", "--verbose"],
                Ok(with(
                    &defaults,
                    json!({"options": {"repeatable": 2, "verbose": true}}),
                )),
            ),
            (&["-s", "-s"], Err(INVALID)),
            (&["-o"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_registry_from_usage_only() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [-hsoFILE] [--quiet | --verbose] [INPUT ...]
#
"#;
    check(
        file,
        &[
            (
                &[],
                Ok(json!({
                    "options": {
                        "E": false,
                        "F": false,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": false,
                        "s": false,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-F"],
                Ok(json!({
                    "options": {
                        "E": false,
                        "F": true,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": false,
                        "s": false,
                        "verbose": false,
                    },
                })),
            ),
            // Unlike legacy (as in docopt 0.6.2): a short option never fills the INPUT slot (legacy
            // returned `"input": ["-s"]`).
            (
                &["-F", "-s"],
                Ok(json!({
                    "options": {
                        "E": false,
                        "F": true,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": false,
                        "s": true,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-s", "-F"],
                Ok(json!({
                    "options": {
                        "E": false,
                        "F": true,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": false,
                        "s": true,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["-s"],
                Ok(json!({
                    "options": {
                        "E": false,
                        "F": false,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": false,
                        "s": true,
                        "verbose": false,
                    },
                })),
            ),
            (
                &["--quiet", "in"],
                Ok(json!({
                    "input": ["in"],
                    "options": {
                        "E": false,
                        "F": false,
                        "I": false,
                        "L": false,
                        "h": false,
                        "o": false,
                        "quiet": true,
                        "s": false,
                        "verbose": false,
                    },
                })),
            ),
        ],
    );
}

#[test]
fn repeatable_option_from_usage_only() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [--repeatable]...
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"repeatable": 0}}))),
            (
                &["--repeatable", "--repeatable"],
                Ok(json!({"options": {"repeatable": 2}})),
            ),
        ],
    );
}

#[test]
fn repeatable_option_with_description() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [--repeatable]...
#
# -r --repeatable  can be repeated. E.g.: -rr
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"repeatable": 0}}))),
            (&["-rr"], Ok(json!({"options": {"repeatable": 2}}))),
            (
                &["--repeatable", "-r"],
                Ok(json!({"options": {"repeatable": 2}})),
            ),
        ],
    );
}

#[test]
fn repeated_exclusive_flags_are_counters() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--quiet | --verbose]...
#
"#;

    // Unlike legacy (as in docopt 0.6.2): both alternatives are counters; legacy reported `quiet`
    // as a boolean and rejected any repetition.
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"quiet": 0, "verbose": 0}}))),
            (
                &["--quiet"],
                Ok(json!({"options": {"quiet": 1, "verbose": 0}})),
            ),
            (
                &["--quiet", "--quiet"],
                Ok(json!({"options": {"quiet": 2, "verbose": 0}})),
            ),
            (
                &["--verbose", "--verbose", "--quiet"],
                Ok(json!({"options": {"quiet": 1, "verbose": 2}})),
            ),
        ],
    );
}

#[test]
fn independent_repeatable_flags_before_positional_are_counters() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [--verbose]... [--quiet]... <file>
#
# Options:
#   -v --verbose  print more text
#   -q --quiet    print less text
#
"#;

    // As in docopt 0.6.2, both flags are counters; legacy rejected every argv for this declaration.
    check(
        file,
        &[
            (
                &["file"],
                Ok(json!({"file": "file", "options": {"quiet": 0, "verbose": 0}})),
            ),
            (
                &["-vvvv", "-qq", "file"],
                Ok(json!({"file": "file", "options": {"quiet": 2, "verbose": 4}})),
            ),
            (
                &["--verbose", "-v", "--quiet", "file"],
                Ok(json!({"file": "file", "options": {"quiet": 1, "verbose": 2}})),
            ),
            (&[], Err(INVALID)),
            (&["-v"], Err(INVALID)),
        ],
    );
}

#[test]
fn value_option_from_usage_only() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [--param=<value>]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"param": null}}))),
            (&["--param=x"], Ok(json!({"options": {"param": "x"}}))),
            (&["--param", "x"], Ok(json!({"options": {"param": "x"}}))),
        ],
    );
}

#[test]
fn uppercase_value_option_from_usage_only() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.rh [--param=VALUE]
#
"#;
    check(
        file,
        &[
            (&[], Ok(json!({"options": {"param": null}}))),
            (&["--param", "x"], Ok(json!({"options": {"param": "x"}}))),
        ],
    );
}

#[test]
fn short_option_clusters_in_usage() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo a [-h]
#   foo b [-qsh]
#
# Options:
#   -h --help    help
#   -s --sorted  sorted
#   -q --quiet   quiet
#
"#;
    check(
        file,
        &[
            (
                &["a"],
                Ok(json!({
                    "a": true,
                    "b": false,
                    "options": {"help": false, "quiet": false, "sorted": false},
                })),
            ),
            (
                &["b", "-qs"],
                Ok(json!({
                    "a": false,
                    "b": true,
                    "options": {"help": false, "quiet": true, "sorted": true},
                })),
            ),
            (
                &["b", "-q", "-s"],
                Ok(json!({
                    "a": false,
                    "b": true,
                    "options": {"help": false, "quiet": true, "sorted": true},
                })),
            ),
            (&["a", "-s"], Err(INVALID)),
            (&["b", "--help"], Err(HELP)),
            (&["a", "-h"], Err(HELP)),
        ],
    );
}

#[test]
fn value_option_and_exclusive_flags() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [-o FILE] [--sorted | --quiet]
#
# Options:
#   -o FILE       output [default: ./test.txt]
#   -s --sorted   sorted
#   -q --quiet    quiet
#
"#;
    // Unlike legacy: empty argv is accepted; legacy rejected it.
    check(
        file,
        &[
            (
                &[],
                Ok(json!({"options": {"o": "./test.txt", "quiet": false, "sorted": false}})),
            ),
            (
                &["-o", "x"],
                Ok(json!({"options": {"o": "x", "quiet": false, "sorted": false}})),
            ),
            (
                &["-o", "x", "--sorted"],
                Ok(json!({"options": {"o": "x", "quiet": false, "sorted": true}})),
            ),
            (
                &["--quiet"],
                Ok(json!({"options": {"o": "./test.txt", "quiet": true, "sorted": false}})),
            ),
            (&["--sorted", "--quiet"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_before_optional_command() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [options] [a]
#
# Options:
#   -h --help    help
#   -s --sorted  sorted
#   -q --quiet   quiet
#
"#;
    check(
        file,
        &[
            (
                &[],
                Ok(json!({
                    "a": false,
                    "options": {"help": false, "quiet": false, "sorted": false},
                })),
            ),
            (
                &["--sorted", "a"],
                Ok(json!({
                    "a": true,
                    "options": {"help": false, "quiet": false, "sorted": true},
                })),
            ),
            (
                &["-qs"],
                Ok(json!({
                    "a": false,
                    "options": {"help": false, "quiet": true, "sorted": true},
                })),
            ),
            (&["a", "-q"], Err(INVALID)),
        ],
    );
}

#[test]
fn options_shortcut_before_command_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [options] (a|b)
#
# Options:
#   -h --help    help
#   -s --sorted  sorted
#   -q --quiet   quiet
#
"#;
    check(
        file,
        &[
            (
                &["-qs", "b"],
                Ok(json!({
                    "a": false,
                    "b": true,
                    "options": {"help": false, "quiet": true, "sorted": true},
                })),
            ),
            (
                &["a"],
                Ok(json!({
                    "a": true,
                    "b": false,
                    "options": {"help": false, "quiet": false, "sorted": false},
                })),
            ),
            (&[], Err(INVALID)),
        ],
    );
}

#[test]
fn short_and_long_alternative_without_descriptions() {
    // Undescribed `-h` and `--help` are two options. Unlike legacy (as in docopt 0.6.2): both are
    // help options; legacy bound `options.h` for `-h`.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo -h | --help
#
"#;
    check(
        file,
        &[
            (&["-h"], Err(HELP)),
            (&["--help"], Err(HELP)),
            (&[], Err(INVALID)),
        ],
    );
}

#[test]
fn short_and_long_alternative_without_spaces() {
    // Unlike legacy (as in docopt 0.6.2): `-h` is a help option; legacy bound `options.h`.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo -h|--help
#
"#;
    check(file, &[(&["-h"], Err(HELP)), (&["--help"], Err(HELP))]);
}

#[test]
fn ambiguous_short_alias_error_names_the_alias() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options]
#
# Options:
#   -u --sysupgrade  upgrade
#   -u --upgrades    list upgrades
#
"#;
    let error = script_cli::parse(file, &["-u"]).unwrap_err();
    assert_eq!(error.to_string(), "Ambiguous option alias: -u");
}

#[test]
fn option_cannot_exceed_its_declared_occurrences_across_positions() {
    // Legacy and docopt 0.6.2 reject a third `-a`: the pattern declares it twice.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-b] <x> [-a]
#
"#;
    check(
        file,
        &[
            (
                &["-a", "-a", "x"],
                Ok(json!({"options": {"a": 2, "b": false}, "x": "x"})),
            ),
            (
                &["-a", "x", "-a"],
                Ok(json!({"options": {"a": 2, "b": false}, "x": "x"})),
            ),
            (&["-a", "-a", "x", "-a"], Err(INVALID)),
            (&["-a", "-b", "-a", "x", "-a"], Err(INVALID)),
        ],
    );
}

#[test]
fn double_dash_separator() {
    // Docopt 0.6.2 semantics: `--` is a command, and every argument after it is a word even if
    // it looks like an option. Legacy exposed it as a malformed `options[""]` flag and kept
    // parsing options after it.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-v] [--] <args>...
#
"#;
    check(
        file,
        &[
            (
                &["-v", "--", "-v", "a"],
                Ok(json!({"__": true, "args": ["-v", "a"], "options": {"v": true}})),
            ),
            (
                &["--", "--unknown", "--"],
                Ok(json!({"__": true, "args": ["--unknown", "--"], "options": {"v": false}})),
            ),
            (
                &["a"],
                Ok(json!({"__": false, "args": ["a"], "options": {"v": false}})),
            ),
            // Docopt 0.6.2 does not backtrack out of `[--]` and rejects it; the compiled parser
            // finds the only binding.
            (
                &["--"],
                Ok(json!({"__": false, "args": ["--"], "options": {"v": false}})),
            ),
        ],
    );
}

#[test]
fn double_dash_without_separator_in_usage_ends_options() {
    // POSIX end of options when the usage does not declare `[--]`: the first `--` is dropped and
    // every argument after it is a word. Docopt 0.6.2 also stops parsing options there but keeps
    // `--` as a word; legacy rejected `--` as an unknown option.
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x>...
#
# Options:
#   -h --help  show this help
#   -v         verbose
#
"#;
    let words = |values: &[&str], verbose: bool| {
        Ok(json!({"options": {"help": false, "v": verbose}, "x": values}))
    };
    check(
        file,
        &[
            (&["a", "--", "-b", "c"], words(&["a", "-b", "c"], false)),
            (&["--", "--help"], words(&["--help"], false)),
            (&["-v", "--", "-v", "-h"], words(&["-v", "-h"], true)),
            (&["--", "a", "--"], words(&["a", "--"], false)),
            (&["--"], Err(INVALID)),
            (&["--help", "--", "a"], Err(HELP)),
        ],
    );
}

#[test]
fn bullet_lines_in_help_are_not_options() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [options] <x>
#
# Notes:
#   - first note
#   -- second note
#
# Options:
#   -v  verbose
#
"#;
    check(
        file,
        &[(&["x"], Ok(json!({"options": {"v": false}, "x": "x"})))],
    );
}
