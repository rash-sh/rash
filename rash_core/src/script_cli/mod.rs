mod grammar;
mod matcher;
mod options;

use regex::Regex;
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
    let re = Regex::new(r"#(.*)").unwrap();
    file.split('\n')
        .skip(1)
        .map_while(|line| re.captures(line))
        .filter(|cap| !cap[1].starts_with('!'))
        .map(|cap| cap[1].to_owned().replacen(' ', "", 1))
        .chain([
            "Note: Options must be preceded by `--`. If not, you are passing options directly to rash."
                .to_owned(),
            "For more information check rash options with `rash --help`.".to_owned(),
            String::new(),
        ])
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_usage_multiline(doc: &str) -> Option<Vec<String>> {
    let re = Regex::new(r"(?mi)Usage:\n((.|\n)*?(^[a-z\n]|\z))").unwrap();
    let re_rm_indentation = Regex::new(r"\s+(.*)").unwrap();
    let cap = re.captures_iter(doc).next()?;
    Some(
        cap[1]
            .split('\n')
            .map_while(|line| re_rm_indentation.captures(line))
            .map(|cap| cap[1].to_owned())
            .collect::<Vec<_>>(),
    )
}

fn parse_usage_one_line(doc: &str) -> Option<Vec<String>> {
    let re = Regex::new(r"(?i)Usage:\s+(.*)\n").unwrap();
    let cap = re.captures_iter(doc).next()?;
    Some(vec![cap[1].to_owned()])
}

fn parse_usage(doc: &str) -> Option<Vec<String>> {
    parse_usage_multiline(doc).or_else(|| parse_usage_one_line(doc))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str =
        "Note: Options must be preceded by `--`. If not, you are passing options directly to rash.
For more information check rash options with `rash --help`.
";

    #[test]
    fn help_skips_shebang_and_strips_comment_prefix() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

        assert_eq!(
            parse_help(file),
            format!("\nUsage:\n  cp <source> <dest>\n  cp <source>... <dest>\n\n{NOTE}")
        );
    }

    #[test]
    fn help_stops_at_first_non_comment_line() {
        let file = r#"
#!/usr/bin/env -S rash --diff
#
# dots easy manage of your dotfiles.
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
doe: "a deer, a female deer"
# comment example
"#;

        assert_eq!(
            parse_help(file),
            format!(
                "\ndots easy manage of your dotfiles.\n\nUsage:\n  ./dots (install|update|help) <package_filters>...\n\n{NOTE}"
            )
        );
    }

    #[test]
    fn multiline_usage() {
        let doc = "\nUsage:\n  cp <source> <dest>\n  cp <source>... <dest>\n";
        assert_eq!(
            parse_usage(doc),
            Some(vec![
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
            ])
        );
    }

    #[test]
    fn multiline_usage_ends_at_blank_line() {
        let doc = "\nUsage:\n  cp <source> <dest>\n  cp <source>... <dest>\n\nfoo\n";
        assert_eq!(
            parse_usage(doc),
            Some(vec![
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
            ])
        );
    }

    #[test]
    fn multiline_usage_ends_at_next_section() {
        let doc = "\nUsage:\n  cp <source> <dest>\n  cp <source>... <dest>\nFoo:\n  buu\n  fuu\n";
        assert_eq!(
            parse_usage(doc),
            Some(vec![
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
            ])
        );
    }

    #[test]
    fn one_line_usage() {
        let doc = "\nUsage:  cp <source> <dest>\n";
        assert_eq!(
            parse_usage(doc),
            Some(vec!["cp <source> <dest>".to_owned()])
        );
    }

    #[test]
    fn missing_usage() {
        assert_eq!(parse_usage("\nNo usage here\n"), None);
    }
}
