use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::error::{Error, ErrorKind, Result};

use super::{InputToken, Token};

/// One logical option: its aliases and value semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
struct OptionSpec {
    short: Option<String>,
    long: Option<String>,
    takes_value: bool,
    default_value: Option<String>,
    repeatable: bool,
}

impl OptionSpec {
    /// Output key: the preferred name without dashes, with `-` replaced by `_`.
    fn key(&self) -> String {
        self.preferred_name()
            .trim_start_matches('-')
            .replace('-', "_")
    }

    /// Long alias if any, else the short one. Specs are only created with at least one alias.
    fn preferred_name(&self) -> &str {
        self.long
            .as_deref()
            .or(self.short.as_deref())
            .unwrap_or_default()
    }
}

/// All options of a declaration, discovered from option descriptions and usage patterns.
///
/// An alias shared by several options is ambiguous: it is kept out of alias resolution, and the
/// options remain reachable through their other aliases.
#[derive(Debug, Default)]
pub(super) struct OptionRegistry {
    specs: Vec<OptionSpec>,
    aliases: HashMap<String, usize>,
    ambiguous_aliases: HashSet<String>,
    /// Whether a usage pattern declares the `--` separator command.
    separator: bool,
}

impl OptionRegistry {
    /// Register the options described in `help` (lines starting with `-`), then the options
    /// that only appear in `usages`.
    pub(super) fn from_doc(help: &str, usages: &[String]) -> Result<Self> {
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

    pub(super) fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Whether option `id` requests help: a flag (taking no value) whose aliases are `--help`
    /// and/or `-h`, as in docopt. `-h` is not a help option when it is an alias of another long
    /// option, such as `-h, --human`.
    pub(super) fn is_help(&self, id: usize) -> bool {
        self.specs.get(id).is_some_and(|spec| {
            !spec.takes_value
                && match spec.long.as_deref() {
                    Some(long) => long == "--help",
                    None => spec.short.as_deref() == Some("-h"),
                }
        })
    }

    /// Mark flags that can occur more than once as counters. Value options keep scalar values.
    pub(super) fn set_repeatable(&mut self, ids: &HashSet<usize>) -> Result<()> {
        for id in ids {
            let Some(spec) = self.specs.get_mut(*id) else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Unknown option id {id}"),
                ));
            };
            if !spec.takes_value {
                spec.repeatable = true;
            }
        }
        Ok(())
    }

    /// Tokenize a usage pattern, dropping its program name and resolving options to ids. A value
    /// placeholder after an option that takes a separate value is dropped too.
    pub(super) fn tokenize_usage(&self, usage: &str) -> Result<Vec<Token>> {
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
                Token::Atom(atom) if atom.starts_with('-') && !is_dash_command(atom) => {
                    let (option_ids, takes_separate_value) = self.expand_usage_option(atom)?;
                    out.extend(option_ids.into_iter().map(Token::Option));
                    if takes_separate_value && is_usage_value_placeholder(tokens.get(i + 1)) {
                        i += 1;
                    }
                }
                token => out.push(token.clone()),
            }
            i += 1;
        }
        Ok(out)
    }

    /// Normalize argv: resolve option aliases, split short clusters and attach option values.
    ///
    /// The first `--` argument ends the options: every argument after it is a word, even if it
    /// starts with `-`. When the usage declares `--`, that `--` is a word matching the command;
    /// otherwise it is dropped.
    pub(super) fn normalize_args(&self, args: &[&str]) -> Result<Vec<InputToken>> {
        let mut out = Vec::with_capacity(args.len());
        let mut args = args.iter().copied();

        while let Some(arg) = args.next() {
            if arg == "--" {
                let separator = self.separator.then_some(arg);
                out.extend(
                    separator
                        .into_iter()
                        .chain(args.by_ref())
                        .map(|word| InputToken::Word(word.to_owned())),
                );
            } else if arg.starts_with("--") {
                out.push(self.normalize_long(arg, &mut args)?);
            } else if let Some(cluster) = arg.strip_prefix('-').filter(|body| !body.is_empty()) {
                self.normalize_short_cluster(arg, cluster, &mut args, &mut out)?;
            } else {
                out.push(InputToken::Word(arg.to_owned()));
            }
        }

        Ok(out)
    }

    /// `--name`, `--name=value`, or `--name value` for options taking a value.
    fn normalize_long<'a>(
        &self,
        arg: &str,
        rest: &mut impl Iterator<Item = &'a str>,
    ) -> Result<InputToken> {
        let (name, attached) = match arg.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (arg, None),
        };
        let id = self.resolve(name)?;
        let value = match (self.specs[id].takes_value, attached) {
            (true, Some(value)) => Some(value.to_owned()),
            (true, None) => Some(next_value(name, rest)?),
            (false, Some(_)) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Option {name} does not take a value"),
                ));
            }
            (false, None) => None,
        };
        Ok(InputToken::Option { id, value })
    }

    /// `-abc` is `-a -b -c`; an option taking a value consumes the rest of the cluster (without a
    /// leading `=`) or, if nothing is left, the next argument.
    fn normalize_short_cluster<'a>(
        &self,
        arg: &str,
        cluster: &str,
        rest: &mut impl Iterator<Item = &'a str>,
        out: &mut Vec<InputToken>,
    ) -> Result<()> {
        for (offset, ch) in cluster.char_indices() {
            if ch == '=' {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid short option cluster: {arg}"),
                ));
            }
            let alias = format!("-{ch}");
            let id = self.resolve(&alias)?;
            if !self.specs[id].takes_value {
                out.push(InputToken::Option { id, value: None });
                continue;
            }

            let attached = &cluster[offset + ch.len_utf8()..];
            let value = if attached.is_empty() {
                next_value(&alias, rest)?
            } else {
                attached.strip_prefix('=').unwrap_or(attached).to_owned()
            };
            out.push(InputToken::Option {
                id,
                value: Some(value),
            });
            break;
        }
        Ok(())
    }

    /// Value of every option before matching: the default (or `null`) for value options, `0`
    /// for counters and `false` for flags.
    pub(super) fn initial_options(&self) -> Map<String, Value> {
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

    /// Record one matched occurrence of option `id`.
    pub(super) fn apply_capture(
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

    pub(super) fn all_ids(&self) -> impl Iterator<Item = usize> + '_ {
        0..self.specs.len()
    }

    /// Option of an unambiguous alias.
    fn find(&self, alias: &str) -> Option<usize> {
        if self.ambiguous_aliases.contains(alias) {
            None
        } else {
            self.aliases.get(alias).copied()
        }
    }

    /// Like [`Self::find`], failing for unknown and ambiguous aliases.
    fn resolve(&self, alias: &str) -> Result<usize> {
        if self.ambiguous_aliases.contains(alias) {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Ambiguous option alias: {alias}"),
            ));
        }
        self.aliases
            .get(alias)
            .copied()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Unknown option: {alias}")))
    }

    /// Register an option description such as `-o, --output=FILE  text [default: out]`. Any
    /// non-option word in the declaration part means the option takes a value. A line with an
    /// option word without a name, such as the Markdown bullet `- note`, is not an option.
    fn add_description_line(&mut self, line: &str) -> Result<()> {
        let (declaration, description) = line.split_once("  ").unwrap_or((line, ""));
        let declaration = declaration.replace(',', " ");
        let mut short = None;
        let mut long = None;
        let mut takes_value = false;

        for word in declaration.split_whitespace() {
            if word.starts_with('-') && !has_option_name(word) {
                return Ok(());
            }
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

        let default_value = default_value(description).map(str::to_owned);

        self.upsert(OptionSpec {
            short,
            long,
            takes_value,
            default_value,
            repeatable: false,
        })?;
        Ok(())
    }

    /// Register options that only appear in a usage pattern.
    fn discover_usage_options(&mut self, usage: &str) -> Result<()> {
        let words = usage
            .replace(['[', ']', '(', ')', '|'], " ")
            .split_whitespace()
            .map(|word| word.strip_suffix("...").unwrap_or(word).to_owned())
            .collect::<Vec<_>>();

        for word in words {
            match word.as_str() {
                "--" => self.separator = true,
                "-" => {}
                long if long.starts_with("--") => {
                    let (name, has_value) = split_option_declaration(long);
                    self.upsert(OptionSpec {
                        short: None,
                        long: Some(name),
                        takes_value: has_value,
                        default_value: None,
                        repeatable: false,
                    })?;
                }
                short if short.starts_with('-') => self.discover_short_cluster(short)?,
                _ => {}
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
            if self.ambiguous_aliases.contains(&alias) {
                break;
            }
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

    /// Option ids of a usage option word (a short cluster may hold several), and whether its last
    /// option takes its value from the next usage token.
    fn expand_usage_option(&self, atom: &str) -> Result<(Vec<usize>, bool)> {
        if atom.starts_with("--") {
            let name = atom.split_once('=').map_or(atom, |(name, _)| name);
            let id = self.resolve(name)?;
            let spec = &self.specs[id];
            return Ok((vec![id], spec.takes_value && !atom.contains('=')));
        }

        let body = atom.strip_prefix('-').unwrap_or(atom);
        let mut ids = Vec::new();
        let mut takes_separate_value = false;
        for (offset, ch) in body.char_indices() {
            if ch == '=' {
                break;
            }
            let alias = format!("-{ch}");
            let id = self.resolve(&alias)?;
            ids.push(id);
            if self.specs[id].takes_value {
                let next_offset = offset + ch.len_utf8();
                takes_separate_value = body[next_offset..].is_empty();
                break;
            }
        }
        if ids.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid option in usage: {atom}"),
            ));
        }
        Ok((ids, takes_separate_value))
    }

    /// Merge `incoming` into the option sharing one of its aliases, or register it as a new one.
    fn upsert(&mut self, incoming: OptionSpec) -> Result<usize> {
        let Some(id) = self.existing(&incoming)? else {
            return self.insert_distinct(incoming);
        };

        let existing = &self.specs[id];
        let shared_short_with_distinct_longs = existing.short.is_some()
            && existing.short == incoming.short
            && matches!(
                (&existing.long, &incoming.long),
                (Some(existing_long), Some(incoming_long)) if existing_long != incoming_long
            );
        if shared_short_with_distinct_longs {
            return self.insert_distinct(incoming);
        }

        check_mergeable(existing, &incoming)?;
        self.merge(id, incoming);
        Ok(id)
    }

    /// Option matching one of the aliases of `incoming`; aliases of different options conflict.
    fn existing(&self, incoming: &OptionSpec) -> Result<Option<usize>> {
        let short_existing = incoming.short.as_ref().and_then(|alias| self.find(alias));
        let long_existing = incoming.long.as_ref().and_then(|alias| self.find(alias));
        match (short_existing, long_existing) {
            (Some(short), Some(long)) if short != long => Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Option aliases resolve to different options: {} {}",
                    incoming.short.as_deref().unwrap_or_default(),
                    incoming.long.as_deref().unwrap_or_default()
                ),
            )),
            (Some(id), _) | (_, Some(id)) => Ok(Some(id)),
            (None, None) => Ok(None),
        }
    }

    /// Fill the missing aliases, value arity and default of option `id` from `incoming`.
    fn merge(&mut self, id: usize, incoming: OptionSpec) {
        let spec = &mut self.specs[id];
        if spec.short.is_none() {
            spec.short = incoming.short;
        }
        if spec.long.is_none() {
            spec.long = incoming.long;
        }
        spec.takes_value |= incoming.takes_value;
        if spec.default_value.is_none() {
            spec.default_value = incoming.default_value;
        }
        for alias in [&spec.short, &spec.long].into_iter().flatten() {
            if !self.ambiguous_aliases.contains(alias) {
                self.aliases.insert(alias.clone(), id);
            }
        }
    }

    /// Register `incoming` as a new option. Aliases already taken become ambiguous; the option
    /// needs at least one alias of its own.
    fn insert_distinct(&mut self, incoming: OptionSpec) -> Result<usize> {
        let id = self.specs.len();
        let aliases = [incoming.short.as_ref(), incoming.long.as_ref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let has_unique_alias = aliases
            .iter()
            .any(|alias| !self.aliases.contains_key(alias.as_str()));
        if !has_unique_alias
            && aliases
                .iter()
                .any(|alias| self.ambiguous_aliases.contains(*alias))
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Option has no unambiguous alias: {}",
                    incoming.preferred_name()
                ),
            ));
        }

        for alias in aliases {
            if self.aliases.contains_key(alias.as_str()) {
                self.ambiguous_aliases.insert(alias.clone());
            } else {
                self.aliases.insert(alias.clone(), id);
            }
        }
        self.specs.push(incoming);
        Ok(id)
    }
}

