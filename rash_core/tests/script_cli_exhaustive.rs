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

    for args in enumerate_argv(&["-a", "-b", "-c"], 4) {
        assert_parity(file, &args);
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

    for args in enumerate_argv(&["a", "b", "c"], 3) {
        assert_parity(file, &args);
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
