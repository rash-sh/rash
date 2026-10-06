use rash_core::{docopt, error::ErrorKind, script_cli};
use serde_json::{Value, json};

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

/// Assert the compiled parser result against an explicit expectation (`None` means rejection).
///
/// Used where legacy intentionally diverges from reference Docopt 0.6.2 semantics.
fn assert_compiled(file: &str, args: &[&str], expected: Option<Value>) {
    match (script_cli::parse(file, args), expected) {
        (Ok(compiled), Some(expected)) => assert_eq!(compiled, expected, "args={args:?}"),
        (Err(compiled), None) => {
            assert_eq!(compiled.kind(), ErrorKind::InvalidData, "args={args:?}")
        }
        (compiled, expected) => {
            panic!(
                "unexpected result for args={args:?}: compiled={compiled:?} expected={expected:?}"
            )
        }
    }
}

fn count(args: &[&str], token: &str) -> usize {
    args.iter().filter(|arg| **arg == token).count()
}

fn enumerate_argv<'a>(alphabet: &'a [&'a str], max_len: usize) -> Vec<Vec<&'a str>> {
    let mut all = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];

    for _ in 0..max_len {
        let mut next = Vec::new();
        for prefix in frontier {
            for token in alphabet {
                let mut argv = prefix.clone();
                argv.push(*token);
                all.push(argv.clone());
                next.push(argv);
            }
        }
        frontier = next;
    }

    all
}

#[test]
fn exhaustive_unordered_optional_flags() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-b] [-c]
#
# Options:
#   -a --alpha    alpha
#   -b --beta     beta
#   -c --charlie  charlie
#
"#;

    // Reference Docopt rejects a flag repeated beyond its declared occurrences; legacy accepts it.
    for args in enumerate_argv(&["-a", "-b", "-c"], 4) {
        let flags = ["-a", "-b", "-c"];
        let expected = flags.iter().all(|flag| count(&args, flag) <= 1).then(|| {
            json!({"options": {
                "alpha": args.contains(&"-a"),
                "beta": args.contains(&"-b"),
                "charlie": args.contains(&"-c"),
            }})
        });
        assert_compiled(file, &args, expected);
    }
}

#[test]
fn exhaustive_options_shortcut_flags() {
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

    for args in enumerate_argv(&["-a", "-b", "--alpha", "--beta"], 3) {
        assert_parity(file, &args);
    }
}

#[test]
fn exhaustive_optional_commands_and_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [prepare] (start|stop) [force]
#
"#;

    for args in enumerate_argv(&["prepare", "start", "stop", "force", "other"], 4) {
        assert_parity(file, &args);
    }
}

#[test]
fn exhaustive_repeated_command_counts() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [(a | b)] [(a | b)]
#
"#;

    // Commands repeated in the pattern are counters, even when matched once (reference Docopt
    // `{"a": 1, "b": 0}`); legacy reports `true` for a single match.
    for args in enumerate_argv(&["a", "b", "c"], 3) {
        let expected = (args.len() <= 2 && !args.contains(&"c"))
            .then(|| json!({"a": count(&args, "a"), "b": count(&args, "b")}));
        assert_compiled(file, &args, expected);
    }
}

#[test]
fn exhaustive_duplicated_optional_flag_counts() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [-a] [-a] [-b]
#
# Options:
#   -a --alpha  alpha
#   -b --beta   beta
#
"#;

    // Reference Docopt: an option declared twice is a counter bounded by its declarations.
    for args in enumerate_argv(&["-a", "-b"], 4) {
        let expected = (count(&args, "-a") <= 2 && count(&args, "-b") <= 1).then(
            || json!({"options": {"alpha": count(&args, "-a"), "beta": args.contains(&"-b")}}),
        );
        assert_compiled(file, &args, expected);
    }
}

#[test]
fn exhaustive_repeated_positional_collects_list() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [<x>] [<x>]
#
"#;

    // Reference Docopt collects a positional declared twice into a list; legacy keeps the last.
    // Absent positionals are omitted (Rash convention shared with legacy).
    for args in enumerate_argv(&["p", "q"], 3) {
        let expected = match args.len() {
            0 => Some(json!({})),
            1 | 2 => Some(json!({"x": args})),
            _ => None,
        };
        assert_compiled(file, &args, expected);
    }
}

#[test]
fn exhaustive_option_and_positional_interleaving() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <input> [--verbose] [<output>]
#
# Options:
#   -v --verbose  verbose
#
"#;

    for args in enumerate_argv(&["in", "out", "-v", "--verbose"], 4) {
        assert_parity(file, &args);
    }
}
