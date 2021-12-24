use crate::error::{Error, ErrorKind, Result};
use crate::utils::merge_json;
use crate::vars::Vars;

use lazy_static;
use regex::{Regex, RegexSet};
use tera::Context;

const WORDS_REGEX: &str = r"[-a-zA-Z]+(?:[_\-][a-zA-Z]+)*";

lazy_static! {
    static ref RE_INNER_PARENTHESIS: Regex = Regex::new(r"\(([^\(]+?)\)(\.\.\.)?").unwrap();
    static ref RE_INNER_BRACKETS: Regex = Regex::new(r"\[([^\[]+?)\](\.\.\.)?").unwrap();
    static ref RE_POSITIONAL_REPEATABLE: Regex =
        Regex::new(&format!(r"(<{}>)(\.\.\.)", WORDS_REGEX)).unwrap();
}

/// Parse file doc and args to return docopts variables.
/// Supports help subcommand to print help and exit.
pub fn parse(file: &str, args: &[&str]) -> Result<Vars> {
    let help_msg = parse_help(file);
    let mut vars = Context::new();
    let usages = match parse_usage(&help_msg) {
        Some(usages) => usages,
        None => return Ok(vars),
    };

    let expanded_usages = expand_usages(usages, args.len());

    let arg_kind_set = RegexSet::new(&[
        format!(r"^{}$", WORDS_REGEX),
        format!(r"^<{}>\+?$", WORDS_REGEX),
    ])
    .unwrap();

    let args_defs: Vec<Vec<String>> = expanded_usages
        .iter()
        .map(|usage| {
            usage
                // all arguments
                .split_whitespace()
                // skip arg 0 (script name)
                .skip(1)
                .map(String::from)
                .collect::<Vec<String>>()
        })
        .collect();
    let args_kinds = args_defs
        .iter()
        .map(|args_def| {
            args_def
                .clone()
                .iter()
                .map(|w| {
                    let matches: Vec<_> = arg_kind_set.matches(w).into_iter().collect();
                    match matches.len() {
                        1 => matches.into_iter().next(),
                        _ => None,
                    }
                })
                .collect::<Option<Vec<usize>>>()
        })
        .collect::<Option<Vec<Vec<usize>>>>()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid usage: {}", &help_msg),
            )
        })?;

    let _ = args_defs
        .iter()
        .enumerate()
        .map(|(usage_idx, args_def)| {
            args_def
                .iter()
                .enumerate()
                .filter_map(|(idx, arg_def)| match args_kinds[usage_idx].get(idx) {
                    // safe unwrap: all args_kinds were previously checked
                    Some(0) => Some(Context::from_value(json!({arg_def: false})).unwrap()),
                    _ => None,
                })
                .collect::<Vec<Vars>>()
        })
        .flatten()
        .for_each(|x| vars.extend(x));

    let vars_vec = args_defs
        .iter()
        .enumerate()
        .find_map(|(usage_idx, args_def)| {
            if args.len() != args_kinds[usage_idx].len() {
                return None;
            };
            args.iter()
                .enumerate()
                .map(|(idx, arg)| match args_kinds[usage_idx][idx] {
                    0 => parse_required(arg, &args_def[idx]),
                    1 => Some(parse_positional(arg, &args_def[idx])),
                    _ => unreachable!(),
                })
                .collect::<Option<Vec<Vars>>>()
        })
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, help_msg.clone()))?;

    let mut new_vars_json = vars.into_json();
    let _ = vars_vec
        .into_iter()
        .map(|x| x.into_json())
        .for_each(|x| merge_json(&mut new_vars_json, x));
    // safe unwrap: new_vars_json is a json object
    let new_vars = Context::from_value(new_vars_json).unwrap();
    match new_vars.get("help") {
        Some(json!(true)) => Err(Error::new(ErrorKind::GracefulExit, help_msg)),
        _ => Ok(new_vars),
    }
}

fn parse_help(file: &str) -> String {
    let re = Regex::new(r"#(.*)").unwrap();
    file.split('\n')
        .into_iter()
        // skip first empty line cause split
        .skip(1)
        .map_while(|line| re.captures(line))
        .filter(|cap| !cap[1].starts_with('!'))
        .map(|cap| cap[1].to_string().replacen(" ", "", 1))
        .collect::<Vec<String>>()
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
            .map(|cap| cap[1].to_string())
            .collect::<Vec<String>>(),
    )
}

fn parse_usage_one_line(doc: &str) -> Option<Vec<String>> {
    let re = Regex::new(r"(?i)Usage:\s+(.*)\n").unwrap();
    let cap = re.captures_iter(doc).next()?;
    Some(vec![cap[1].to_string()])
}

fn parse_usage(doc: &str) -> Option<Vec<String>> {
    match parse_usage_multiline(doc) {
        Some(usage) => Some(usage),
        None => parse_usage_one_line(doc),
    }
}

