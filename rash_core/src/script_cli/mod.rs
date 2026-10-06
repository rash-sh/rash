//! Script CLI parser: turns the usage declaration in a Rash script's leading comment block, and
//! the script arguments, into template variables.
//!
//! The pipeline is:
//!
//! 1. extract the help text and its `Usage:` patterns from the script comments;
//! 2. build the option registry from the option descriptions and usage patterns ([`options`]);
//! 3. parse every usage pattern into an AST and analyze symbol multiplicity ([`grammar`]);
//! 4. compile the patterns into one epsilon-NFA and match the normalized argv ([`matcher`]);
//! 5. turn the captures of the highest-priority successful match into variables.

mod grammar;
mod matcher;
mod options;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::error::{Error, ErrorKind, Result};

use grammar::Metadata;
use matcher::Capture;
use options::OptionRegistry;

/// Regex compiled on first use; a compilation error is reported by [`compiled`].
type LazyRegex = LazyLock<std::result::Result<Regex, regex::Error>>;

/// Usage block made of the indented lines after a `Usage:` line.
static USAGE_MULTILINE_RE: LazyRegex =
    LazyLock::new(|| Regex::new(r"(?mi)Usage:\n((.|\n)*?(^[a-z\n]|\z))"));
/// Single usage pattern on the `Usage:` line itself.
static USAGE_ONE_LINE_RE: LazyRegex = LazyLock::new(|| Regex::new(r"(?i)Usage:\s+(.*)\n"));

const HELP_FOOTER: [&str; 3] = [
    "Note: Options must be preceded by `--`. If not, you are passing options directly to rash.",
    "For more information check rash options with `rash --help`.",
    "",
];

/// Lexical token of a usage pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Pipe,
    Ellipsis,
    Atom(String),
    /// Option resolved to its id in the [`OptionRegistry`].
    Option(usize),
}

/// Script argument after option normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
enum InputToken {
    Word(String),
    Option { id: usize, value: Option<String> },
}

/// Parse the CLI declaration embedded in a Rash script and return template variables.
///
/// The syntax is Docopt-inspired, but the implementation is Rash-specific. Usage patterns are
/// parsed into an AST, compiled into an epsilon-NFA, and matched directly against normalized argv.
/// No concrete usage combinations are generated. When several matches are possible, the first
/// pattern in declaration order wins, and within a pattern optional elements and repetitions
/// consume as many arguments as still allow a match.
///
/// A script without a `Usage:` declaration yields an empty object.
///
/// # Errors
///
/// - [`ErrorKind::GracefulExit`] with the help text when help is requested.
/// - [`ErrorKind::InvalidData`] with the help text when `args` match no usage pattern, and with a
///   specific message when the declaration is invalid or an argument is not a declared option.
pub fn parse(file: &str, args: &[&str]) -> Result<Value> {
    let help_msg = parse_help(file);
    let Some(usages) = parse_usage(&help_msg)? else {
        return Ok(json!({}));
    };

    let mut options = OptionRegistry::from_doc(&help_msg, &usages)?;
    let patterns = usages
        .iter()
        .map(|usage| options.tokenize_usage(usage).and_then(grammar::parse))
        .collect::<Result<Vec<_>>>()?;

    let metadata = grammar::analyze(&patterns);
    check_reserved_names(&metadata, &options)?;
    options.set_repeatable(&metadata.repeatable_options)?;

    let normalized_args = options.normalize_args(args)?;
    let nfa = matcher::compile(&patterns, &options);
    let captures = matcher::execute(&nfa, &normalized_args)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, help_msg.clone()))?;

    let vars = build_vars(&metadata, &options, captures)?;
    if help_requested(&vars) {
        Err(Error::new(ErrorKind::GracefulExit, help_msg))
    } else {
        Ok(vars)
    }
}

/// Reject a command or positional named `options` when there are options: the `options` variable
/// holds the option values.
fn check_reserved_names(metadata: &Metadata, options: &OptionRegistry) -> Result<()> {
    let declares_options_symbol = metadata.command_repeated.contains_key("options")
        || metadata.positional_repeated.contains_key("options");
    if declares_options_symbol && !options.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "`options` is a reserved name when the usage declares options: rename the `options` \
             command or positional",
        ));
    }
    Ok(())
}

