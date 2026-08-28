use std::collections::{BTreeMap, HashMap, HashSet};

use crate::error::{Error, ErrorKind, Result};

use super::Token;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Atom {
    Command { literal: String, key: String },
    Positional { key: String },
    Option(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Expr {
    Empty,
    Atom(Atom),
    Sequence(Vec<Expr>),
    Alternative(Vec<Expr>),
    Optional(Box<Expr>),
    Repeat(Box<Expr>),
    OptionsShortcut,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Metadata {
    pub command_repeated: BTreeMap<String, bool>,
    pub positional_repeated: BTreeMap<String, bool>,
    pub repeatable_options: HashSet<usize>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Symbol {
    Command(String),
    Positional(String),
    Option(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Count {
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
}

pub(super) fn parse(tokens: Vec<Token>) -> Result<Expr> {
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_alternative()?;
    if parser.pos != parser.tokens.len() {
        return Err(parser.invalid("unexpected trailing token"));
    }
    Ok(expr)
}

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
        Expr::Sequence(items) | Expr::Alternative(items) => {
            for item in items {
                collect_explicit_options(item, out);
            }
        }
        Expr::Optional(inner) | Expr::Repeat(inner) => collect_explicit_options(inner, out),
        Expr::Empty | Expr::Atom(_) | Expr::OptionsShortcut => {}
    }
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
        Expr::Optional(inner) => occurrences(inner),
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
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Expr::Alternative(branches)
        })
    }

    fn parse_sequence(&mut self) -> Result<Expr> {
        let mut items = Vec::new();
        while let Some(token) = self.peek() {
            if matches!(token, Token::RightBracket | Token::RightParen | Token::Pipe) {
                break;
            }
            items.push(self.parse_primary()?);
        }
        Ok(match items.len() {
            0 => Expr::Empty,
            1 => items.pop().unwrap(),
            _ => Expr::Sequence(items),
        })
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
                inner
            }
            Token::LeftBracket => {
                let inner = self.parse_alternative()?;
                self.expect(Token::RightBracket)?;
                if matches!(
                    &inner,
                    Expr::Atom(Atom::Command { literal, .. }) if literal == "options"
                ) {
                    Expr::OptionsShortcut
                } else {
                    Expr::Optional(Box::new(inner))
                }
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

fn classify_atom(value: String) -> Result<Atom> {
    if value.starts_with('<') {
        let Some(name) = value.strip_prefix('<').and_then(|v| v.strip_suffix('>')) else {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid positional argument: {value}"),
            ));
        };
        if name.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Positional argument names cannot be empty",
            ));
        }
        return Ok(Atom::Positional {
            key: normalize_key(name),
        });
    }

    let has_alpha = value.chars().any(char::is_alphabetic);
    let is_uppercase_positional = has_alpha
        && value.chars().all(|c| {
            !c.is_alphabetic() || c.is_uppercase()
        })
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'));

    if is_uppercase_positional {
        Ok(Atom::Positional {
            key: normalize_key(&value.to_lowercase()),
        })
    } else {
        Ok(Atom::Command {
            key: normalize_key(&value),
            literal: value,
        })
    }
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
}
