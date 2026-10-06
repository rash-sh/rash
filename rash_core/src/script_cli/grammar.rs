use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::{Error, ErrorKind, Result};

use super::Token;

/// Leaf of a usage pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Atom {
    Command { literal: String, key: String },
    Positional { key: String },
    Option(usize),
}

/// Usage pattern AST. Its size depends only on the declaration, never on argv.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Expr {
    Empty,
    Atom(Atom),
    Sequence(Vec<Expr>),
    Alternative(Vec<Expr>),
    Optional(Box<Expr>),
    /// Parenthesized group.
    Required(Box<Expr>),
    /// `expr...`: one or more occurrences.
    Repeat(Box<Expr>),
    /// Adjacent optional options matched in any order, each up to its per-pattern limit.
    OptionsGroup(Vec<usize>),
    /// `[options]`: every option that no usage pattern references explicitly.
    OptionsShortcut,
}

/// Multiplicity of every symbol across all usage patterns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Metadata {
    /// Commands, and whether any pattern can match them more than once.
    pub(super) command_repeated: BTreeMap<String, bool>,
    /// Positionals, and whether any pattern can match them more than once.
    pub(super) positional_repeated: BTreeMap<String, bool>,
    /// Options that some pattern can match more than once.
    pub(super) repeatable_options: HashSet<usize>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Symbol {
    Command(String),
    Positional(String),
    Option(usize),
}

/// Maximum number of occurrences of a symbol in one match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Count {
    Finite(usize),
    Unbounded,
}

impl Count {
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unbounded, _) | (_, Self::Unbounded) => Self::Unbounded,
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a.saturating_add(b)),
        }
    }

    fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unbounded, _) | (_, Self::Unbounded) => Self::Unbounded,
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a.max(b)),
        }
    }

    fn repeated(self) -> bool {
        matches!(self, Self::Unbounded | Self::Finite(2..))
    }

    fn present(self) -> bool {
        !matches!(self, Self::Finite(0))
    }

    /// Whether one more occurrence fits after `seen` occurrences were already consumed.
    pub(super) fn allows_another(self, seen: usize) -> bool {
        match self {
            Self::Unbounded => true,
            Self::Finite(limit) => seen < limit,
        }
    }
}

/// Parse the tokens of one usage pattern, without its program name.
pub(super) fn parse(tokens: Vec<Token>) -> Result<Expr> {
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_alternative()?;
    if parser.pos != parser.tokens.len() {
        return Err(parser.invalid("unexpected trailing token"));
    }
    Ok(normalize_option_groups(expr))
}

/// Compute the symbol multiplicity of all patterns: the maximum over patterns of the maximum
/// occurrences within each pattern.
pub(super) fn analyze(patterns: &[Expr]) -> Metadata {
    let mut total = HashMap::<Symbol, Count>::new();

    for pattern in patterns {
        merge_max(&mut total, occurrences(pattern));
    }

    let mut metadata = Metadata::default();
    for (symbol, count) in total {
        match symbol {
            Symbol::Command(key) => {
                metadata.command_repeated.insert(key, count.repeated());
            }
            Symbol::Positional(key) => {
                metadata.positional_repeated.insert(key, count.repeated());
            }
            Symbol::Option(id) if count.repeated() => {
                metadata.repeatable_options.insert(id);
            }
            Symbol::Option(_) => {}
        }
    }
    metadata
}

/// Options referenced explicitly in `expr`, which `[options]` excludes.
pub(super) fn explicit_options(expr: &Expr) -> HashSet<usize> {
    let mut out = HashSet::new();
    collect_explicit_options(expr, &mut out);
    out
}

fn collect_explicit_options(expr: &Expr, out: &mut HashSet<usize>) {
    match expr {
        Expr::Atom(Atom::Option(id)) => {
            out.insert(*id);
        }
        Expr::OptionsGroup(ids) => out.extend(ids.iter().copied()),
        Expr::Sequence(items) | Expr::Alternative(items) => {
            for item in items {
                collect_explicit_options(item, out);
            }
        }
        Expr::Optional(inner) | Expr::Required(inner) | Expr::Repeat(inner) => {
            collect_explicit_options(inner, out)
        }
        Expr::Empty | Expr::Atom(_) | Expr::OptionsShortcut => {}
    }
}