/// Initial values of every command and option, updated with the captures of the match.
///
/// Absent positionals are omitted.
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
        let initial = if *repeated {
            Value::from(0_u64)
        } else {
            Value::Bool(false)
        };
        root.insert(command.clone(), initial);
    }

    for capture in captures {
        match capture {
            Capture::Command(key) => apply_command(&mut root, metadata, key),
            Capture::Positional { key, value } => {
                apply_positional(&mut root, metadata, key, value)?
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

/// Repeatable commands count their occurrences; other commands become `true`.
fn apply_command(root: &mut Map<String, Value>, metadata: &Metadata, key: String) {
    if is_repeated(&metadata.command_repeated, &key) {
        let count = root.get(&key).and_then(Value::as_u64).unwrap_or_default() + 1;
        root.insert(key, Value::from(count));
    } else {
        root.insert(key, Value::Bool(true));
    }
}

/// Repeatable positionals collect a list of values; other positionals hold a single string.
fn apply_positional(
    root: &mut Map<String, Value>,
    metadata: &Metadata,
    key: String,
    value: String,
) -> Result<()> {
    if !is_repeated(&metadata.positional_repeated, &key) {
        root.insert(key, Value::String(value));
        return Ok(());
    }

    match root.entry(key).or_insert_with(|| Value::Array(Vec::new())) {
        Value::Array(values) => {
            values.push(Value::String(value));
            Ok(())
        }
        current => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Positional argument changed type unexpectedly: {current}"),
        )),
    }
}

fn is_repeated(repeated: &BTreeMap<String, bool>, key: &str) -> bool {
    repeated.get(key).copied().unwrap_or(false)
}

fn help_requested(vars: &Value) -> bool {
    value_enabled(vars.get("help"))
        || value_enabled(vars.get("options").and_then(|options| options.get("help")))
}

fn value_enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_u64().is_some_and(|value| value > 0),
        _ => false,
    }
}

/// Help text: the comment block after the first line of `file`, without the comment marker, the
/// first following space, or `#!` lines, followed by a note about passing options to scripts.
fn parse_help(file: &str) -> String {
    file.split('\n')
        .skip(1)
        .map_while(|line| line.split_once('#').map(|(_, comment)| comment))
        .filter(|comment| !comment.starts_with('!'))
        .map(|comment| comment.replacen(' ', "", 1))
        .chain(HELP_FOOTER.map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compiled(regex: &'static LazyRegex) -> Result<&'static Regex> {
    LazyLock::force(regex)
        .as_ref()
        .map_err(|error| Error::new(ErrorKind::Other, error.clone()))
}

/// Text after the first whitespace run of `line`, or `None` if `line` has no whitespace.
fn strip_indentation(line: &str) -> Option<&str> {
    let start = line.find(char::is_whitespace)?;
    Some(line[start..].trim_start_matches(char::is_whitespace))
}

fn parse_usage_multiline(doc: &str) -> Result<Option<Vec<String>>> {
    let Some(captures) = compiled(&USAGE_MULTILINE_RE)?.captures(doc) else {
        return Ok(None);
    };
    Ok(Some(
        captures[1]
            .split('\n')
            .map_while(strip_indentation)
            .map(str::to_owned)
            .collect(),
    ))
}

fn parse_usage_one_line(doc: &str) -> Result<Option<Vec<String>>> {
    Ok(compiled(&USAGE_ONE_LINE_RE)?
        .captures(doc)
        .map(|captures| vec![captures[1].to_owned()]))
}

/// Usage patterns of the help text, or `None` if it declares no usage.
fn parse_usage(doc: &str) -> Result<Option<Vec<String>>> {
    match parse_usage_multiline(doc)? {
        Some(usages) => Ok(Some(usages)),
        None => parse_usage_one_line(doc),
    }
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
            parse_usage(doc).unwrap(),
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
            parse_usage(doc).unwrap(),
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
            parse_usage(doc).unwrap(),
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
            parse_usage(doc).unwrap(),
            Some(vec!["cp <source> <dest>".to_owned()])
        );
    }

    #[test]
    fn missing_usage() {
        assert_eq!(parse_usage("\nNo usage here\n").unwrap(), None);
    }
}
