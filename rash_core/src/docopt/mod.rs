mod options;
mod utils;

use utils::{get_smallest_regex_match, RegexMatch, WORDS_REGEX, WORDS_UPPERCASE_REGEX};

use crate::error::{Error, ErrorKind, Result};
use crate::utils::merge_json;
use crate::vars::Vars;

use std::collections::HashSet;

use minijinja::context;
use regex::{Regex, RegexSet};

/// Parse file doc and args to return docopts variables.
/// Supports help subcommand to print help and exit.
pub fn parse(file: &str, args: &[&str]) -> Result<Vars> {
    let help_msg = parse_help(file);
    let usages = match parse_usage(&help_msg) {
        Some(usages) => usages,
        None => return Ok(context! {}),
    };

    let options = options::Options::parse_doc(&help_msg, &usages)?;
    trace!("options: {options:?}");

    let expanded_args =
        options.expand_args(&args.iter().copied().map(String::from).collect::<Vec<_>>())?;

    let usage_set = HashSet::from_iter(usages.iter().cloned());

    let opts = expanded_args
        .iter()
        .filter(|x| x.starts_with('-'))
        .map(std::ops::Deref::deref)
        .collect::<Vec<_>>();
    let extended_usages = options.extend_usages(usage_set).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid usage: {}", &help_msg),
        )
    })?;

    let expanded_usages = expand_usages(extended_usages, expanded_args.len(), &opts);
    trace!("expanded usages: {expanded_usages:?}");

    let arg_kind_set = RegexSet::new([
        format!(r"^{WORDS_REGEX}\+?$"),
        format!(r"^<{WORDS_REGEX}>|{WORDS_UPPERCASE_REGEX}\+?$"),
        // options: must be between `{}`
        format!(r"(\{{[^\[]+?\}}|\-\-?{WORDS_REGEX})$"),
    ])
    .unwrap();

    let args_defs: Vec<Vec<String>> = expanded_usages
        .iter()
        .map(|usage| {
            usage
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
                        1 => Some(matches[0]),
                        _ => None,
                    }
                })
                .collect::<Option<Vec<usize>>>()
        })
        .collect::<Option<Vec<Vec<usize>>>>()
        .ok_or_else(|| {
            trace!("args: {args:?}");
            trace!("args_defs: {args_defs:?}");
            Error::new(ErrorKind::InvalidData, format!("Invalid usage: {help_msg}"))
        })?;

    let args_defs_expand_repeatable: Vec<Vec<String>> = args_defs
        .iter()
        .map(|args_def| {
            args_def
                .iter()
                .map(|arg_def| {
                    let repeatable_arg = format!("{arg_def}+");
                    if !arg_def.ends_with('+')
                        && args_defs.iter().any(|x| x.contains(&repeatable_arg))
                    {
                        repeatable_arg
                    } else {
                        arg_def.to_owned()
                    }
                })
                .collect()
        })
        .collect();

    let mut vars = options.initial_vars();
    // TODO: change vars.extend to a more functional approach with context! {vars, ..context!{}}
    // init vars

    // create lambda for extend vars
    let extend_vars = |vars: Vars, x: Vars| {
        context! {
        ..vars,
        ..x}
    };

    args_defs_expand_repeatable
        .clone()
        .iter()
        .enumerate()
        .flat_map(|(usage_idx, args_def)| {
            args_def
                .iter()
                .enumerate()
                .filter_map(|(idx, arg_def)| match args_kinds[usage_idx].get(idx) {
                    Some(0) => Some(Vars::from_serialize(
                        match args_defs_expand_repeatable
                            .iter()
                            .any(|args_def| args_def.iter().filter(|&x| x == arg_def).count() > 1)
                        {
                            true => json!({arg_def: 0}),
                            false => json!({arg_def: false}),
                        },
                    )),
                    Some(1) | Some(2) => None,
                    _ => unreachable!(),
                })
                .collect::<Vec<Vars>>()
        })
        .for_each(|x| vars = extend_vars(vars.clone(), x));

    let vars_vec = args_defs_expand_repeatable
        .iter()
        .enumerate()
        .find_map(|(usage_idx, args_def)| {
            if expanded_args.len() != args_kinds[usage_idx].len() {
                return None;
            };
            expanded_args
                .iter()
                .enumerate()
                .map(|(idx, arg)| match args_kinds[usage_idx][idx] {
                    // order matters
                    0 => parse_required(arg, &args_def[idx], args_def),
                    // order matters
                    1 => {
                        if arg == "--help" || arg == "-h" {
                            options.parse(arg, arg)
                        } else if arg.starts_with("--") {
                            None
                        } else {
                            Some(parse_positional(arg, &args_def[idx]))
                        }
                    }
                    2 => options.parse(arg, &args_def[idx]),
                    _ => unreachable!(),
                })
                .collect::<Option<Vec<Vars>>>()
        })
        .ok_or_else(|| {
            trace!("args: {args:?}");
            trace!("args_defs_expand_repeatable: {args_defs_expand_repeatable:?}");
            Error::new(ErrorKind::InvalidData, help_msg.clone())
        })?;

    let mut new_vars_json = json! {vars.clone()};
    vars_vec
        .into_iter()
        .map(|x| json! {x})
        .for_each(|x| merge_json(&mut new_vars_json, x));
    let new_vars = Vars::from_serialize(new_vars_json);

    match new_vars.get_attr("help") {
        Ok(y) if y.is_true() => Err(Error::new(ErrorKind::GracefulExit, help_msg)),
        _ => match new_vars.get_attr("options") {
            Ok(options) => match options.get_attr("help") {
                Ok(z) if z.is_true() => Err(Error::new(ErrorKind::GracefulExit, help_msg)),
                _ => Ok(new_vars),
            },
            _ => Ok(new_vars),
        },
    }
}