/// Maximum number of times each option may occur in a single match of `pattern`.
pub(super) fn option_limits(pattern: &Expr) -> HashMap<usize, Count> {
    occurrences(pattern)
        .into_iter()
        .filter_map(|(symbol, count)| match symbol {
            Symbol::Option(id) => Some((id, count)),
            Symbol::Command(_) | Symbol::Positional(_) => None,
        })
        .collect()
}

fn occurrences(expr: &Expr) -> HashMap<Symbol, Count> {
    match expr {
        Expr::Empty | Expr::OptionsShortcut => HashMap::new(),
        Expr::Atom(atom) => {
            let symbol = match atom {
                Atom::Command { key, .. } => Symbol::Command(key.clone()),
                Atom::Positional { key } => Symbol::Positional(key.clone()),
                Atom::Option(id) => Symbol::Option(*id),
            };
            HashMap::from([(symbol, Count::Finite(1))])
        }
        Expr::OptionsGroup(ids) => {
            let mut out = HashMap::new();
            for id in ids {
                merge_add(
                    &mut out,
                    HashMap::from([(Symbol::Option(*id), Count::Finite(1))]),
                );
            }
            out
        }
        Expr::Sequence(items) => {
            let mut out = HashMap::new();
            for item in items {
                merge_add(&mut out, occurrences(item));
            }
            out
        }
        Expr::Alternative(items) => {
            let mut out = HashMap::new();
            for item in items {
                merge_max(&mut out, occurrences(item));
            }
            out
        }
        Expr::Optional(inner) | Expr::Required(inner) => occurrences(inner),
        Expr::Repeat(inner) => occurrences(inner)
            .into_iter()
            .map(|(symbol, count)| {
                let count = if count.present() {
                    Count::Unbounded
                } else {
                    Count::Finite(0)
                };
                (symbol, count)
            })
            .collect(),
    }
}

fn merge_add(target: &mut HashMap<Symbol, Count>, source: HashMap<Symbol, Count>) {
    for (symbol, count) in source {
        target
            .entry(symbol)
            .and_modify(|current| *current = current.add(count))
            .or_insert(count);
    }
}

fn merge_max(target: &mut HashMap<Symbol, Count>, source: HashMap<Symbol, Count>) {
    for (symbol, count) in source {
        target
            .entry(symbol)
            .and_modify(|current| *current = current.max(count))
            .or_insert(count);
    }
}

/// Merge runs of adjacent optional options into unordered option groups.
fn normalize_option_groups(expr: Expr) -> Expr {
    match expr {
        Expr::Sequence(items) => normalize_sequence(items),
        Expr::Alternative(items) => {
            Expr::Alternative(items.into_iter().map(normalize_option_groups).collect())
        }
        Expr::Optional(inner) => Expr::Optional(Box::new(normalize_option_groups(*inner))),
        Expr::Required(inner) => Expr::Required(Box::new(normalize_option_groups(*inner))),
        Expr::Repeat(inner) => Expr::Repeat(Box::new(normalize_option_groups(*inner))),
        other => other,
    }
}

fn normalize_sequence(items: Vec<Expr>) -> Expr {
    let mut out = Vec::with_capacity(items.len());
    let mut option_run = Vec::new();

    for item in items.into_iter().map(normalize_option_groups) {
        if let Expr::Optional(inner) = &item
            && let Expr::Atom(Atom::Option(id)) = inner.as_ref()
        {
            option_run.push(*id);
            continue;
        }
        flush_option_run(&mut out, &mut option_run);
        out.push(item);
    }
    flush_option_run(&mut out, &mut option_run);

    single_or(out, Expr::Sequence)
}

fn flush_option_run(out: &mut Vec<Expr>, run: &mut Vec<usize>) {
    match run.as_slice() {
        [] => {}
        [id] => out.push(Expr::Optional(Box::new(Expr::Atom(Atom::Option(*id))))),
        _ => out.push(Expr::OptionsGroup(std::mem::take(run))),
    }
    run.clear();
}

