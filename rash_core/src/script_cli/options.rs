use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::error::{Error, ErrorKind, Result};

use super::{InputToken, Token};

static RE_DEFAULT_VALUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[default: (.*)\]").unwrap());

#[derive(Clone, Debug, PartialEq, Eq)]
struct OptionSpec {
    short: Option<String>,
    long: Option<String>,
    takes_value: bool,
    default_value: Option<String>,
    repeatable: bool,
}

impl OptionSpec {
    fn key(&self) -> String {
        self.preferred_name()
            .trim_start_matches('-')
            .replace('-', "_")
    }

    fn preferred_name(&self) -> &str {
        self.long
            .as_deref()
            .or(self.short.as_deref())
            .expect("option must have at least one name")
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct OptionRegistry {
    specs: Vec<OptionSpec>,
    aliases: HashMap<String, usize>,
}

impl OptionRegistry {
    pub fn from_doc(help: &str, usages: &[String]) -> Result<Self> {
        let mut registry = Self::default();

        for line in help.lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('-') {
                continue;
            }
            registry.add_description_line(trimmed)?;
        }

        for usage in usages {
            registry.discover_usage_options(usage)?;
        }

        Ok(registry)
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    pub fn is_help(&self, id: usize) -> bool {
        self.specs.get(id).is_some_and(|spec| spec.key() == "help")
    }

    pub fn set_repeatable(&mut self, ids: &HashSet<usize>) -> Result<()> {
        for id in ids {
            let Some(spec) = self.specs.get_mut(*id) else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unknown option id {id}"),
                ));
            };
            if spec.takes_value {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "Repeatable options with values are not supported: {}",
                        spec.preferred_name()
                    ),
                ));
            }
            spec.repeatable = true;
        }
        Ok(())
    }

    pub fn tokenize_usage(&self, usage: &str) -> Result<Vec<Token>> {
        let mut tokens = lex_usage(usage)?;
        if !matches!(tokens.first(), Some(Token::Atom(_))) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Usage must start with a program name: {usage}"),
            ));
        }
        tokens.remove(0);

        let mut out = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Atom(atom) if atom.starts_with('-') => {
                    let (option_ids, consume_next) = self.expand_usage_option(atom)?;
                    out.extend(option_ids.into_iter().map(Token::Option));
                    if consume_next {
                        let Some(Token::Atom(_)) = tokens.get(i + 1) else {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                format!("Option {atom} requires a value placeholder in usage"),
                            ));
                        };
                        i += 1;
                    }
                }
                token => out.push(token.clone()),
            }
            i += 1;
        }
        Ok(out)
    }

    pub fn normalize_args(&self, args: &[&str]) -> Result<Vec<InputToken>> {
        let mut out = Vec::with_capacity(args.len());
        let mut i = 0;

        while i < args.len() {
            let arg = args[i];
            if arg == "-" {
                out.push(InputToken::Word(arg.to_owned()));
                i += 1;
                continue;
            }

            if arg.starts_with("--") {
                if arg == "--" {
                    return Err(Error::new(ErrorKind::InvalidData, "Unknown option: --"));
                }
                let (name, attached) = match arg.split_once('=') {
                    Some((name, value)) => (name, Some(value.to_owned())),
                    None => (arg, None),
                };
                let id = self.find(name).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, format!("Unknown option: {name}"))
                })?;
                let spec = &self.specs[id];
                let value = if spec.takes_value {
                    match attached {
                        Some(value) => Some(value),
                        None => {
                            i += 1;
                            Some(
                                args.get(i)
                                    .ok_or_else(|| {
                                        Error::new(
                                            ErrorKind::InvalidData,
                                            format!("Option {name} requires a value"),
                                        )
                                    })?
                                    .to_string(),
                            )
                        }
                    }
                } else {
                    if attached.is_some() {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Option {name} does not take a value"),
                        ));
                    }
                    None
                };
                self.push_option(&mut out, id, value);
                i += 1;
                continue;
            }

            if arg.starts_with('-') {
                let body = &arg[1..];
                if body.is_empty() {
                    out.push(InputToken::Word(arg.to_owned()));
                    i += 1;
                    continue;
                }

                for (offset, ch) in body.char_indices() {
                    if ch == '=' {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Invalid short option cluster: {arg}"),
                        ));
                    }
                    let alias = format!("-{ch}");
                    let id = self.find(&alias).ok_or_else(|| {
                        Error::new(ErrorKind::InvalidData, format!("Unknown option: {alias}"))
                    })?;
                    let spec = &self.specs[id];
                    let next_offset = offset + ch.len_utf8();
                    let rest = &body[next_offset..];

                    if spec.takes_value {
                        let value = if !rest.is_empty() {
                            Some(rest.strip_prefix('=').unwrap_or(rest).to_owned())
                        } else {
                            i += 1;
                            Some(
                                args.get(i)
                                    .ok_or_else(|| {
                                        Error::new(
                                            ErrorKind::InvalidData,
                                            format!("Option {alias} requires a value"),
                                        )
                                    })?
                                    .to_string(),
                            )
                        };
                        self.push_option(&mut out, id, value);
                        break;
                    }

                    self.push_option(&mut out, id, None);
                }

                i += 1;
                continue;
            }

            out.push(InputToken::Word(arg.to_owned()));
            i += 1;
        }

        Ok(out)
    }

    pub fn initial_options(&self) -> Map<String, Value> {
        self.specs
            .iter()
            .map(|spec| {
                let value = if spec.takes_value {
                    spec.default_value
                        .as_ref()
                        .map_or(Value::Null, |value| Value::String(value.clone()))
                } else if spec.repeatable {
                    Value::from(0_u64)
                } else {
                    Value::Bool(false)
                };
                (spec.key(), value)
            })
            .collect()
    }

    pub fn apply_capture(
        &self,
        options: &mut Map<String, Value>,
        id: usize,
        value: Option<&str>,
    ) -> Result<()> {
        let spec = self
            .specs
            .get(id)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Unknown option id {id}")))?;
        let key = spec.key();
        if spec.takes_value {
            options.insert(
                key,
                Value::String(
                    value
                        .ok_or_else(|| {
                            Error::new(
                                ErrorKind::InvalidData,
                                format!("Option {} requires a value", spec.preferred_name()),
                            )
                        })?
                        .to_owned(),
                ),
            );
        } else if spec.repeatable {
            let count = options
                .get(&key)
                .and_then(Value::as_u64)
                .unwrap_or_default()
                + 1;
            options.insert(key, Value::from(count));
        } else {
            options.insert(key, Value::Bool(true));
        }
        Ok(())
    }

    pub fn all_ids(&self) -> impl Iterator<Item = usize> + '_ {
        0..self.specs.len()
    }

    fn push_option(&self, out: &mut Vec<InputToken>, id: usize, value: Option<String>) {
        out.push(InputToken::Option { id, value });
    }

    fn find(&self, alias: &str) -> Option<usize> {
        self.aliases.get(alias).copied()
    }

    fn add_description_line(&mut self, line: &str) -> Result<()> {
        let (declaration, description) = line.split_once("  ").unwrap_or((line, ""));
        let declaration = declaration.replace(',', " ");
        let mut short = None;
        let mut long = None;
        let mut takes_value = false;

        for word in declaration.split_whitespace() {
            if word.starts_with("--") {
                let (name, has_value) = split_option_declaration(word);
                long = Some(name);
                takes_value |= has_value;
            } else if word.starts_with('-') {
                let (name, has_value) = split_option_declaration(word);
                short = Some(name);
                takes_value |= has_value;
            } else {
                takes_value = true;
            }
        }

        if short.is_none() && long.is_none() {
            return Ok(());
        }

        let default_value = RE_DEFAULT_VALUE
            .captures(description)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_owned());

        self.upsert(OptionSpec {
            short,
            long,
            takes_value,
            default_value,
            repeatable: false,
        })?;
        Ok(())
    }

    fn discover_usage_options(&mut self, usage: &str) -> Result<()> {
        let words = usage
            .replace(['[', ']', '(', ')', '|'], " ")
            .split_whitespace()
            .map(|word| word.strip_suffix("...").unwrap_or(word).to_owned())
            .collect::<Vec<_>>();

        for word in words {
            if word.starts_with("--") && word != "--" {
                let (name, has_value) = split_option_declaration(&word);
                self.upsert(OptionSpec {
                    short: None,
                    long: Some(name),
                    takes_value: has_value,
                    default_value: None,
                    repeatable: false,
                })?;
            } else if word.starts_with('-') && word != "-" {
                self.discover_short_cluster(&word)?;
            }
        }
        Ok(())
    }

    fn discover_short_cluster(&mut self, word: &str) -> Result<()> {
        let body = &word[1..];
        for (offset, ch) in body.char_indices() {
            if ch == '=' {
                break;
            }
            let alias = format!("-{ch}");
            let next_offset = offset + ch.len_utf8();
            let rest = &body[next_offset..];

            if let Some(id) = self.find(&alias) {
                if self.specs[id].takes_value {
                    break;
                }
                continue;
            }

            let takes_value = rest.starts_with('=');
            self.upsert(OptionSpec {
                short: Some(alias),
                long: None,
                takes_value,
                default_value: None,
                repeatable: false,
            })?;
            if takes_value {
                break;
            }
        }
        Ok(())
    }

    fn expand_usage_option(&self, atom: &str) -> Result<(Vec<usize>, bool)> {
        if atom.starts_with("--") {
            let name = atom.split_once('=').map_or(atom, |(name, _)| name);
            let id = self.find(name).ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, format!("Unknown option: {name}"))
            })?;
            let spec = &self.specs[id];
            return Ok((vec![id], spec.takes_value && !atom.contains('=')));
        }

        let body = atom.strip_prefix('-').unwrap_or(atom);
        let mut ids = Vec::new();
        let mut consume_next = false;
        for (offset, ch) in body.char_indices() {
            if ch == '=' {
                break;
            }
            let alias = format!("-{ch}");
            let id = self.find(&alias).ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, format!("Unknown option: {alias}"))
            })?;
            ids.push(id);
            if self.specs[id].takes_value {
                let next_offset = offset + ch.len_utf8();
                consume_next = body[next_offset..].is_empty();
                break;
            }
        }
        if ids.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid option in usage: {atom}"),
            ));
        }
        Ok((ids, consume_next))
    }

    fn upsert(&mut self, incoming: OptionSpec) -> Result<usize> {
        let existing = incoming
            .short
            .as_ref()
            .and_then(|alias| self.find(alias))
            .or_else(|| incoming.long.as_ref().and_then(|alias| self.find(alias)));

        if let Some(id) = existing {
            let spec = &mut self.specs[id];
            if let (Some(a), Some(b)) = (&spec.short, &incoming.short)
                && a != b
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Conflicting short option aliases: {a} and {b}"),
                ));
            }
            if let (Some(a), Some(b)) = (&spec.long, &incoming.long)
                && a != b
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Conflicting long option aliases: {a} and {b}"),
                ));
            }
            if let (Some(a), Some(b)) = (&spec.default_value, &incoming.default_value)
                && a != b
            {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Conflicting defaults for option {}", spec.preferred_name()),
                ));
            }

            if spec.short.is_none() {
                spec.short = incoming.short.clone();
            }
            if spec.long.is_none() {
                spec.long = incoming.long.clone();
            }
            spec.takes_value |= incoming.takes_value;
            if spec.default_value.is_none() {
                spec.default_value = incoming.default_value.clone();
            }
            if let Some(alias) = &spec.short {
                self.aliases.insert(alias.clone(), id);
            }
            if let Some(alias) = &spec.long {
                self.aliases.insert(alias.clone(), id);
            }
            return Ok(id);
        }

        let id = self.specs.len();
        if let Some(alias) = &incoming.short {
            if self.aliases.insert(alias.clone(), id).is_some() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Duplicate option alias: {alias}"),
                ));
            }
        }
        if let Some(alias) = &incoming.long {
            if self.aliases.insert(alias.clone(), id).is_some() {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Duplicate option alias: {alias}"),
                ));
            }
        }
        self.specs.push(incoming);
        Ok(id)
    }
}