fn parse_help(file: &str) -> String {
    let re = Regex::new(r"#(.*)").unwrap();
    file.split('\n')
        // skip first empty line cause split
        .skip(1)
        .map_while(|line| re.captures(line))
        .filter(|cap| !cap[1].starts_with('!'))
        .map(|cap| cap[1].to_owned().replacen(' ', "", 1))
        .chain(vec![
            "Note: Options must be preceded by `--`. If not, you are passing options directly to rash.".to_owned(),
            "For more information check rash options with `rash --help`.".to_owned(),
            "".to_owned(),
        ])
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
            .map(|cap| cap[1].to_owned())
            .collect::<Vec<String>>(),
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

fn repeat_until_fill(
    usage: String,
    replace: &str,
    pattern: &str,
    args_len: usize,
    opts_len: usize,
) -> String {
    let args_without_this = usage.replace(replace, "").replace("[]", "");
    let args = args_without_this.split_whitespace().skip(1);
    let args_in_pattern = pattern.split_whitespace().count();
    if pattern.starts_with('{') {
        let pattern_repetitions = opts_len;
        let pattern_repeatable = pattern.split_whitespace().collect::<Vec<_>>().join(" ") + " ";
        usage.replace(
            replace,
            pattern_repeatable.repeat(pattern_repetitions).trim(),
        )
    } else {
        let current_args = args.filter(|x| !x.starts_with('{')).count();
        let repetitions = args_len
            .saturating_sub(current_args)
            .saturating_sub(opts_len)
            / args_in_pattern;
        let pattern_repetitions = if repetitions > 0 { repetitions } else { 1 };
        let pattern_repeatable = format!(
            "{}{}",
            if pattern.starts_with(' ') { " " } else { "" },
            pattern.split_whitespace().collect::<Vec<_>>().join("+ ") + "+ "
        );
        usage.replace(
            replace,
            pattern_repeatable.repeat(pattern_repetitions).trim_end(),
        )
    }
}

fn is_usage(possible_usage: &str, opts: &[&str]) -> bool {
    let possible_usage_opts = possible_usage
        .split(' ')
        .filter(|x| !x.contains(['(', '[', ']', ')']))
        .filter(|x| x.starts_with('-'))
        .collect::<Vec<_>>();

    possible_usage_opts.len() <= opts.len() && possible_usage_opts.iter().all(|x| opts.contains(x))
}

fn expand_usages(usages: HashSet<String>, args_len: usize, opts: &[&str]) -> HashSet<String> {
    let mut new_usages = HashSet::new();
    let mut usages_iter: Box<dyn Iterator<Item = String>> = Box::new(usages.into_iter());

    let opts_len = opts.len();
    loop {
        if let Some(usage) = usages_iter.next() {
            match get_smallest_regex_match(&usage.clone()) {
                Some((RegexMatch::InnerParenthesis, cap)) => match cap.get(2) {
                    // repeat sequence until fill usage
                    Some(_) => {
                        usages_iter = Box::new(usages_iter.chain(std::iter::once(
                            repeat_until_fill(usage.clone(), &cap[0], &cap[1], args_len, opts_len),
                        )));
                    }
                    None => {
                        #[allow(clippy::needless_collect)]
                        let splitted: Vec<String> = cap[1]
                            .split('|')
                            .map(|w| usage.clone().replacen(&cap[0].to_owned(), w.trim(), 1))
                            .collect();
                        usages_iter = Box::new(usages_iter.chain(splitted))
                    }
                },
                Some((RegexMatch::InnerBrackets, cap)) => {
                    usages_iter = Box::new(
                        usages_iter.chain(std::iter::once(
                            usage
                                .replacen(&cap[0].to_owned(), "", 1)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" "),
                        )),
                    );

                    // if captured repeatable(`...`): add usage with that repeatable case
                    if cap.len() == 3 {
                        usages_iter = Box::new(usages_iter.chain(std::iter::once(usage.replacen(
                            &cap[0].to_owned(),
                            &format!("{}{}", &cap[1], &cap[2]),
                            1,
                        ))));
                    }
                    let possible_usage = usage.replacen(&cap[0].to_owned(), &cap[1], 1);
                    if is_usage(&possible_usage, opts) {
                        usages_iter = Box::new(usages_iter.chain(std::iter::once(possible_usage)));
                    }
                }
                Some((RegexMatch::Repeatable, cap)) => {
                    let repeated_usage =
                        repeat_until_fill(usage.clone(), &cap[0], &cap[1], args_len, opts_len);
                    let remove_empty_repeatable = repeated_usage
                        .split_whitespace()
                        .filter(|w| *w != "[]")
                        .collect::<Vec<_>>()
                        .join(" ");
                    usages_iter = Box::new(
                        usages_iter.chain(std::iter::once(remove_empty_repeatable.clone())),
                    );
                }
                None if usage.contains('|') => {
                    // safe unwrap: usage contains `|`
                    let splitted = usage.split_once('|').unwrap();

                    let left_w = splitted.0.trim_end().rsplit(' ').next().unwrap();
                    let right_w = splitted.1.trim_start().split(' ').next().unwrap();

                    let left_end = splitted.0.replace(splitted.0.trim_end(), "");
                    let right_start = splitted.1.replace(splitted.1.trim_start(), "");

                    let get_usage = |x: String| {
                        usage.clone().replacen(
                            &format!("{left_w}{left_end}|{right_start}{right_w}"),
                            &x,
                            1,
                        )
                    };
                    usages_iter = Box::new(
                        usages_iter
                            .chain(std::iter::once(get_usage(left_w.to_owned())))
                            .chain(std::iter::once(get_usage(right_w.to_owned()))),
                    );
                }
                Some((RegexMatch::InnerCurlyBraces, cap)) => {
                    let usage_without_cap = usage
                        .replacen(&cap[0].to_owned(), "", 1)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    usages_iter =
                        Box::new(usages_iter.chain(std::iter::once(usage_without_cap.clone())));
                    if let Some((RegexMatch::InnerCurlyBraces, cap)) =
                        get_smallest_regex_match(&usage_without_cap)
                    {
                        usages_iter = Box::new(
                            usages_iter.chain(std::iter::once(
                                usage
                                    .replacen(&cap[0].to_owned(), "", 1)
                                    .split_whitespace()
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            )),
                        )
                    };
                    match cap.get(2) {
                        // repeat sequence until fill usage
                        Some(_) => {
                            new_usages.insert(repeat_until_fill(
                                usage.clone(),
                                &cap[0],
                                &cap[1],
                                args_len,
                                opts_len,
                            ));
                        }
                        None => {
                            new_usages.insert(usage);
                        }
                    }
                }
                None => {
                    new_usages.insert(usage);
                }
            };
        } else {
            return new_usages;
        }
    }
}