/// `Empty` for no items, the item itself for one, and `many(items)` otherwise.
fn single_or(items: Vec<Expr>, many: fn(Vec<Expr>) -> Expr) -> Expr {
    match <[Expr; 1]>::try_from(items) {
        Ok([item]) => item,
        Err(items) if items.is_empty() => Expr::Empty,
        Err(items) => many(items),
    }
}

fn contains_nested_optional(expr: &Expr) -> bool {
    match expr {
        Expr::Optional(_) | Expr::OptionsShortcut => true,
        Expr::Sequence(items) | Expr::Alternative(items) => {
            items.iter().any(contains_nested_optional)
        }
        Expr::Required(inner) | Expr::Repeat(inner) => contains_nested_optional(inner),
        Expr::Empty | Expr::Atom(_) | Expr::OptionsGroup(_) => false,
    }
}

/// `[inner]`: `[options]` is the options shortcut, a flat sequence makes every element
/// independently optional, and anything else (including a sequence with a nested optional) is
/// optional as a whole.
fn bracketed(inner: Expr) -> Expr {
    match inner {
        Expr::Atom(Atom::Command { literal, .. }) if literal == "options" => Expr::OptionsShortcut,
        Expr::Sequence(items) if !items.iter().any(contains_nested_optional) => Expr::Sequence(
            items
                .into_iter()
                .map(|item| Expr::Optional(Box::new(item)))
                .collect(),
        ),
        inner => Expr::Optional(Box::new(inner)),
    }
}

/// Recursive descent parser over the tokens of one usage pattern.
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_alternative(&mut self) -> Result<Expr> {
        let mut branches = vec![self.parse_sequence()?];
        while self.consume_if(&Token::Pipe) {
            branches.push(self.parse_sequence()?);
        }
        Ok(single_or(branches, Expr::Alternative))
    }

    fn parse_sequence(&mut self) -> Result<Expr> {
        let mut items = Vec::new();
        while let Some(token) = self.peek() {
            if matches!(token, Token::RightBracket | Token::RightParen | Token::Pipe) {
                break;
            }
            items.push(self.parse_primary()?);
        }
        Ok(single_or(items, Expr::Sequence))
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self
            .next()
            .cloned()
            .ok_or_else(|| self.invalid("unexpected end of usage"))?;

        let mut expr = match token {
            Token::LeftParen => {
                let inner = self.parse_alternative()?;
                self.expect(Token::RightParen)?;
                Expr::Required(Box::new(inner))
            }
            Token::LeftBracket => {
                let inner = self.parse_alternative()?;
                self.expect(Token::RightBracket)?;
                bracketed(inner)
            }
            Token::Atom(value) => Expr::Atom(classify_atom(value)?),
            Token::Option(id) => Expr::Atom(Atom::Option(id)),
            Token::Ellipsis => return Err(self.invalid("ellipsis has no preceding expression")),
            Token::RightBracket | Token::RightParen | Token::Pipe => {
                return Err(self.invalid("unexpected delimiter"));
            }
        };

        if self.consume_if(&Token::Ellipsis) {
            expr = Expr::Repeat(Box::new(expr));
            if self.consume_if(&Token::Ellipsis) {
                return Err(self.invalid("duplicate ellipsis"));
            }
        }
        Ok(expr)
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        if self.consume_if(&expected) {
            Ok(())
        } else {
            Err(self.invalid(&format!("expected {expected:?}")))
        }
    }

    fn consume_if(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn invalid(&self, message: &str) -> Error {
        Error::new(
            ErrorKind::InvalidData,
            format!("Invalid usage grammar at token {}: {message}", self.pos),
        )
    }
}

/// `<name>` and `NAME` are positionals and `name` is a command; names are ASCII words joined by
/// `-` or `_`.
fn classify_atom(value: String) -> Result<Atom> {
    if value.starts_with('<') {
        let Some(name) = value.strip_prefix('<').and_then(|v| v.strip_suffix('>')) else {
            return Err(invalid_atom(&value));
        };
        if !is_lower_word(name) {
            return Err(invalid_atom(&value));
        }
        return Ok(Atom::Positional {
            key: normalize_key(name),
        });
    }

    if is_upper_word(&value) {
        Ok(Atom::Positional {
            key: normalize_key(&value.to_lowercase()),
        })
    } else if is_lower_word(&value) {
        Ok(Atom::Command {
            key: normalize_key(&value),
            literal: value,
        })
    } else {
        Err(invalid_atom(&value))
    }
}

