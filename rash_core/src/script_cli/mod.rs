mod grammar;
mod matcher;
mod options;

use serde_json::{Map, Value};

use crate::error::{Error, ErrorKind, Result};

use grammar::Metadata;
use matcher::{Capture, MatchError};
use options::OptionRegistry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Token {
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Pipe,
    Ellipsis,
    Atom(String),
    Option(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InputToken {
    Word(String),
    Option { id: usize, value: Option<String> },
}

/// Parse the CLI declaration embedded in a Rash script and return template variables.
///
/// The syntax is Docopt-inspired, but the implementation is Rash-specific. Usage patterns are
/// parsed into an AST, compiled into an epsilon-NFA, and matched directly against normalized argv.
/// No concrete usage combinations are generated.
pub fn parse(file: &str, args: &[&str]) -> Result<Value> {
    let help_msg = parse_help(file);
    let usages = match parse_usage(&help_msg) {
        Some(usages) => usages,
        None => return Ok(json!({})),
    };

    let mut options = OptionRegistry::from_doc(&help_msg, &usages)?;
    let patterns = usages
        .iter()
        .map(|usage| options.tokenize_usage(usage).and_then(grammar::parse))
        .collect::<Result<Vec<_>>>()?;

    let metadata = grammar::analyze(&patterns);
    options.set_repeatable(&metadata.repeatable_options)?;

    let normalized_args = options.normalize_args(args)?;
    let nfa = matcher::compile(&patterns, &options);
    let captures = match matcher::execute(&nfa, &normalized_args) {
        Ok(captures) => captures,
        Err(MatchError::NoMatch) => {
            return Err(Error::new(ErrorKind::InvalidData, help_msg));
        }
        Err(MatchError::Ambiguous) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Ambiguous usage declaration.\n\n{help_msg}"),
            ));
        }
    };

    let vars = build_vars(&metadata, &options, captures)?;
    if help_requested(&vars) {
        Err(Error::new(ErrorKind::GracefulExit, help_msg))
    } else {
        Ok(vars)
    }
}

fn build_vars(
    metadata: &Metadata,
    options: &OptionRegistry,
    captures: Vec<Capture>,
) -> Result<Value> {
    let mut root = Map::new();

    if !options.is_empty() {
        root.insert(
            "options".to_owned(),
            Value::Object(options.initial_options()),
        );
    }

    for (command, repeated) in &metadata.command_repeated {
        root.insert(
            command.clone(),
            if *repeated {
                Value::from(0_u64)
            } else {
                Value::Bool(false)
            },
        );
    }

    for capture in captures {
        match capture {
            Capture::Command(key) => {
                if metadata
                    .command_repeated
                    .get(&key)
                    .copied()
                    .unwrap_or(false)
                {
                    let count = root.get(&key).and_then(Value::as_u64).unwrap_or_default() + 1;
                    root.insert(key, Value::from(count));
                } else {
                    root.insert(key, Value::Bool(true));
                }
            }
            Capture::Positional { key, value } => {
                if metadata
                    .positional_repeated
                    .get(&key)
                    .copied()
                    .unwrap_or(false)
                {
                    match root.entry(key).or_insert_with(|| Value::Array(Vec::new())) {
                        Value::Array(values) => values.push(Value::String(value)),
                        current => {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Positional argument changed type unexpectedly: {current}"),
                            ));
                        }
                    }
                } else {
                    root.insert(key, Value::String(value));
                }
            }
            Capture::Option { id, value } => {
                let options_value = root
                    .get_mut("options")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidData,
                            "Option capture without options context",
                        )
                    })?;
                options.apply_capture(options_value, id, value.as_deref())?;
            }
        }
    }

    Ok(Value::Object(root))
}

fn help_requested(vars: &Value) -> bool {
    value_enabled(vars.get("help"))
        || vars
            .get("options")
            .and_then(|options| options.get("help"))
            .is_some_and(|value| value_enabled(Some(value)))
}

fn value_enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_u64().is_some_and(|value| value > 0),
        _ => false,
    }
}

