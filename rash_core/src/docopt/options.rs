use crate::docopt::utils::{expand_brackets, split_keeping_separators};
use crate::error::{Error, ErrorKind, Result};
use crate::utils::merge_json;

use std::collections::HashSet;
use std::sync::LazyLock;

use itertools::Itertools;
use regex::Regex;
use serde_json::Value;

const OPTIONS_MARK: &str = "[options]";

static RE_DEFAULT_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[default: (.*)\]").unwrap());

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum OptionArg {
    Simple {
        short: Option<String>,
        long: Option<String>,
    },
    Repeatable {
        short: Option<String>,
        long: Option<String>,
    },
    WithParam {
        short: Option<String>,
        long: Option<String>,
        default_value: Option<String>,
    },
}

impl OptionArg {
    pub fn get_short(&self) -> Option<String> {
        match self {
            OptionArg::Simple { short, .. } => short.clone(),
            OptionArg::Repeatable { short, .. } => short.clone(),
            OptionArg::WithParam { short, .. } => short.clone(),
        }
    }

    pub fn get_long(&self) -> Option<String> {
        match self {
            OptionArg::Simple { long, .. } => long.clone(),
            OptionArg::Repeatable { long, .. } => long.clone(),
            OptionArg::WithParam { long, .. } => long.clone(),
        }
    }

    pub fn get_simple_representation(&self) -> String {
        self.get_long()
            // safe unwrap: if it long is None, short should be Some
            .unwrap_or_else(|| self.get_short().unwrap())
    }

    pub fn get_key_representation(&self) -> String {
        self.get_simple_representation()
            .replacen('-', "", 2)
            .replace('-', "_")
    }

    pub fn get_representation(&self) -> String {
        let repr = self.get_simple_representation();
        match self {
            OptionArg::Simple { .. } => repr,
            OptionArg::Repeatable { .. } => repr,
            OptionArg::WithParam { .. } => format!("{repr}=<{repr}>"),
        }
    }

    pub fn merge(&self, option: &Self) -> Result<Self> {
        if option == self {
            return Ok(self.clone());
        }

        let compare_attr = |a: Option<String>, b: Option<String>| -> Result<Option<String>> {
            match (a, b) {
                (Some(x), Some(y)) => {
                    if x != y {
                        Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Not mergeable options: {x} {y}"),
                        ))
                    } else {
                        Ok(Some(x))
                    }
                }
                (None, Some(y)) => Ok(Some(y)),
                (Some(x), None) => Ok(Some(x)),
                (None, None) => Ok(None),
            }
        };

        let long = compare_attr(self.get_long(), option.get_long())?;
        let short = compare_attr(self.get_short(), option.get_short())?;

        // compare types
        match (self, option) {
            (OptionArg::Simple { .. }, OptionArg::Simple { .. }) => {
                Ok(OptionArg::Simple { short, long })
            }
            (OptionArg::Simple { .. }, OptionArg::Repeatable { .. }) => {
                Ok(OptionArg::Repeatable { short, long })
            }
            (OptionArg::Simple { .. }, OptionArg::WithParam { default_value, .. }) => {
                Ok(OptionArg::WithParam {
                    short,
                    long,
                    default_value: default_value.clone(),
                })
            }
            (OptionArg::Repeatable { .. }, OptionArg::Simple { .. }) => {
                Ok(OptionArg::Repeatable { short, long })
            }
            (OptionArg::Repeatable { .. }, OptionArg::Repeatable { .. }) => {
                Ok(OptionArg::Repeatable { short, long })
            }
            (OptionArg::Repeatable { .. }, OptionArg::WithParam { .. })
            | (OptionArg::WithParam { .. }, OptionArg::Repeatable { .. }) => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Not mergeable options: {self:?} {option:?}"),
            )),
            (OptionArg::WithParam { default_value, .. }, OptionArg::Simple { .. }) => {
                Ok(OptionArg::WithParam {
                    short,
                    long,
                    default_value: default_value.clone(),
                })
            }
            (
                OptionArg::WithParam { default_value, .. },
                OptionArg::WithParam {
                    default_value: option_default_value,
                    ..
                },
            ) => Ok(OptionArg::WithParam {
                short,
                long,
                default_value: compare_attr(default_value.clone(), option_default_value.clone())?,
            }),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Options {
    hash_set: HashSet<OptionArg>,
}