fn repeat_until_fill(usage: String, replace: &str, pattern: &str, args_len: usize) -> String {
    let current_args = usage
        .replace(replace, "")
        .split_whitespace()
        .skip(1)
        .count();
    let args_in_pattern = pattern.split_whitespace().count();
    let pattern_repetitions = (args_len - current_args) / args_in_pattern;
    let pattern_repeatable = pattern.split_whitespace().collect::<Vec<_>>().join("+ ") + "+ ";
    usage.replace(
        replace,
        pattern_repeatable.repeat(pattern_repetitions).trim(),
    )
}

fn expand_usages(usages: Vec<String>, args_len: usize) -> Vec<String> {
    let mut new_usages: Vec<String> = Vec::new();

    usages
        .into_iter()
        .for_each(|usage| match RE_INNER_PARENTHESIS.captures(&usage) {
            Some(cap) => match cap.get(2) {
                // repeat sequence until fill usage
                Some(_) => new_usages.extend(expand_usages(
                    vec![repeat_until_fill(usage.clone(), &cap[0], &cap[1], args_len)],
                    args_len,
                )),
                None => cap[1].split('|').for_each(|w| {
                    new_usages.extend(expand_usages(
                        vec![usage.replace(&cap[0].to_string(), w.trim())],
                        args_len,
                    ))
                }),
            },
            None => match RE_INNER_BRACKETS.captures(&usage) {
                Some(cap) => {
                    new_usages.extend(expand_usages(
                        vec![usage.replace(&cap[0].to_string(), &cap[1])],
                        args_len,
                    ));
                    new_usages.extend(expand_usages(
                        vec![usage
                            .replace(&cap[0].to_string(), "")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")],
                        args_len,
                    ));
                }
                None => match RE_POSITIONAL_REPEATABLE.captures(&usage) {
                    Some(cap) => new_usages.push(repeat_until_fill(
                        usage.clone(),
                        &cap[0],
                        &cap[1],
                        args_len,
                    )),
                    None => new_usages.push(usage),
                },
            },
        });
    new_usages
}

fn parse_required(arg: &str, def: &str) -> Option<Vars> {
    if arg == def {
        Some(Context::from_value(json!({arg: true})).unwrap())
    } else {
        None
    }
}

fn parse_positional(arg: &str, def: &str) -> Vars {
    // safe unwrap: Must be a positional argument definition
    let key = def.strip_prefix('<').unwrap().split_once('>').unwrap().0;
    let value = if def.ends_with('+') {
        json!({ key: vec![arg] })
    } else {
        json!({ key: arg })
    };
    Context::from_value(value).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
"#;

        let args = vec!["install", "foo"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Context::from_value(json!(
            {
                "help": false,
                "install": true,
                "update": false,
                "package_filters": vec!["foo"],
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_cp_example() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

        let args = vec!["foo", "boo", "/tmp"];
        let result = parse(file, &args).unwrap();
        assert_eq!(
            result,
            Context::from_value(json!({
                "source": ["foo", "boo"],
                "dest": "/tmp",
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_double_repeatable() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo (<a> <b>)...
#
"#;

        let args = vec!["a", "b", "c", "d"];
        let result = parse(file, &args).unwrap();
        assert_eq!(
            result,
            Context::from_value(json!({
                "a": ["a", "c"],
                "b": ["b", "d"],
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_double_repeatable_error() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo (<a> <b>)...
#
"#;

        let args = vec!["a", "b", "c"];
        let error = parse(file, &args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData)
    }

    #[test]
    fn test_parse_repeatable() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
"#;

        let args = vec!["install", "foo", "boo"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Context::from_value(json!(
            {
                "help": false,
                "install": true,
                "update": false,
                "package_filters": vec!["foo", "boo"],
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_print_help() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
"#;

        let args = vec!["help"];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::GracefulExit)
    }

    #[test]
    fn test_parse_help() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   cp <source> <dest>
#   cp <source>... <dest>
#
"#;

        let result = parse_help(file);
        assert_eq!(
            result,
            r#"
Usage:
  cp <source> <dest>
  cp <source>... <dest>
"#
        )
    }

    #[test]
    fn test_parse_help_complete_example() {
        let file = r#"
#!/usr/bin/env -S rash --diff
#
# dots easy manage of your dotfiles.
#
# Usage:
#   ./dots (install|update|help) <package_filters>...
#
# Arguments:
#   package_filters   List of regex matching packages wanted.
#
# Options:
#   -c,--check  dry-run mode
#
# Examples:
#   ./dots install --check '.*zsh.*'
#
# Subcommands:
#   install   Copy files to host.
#   update    Get files from host.
#   help      Show this screen.
#
doe: "a deer, a female deer"
ray: "a drop of golden sun"
pi: 3.14159
xmas: true
french-hens: 3
calling-birds:
  - huey
  - dewey
  - louie
  - fred
# comment example
xmas-fifth-day:
  # yep, another comment example
  calling-birds: four
  french-hens: 3
  golden-rings: 5
  partridges:
    count: 1
    location: "a pear tree"
  turtle-doves: two
"#;

        let result = parse_help(file);
        assert_eq!(
            result,
            r#"
dots easy manage of your dotfiles.

Usage:
  ./dots (install|update|help) <package_filters>...

Arguments:
  package_filters   List of regex matching packages wanted.

Options:
  -c,--check  dry-run mode

Examples:
  ./dots install --check '.*zsh.*'

Subcommands:
  install   Copy files to host.
  update    Get files from host.
  help      Show this screen.
"#
        )
    }

    #[test]
    fn test_parse_usage() {
        let file = r#"
Usage:
  cp <source> <dest>
  cp <source>... <dest>
"#;

        let result = parse_usage(file).unwrap();
        assert_eq!(
            result,
            vec![
                "cp <source> <dest>".to_string(),
                "cp <source>... <dest>".to_string(),
            ]
        )
    }

    #[test]
    fn test_parse_usage_one_line() {
        let file = r#"
Usage:  cp <source> <dest>
"#;

        let result = parse_usage(file).unwrap();
        assert_eq!(result, vec!["cp <source> <dest>".to_string(),])
    }

    #[test]
    fn test_parse_usage_new_line_after() {
        let file = r#"
Usage:
  cp <source> <dest>
  cp <source>... <dest>

"#;

        let result = parse_usage(file).unwrap();
        assert_eq!(
            result,
            vec![
                "cp <source> <dest>".to_string(),
                "cp <source>... <dest>".to_string(),
            ]
        )
    }

    #[test]
    fn test_parse_usage_section_after() {
        let file = r#"
Usage:
  cp <source> <dest>
  cp <source>... <dest>
Foo:
  buu
  fuu
"#;

        let result = parse_usage(file).unwrap();
        assert_eq!(
            result,
            vec![
                "cp <source> <dest>".to_string(),
                "cp <source>... <dest>".to_string(),
            ]
        )
    }

    #[test]
    fn test_expand_usages() {
        let usages = vec!["foo ((a | b) (c | d))".to_string()];

        let result = expand_usages(usages, 2);
        assert_eq!(
            result,
            vec![
                "foo a c".to_string(),
                "foo a d".to_string(),
                "foo b c".to_string(),
                "foo b d".to_string(),
            ]
        )
    }

    #[test]
    fn test_expand_usages_tree() {
        let usages = vec!["foo (a | b | c)".to_string()];

        let result = expand_usages(usages, 1);
        assert_eq!(
            result,
            vec![
                "foo a".to_string(),
                "foo b".to_string(),
                "foo c".to_string(),
            ]
        )
    }

    #[test]
    fn test_expand_usages_optional() {
        let usages = vec!["foo a [b] c".to_string()];

        let result = expand_usages(usages, 1);
        assert_eq!(
            result,
            vec!["foo a b c".to_string(), "foo a c".to_string(),]
        )
    }

    #[test]
    fn test_expand_usages_positional() {
        let usages = vec!["foo (a <b> | c <d>)".to_string()];

        let result = expand_usages(usages, 2);
        assert_eq!(
            result,
            vec!["foo a <b>".to_string(), "foo c <d>".to_string(),]
        )
    }

    #[test]
    fn test_repeat_until_fill() {
        let usage = "foo (<a> <b>)... <c>".to_string();
        let replace = "(<a> <b>)...";
        let pattern = "<a> <b>";

        let result = repeat_until_fill(usage, replace, pattern, 5);
        assert_eq!(result, "foo <a>+ <b>+ <a>+ <b>+ <c>")
    }

    #[test]
    fn test_repeat_until_fill_simple() {
        let usage = "foo <a>... <b>".to_string();
        let replace = "<a>...";
        let pattern = "<a>";

        let result = repeat_until_fill(usage, replace, pattern, 4);
        assert_eq!(result, "foo <a>+ <a>+ <a>+ <b>")
    }

    #[test]
    fn test_parse_required() {
        let arg_def = r"foo";

        let arg = "foo";
        let result = parse_required(arg, &arg_def).unwrap();
        assert_eq!(
            result,
            Context::from_value(json!({
                "foo": true,
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_required_fails() {
        let arg_def = r"foo";

        let arg = "boo";
        let result = parse_required(arg, &arg_def);
        assert_eq!(result, None)
    }

    #[test]
    fn test_parse_positional() {
        let arg_def = r"<foo>";

        let arg = "boo";
        let result = parse_positional(arg, &arg_def);
        assert_eq!(
            result,
            Context::from_value(json!({
                "foo": "boo",
            }))
            .unwrap()
        )
    }

    #[test]
    fn test_parse_positional_repeatable() {
        let arg_def = r"<foo>+";

        let arg = "boo";
        let result = parse_positional(arg, &arg_def);
        assert_eq!(
            result,
            Context::from_value(json!({
                "foo": vec!["boo"],
            }))
            .unwrap()
        )
    }
}