fn parse_help(file: &str) -> String {
    file.split('\n')
        .skip(1)
        .map_while(|line| {
            line.find('#')
                .map(|position| line[position + 1..].to_owned())
        })
        .filter(|line| !line.starts_with('!'))
        .map(|line| line.strip_prefix(' ').unwrap_or(&line).to_owned())
        .chain([
            "Note: Options must be preceded by `--`. If not, you are passing options directly to rash."
                .to_owned(),
            "For more information check rash options with `rash --help`.".to_owned(),
            String::new(),
        ])
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_usage(doc: &str) -> Option<Vec<String>> {
    let lines = doc.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("usage:"))
        {
            continue;
        }

        let first = trimmed[6..].trim();
        if !first.is_empty() {
            return Some(vec![first.to_owned()]);
        }

        let mut usages = Vec::new();
        for next in lines.iter().skip(index + 1) {
            if next.trim().is_empty() {
                break;
            }
            if next.chars().next().is_some_and(char::is_whitespace) {
                usages.push(next.trim().to_owned());
            } else {
                break;
            }
        }
        return (!usages.is_empty()).then_some(usages);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_parity(file: &str, args: &[&str]) {
        let legacy = crate::docopt::parse(file, args);
        let compiled = parse(file, args);
        match (legacy, compiled) {
            (Ok(legacy), Ok(compiled)) => assert_eq!(compiled, legacy),
            (Err(legacy), Err(compiled)) => assert_eq!(compiled.kind(), legacy.kind()),
            (legacy, compiled) => {
                panic!("parser mismatch: legacy={legacy:?} compiled={compiled:?}")
            }
        }
    }

    #[test]
    fn parity_dotfiles_cli() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
"#;
        assert_parity(file, &["install", "foo", "bar"]);
        assert_parity(file, &["update", "foo"]);
        assert_parity(file, &["help", "foo"]);
    }

    #[test]
    fn parity_repeatable_group() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo (<a> <b>)...
#
"#;
        assert_parity(file, &["a", "b", "c", "d"]);
        assert_parity(file, &["a", "b", "c"]);
    }

    #[test]
    fn parity_options_aliases_defaults_and_clusters() {
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
        assert_parity(file, &["-o", "yea", "--sorted"]);
        assert_parity(file, &["-h"]);
    }

    #[test]
    fn parity_repeatable_flag() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [-d]...
#
"#;
        assert_parity(file, &[]);
        assert_parity(file, &["-d"]);
        assert_parity(file, &["-dd"]);
        assert_parity(file, &["-d", "-d"]);
    }

    #[test]
    fn parity_complex_docopt_example() {
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
"#;
        assert_parity(file, &["mine", "set", "10", "50", "--drifting"]);
        assert_parity(file, &["ship", "foo", "move", "2", "3", "-s20"]);
        assert_parity(file, &["ship", "new", "a", "b", "c"]);
    }

    #[test]
    fn parity_options_shortcut_matrix() {
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
        for mask in 0..8 {
            let mut args = Vec::new();
            if mask & 1 != 0 {
                args.push("--alpha");
            }
            if mask & 2 != 0 {
                args.push("--beta");
            }
            if mask & 4 != 0 {
                args.push("--gamma");
            }
            args.push("target");
            assert_parity(file, &args);
        }
    }

    #[test]
    fn options_shortcut_is_scoped_per_usage_pattern() {
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
        let result = parse(file, &["get", "--force"]).unwrap();
        assert_eq!(result["options"]["force"], true);
    }

    #[test]
    fn ambiguous_usage_is_rejected_deterministically() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   tool <source> <dest>
#   tool <input> <output>
#
"#;
        let error = parse(file, &["a", "b"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains("Ambiguous usage declaration"));
    }

    #[test]
    fn ten_thousand_repeatable_arguments_do_not_expand_grammar() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: tool <file>...
#
"#;
        let owned = (0..10_000)
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let result = parse(file, &args).unwrap();
        assert_eq!(result["file"].as_array().unwrap().len(), 10_000);
    }
}