impl Options {
    fn new(hash_set: HashSet<OptionArg>) -> Self {
        Options { hash_set }
    }

    fn get_option_arg(option_line: &str) -> OptionArg {
        let (option, description) =
            if let Some((option, description)) = option_line.split_once("  ") {
                (option, description)
            } else {
                (option_line, "")
            };

        let mut short: Option<String> = None;
        let mut long: Option<String> = None;
        let mut is_repeatable = false;
        let mut is_with_param = false;

        for w in option
            .replace([',', '='], " ")
            .replace('.', " repeatable ")
            .split_whitespace()
        {
            if w.starts_with("--") {
                long = Some(w.to_owned());
            } else if w.starts_with('-') {
                short = Some(w.to_owned());
            } else if w == "repeatable" {
                is_repeatable = true;
            } else {
                is_with_param = true;
            }
        }

        if is_with_param {
            let default_value = if let Some(cap) = RE_DEFAULT_VALUE.captures(description) {
                cap.get(1).map(|x| x.as_str().to_owned())
            } else {
                None
            };
            OptionArg::WithParam {
                short,
                long,
                default_value,
            }
        } else if is_repeatable {
            OptionArg::Repeatable { short, long }
        } else {
            OptionArg::Simple { short, long }
        }
    }

    fn find(&self, arg_usage: &str) -> Option<OptionArg> {
        if arg_usage.starts_with("--") {
            self.hash_set
                .clone()
                .into_iter()
                .find_map(|option_arg| match option_arg.get_long() {
                    Some(long) if long == arg_usage => Some(option_arg),
                    _ => None,
                })
        } else if arg_usage.starts_with('-') {
            self.hash_set
                .clone()
                .into_iter()
                .find_map(|option_arg| match option_arg.get_short() {
                    Some(short) if short == arg_usage => Some(option_arg),
                    _ => None,
                })
        } else {
            None
        }
    }

    fn extend(&mut self, options: Options) -> Result<Self> {
        options.hash_set.iter().try_for_each(|option| {
            match self.find(&option.get_simple_representation()) {
                Some(duplicated_option) => {
                    if option != &duplicated_option {
                        self.hash_set.remove(&duplicated_option);
                        self.hash_set.insert(duplicated_option.merge(option)?);
                    }
                }
                None => {
                    self.hash_set.insert(option.clone());
                }
            };
            Ok::<(), Error>(())
        })?;
        Ok(self.clone())
    }

