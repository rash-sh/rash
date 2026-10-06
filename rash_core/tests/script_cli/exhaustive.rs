//! Bounded exhaustive argv enumeration: every argv up to a fixed length over a small alphabet is
//! checked against a hand-written model of the declaration.

use serde_json::json;

use crate::{Expected, INVALID, parse};

fn count(args: &[&str], token: &str) -> usize {
    args.iter().filter(|arg| **arg == token).count()
}

fn is_option(arg: &str) -> bool {
    arg.starts_with('-')
}

/// All argv vectors of length `0..=max_len` over `alphabet`.
fn enumerate_argv<'a>(alphabet: &[&'a str], max_len: usize) -> Vec<Vec<&'a str>> {
    let mut all = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];

    for _ in 0..max_len {
        frontier = frontier
            .iter()
            .flat_map(|prefix| {
                alphabet.iter().map(|token| {
                    let mut argv: Vec<&str> = prefix.clone();
                    argv.push(token);
                    argv
                })
            })
            .collect();
        all.extend(frontier.iter().cloned());
    }

    all
}

#[track_caller]
fn check_model(file: &str, alphabet: &[&str], max_len: usize, model: impl Fn(&[&str]) -> Expected) {
    for args in enumerate_argv(alphabet, max_len) {
        assert_eq!(parse(file, &args), model(&args), "args={args:?}");
    }
}

#[test]
fn unordered_optional_flags() {
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

    // Unlike legacy (as in docopt 0.6.2): a flag repeated beyond its declarations is rejected.
    check_model(file, &["-a", "-b", "-c"], 4, |args| {
        if ["-a", "-b", "-c"].iter().any(|flag| count(args, flag) > 1) {
            return Err(INVALID);
        }
        Ok(json!({"options": {
            "alpha": args.contains(&"-a"),
            "beta": args.contains(&"-b"),
            "charlie": args.contains(&"-c"),
        }}))
    });
}

#[test]
fn options_shortcut_flags() {
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

    // A bare `[options]` with two or more options accepts repeated flags (shared with legacy).
    check_model(file, &["-a", "-b", "--alpha", "--beta"], 3, |args| {
        Ok(json!({"options": {
            "alpha": args.contains(&"-a") || args.contains(&"--alpha"),
            "beta": args.contains(&"-b") || args.contains(&"--beta"),
        }}))
    });
}

#[test]
fn optional_commands_around_alternative() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [prepare] (start|stop) [force]
#
"#;

    check_model(
        file,
        &["prepare", "start", "stop", "force", "other"],
        4,
        |args| {
            let (prepare, rest) = match args {
                ["prepare", rest @ ..] => (true, rest),
                rest => (false, rest),
            };
            let (command, force) = match rest {
                [command @ ("start" | "stop")] => (*command, false),
                [command @ ("start" | "stop"), "force"] => (*command, true),
                _ => return Err(INVALID),
            };
            Ok(json!({
                "prepare": prepare,
                "start": command == "start",
                "stop": command == "stop",
                "force": force,
            }))
        },
    );
}

#[test]
fn repeated_command_counts() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [(a | b)] [(a | b)]
#
"#;

    // Unlike legacy (as in docopt 0.6.2): commands repeated in the pattern are counters even when
    // matched once.
    check_model(file, &["a", "b", "c"], 3, |args| {
        if args.len() > 2 || args.contains(&"c") {
            return Err(INVALID);
        }
        Ok(json!({"a": count(args, "a"), "b": count(args, "b")}))
    });
}

#[test]
fn duplicated_optional_flag_counts() {
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

    // Unlike legacy (as in docopt 0.6.2): an option declared twice is a counter bounded by its
    // declarations.
    check_model(file, &["-a", "-b"], 4, |args| {
        if count(args, "-a") > 2 || count(args, "-b") > 1 {
            return Err(INVALID);
        }
        Ok(json!({"options": {"alpha": count(args, "-a"), "beta": args.contains(&"-b")}}))
    });
}

#[test]
fn repeated_positional_collects_list() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool [<x>] [<x>]
#
"#;

    // Unlike legacy (as in docopt 0.6.2): a positional declared twice collects a list. Absent
    // positionals are omitted.
    check_model(file, &["p", "q"], 3, |args| match args.len() {
        0 => Ok(json!({})),
        1 | 2 => Ok(json!({"x": args})),
        _ => Err(INVALID),
    });
}

#[test]
fn option_and_positional_interleaving() {
    let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <input> [--verbose] [<output>]
#
# Options:
#   -v --verbose  verbose
#
"#;

    // Options are matched at their declared position.
    check_model(file, &["in", "out", "-v", "--verbose"], 4, |args| {
        let (input, verbose, output) = match args {
            [input] => (*input, false, None),
            [input, flag] if is_option(flag) => (*input, true, None),
            [input, output] => (*input, false, Some(*output)),
            [input, flag, output] if is_option(flag) => (*input, true, Some(*output)),
            _ => return Err(INVALID),
        };
        if is_option(input) || output.is_some_and(is_option) {
            return Err(INVALID);
        }
        let mut expected = json!({"input": input, "options": {"verbose": verbose}});
        if let Some(output) = output {
            expected["output"] = json!(output);
        }
        Ok(expected)
    });
}