fn parse_required(arg: &str, def: &str, defs: &[String]) -> Option<Vars> {
    if arg == def {
        let value = if defs.iter().filter(|&x| *x == def).count() > 1 {
            json!({arg: 1})
        } else {
            json!({arg: true})
        };
        Some(Vars::from_serialize(value))
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
    Vars::from_serialize(value)
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
            Vars::from_serialize(json!(
            {
                "help": false,
                "install": true,
                "update": false,
                "package_filters": vec!["foo"],
            }))
        );

        let args = vec!["install", "foo", "boo"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "help": false,
                "install": true,
                "update": false,
                "package_filters": vec!["foo", "boo"],
            }))
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
            Vars::from_serialize(json!({
                "source": ["foo", "boo"],
                "dest": "/tmp",
            }))
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
            Vars::from_serialize(json!({
                "a": ["a", "c"],
                "b": ["b", "d"],
            }))
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
    fn test_parse_invalid_option() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [-d]
#
"#;

        let args = vec!["-a"];
        let error = parse(file, &args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);

        let args = vec!["-ad"];
        let error = parse(file, &args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);

        let args = vec!["-a", "-d"];
        let error = parse(file, &args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_valid_option() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [-d]
"#;

        let args = vec!["-d"];
        let result = parse(file, &args).unwrap();
        assert_eq!(
            result,
            Vars::from_serialize(json!({
                "options": {
                    "d": true,
                }
            }))
        )
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
            Vars::from_serialize(json!(
            {
                "help": false,
                "install": true,
                "update": false,
                "package_filters": vec!["foo", "boo"],
            }))
        );

        let args = vec!["install"];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_repeatable_commands() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   foo [(a | b)] [(a | b)]