/// Check that `incoming` describes the same option as `existing`: no different aliases or
/// defaults.
fn check_mergeable(existing: &OptionSpec, incoming: &OptionSpec) -> Result<()> {
    if let (Some(a), Some(b)) = (&existing.short, &incoming.short)
        && a != b
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Conflicting short option aliases: {a} and {b}"),
        ));
    }
    if let (Some(a), Some(b)) = (&existing.long, &incoming.long)
        && a != b
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("Conflicting long option aliases: {a} and {b}"),
        ));
    }
    if let (Some(a), Some(b)) = (&existing.default_value, &incoming.default_value)
        && a != b
    {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Conflicting defaults for option {}",
                existing.preferred_name()
            ),
        ));
    }
    Ok(())
}

/// Next argument as the value of option `name`.
fn next_value<'a>(name: &str, rest: &mut impl Iterator<Item = &'a str>) -> Result<String> {
    rest.next().map(str::to_owned).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Option {name} requires a value"),
        )
    })
}

/// Text between `[default: ` and the last `]` of an option description.
fn default_value(description: &str) -> Option<&str> {
    let (_, rest) = description.split_once("[default: ")?;
    rest.rfind(']').map(|end| &rest[..end])
}

/// `-` (stdin/stdout by convention) and `--` (end of options) are commands, not options.
fn is_dash_command(word: &str) -> bool {
    matches!(word, "-" | "--")
}