fn split_option_declaration(value: &str) -> (String, bool) {
    match value.split_once('=') {
        Some((name, _)) => (name.to_owned(), true),
        None => (value.to_owned(), false),
    }
}

fn lex_usage(usage: &str) -> Result<Vec<Token>> {
    let chars = usage.char_indices().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    let flush = |current: &mut String, tokens: &mut Vec<Token>| {
        if !current.is_empty() {
            tokens.push(Token::Atom(std::mem::take(current)));
        }
    };

    while i < chars.len() {
        let (_, ch) = chars[i];
        if ch.is_whitespace() {
            flush(&mut current, &mut tokens);
            i += 1;
            continue;
        }
        if matches!(ch, '[' | ']' | '(' | ')' | '|') {
            flush(&mut current, &mut tokens);
            tokens.push(match ch {
                '[' => Token::LeftBracket,
                ']' => Token::RightBracket,
                '(' => Token::LeftParen,
                ')' => Token::RightParen,
                '|' => Token::Pipe,
                _ => unreachable!(),
            });
            i += 1;
            continue;
        }
        if ch == '.' && i + 2 < chars.len() && chars[i + 1].1 == '.' && chars[i + 2].1 == '.' {
            flush(&mut current, &mut tokens);
            tokens.push(Token::Ellipsis);
            i += 3;
            continue;
        }
        current.push(ch);
        i += 1;
    }
    flush(&mut current, &mut tokens);

    if tokens.is_empty() {
        return Err(Error::new(ErrorKind::InvalidData, "Empty usage pattern"));
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_short_cluster_with_value() {
        let help =
            "Usage: tool [-vfo FILE]\n\n-v --verbose  verbose\n-f --force  force\n-o FILE  output";
        let usages = vec!["tool [-vfo FILE]".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        let tokens = registry.tokenize_usage(&usages[0]).unwrap();
        assert!(matches!(tokens[1], Token::Option(_)));
    }

    #[test]
    fn normalizes_runtime_options() {
        let help = "Usage: tool [options] <file>\n\n-v --verbose  verbose\n-o FILE  output";
        let usages = vec!["tool [options] <file>".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        let input = registry.normalize_args(&["-v", "-oout", "file"]).unwrap();
        assert_eq!(input.len(), 3);
    }

    #[test]
    fn repeated_simple_options_are_left_to_the_grammar() {
        let help = "Usage: tool [options]\n\n-v --verbose  verbose";
        let usages = vec!["tool [options]".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        assert_eq!(registry.normalize_args(&["-vv"]).unwrap().len(), 2);
    }
}