#
"#;

        let args = vec!["a", "a"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "a": 2,
                "b": 0,
            }))
        )
    }

    #[test]
    fn test_parse_repeatable_option() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [-d]...
#
"#;

        let args = vec![];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "d": 0,
                },
            }))
        );

        let args = vec!["-d"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "d": 1,
                },
            }))
        );

        let args = vec!["-dd"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "d": 2,
                },
            }))
        );

        let args = vec!["-d", "-d"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "d": 2,
                },
            }))
        );
    }

    #[test]
    fn test_parse_optional() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]
#
"#;
        let args = vec![];
        let result = parse(file, &args).unwrap();

        assert_eq!(result, Vars::from_serialize(json!({})));

        let args = vec!["x"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "d": "x",
            }))
        );
    }

    #[test]
    fn test_parse_repeatable_optional() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>...]
#
"#;

        let args = vec![];
        let result = parse(file, &args).unwrap();

        assert_eq!(result, Vars::from_serialize(json!({})));

        let args = vec!["x"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "d": vec!["x"],
            }))
        );

        let args = vec!["x", "y"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "d": vec!["x", "y"],
            }))
        );

        let file = r#"
#!/usr/bin/env rash
#
# Usage: foo [<d>]...
#
"#;

        let args = vec![];
        let result = parse(file, &args).unwrap();

        assert_eq!(result, Vars::from_serialize(json!({})));

        let args = vec!["x"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "d": vec!["x"],
            }))
        );

        let args = vec!["x", "y"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "d": vec!["x", "y"],
            }))
        );
    }

    #[test]
    fn test_parse_options() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   naval_fate.py ship new <name>...
