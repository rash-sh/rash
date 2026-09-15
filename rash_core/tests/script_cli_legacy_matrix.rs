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
fn repeatable_commands_keep_count_semantics() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [(a | b)] [(a | b)]
#
"#;

    for args in [
        vec![],
        vec!["a"],
        vec!["b"],
        vec!["a", "a"],
        vec!["a", "b"],
        vec!["b", "b"],
    ] {
        assert_parity(file, &args);
    }
}

#[test]
fn optional_positional_forms_keep_shape() {
    let optional = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]
#
"#;
    assert_parity(optional, &[]);
    assert_parity(optional, &["x"]);

    let repeat_inside = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>...]
#
"#;
    assert_parity(repeat_inside, &[]);
    assert_parity(repeat_inside, &["x"]);
    assert_parity(repeat_inside, &["x", "y"]);

    let repeat_outside = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]...
#
"#;
    assert_parity(repeat_outside, &[]);
    assert_parity(repeat_outside, &["x"]);
    assert_parity(repeat_outside, &["x", "y"]);
}

#[test]
fn value_bearing_short_cluster_matches_legacy() {
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

    assert_parity(file, &["-qn", "10", "443"]);
    assert_parity(file, &["-q", "-n10", "443"]);
    assert_parity(file, &["--number=10", "443"]);
}

#[test]
fn invalid_short_options_keep_error_class() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [-d]
#
"#;

    assert_parity(file, &["-a"]);
    assert_parity(file, &["-ad"]);
    assert_parity(file, &["-a", "-d"]);
}

#[test]
fn cp_overlapping_usages_keep_repeatable_output_shape() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

    assert_parity(file, &["foo", "/tmp"]);
    assert_parity(file, &["foo", "bar", "/tmp"]);
    assert_parity(file, &["foo", "bar", "baz", "/tmp"]);
}

#[test]
fn option_placeholder_can_be_inferred_from_description() {
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

    assert_parity(file, &["daemon-reload"]);
    assert_parity(file, &["--type=timer", "daemon-reexec"]);
    assert_parity(file, &["--type", "timer", "daemon-reexec"]);
}

#[test]
fn command_options_and_positional_options_keep_positions() {
    let commands = r#"
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
    assert_parity(commands, &["--verbose", "start", "--force"]);
    assert_parity(commands, &["stop", "--force"]);

    let positionals = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./tool <input> [--verbose] <output>
#
# Options:
#   --verbose  Show detailed output
#
"#;
    assert_parity(positionals, &["input.txt", "--verbose", "output.txt"]);
    assert_parity(positionals, &["input.txt", "output.txt"]);
}

#[test]
fn complex_naval_fate_success_and_failure_paths_match() {
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

    for args in [
        vec![],
        vec!["mine", "set", "10", "50", "--drifting"],
        vec!["mine", "set", "10", "50", "--speed=50"],
        vec!["ship", "foo", "move", "2", "3", "-s", "20"],
        vec!["ship", "foo", "move", "2", "3", "-s20"],
        vec!["ship", "foo", "move", "2", "3", "-s=20"],
        vec!["ship", "foo", "move", "2", "3", "-s20", "-x"],
    ] {
        assert_parity(file, &args);
    }
}