    pub fn parse_doc(doc: &str, usages: &[String]) -> Result<Self> {
        let mut description_options = Options::new(
            doc.split('\n')
                .filter_map(|line| {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with('-') {
                        Some(trimmed)
                    } else {
                        None
                    }
                })
                .map(Self::get_option_arg)
                .collect::<HashSet<OptionArg>>(),
        );
        let usage_options = usages
            .iter()
            .flat_map(|usage| {
                usage
                    .clone()
                    .replace('|', " | ")
                    .replace('[', " [ ")
                    //mark repeatable options
                    .replace("]...", ". ] ")
                    .replace(']', " ] ")
                    .split_whitespace()
                    // skip arg 0 (script name)
                    .skip(1)
                    .flat_map(|arg| {
                        let is_option = arg.starts_with('-');
                        if is_option && !arg.starts_with("--") {
                            let mut is_end_args_in_chars = false;
                            arg.chars()
                                // skip `-`
                                .skip(1)
                                .filter_map(|arg_char| {
                                    if is_end_args_in_chars {
                                        None
                                    } else {
                                        match description_options.find(&format!("-{arg_char}")) {
                                            Some(OptionArg::WithParam { .. }) => {
                                                is_end_args_in_chars = true;
                                                Some(format!("-{arg_char}={arg_char}"))
                                            }
                                            _ => {
                                                // repeatable options
                                                if arg_char == '.' {
                                                    Some(arg_char.to_string())
                                                } else {
                                                    Some(format!("-{arg_char}"))
                                                }
                                            }
                                        }
                                    }
                                })
                                .collect::<Vec<String>>()
                        } else if is_option {
                            vec![arg.to_owned()]
                        } else {
                            vec![]
                        }
                    })
                    .collect::<Vec<_>>()
                    .iter()
                    .circular_tuple_windows()
                    .filter_map(|(previous_arg, arg)| {
                        let is_previous_option = previous_arg.starts_with('-');
                        let is_option = arg.starts_with('-');
                        if is_previous_option && is_option {
                            Some(Self::get_option_arg(previous_arg))
                        } else if is_previous_option && (arg == ".") {
                            Some(Self::get_option_arg(&format!("{previous_arg}.")))
                        } else if is_previous_option && !is_option {
                            Some(Self::get_option_arg(&format!("{previous_arg}={arg}")))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<HashSet<_>>();

        description_options.extend(Options::new(usage_options))
    }

    pub fn parse(&self, arg: &str, def: &str) -> Option<Value> {
        let (arg_key, arg_value_option) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (arg, None),
        };
        let option_arg = self.find(arg_key)?;

        // check arg is in def
        def.replace(['{', '}'], "")
            .split('#')
            .find(|arg_def| &option_arg.get_representation() == arg_def)?;

        let value = match option_arg {
            // safe unwrap: WithParams always has `=` in the representation
            OptionArg::WithParam { .. } => json!(arg_value_option.unwrap()),
            OptionArg::Repeatable { .. } => json!(1),
            OptionArg::Simple { .. } => json!(true),
        };

        Some(json!({
            "options": {
                option_arg.get_key_representation(): value
            }
        }))
    }

    pub fn initial_vars(&self) -> Value {
        // TODO: refactor to more functional way and remove JSON. Look for a better structure to
        // store state
        let mut new_vars_json = json!({});

        self.hash_set
            .clone()
            .into_iter()
            .map(|option_arg| {
                let value = match option_arg.clone() {
                    OptionArg::Simple { .. } => {
                        json!(false)
                    }
                    OptionArg::Repeatable { .. } => {
                        json!(0)
                    }
                    OptionArg::WithParam { default_value, .. } => {
                        if default_value.is_some() {
                            json!(default_value.unwrap())
                        } else {
                            json!(null)
                        }
                    }
                };
                json!(
                { "options":
                    { option_arg.get_key_representation(): value
                    }
                })
            })
            .for_each(|x| merge_json(&mut new_vars_json, x));
        new_vars_json
    }

    /// Normalize command-line options according to docopt conventions.
    ///
    /// This function performs the following transformations:
    /// - Unstacks short options (e.g., `-abc` → `-a -b -c`)
    /// - Expands short options to their long form when available (e.g., `-q` → `--quiet`)
    /// - Normalizes option-parameter formats (e.g., `-o FILE` → `-o=FILE`)
    /// - Handles attached parameters (e.g., `-oFILE` → `-o=FILE`)
    pub fn normalize_options(&self, args: &[String]) -> Result<Vec<String>> {
        let mut is_antepenultimate_with_param = false;
        args.iter()
            .flat_map(|arg| {
                if arg.starts_with('-') && !arg.starts_with("--") {
                    let mut is_previously_added = false;
                    arg.chars()
                        // skip `-`
                        .skip(1)
                        .flat_map(|arg_char| {
                            if is_previously_added {
                                return vec![];
                            }
                            let short = format!("-{arg_char}");
                            match self.find(&short) {
                                Some(option_arg) => {
                                    let mut result = vec![option_arg.get_simple_representation()];
                                    if matches!(option_arg, OptionArg::WithParam { .. }) {
                                        let split_char = match arg.contains('=') {
                                            true => '=',
                                            false => arg_char,
                                        };
                                        let param = arg.split_once(split_char).unwrap().1;
                                        if !param.is_empty() {
                                            result.push(param.to_owned());
                                        }
                                        is_previously_added = true;
                                    }
                                    result
                                }
                                None => {
                                    vec![short]
                                }
                            }
                        })
                        .collect::<Vec<String>>()
                } else {
                    match arg.split_once('=') {
                        Some((a, b)) => vec![a.to_owned(), b.to_owned()],
                        None => vec![arg.to_owned()],
                    }
                }
            })
            .collect::<Vec<_>>()
            .iter()
            .circular_tuple_windows()
            // transform options with params to --{option}={value}
            .filter_map(|(previous_arg, arg)| {
                if previous_arg.starts_with('-') {
                    self.find(previous_arg)
                        .map(|option_arg| {
                            let repr = option_arg.get_simple_representation();
                            match option_arg {
                                OptionArg::WithParam { .. } => {
                                    is_antepenultimate_with_param = true;
                                    Ok(format!("{repr}={arg}"))
                                }
                                OptionArg::Simple { .. } | OptionArg::Repeatable { .. } => Ok(repr),
                            }
                        })
                        .or_else(|| {
                            Some(Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Unknown option: {previous_arg}"),
                            )))
                        })
                } else if is_antepenultimate_with_param {
                    is_antepenultimate_with_param = false;
                    match previous_arg.as_str() {
                        ")" | "]" | "|" => Some(Ok(previous_arg.to_owned())),
                        _ => None,
                    }
                } else {
                    Some(Ok(previous_arg.to_owned()))
                }
            })
            .collect()
    }

    /// Extend usages with the normalized representation of all available options.
    ///
    /// Transforms patterns containing `[options]` by substituting a compact syntax that combines all options
    /// within curly braces separated by # characters. This creates an internal format the parser uses to
    /// efficiently match command-line arguments against patterns with options.
    pub fn extend_usages(&self, usages: HashSet<String>) -> Option<HashSet<String>> {
        let mut options_in_usage = Vec::new();

        let represented_usages = usages
            .iter()
            .map(|usage| {
                match self.normalize_options(
                    &usage
                        .replace('|', " | ")
                        .split_whitespace()
                        .flat_map(|w| split_keeping_separators(w, &['[', ']', '(', ')']))
                        .collect::<Vec<_>>(),
                ) {
                    Ok(expanded_usage) => Some(
                        expanded_usage
                            .iter()
                            .map(|arg| {
                                if arg.starts_with('-') {
                                    // safe unwrap: split always return at least one field
                                    if let Some(option_arg) =
                                        self.find(arg.split('=').next().unwrap())
                                    {
                                        options_in_usage.push(option_arg.clone());
                                        Some(option_arg.get_representation())
                                    } else {
                                        None
                                    }
                                } else {
                                    Some(arg.to_owned())
                                }
                            })
                            .collect::<Option<Vec<String>>>()?
                            .join(" ")
                            .replace("[ ", "[")
                            .replace(" ]", "]")
                            .replace("( ", "(")
                            .replace(" )", ")")
                            .replace(" ...", "...")
                            .replace(" |", "|")
                            .replace("| ", "|"),
                    ),
                    Err(_) => None,
                }
            })
            .collect::<Option<HashSet<String>>>()?;

        let replaced_options_usages = represented_usages.iter().map(|usage| {
            if usage.contains(OPTIONS_MARK) {
                let remaining_options: Vec<_> = self
                    .hash_set
                    .clone()
                    .into_iter()
                    .filter(|o| !options_in_usage.contains(o))
                    .map(|o| o.get_representation())
                    .collect();
                usage.replace(OPTIONS_MARK, &format!("[{}]", remaining_options.join(" ")))
            } else {
                usage.to_owned()
            }
        });

        let expanded_brackets_usages = replaced_options_usages.map(|usage| expand_brackets(&usage));

        let mut option_groups: Vec<String> = Vec::new();
        Some(
            expanded_brackets_usages
                .map(|usage| {
                    let mut new_usage = usage.clone();
                    let mut bracket_groups = usage
                        .split('[')
                        .flat_map(|group| group.split('('))
                        // first group is non bracket group
                        .skip(1)
                        // remove empty strings
                        .filter(|v| !v.is_empty())
                        .peekable();
                    while let Some(bracket_group) = bracket_groups.next() {
                        let is_last = bracket_groups.peek().is_none();
                        let is_same_group_of_options = bracket_group.starts_with('-')
                            && bracket_group.split_once(']').is_some()
                            && !bracket_group.contains('|');
                        if bracket_group.starts_with('-')
                            && bracket_group.contains('|')
                            && bracket_group.split_once(']').is_some()
                        {
                            new_usage = new_usage
                                .replace(
                                    &format!("[{bracket_group}"),
                                    &format!(
                                        "{{{}}}",
                                        bracket_group.replace(['[', ']'], "").replace('|', "#")
                                    ),
                                )
                                .replace(" }", "} ")
                        }
                        if is_same_group_of_options {
                            option_groups
                                // safe unwrap: checked because is_option
                                .push(bracket_group.split_once(']').unwrap().0.to_owned());
                        }
                        if (is_same_group_of_options && is_last)
                            || (!is_same_group_of_options && !option_groups.is_empty())
                        {
                            new_usage = new_usage.replace(
                                &option_groups.iter().map(|s| format!("[{s}]")).join(" "),
                                &format!(
                                    "{{{}}}{}",
                                    option_groups.clone().join("#"),
                                    if option_groups.len() > 1 { "..." } else { "" }
                                ),
                            );
                            option_groups = Vec::new();
                        }
                    }
                    new_usage
                })
                .collect::<HashSet<String>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_parse_doc() {
        let usage = "my_program.rh [-hsoFILE] [--repeatable]... [--quiet | --verbose] [INPUT ...]";
        let file = format!(
            r#"
Usage: {usage}

-h --help        show this
-s --sorted      sorted output
-o FILE          specify output file [default: ./test.txt]
-r --repeatable  can be repeated. E.g.: -rr
--quiet          print less text
--verbose        print more text
"#
        );

        let result = Options::parse_doc(&file, &[usage.to_owned()]).unwrap();

        assert_eq!(
            result,
            Options::new(HashSet::from([
                OptionArg::Simple {
                    short: Some("-h".to_owned()),
                    long: Some("--help".to_owned()),
                },
                OptionArg::Simple {
                    short: Some("-s".to_owned()),
                    long: Some("--sorted".to_owned()),
                },
                OptionArg::WithParam {
                    short: Some("-o".to_owned()),
                    long: None,
                    default_value: Some("./test.txt".to_owned()),
                },
                OptionArg::Repeatable {
                    short: Some("-r".to_owned()),
                    long: Some("--repeatable".to_owned()),
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--quiet".to_owned()),
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--verbose".to_owned()),
                },
            ]))
        )
    }

    #[test]
    fn test_options_parse_doc_without_description() {
        let usage = "my_program.rh [-hsoFILE] [--quiet | --verbose] [INPUT ...]";
        let file = format!(
            r#"
{usage}
"#
        );

        let result = Options::parse_doc(&file, &[usage.to_owned()]).unwrap();

        assert_eq!(
            result,
            Options::new(HashSet::from([
                OptionArg::Simple {
                    short: Some("-h".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-s".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-o".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-F".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-I".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-L".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: Some("-E".to_owned()),
                    long: None,
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--verbose".to_owned()),
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--quiet".to_owned()),
                },
            ]))
        )
    }

    #[test]
    fn test_options_parse_repeatable_argument() {
        let usage = "my_program.rh [--repeatable]...";
        let file = format!(
            r#"
{usage}
"#
        );

        let result = Options::parse_doc(&file, &[usage.to_owned()]).unwrap();

        assert_eq!(
            result,
            Options::new(HashSet::from([OptionArg::Repeatable {
                short: None,
                long: Some("--repeatable".to_owned()),
            },]))
        )
    }

    #[test]
    fn test_options_parse_with_param_argument() {
        let usage = "my_program.rh [--param=<value>]";
        let file = format!(
            r#"
{usage}
"#
        );

        let result = Options::parse_doc(&file, &[usage.to_owned()]).unwrap();

        assert_eq!(
            result,
            Options::new(HashSet::from([OptionArg::WithParam {
                short: None,
                long: Some("--param".to_owned()),
                default_value: None,
            },]))
        );

        let usage = "my_program.rh [--param=VALUE]";
        let file = format!(
            r#"
{usage}
"#
        );

        let result = Options::parse_doc(&file, &[usage.to_owned()]).unwrap();

        assert_eq!(
            result,
            Options::new(HashSet::from([OptionArg::WithParam {
                short: None,
                long: Some("--param".to_owned()),
                default_value: None,
            },]))
        )
    }

    #[test]
    fn test_options_parse() {
        let options = Options::new(HashSet::from([OptionArg::Simple {
            short: Some("-h".to_owned()),
            long: Some("--help".to_owned()),
        }]));

        let arg_def = r"--help";

        let arg = "--help";
        let result = options.parse(arg, arg_def).unwrap();

        assert_eq!(
            result,
            json!({
                "options": {
                    "help": true,
                }
            })
        )
    }

    #[test]
    fn test_options_parse_multiple() {
        let options = Options::new(HashSet::from([
            OptionArg::Simple {
                short: Some("-h".to_owned()),
                long: Some("--help".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::WithParam {
                short: Some("-o".to_owned()),
                long: None,
                default_value: Some("./test.txt".to_owned()),
            },
            OptionArg::Simple {
                short: None,
                long: Some("--quiet".to_owned()),
            },
            OptionArg::Simple {
                short: None,
                long: Some("--verbose".to_owned()),
            },
        ]));

        let arg_def = r"{--help#--sorted#-o=<-o>#--quiet|--verbose}";
        let arg = "--sorted";
        let result = options.parse(arg, arg_def).unwrap();

        assert_eq!(
            result,
            json!({
                "options": {
                    "sorted": true,
                }
            })
        );

        let arg_def = r"{--help#--sorted#-o=<-o>#--quiet|--verbose}";
        let arg = "-o=Fgwe=sad";
        let result = options.parse(arg, arg_def).unwrap();

        assert_eq!(
            result,
            json!({
                "options": {
                    "o": "Fgwe=sad",
                }
            })
        );

        let arg_def = r"{--help#--sorted#-o=<-o>#--quiet|--verbose}";
        let arg = "-o=Fgwe=sad";
        let result = options.parse(arg, arg_def).unwrap();

        assert_eq!(
            result,
            json!({
                "options": {
                    "o": "Fgwe=sad",
                }
            })
        );

        let arg_def = r"{--help#--sorted#-o=<-o>#--quiet|--verbose}";
        let arg = "--object";
        let result = options.parse(arg, arg_def);

        assert_eq!(result, None);
    }

    #[test]
    fn test_options_normalize_options() {
        let options = Options::new(HashSet::from([
            OptionArg::WithParam {
                short: Some("-o".to_owned()),
                long: None,
                default_value: Some("./test.txt".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-q".to_owned()),
                long: Some("--quiet".to_owned()),
            },
        ]));

        let args = vec!["-qo".to_owned(), "yea".to_owned(), "--sorted".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(
            result,
            vec![
                "--quiet".to_owned(),
                "-o=yea".to_owned(),
                "--sorted".to_owned(),
            ],
        );

        let args = vec!["-qo".to_owned(), "yea".to_owned(), "-s".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(
            result,
            vec![
                "--quiet".to_owned(),
                "-o=yea".to_owned(),
                "--sorted".to_owned(),
            ],
        );

        let args = vec!["-qoyea".to_owned(), "-s".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(
            result,
            vec![
                "--quiet".to_owned(),
                "-o=yea".to_owned(),
                "--sorted".to_owned(),
            ],
        );

        let args = vec!["-sq".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["--sorted".to_owned(), "--quiet".to_owned(),],);

        let args = vec!["-o=yea".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["-o=yea".to_owned()]);

        let args = vec!["-oyeo".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["-o=yeo".to_owned()]);

        let args = vec!["-o=FOO=yea".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["-o=FOO=yea".to_owned()]);

        let options = Options::new(HashSet::from([OptionArg::WithParam {
            short: Some("-t".to_owned()),
            long: Some("--test".to_owned()),
            default_value: Some("all".to_owned()),
        }]));

        let args = vec!["-tall-except-one".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["--test=all-except-one".to_owned()]);

        let args = vec!["--test=none".to_owned()];

        let result = options.normalize_options(&args).unwrap();
        assert_eq!(result, vec!["--test=none".to_owned()]);
    }

    #[test]
    fn test_options_extend_usage() {
        let options = Options::new(HashSet::from([
            OptionArg::Simple {
                short: Some("-h".to_owned()),
                long: Some("--help".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-q".to_owned()),
                long: Some("--quiet".to_owned()),
            },
        ]));

        let usages = HashSet::from(["foo a [-h]".to_owned(), "foo b [-qsh]".to_owned()]);

        let result = options.extend_usages(usages).unwrap();

        assert_eq!(
            result,
            HashSet::from([
                "foo a {--help}".to_owned(),
                "foo b {--quiet#--sorted#--help}...".to_owned(),
            ])
        )
    }

    #[test]
    fn test_options_extend_simple() {
        let options = Options::new(HashSet::from([OptionArg::Simple {
            short: Some("-h".to_owned()),
            long: Some("--help".to_owned()),
        }]));

        let usages = HashSet::from(["foo --help".to_owned()]);

        let result = options.extend_usages(usages).unwrap();

        assert_eq!(result, HashSet::from(["foo --help".to_owned(),]))
    }

    #[test]
    fn test_options_extend_usage_with_params() {
        let options = Options::new(HashSet::from([
            OptionArg::WithParam {
                short: Some("-o".to_owned()),
                long: None,
                default_value: Some("./test.txt".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-q".to_owned()),
                long: Some("--quiet".to_owned()),
            },
        ]));

        let usages = HashSet::from(["foo a [options] <ARG>".to_owned()]);

        let result = options.extend_usages(usages).unwrap();
        // extract options from usage and create hashSet to compare
        // because we cannot ensure order here
        let s = result
            .iter()
            .next()
            .unwrap()
            .replace("foo a {", "")
            .replace("}... <ARG>", "");
        let result_options: HashSet<&str> = s.split('#').collect();
        assert_eq!(
            result_options,
            HashSet::from(["-o=<-o>", "--sorted", "--quiet"])
        )
    }

    #[test]
    fn test_options_extend_usage_with_params_and_or() {
        let options = Options::new(HashSet::from([
            OptionArg::WithParam {
                short: Some("-o".to_owned()),
                long: None,
                default_value: Some("./test.txt".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-q".to_owned()),
                long: Some("--quiet".to_owned()),
            },
        ]));

        let usages = HashSet::from(["foo [-o FILE] [--sorted | --quiet]".to_owned()]);

        let result = options.extend_usages(usages).unwrap();

        assert_eq!(
            result,
            HashSet::from(["foo {-o=<-o>} {--sorted#--quiet}".to_owned()])
        )
    }

    #[test]
    fn test_options_extend_usage_options() {
        let options = Options::new(HashSet::from([
            OptionArg::Simple {
                short: Some("-h".to_owned()),
                long: Some("--help".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-s".to_owned()),
                long: Some("--sorted".to_owned()),
            },
            OptionArg::Simple {
                short: Some("-q".to_owned()),
                long: Some("--quiet".to_owned()),
            },
        ]));

        let usages = HashSet::from(["foo [options] [a]".to_owned()]);

        let result = options.extend_usages(usages).unwrap();

        let s = result
            .iter()
            .next()
            .unwrap()
            .replace("foo {", "")
            .replace("}... [a]", "");
        let result_options: HashSet<&str> = s.split('#').collect();
        assert_eq!(
            result_options,
            HashSet::from(["--help", "--sorted", "--quiet"])
        );

        let usages = HashSet::from(["foo [options] (a|b)".to_owned()]);

        let result = options.extend_usages(usages).unwrap();

        let s = result
            .iter()
            .next()
            .unwrap()
            .replace("foo {", "")
            .replace("}... (a|b)", "");
        let result_options: HashSet<&str> = s.split('#').collect();
        assert_eq!(
            result_options,
            HashSet::from(["--help", "--sorted", "--quiet"])
        );
    }
}