#   naval_fate.py ship <name> move <x> <y> [--speed=<kn>]
#   naval_fate.py ship shoot <x> <y>
#   naval_fate.py mine (set|remove) <x> <y> [--moored|--drifting]
#   naval_fate.py -h | --help
#   naval_fate.py --version
#
# Options:
#   -h --help        Show this screen.
#   -v --version     Show version.
#   -s --speed=<kn>  Speed in knots [default: 10].
#   --moored         Moored (anchored) mine.
#   --drifting       Drifting mine.
"#;

        let args = vec![];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidData);

        let args = vec!["mine", "set", "10", "50", "--drifting"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "drifting": true,
                    "help": false,
                    "moored": false,
                    "speed": "10",
                    "version": false,
                },
                "x": "10",
                "y": "50",
                "mine": true,
                "move": false,
                "new": false,
                "remove": false,
                "set": true,
                "ship": false,
                "shoot": false
            }))
        );

        let args = vec!["mine", "set", "10", "50", "--speed=50"];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidData);

        let args = vec!["ship", "foo", "move", "2", "3", "-s", "20"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "drifting": false,
                    "help": false,
                    "moored": false,
                    "speed": "20",
                    "version": false,
                },
                "name": [
                    "foo"
                ],
                "x": "2",
                "y": "3",
                "mine": false,
                "move": true,
                "new": false,
                "remove": false,
                "set": false,
                "ship": true,
                "shoot": false
            }))
        );

        let args = vec!["ship", "foo", "move", "2", "3", "-s20"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "drifting": false,
                    "help": false,
                    "moored": false,
                    "speed": "20",
                    "version": false,
                },
                "name": [
                    "foo"
                ],
                "x": "2",
                "y": "3",
                "mine": false,
                "move": true,
                "new": false,
                "remove": false,
                "set": false,
                "ship": true,
                "shoot": false
            }))
        );

        let args = vec!["ship", "foo", "move", "2", "3", "-s=20"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "drifting": false,
                    "help": false,
                    "moored": false,
                    "speed": "20",
                    "version": false,
                },
                "name": [
                    "foo"
                ],
                "x": "2",
                "y": "3",
                "mine": false,
                "move": true,
                "new": false,
                "remove": false,
                "set": false,
                "ship": true,
                "shoot": false
            }))
        );

        let args = vec!["ship", "foo", "move", "2", "3", "-s20", "-x"];
        let error = parse(file, &args).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn test_parse_option_unsorted() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: my_program.py [-hso FILE] [--quiet | --verbose] [INPUT ...]
#
# -h --help    show this
# -s --sorted  sorted output
# -o FILE      specify output file [default: ./test.txt]
# --quiet      print less text
# --verbose    print more text
# --dry-run    run without modifications
#
"#;

        let args = vec!["-o", "yea", "--sorted"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "dry_run": false,
                    "help": false,
                    "quiet": false,
                    "sorted": true,
                    "verbose": false,
                    "o": "yea",
                },
            }))
        )
    }

    #[test]
    fn test_parse_option_multi_word() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage: multi_word.py [options]
#
# Options
#   -h --help    show this
#   --dry-run    run without modifications
#   --fast-run   run using max CPU cores
#
"#;

        let args = vec!["--fast-run"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "dry_run": false,
                    "fast_run": true,
                    "help": false,
                },
            }))
        )
    }

    #[test]
    fn test_parse_option_placeholder() {
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
"#;

        let args = vec!["-qn", "10", "443"];
        let result = parse(file, &args).unwrap();

        assert_eq!(
            result,
            Vars::from_serialize(json!(
            {
                "options": {
                    "apply": false,
                    "help": false,
                    "number": "10",
                    "timeout": null,
                    "version": false,
                    "q": true,
                },
                "port": "443"
            }))
        );
    }

    #[test]
    fn test_parse_print_help() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots (install|update|help) [<package_filters>...]