/// Whether a usage token is a value placeholder: `<value>` or an ASCII uppercase word like
/// `FILE`, as for positionals.
fn is_usage_value_placeholder(token: Option<&Token>) -> bool {
    let Some(Token::Atom(value)) = token else {
        return false;
    };

    if value.starts_with('<') && value.ends_with('>') {
        return true;
    }

    value.bytes().any(|byte| byte.is_ascii_uppercase())
        && value
            .bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-'))
}

/// Whether an option word such as `-o`, `--out` or `--out=FILE` names an option, unlike `-`,
/// `--` or `--=FILE`.
fn has_option_name(word: &str) -> bool {
    !split_option_declaration(word)
        .0
        .trim_start_matches('-')
        .is_empty()
}

fn split_option_declaration(value: &str) -> (String, bool) {
    match value.split_once('=') {
        Some((name, _)) => (name.to_owned(), true),
        None => (value.to_owned(), false),
    }
}

fn delimiter(ch: char) -> Option<Token> {
    match ch {
        '[' => Some(Token::LeftBracket),
        ']' => Some(Token::RightBracket),
        '(' => Some(Token::LeftParen),
        ')' => Some(Token::RightParen),
        '|' => Some(Token::Pipe),
        _ => None,
    }
}

/// Split a usage pattern into delimiters, `...` and whitespace-separated atoms.
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
        if let Some(delimiter) = delimiter(ch) {
            flush(&mut current, &mut tokens);
            tokens.push(delimiter);
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
    fn documented_value_option_does_not_require_usage_placeholder() {
        let help = "Usage: tool [--type]\n\n--type=TYPE  resource type";
        let usages = vec!["tool [--type]".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        assert!(registry.tokenize_usage(&usages[0]).is_ok());
    }

    #[test]
    fn separate_usage_value_placeholder_is_not_a_positional() {
        let help = "Usage: tool [-o FILE]\n\n-o FILE  output";
        let usages = vec!["tool [-o FILE]".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        let tokens = registry.tokenize_usage(&usages[0]).unwrap();
        assert_eq!(
            tokens
                .iter()
                .filter(|token| matches!(token, Token::Atom(_)))
                .count(),
            0
        );
    }

    #[test]
    fn repeatable_value_option_keeps_scalar_output_type() {
        let help = "Usage: tool [--tag=<value>]...";
        let usages = vec!["tool [--tag=<value>]...".to_owned()];
        let mut registry = OptionRegistry::from_doc(help, &usages).unwrap();
        registry.set_repeatable(&HashSet::from([0])).unwrap();
        assert_eq!(registry.initial_options()["tag"], Value::Null);
    }

    #[test]
    fn help_option_is_a_help_or_h_flag() {
        let is_help = |declaration: &str| {
            let help = format!("Usage: tool [options]\n\n{declaration}  description");
            OptionRegistry::from_doc(&help, &["tool [options]".to_owned()])
                .unwrap()
                .is_help(0)
        };
        assert!(is_help("-h --help"));
        assert!(is_help("--help"));
        assert!(is_help("-h"));
        assert!(!is_help("-h --human"));
        assert!(!is_help("-h <host>"));
        assert!(!is_help("--help=<topic>"));
        assert!(!is_help("-x"));
    }

    #[test]
    fn shared_short_alias_is_kept_as_deterministic_ambiguity() {
        let help =
            "Usage: tool [options]\n\n-u --sysupgrade  upgrade\n-u --upgrades  list upgrades";
        let usages = vec!["tool [options]".to_owned()];
        let registry = OptionRegistry::from_doc(help, &usages).unwrap();
        assert!(registry.normalize_args(&["--sysupgrade"]).is_ok());
        assert!(registry.normalize_args(&["--upgrades"]).is_ok());
        let error = registry.normalize_args(&["-u"]).unwrap_err();
        assert!(error.to_string().contains("Ambiguous option alias: -u"));
        let initial = registry.initial_options();
        assert!(initial.contains_key("sysupgrade"));
        assert!(initial.contains_key("upgrades"));
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
