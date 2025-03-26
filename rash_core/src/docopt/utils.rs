use itertools::Itertools;
use regex::{Captures, Regex};

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

pub const WORDS_REGEX: &str = r"[a-z]+(?:[_\-][a-z]+)*";
pub const WORDS_UPPERCASE_REGEX: &str = r"[A-Z]+(?:[_\-][A-Z]+)*";

static RE_INNER_PARENTHESIS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(([^\(]+?)\)(\.\.\.)?").unwrap());
static RE_INNER_BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\[]+?)\](\.\.\.)?").unwrap());
static RE_INNER_CURLY_BRACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\{[^\[]+?\})(\.\.\.)?").unwrap());
static RE_REPEATABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(<{WORDS_REGEX}>|{WORDS_UPPERCASE_REGEX})\x20?(\.\.\.)"
    ))
    .unwrap()
});

#[derive(Debug, Clone, Copy)]
pub enum RegexMatch {
    InnerParenthesis,
    InnerBrackets,
    InnerCurlyBraces,
    Repeatable,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UsageCandidate {
    /// The usage string pattern being processed
    pub usage: String,
    /// Flag to control whether curly braces should be ignored during regex matching
    pub ignore_curly_braces: bool,
}

impl UsageCandidate {
    pub fn new(usage: String, ignore_curly_braces: bool) -> Self {
        Self {
            usage,
            ignore_curly_braces,
        }
    }

    pub fn from_usage(usage: String) -> Self {
        Self::new(usage, false)
    }

    pub fn calculate_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

fn get_vec_from_cap(cap: &Captures) -> Vec<String> {
    cap.iter()
        .filter_map(|option_match| Some(option_match?.as_str().to_owned()))
        .collect::<Vec<String>>()
}

pub fn expand_brackets(s: &str) -> String {
    let mut re_vec = RE_INNER_BRACKETS
        .captures_iter(s)
        .map(|cap| get_vec_from_cap(&cap))
        .filter(|cap| {
            cap[1]
                .split_whitespace()
                .filter(|x| !x.starts_with("<-"))
                .count()
                > 1
        })
        .collect::<Vec<_>>();
    re_vec.sort_by_key(|x| x[0].len());
    re_vec
        .iter()
        .find_map(|cap| {
            if cap[1].contains('|')
                || cap[1].ends_with("...")
                || RE_INNER_PARENTHESIS.captures(&cap[1]).is_some()
            {
                None
            } else {
                Some(expand_brackets(
                    &s.replacen(
                        &format!("[{}]", cap[1]),
                        &cap[1]
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .iter()
                            .circular_tuple_windows()
                            // <-o>, if - and then <, same group
                            .map(|(w, next)| {
                                if w.starts_with("<-") {
                                    format!("{w}]")
                                } else if next.starts_with("<-") {
                                    format!("[{w}")
                                } else {
                                    format!("[{w}]")
                                }
                            })
                            .collect::<Vec<String>>()
                            .join(" "),
                        1,
                    ),
                ))
            }
        })
        .unwrap_or_else(|| s.to_owned())
}

pub fn usage_regex_match(
    usage: &str,
    ignore_curly_braces: bool,
) -> Option<(RegexMatch, Vec<String>)> {
    if let Some(captures) = RE_INNER_PARENTHESIS.captures(usage) {
        return Some((RegexMatch::InnerParenthesis, get_vec_from_cap(&captures)));
    }

    if let Some(captures) = RE_INNER_BRACKETS.captures(usage) {
        return Some((RegexMatch::InnerBrackets, get_vec_from_cap(&captures)));
    }

    if let Some(captures) = RE_REPEATABLE.captures(usage) {
        return Some((RegexMatch::Repeatable, get_vec_from_cap(&captures)));
    }

    if !ignore_curly_braces {
        if let Some(captures) = RE_INNER_CURLY_BRACES.captures(usage) {
            return Some((RegexMatch::InnerCurlyBraces, get_vec_from_cap(&captures)));
        }
    }

    None
}

pub fn split_keeping_separators(text: &str, split_chars: &[char]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut last = 0;

    for (index, matched) in text.match_indices(|c: char| split_chars.contains(&c)) {
        if last != index {
            result.push(text[last..index].to_owned());
        }
        result.push(matched.to_owned());
        last = index + matched.len();
    }
    if last < text.len() {
        result.push(text[last..].to_owned());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_brackets() {
        let usage = "foo [boo fuu] [-a -b -c] [zuu -d]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(
            result,
            "foo [boo] [fuu] [-a] [-b] [-c] [zuu] [-d]".to_owned()
        )
    }

    #[test]
    fn test_expand_brackets_with_param() {
        let usage = "foo [boo fuu] [-o <-o> -a -b -c] [zuu -d]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(
            result,
            "foo [boo] [fuu] [-o <-o>] [-a] [-b] [-c] [zuu] [-d]".to_owned()
        )
    }

    #[test]
    fn test_expand_brackets_with_param_and_white_spaces() {
        let usage = "foo [ boo fuu ] [ -o <-o> -a -b -c ] [ zuu -d ]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(
            result,
            "foo [boo] [fuu] [-o <-o>] [-a] [-b] [-c] [zuu] [-d]".to_owned()
        )
    }

    #[test]
    fn test_expand_brackets_grouped() {
        let usage = "foo [(a | b)] [(a | b)]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(result, "foo [(a | b)] [(a | b)]".to_owned())
    }

    #[test]
    fn test_expand_brackets_grouped_without_parentesis() {
        let usage = "foo [a | b] [a | b]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(result, "foo [a | b] [a | b]".to_owned())
    }

    #[test]
    fn test_expand_brackets_multiple_groups() {
        let usage =
            "my_program.py [--help --sorted -o <-o>] [--quiet | --verbose] [INPUT ...]".to_owned();

        let result = expand_brackets(&usage);

        assert_eq!(
            result,
            "my_program.py [--help] [--sorted] [-o <-o>] [--quiet | --verbose] [INPUT ...]"
                .to_owned()
        )
    }

    #[test]
    fn test_split_keeping_separators() {
        let usage = "foo [boo fuu] [-o <-o> -a -b -c] [zuu -d]";

        let result: Vec<String> = usage
            .split_whitespace()
            .flat_map(|w| split_keeping_separators(w, &['[', ']']))
            .collect();

        assert_eq!(
            result,
            vec![
                "foo".to_owned(),
                "[".to_owned(),
                "boo".to_owned(),
                "fuu".to_owned(),
                "]".to_owned(),
                "[".to_owned(),
                "-o".to_owned(),
                "<-o>".to_owned(),
                "-a".to_owned(),
                "-b".to_owned(),
                "-c".to_owned(),
                "]".to_owned(),
                "[".to_owned(),
                "zuu".to_owned(),
                "-d".to_owned(),
                "]".to_owned(),
            ]
        )
    }
}