#
"#;

        let args = vec!["help"];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::GracefulExit)
    }

    #[test]
    fn test_parse_print_help_as_option() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots [--help]
#
"#;

        let args = vec!["--help"];
        let err = parse(file, &args).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::GracefulExit)
    }

    #[test]
    fn test_parse_print_help_as_option_with_positional_argument() {
        let file = r#"
#!/usr/bin/env rash
#
# Usage:
#   ./dots [--help] <foo>
#
"#;

        let args = vec!["--help"];
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

Note: Options must be preceded by `--`. If not, you are passing options directly to rash.
For more information check rash options with `rash --help`.
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
# Examples:
#   ./dots install '.*zsh.*'
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

Examples:
  ./dots install '.*zsh.*'

Subcommands:
  install   Copy files to host.
  update    Get files from host.
  help      Show this screen.

Note: Options must be preceded by `--`. If not, you are passing options directly to rash.
For more information check rash options with `rash --help`.
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
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
            ]
        )
    }

    #[test]
    fn test_parse_usage_one_line() {
        let file = r#"
Usage:  cp <source> <dest>
"#;

        let result = parse_usage(file).unwrap();
        assert_eq!(result, vec!["cp <source> <dest>".to_owned(),])
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
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
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
                "cp <source> <dest>".to_owned(),
                "cp <source>... <dest>".to_owned(),
            ]
        )
    }

    #[test]
    fn test_expand_usages() {
        let usages = HashSet::from(["foo ((a | b) (c | d))".to_owned()]);

        let result = expand_usages(usages, 2, &[]);
        assert_eq!(
            result,
            HashSet::from([
                "foo a c".to_owned(),
                "foo a d".to_owned(),
                "foo b c".to_owned(),
                "foo b d".to_owned(),
            ])
        )
    }

    #[test]
    fn test_expand_usages_or() {
        let usages = HashSet::from(["foo -h | --help".to_owned()]);

        let result = expand_usages(usages, 2, &["--help"]);
        assert_eq!(
            result,
            HashSet::from(["foo -h".to_owned(), "foo --help".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_or_without_space() {
        let usages = HashSet::from(["foo -h|--help".to_owned()]);

        let result = expand_usages(usages, 2, &["--help"]);
        assert_eq!(
            result,
            HashSet::from(["foo -h".to_owned(), "foo --help".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_all() {
        let usages = HashSet::from(["foo [(-d | --no-deps)] [(-d | --no-deps)]".to_owned()]);

        let result = expand_usages(usages, 2, &["--no-deps", "--no-deps"]);
        assert_eq!(
            result,
            HashSet::from([
                "foo".to_owned(),
                "foo --no-deps".to_owned(),
                "foo --no-deps --no-deps".to_owned(),
            ])
        )
    }

    #[test]
    fn test_expand_usages_all_bad() {
        let usages = HashSet::from(["foo [(-d | --no-deps) (-d | --no-deps)]".to_owned()]);

        let result = expand_usages(usages, 2, &["--no-deps", "--no-deps"]);
        assert_eq!(
            result,
            HashSet::from(["foo".to_owned(), "foo --no-deps --no-deps".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_tree() {
        let usages = HashSet::from(["foo (a | b | c)".to_owned()]);

        let result = expand_usages(usages, 1, &[]);
        assert_eq!(
            result,
            HashSet::from(["foo a".to_owned(), "foo b".to_owned(), "foo c".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_optional() {
        let usages = HashSet::from(["foo a [b] c".to_owned()]);

        let result = expand_usages(usages, 3, &[]);
        assert_eq!(
            result,
            HashSet::from(["foo a b c".to_owned(), "foo a c".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_positional() {
        let usages = HashSet::from(["foo (a <b> | c <d>)".to_owned()]);

        let result = expand_usages(usages, 2, &[]);
        assert_eq!(
            result,
            HashSet::from(["foo a <b>".to_owned(), "foo c <d>".to_owned(),])
        )
    }

    #[test]
    fn test_expand_usages_double_fill() {
        let usages = HashSet::from([
            "my_program.py {--help#--sorted#-o=<-o>#} {--quiet#--verbose}... [INPUT ...]"
                .to_owned(),
        ]);

        let result = expand_usages(usages, 3, &["--sorted", "--quiet"]);
        assert_eq!(
            result,
            HashSet::from([
                "my_program.py {--help#--sorted#-o=<-o>#} {--quiet#--verbose}... INPUT+".to_owned(),
                "my_program.py {--help#--sorted#-o=<-o>#} {--quiet#--verbose}...".to_owned(),
                "my_program.py {--quiet#--verbose} {--quiet#--verbose} INPUT+".to_owned(),
                "my_program.py {--quiet#--verbose} {--quiet#--verbose}".to_owned(),
                "my_program.py {--help#--sorted#-o=<-o>#} INPUT+".to_owned(),
                "my_program.py {--help#--sorted#-o=<-o>#}".to_owned(),
                "my_program.py INPUT+".to_owned(),
                "my_program.py".to_owned(),
            ])
        )
    }

    #[test]
    fn test_repeat_until_fill() {
        let usage = "foo (<a> <b>)... <c>".to_owned();
        let replace = "(<a> <b>)...";
        let pattern = "<a> <b>";

        let result = repeat_until_fill(usage, replace, pattern, 5, 0);
        assert_eq!(result, "foo <a>+ <b>+ <a>+ <b>+ <c>")
    }

    #[test]
    fn test_repeat_until_fill_simple() {
        let usage = "foo <a>... <b>".to_owned();
        let replace = "<a>...";
        let pattern = "<a>";

        let result = repeat_until_fill(usage, replace, pattern, 4, 0);
        assert_eq!(result, "foo <a>+ <a>+ <a>+ <b>")
    }

    #[test]
    fn test_repeat_until_options() {
        let usage = "foo {-s#-o}... <c>...".to_owned();
        let replace = "{-s#-o}...";
        let pattern = "{-s#-o}";

        let result = repeat_until_fill(usage, replace, pattern, 5, 3);
        assert_eq!(result, "foo {-s#-o} {-s#-o} {-s#-o} <c>...")
    }

    #[test]
    fn test_repeat_until_options_expand_positional() {
        let usage = "foo {-s#-o}... <c>...".to_owned();
        let replace = "<c>...";
        let pattern = "<c>";

        let result = repeat_until_fill(usage, replace, pattern, 5, 3);
        assert_eq!(result, "foo {-s#-o}... <c>+ <c>+")
    }

    #[test]
    fn test_parse_required() {
        let arg_def = r"foo";

        let arg = "foo";
        let result = parse_required(arg, arg_def, &[]).unwrap();
        assert_eq!(
            result,
            Vars::from_serialize(json!({
                "foo": true,
            }))
        )
    }

    #[test]
    fn test_parse_required_fails() {
        let arg_def = r"foo";

        let arg = "boo";
        let result = parse_required(arg, arg_def, &[]);
        assert_eq!(result, None)
    }

    #[test]
    fn test_parse_positional() {
        let arg_def = r"<foo>";

        let arg = "boo";
        let result = parse_positional(arg, arg_def);
        assert_eq!(
            result,
            Vars::from_serialize(json!({
                "foo": "boo",
            }))
        )
    }

    #[test]
    fn test_parse_positional_repeatable() {
        let arg_def = r"<foo>+";

        let arg = "boo";
        let result = parse_positional(arg, arg_def);
        assert_eq!(
            result,
            Vars::from_serialize(json!({
                "foo": vec!["boo"],
            }))
        )
    }
}
