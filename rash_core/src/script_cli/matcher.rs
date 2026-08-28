use std::collections::{HashMap, HashSet, VecDeque};

use super::InputToken;
use super::grammar::{self, Atom, Expr};
use super::options::OptionRegistry;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(super) enum Capture {
    Command(String),
    Positional { key: String, value: String },
    Option { id: usize, value: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MatchError {
    NoMatch,
    Ambiguous,
}

#[derive(Clone, Debug)]
enum Matcher {
    Command { literal: String, key: String },
    Positional { key: String },
    Option(usize),
    AnyOption(Vec<bool>),
}

#[derive(Clone, Debug)]
enum Edge {
    Epsilon(usize),
    Consume { matcher: Matcher, target: usize },
}

#[derive(Clone, Debug, Default)]
struct State {
    edges: Vec<Edge>,
}

#[derive(Clone, Debug)]
pub(super) struct Nfa {
    states: Vec<State>,
    start: usize,
    accept: usize,
    help_option: Option<usize>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Candidate {
    state: usize,
    path: Option<usize>,
}

#[derive(Clone, Debug)]
struct PathNode {
    prev: Option<usize>,
    capture: Capture,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PathKey {
    prev: Option<usize>,
    capture: Capture,
}

#[derive(Default)]
struct PathArena {
    nodes: Vec<PathNode>,
    intern: HashMap<PathKey, usize>,
}

impl PathArena {
    fn append(&mut self, prev: Option<usize>, capture: Capture) -> usize {
        let key = PathKey {
            prev,
            capture: capture.clone(),
        };
        if let Some(id) = self.intern.get(&key) {
            return *id;
        }
        let id = self.nodes.len();
        self.nodes.push(PathNode { prev, capture });
        self.intern.insert(key, id);
        id
    }

    fn materialize(&self, path: Option<usize>) -> Vec<Capture> {
        let mut out = Vec::new();
        let mut current = path;
        while let Some(id) = current {
            let node = &self.nodes[id];
            out.push(node.capture.clone());
            current = node.prev;
        }
        out.reverse();
        out
    }
}

pub(super) fn compile(patterns: &[Expr], options: &OptionRegistry) -> Nfa {
    let mut builder = Builder::default();
    let start = builder.state();
    let accept = builder.state();
    let help_option = options.all_ids().find(|id| options.is_help(*id));

    for pattern in patterns {
        let explicit = grammar::explicit_options(pattern);
        let mut allowed = vec![false; options.len()];
        for id in options.all_ids() {
            if !explicit.contains(&id) {
                allowed[id] = true;
            }
        }

        let (pattern_start, pattern_end) = builder.compile_expr(pattern, &allowed);
        builder.epsilon(start, pattern_start);
        builder.epsilon(pattern_end, accept);
    }

    Nfa {
        states: builder.states,
        start,
        accept,
        help_option,
    }
}

pub(super) fn execute(nfa: &Nfa, input: &[InputToken]) -> Result<Vec<Capture>, MatchError> {
    let mut arena = PathArena::default();
    let mut candidates = epsilon_closure(
        nfa,
        [Candidate {
            state: nfa.start,
            path: None,
        }],
    );

    for token in input {
        let mut next = Vec::new();
        for candidate in &candidates {
            for edge in &nfa.states[candidate.state].edges {
                let Edge::Consume { matcher, target } = edge else {
                    continue;
                };
                if let Some(capture) = matches(matcher, token, nfa.help_option) {
                    let path = Some(arena.append(candidate.path, capture));
                    next.push(Candidate {
                        state: *target,
                        path,
                    });
                }
            }
        }

        if next.is_empty() {
            return Err(MatchError::NoMatch);
        }
        candidates = epsilon_closure(nfa, next);
    }

    candidates = epsilon_closure(nfa, candidates);
    let mut outputs = HashSet::new();
    for candidate in candidates {
        if candidate.state == nfa.accept {
            outputs.insert(arena.materialize(candidate.path));
            if outputs.len() > 1 {
                return Err(MatchError::Ambiguous);
            }
        }
    }

    outputs.into_iter().next().ok_or(MatchError::NoMatch)
}

fn epsilon_closure(nfa: &Nfa, seeds: impl IntoIterator<Item = Candidate>) -> Vec<Candidate> {
    let mut queue = seeds.into_iter().collect::<VecDeque<_>>();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    while let Some(candidate) = queue.pop_front() {
        if !seen.insert(candidate) {
            continue;
        }
        out.push(candidate);
        for edge in &nfa.states[candidate.state].edges {
            if let Edge::Epsilon(target) = edge {
                queue.push_back(Candidate {
                    state: *target,
                    path: candidate.path,
                });
            }
        }
    }
    out
}

fn matches(matcher: &Matcher, input: &InputToken, help_option: Option<usize>) -> Option<Capture> {
    match (matcher, input) {
        (Matcher::Command { literal, key }, InputToken::Word(value)) if literal == value => {
            Some(Capture::Command(key.clone()))
        }
        (Matcher::Positional { key }, InputToken::Word(value)) => Some(Capture::Positional {
            key: key.clone(),
            value: value.clone(),
        }),
        (Matcher::Positional { .. }, InputToken::Option { id, value })
            if Some(*id) == help_option && value.is_none() =>
        {
            Some(Capture::Option {
                id: *id,
                value: None,
            })
        }
        (Matcher::Option(expected), InputToken::Option { id, value }) if expected == id => {
            Some(Capture::Option {
                id: *id,
                value: value.clone(),
            })
        }
        (Matcher::AnyOption(allowed), InputToken::Option { id, value })
            if allowed.get(*id).copied().unwrap_or(false) =>
        {
            Some(Capture::Option {
                id: *id,
                value: value.clone(),
            })
        }
        _ => None,
    }
}

#[derive(Default)]
struct Builder {
    states: Vec<State>,
}

impl Builder {
    fn state(&mut self) -> usize {
        let id = self.states.len();
        self.states.push(State::default());
        id
    }

    fn epsilon(&mut self, from: usize, to: usize) {
        self.states[from].edges.push(Edge::Epsilon(to));
    }

    fn consume(&mut self, from: usize, matcher: Matcher, to: usize) {
        self.states[from].edges.push(Edge::Consume {
            matcher,
            target: to,
        });
    }

    fn optional_option(&mut self, id: usize) -> (usize, usize) {
        let start = self.state();
        let end = self.state();
        self.epsilon(start, end);
        self.consume(start, Matcher::Option(id), end);
        (start, end)
    }

    fn option_loop(&mut self, mask: Vec<bool>) -> (usize, usize) {
        let start = self.state();
        let end = self.state();
        self.epsilon(start, end);
        self.consume(start, Matcher::AnyOption(mask), start);
        (start, end)
    }

    fn options_shortcut(&mut self, allowed_options: &[bool]) -> (usize, usize) {
        let mut ids = allowed_options
            .iter()
            .enumerate()
            .filter_map(|(id, allowed)| allowed.then_some(id));
        let first = ids.next();
        let second = ids.next();

        match (first, second) {
            (None, _) => self.compile_expr(&Expr::Empty, allowed_options),
            (Some(id), None) => self.optional_option(id),
            (Some(_), Some(_)) => self.option_loop(allowed_options.to_vec()),
        }
    }

    fn compile_expr(&mut self, expr: &Expr, allowed_options: &[bool]) -> (usize, usize) {
        match expr {
            Expr::Empty => {
                let start = self.state();
                let end = self.state();
                self.epsilon(start, end);
                (start, end)
            }
            Expr::Atom(atom) => {
                let start = self.state();
                let end = self.state();
                let matcher = match atom {
                    Atom::Command { literal, key } => Matcher::Command {
                        literal: literal.clone(),
                        key: key.clone(),
                    },
                    Atom::Positional { key } => Matcher::Positional { key: key.clone() },
                    Atom::Option(id) => Matcher::Option(*id),
                };
                self.consume(start, matcher, end);
                (start, end)
            }
            Expr::Sequence(items) => {
                if items.is_empty() {
                    return self.compile_expr(&Expr::Empty, allowed_options);
                }
                let (start, mut end) = self.compile_expr(&items[0], allowed_options);
                for item in &items[1..] {
                    let (next_start, next_end) = self.compile_expr(item, allowed_options);
                    self.epsilon(end, next_start);
                    end = next_end;
                }
                (start, end)
            }
            Expr::Alternative(branches) => {
                let start = self.state();
                let end = self.state();
                for branch in branches {
                    let (branch_start, branch_end) = self.compile_expr(branch, allowed_options);
                    self.epsilon(start, branch_start);
                    self.epsilon(branch_end, end);
                }
                (start, end)
            }
            Expr::Optional(inner) => {
                let start = self.state();
                let end = self.state();
                let (inner_start, inner_end) = self.compile_expr(inner, allowed_options);
                self.epsilon(start, end);
                self.epsilon(start, inner_start);
                self.epsilon(inner_end, end);
                (start, end)
            }
            Expr::Required(inner) => self.compile_expr(inner, allowed_options),
            Expr::Repeat(inner) => {
                let start = self.state();
                let end = self.state();
                let (inner_start, inner_end) = self.compile_expr(inner, allowed_options);
                self.epsilon(start, inner_start);
                self.epsilon(inner_end, inner_start);
                self.epsilon(inner_end, end);
                (start, end)
            }
            Expr::OptionsGroup(ids) => {
                let mut mask = vec![false; allowed_options.len()];
                for id in ids {
                    if let Some(value) = mask.get_mut(*id) {
                        *value = true;
                    }
                }
                self.option_loop(mask)
            }
            Expr::OptionsShortcut => self.options_shortcut(allowed_options),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_is_a_cycle_not_expansion() {
        let pattern = Expr::Repeat(Box::new(Expr::Atom(Atom::Positional {
            key: "file".into(),
        })));
        let nfa = compile(&[pattern], &OptionRegistry::default());
        let input = (0..10_000)
            .map(|i| InputToken::Word(i.to_string()))
            .collect::<Vec<_>>();
        let captures = execute(&nfa, &input).unwrap();
        assert_eq!(captures.len(), 10_000);
        assert!(nfa.states.len() < 10);
    }

    #[test]
    fn detects_ambiguous_bindings() {
        let patterns = vec![
            Expr::Atom(Atom::Positional { key: "left".into() }),
            Expr::Atom(Atom::Positional {
                key: "right".into(),
            }),
        ];
        let nfa = compile(&patterns, &OptionRegistry::default());
        assert_eq!(
            execute(&nfa, &[InputToken::Word("x".into())]),
            Err(MatchError::Ambiguous)
        );
    }

    #[test]
    fn option_group_accepts_any_declared_order() {
        let pattern = Expr::OptionsGroup(vec![0, 1]);
        let mut registry = OptionRegistry::from_doc(
            "Usage: tool [-a] [-b]\n\n-a  a\n-b  b",
            &["tool [-a] [-b]".to_owned()],
        )
        .unwrap();
        registry.set_repeatable(&HashSet::new()).unwrap();
        let nfa = compile(&[pattern], &registry);
        let input = registry.normalize_args(&["-b", "-a"]).unwrap();
        assert!(execute(&nfa, &input).is_ok());
    }

    #[test]
    fn options_shortcut_with_one_available_option_is_not_repeatable() {
        let pattern = Expr::OptionsShortcut;
        let registry = OptionRegistry::from_doc(
            "Usage: tool [options]\n\n-a  a",
            &["tool [options]".to_owned()],
        )
        .unwrap();
        let nfa = compile(&[pattern], &registry);
        let once = registry.normalize_args(&["-a"]).unwrap();
        let twice = registry.normalize_args(&["-a", "-a"]).unwrap();
        assert!(execute(&nfa, &once).is_ok());
        assert_eq!(execute(&nfa, &twice), Err(MatchError::NoMatch));
    }

    #[test]
    fn options_shortcut_with_multiple_available_options_keeps_legacy_loop() {
        let pattern = Expr::OptionsShortcut;
        let registry = OptionRegistry::from_doc(
            "Usage: tool [options]\n\n-a  a\n-b  b",
            &["tool [options]".to_owned()],
        )
        .unwrap();
        let nfa = compile(&[pattern], &registry);
        let repeated = registry.normalize_args(&["-a", "-a"]).unwrap();
        assert!(execute(&nfa, &repeated).is_ok());
    }

    #[test]
    fn help_option_can_be_consumed_by_positional_matcher() {
        let pattern = Expr::Atom(Atom::Positional {
            key: "target".into(),
        });
        let registry = OptionRegistry::from_doc(
            "Usage: tool <target>\n\n-h --help  help",
            &["tool <target>".to_owned()],
        )
        .unwrap();
        let nfa = compile(&[pattern], &registry);
        let input = registry.normalize_args(&["--help"]).unwrap();
        assert!(matches!(
            execute(&nfa, &input),
            Ok(captures) if matches!(captures.as_slice(), [Capture::Option { .. }])
        ));
    }
}