fn invalid_atom(value: &str) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("Invalid usage identifier: {value}"),
    )
}

fn is_lower_word(value: &str) -> bool {
    is_word(value, u8::is_ascii_lowercase)
}

fn is_upper_word(value: &str) -> bool {
    is_word(value, u8::is_ascii_uppercase)
}

fn is_word(value: &str, predicate: fn(&u8) -> bool) -> bool {
    if value.is_empty() {
        return false;
    }

    value
        .split(['_', '-'])
        .all(|segment| !segment.is_empty() && segment.as_bytes().iter().all(predicate))
}

fn normalize_key(value: &str) -> String {
    value.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_usage() {
        let tokens = vec![
            Token::Atom("ship".into()),
            Token::LeftParen,
            Token::Atom("new".into()),
            Token::Pipe,
            Token::Atom("move".into()),
            Token::RightParen,
            Token::LeftBracket,
            Token::Atom("FILE".into()),
            Token::RightBracket,
            Token::Ellipsis,
        ];

        let parsed = parse(tokens).unwrap();
        assert!(matches!(parsed, Expr::Sequence(_)));
    }

    #[test]
    fn metadata_tracks_repetition_without_expansion() {
        let expr = Expr::Sequence(vec![
            Expr::Atom(Atom::Command {
                literal: "copy".into(),
                key: "copy".into(),
            }),
            Expr::Repeat(Box::new(Expr::Atom(Atom::Positional {
                key: "source".into(),
            }))),
        ]);

        let metadata = analyze(&[expr]);
        assert_eq!(metadata.command_repeated.get("copy"), Some(&false));
        assert_eq!(metadata.positional_repeated.get("source"), Some(&true));
    }

    #[test]
    fn adjacent_optional_options_become_unordered_group() {
        let expr = normalize_option_groups(Expr::Sequence(vec![
            Expr::Optional(Box::new(Expr::Atom(Atom::Option(1)))),
            Expr::Optional(Box::new(Expr::Atom(Atom::Option(2)))),
        ]));
        assert_eq!(expr, Expr::OptionsGroup(vec![1, 2]));
    }

    #[test]
    fn duplicated_option_in_group_is_counted_per_occurrence() {
        let expr = Expr::OptionsGroup(vec![1, 1, 2]);
        let limits = option_limits(&expr);
        assert_eq!(limits.get(&1), Some(&Count::Finite(2)));
        assert_eq!(limits.get(&2), Some(&Count::Finite(1)));
        assert!(analyze(&[expr]).repeatable_options.contains(&1));
    }

    #[test]
    fn flat_bracket_sequence_remains_independently_optional() {
        let parsed = parse(vec![
            Token::LeftBracket,
            Token::Atom("alpha".into()),
            Token::Atom("beta".into()),
            Token::RightBracket,
        ])
        .unwrap();
        assert!(matches!(parsed, Expr::Sequence(_)));
    }

    #[test]
    fn nested_optional_keeps_outer_dependency() {
        let parsed = parse(vec![
            Token::LeftBracket,
            Token::Atom("command".into()),
            Token::LeftBracket,
            Token::Option(0),
            Token::RightBracket,
            Token::RightBracket,
        ])
        .unwrap();
        assert!(matches!(parsed, Expr::Optional(inner) if matches!(*inner, Expr::Sequence(_))));
    }

    #[test]
    fn identifier_grammar_matches_legacy_ascii_words() {
        assert!(is_lower_word("daemon-reload"));
        assert!(is_lower_word("package_filters"));
        assert!(is_upper_word("UNIT-NAME"));
        assert!(!is_lower_word("run2"));
        assert!(!is_lower_word("Run"));
        assert!(!is_upper_word("FILE2"));
        assert!(!is_upper_word("File"));
    }
}
