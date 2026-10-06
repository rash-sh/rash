//! Priority-ordered epsilon-NFA simulation (Pike VM).
//!
//! Every usage pattern is compiled into one NFA whose epsilon edges are ordered by priority:
//! patterns in declaration order, optional content before skipping it, another repetition before
//! leaving a repeat, alternatives in written order and option loops consuming before exiting.
//! Matching advances all threads token by token, keeping them in priority order, and returns the
//! captures of the highest-priority path that consumes the whole argv. That is the first success
//! a backtracking matcher exploring the same choices in that order would find, except that a
//! repetition never takes an iteration that consumes nothing: the epsilon closure of a step visits
//! each state once, so a path never returns to a state without consuming a token. For example,
//! `([go] | <x>)...` never takes an empty `[go]` iteration, so `<x>` matches the next word.
//!
//! Two threads reaching the same state have the same future: option occurrence limits depend only
//! on how often the option occurred in the argv prefix, which every thread has consumed entirely.
//! So only the higher-priority thread is kept per state, and matching is
//! `O(argv × NFA states)`.

use std::collections::{HashMap, HashSet};

use super::InputToken;
use super::grammar::{self, Atom, Count, Expr};
use super::options::OptionRegistry;

/// Binding produced by consuming one input token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Capture {
    Command(String),
    Positional { key: String, value: String },
    Option { id: usize, value: Option<String> },
}

/// Condition for consuming one input token.
#[derive(Debug)]
enum Matcher {
    Command {
        literal: String,
        key: String,
    },
    Positional {
        key: String,
    },
    /// Explicit option, accepted while its occurrences stay within the pattern's limit.
    Option {
        id: usize,
        limit: Count,
    },
    /// Any option whose id is `true` in the mask (`[options]` with several options).
    AnyOption(Vec<bool>),
    /// Unordered option group; each option may match at most its per-pattern limit.
    BoundedOption(Vec<Count>),
}

#[derive(Debug)]
enum Node {
    /// Epsilon edges, highest priority first. The accept state is a split without edges.
    Split(Vec<usize>),
    /// Consume one token accepted by `matcher` and move to `target`.
    Consume { matcher: Matcher, target: usize },
}

/// Epsilon-NFA of all usage patterns, sharing one start and one accept state.
#[derive(Debug)]
pub(super) struct Nfa {
    nodes: Vec<Node>,
    start: usize,
    accept: usize,
}

/// Compile all usage patterns into one NFA. Its size depends only on the declaration.
///
/// `[options]` stands for every option that no usage pattern references explicitly.
pub(super) fn compile(patterns: &[Expr], options: &OptionRegistry) -> Nfa {
    let mut builder = Builder::default();
    let start = builder.split();
    let accept = builder.split();

    let explicit = patterns
        .iter()
        .flat_map(grammar::explicit_options)
        .collect::<HashSet<_>>();
    let shortcut_options = options
        .all_ids()
        .map(|id| !explicit.contains(&id))
        .collect::<Vec<_>>();

    for pattern in patterns {
        builder.option_limits = grammar::option_limits(pattern);
        let (pattern_start, pattern_end) = builder.compile_expr(pattern, &shortcut_options);
        builder.epsilon(start, pattern_start);
        builder.epsilon(pattern_end, accept);
    }

    Nfa {
        nodes: builder.nodes,
        start,
        accept,
    }
}

/// Active NFA state with the last capture of the path that reached it.
#[derive(Clone, Copy, Debug)]
struct Thread {
    state: usize,
    path: Option<usize>,
}

/// One consumed token of a path, stored as a linked list so extending a path never copies it.
struct PathNode {
    prev: Option<usize>,
    /// Consume state that matched the token.
    state: usize,
    token: usize,
}

/// Threads of one step, in priority order, with at most one thread per state.
struct ThreadList {
    threads: Vec<Thread>,
    /// Generation in which each state was last added.
    seen: Vec<usize>,
    generation: usize,
    stack: Vec<usize>,
}

impl ThreadList {
    fn new(states: usize) -> Self {
        Self {
            threads: Vec::new(),
            seen: vec![0; states],
            generation: 0,
            stack: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.threads.clear();
        self.generation += 1;
    }

    /// Add `state` and its epsilon closure in priority order (depth-first, edges in order).
    /// States already reached by a higher-priority thread in this step are skipped.
    fn add(&mut self, nfa: &Nfa, state: usize, path: Option<usize>) {
        self.stack.push(state);
        while let Some(state) = self.stack.pop() {
            if self.seen[state] == self.generation {
                continue;
            }
            self.seen[state] = self.generation;
            match &nfa.nodes[state] {
                Node::Split(targets) => {
                    self.stack.extend(targets.iter().rev());
                    if state == nfa.accept {
                        self.threads.push(Thread { state, path });
                    }
                }
                Node::Consume { .. } => self.threads.push(Thread { state, path }),
            }
        }
    }
}

/// Match `input` and return the captures of the highest-priority path accepting all of it, or
/// `None` if no pattern accepts it.
pub(super) fn execute(nfa: &Nfa, input: &[InputToken]) -> Option<Vec<Capture>> {
    let seen_before = occurrences_before(input);
    let mut arena = Vec::<PathNode>::new();
    let mut current = ThreadList::new(nfa.nodes.len());
    let mut next = ThreadList::new(nfa.nodes.len());
    current.clear();
    current.add(nfa, nfa.start, None);

    for (index, token) in input.iter().enumerate() {
        next.clear();
        for thread in &current.threads {
            let Node::Consume { matcher, target } = &nfa.nodes[thread.state] else {
                continue;
            };
            if accepts(matcher, token, seen_before[index]) {
                arena.push(PathNode {
                    prev: thread.path,
                    state: thread.state,
                    token: index,
                });
                next.add(nfa, *target, Some(arena.len() - 1));
            }
        }
        if next.threads.is_empty() {
            return None;
        }
        std::mem::swap(&mut current, &mut next);
    }

    let thread = current
        .threads
        .iter()
        .find(|thread| thread.state == nfa.accept)?;
    materialize(nfa, input, &arena, thread.path)
}

/// For every token, how many earlier tokens are the same option (0 for words).
fn occurrences_before(input: &[InputToken]) -> Vec<usize> {
    let mut counts = HashMap::<usize, usize>::new();
    input
        .iter()
        .map(|token| match token {
            InputToken::Word(_) => 0,
            InputToken::Option { id, .. } => {
                let count = counts.entry(*id).or_default();
                *count += 1;
                *count - 1
            }
        })
        .collect()
}

/// Whether `matcher` consumes `token`, which follows `seen_before` occurrences of the same option.
fn accepts(matcher: &Matcher, token: &InputToken, seen_before: usize) -> bool {
    match (matcher, token) {
        (Matcher::Command { literal, .. }, InputToken::Word(value)) => literal == value,
        (Matcher::Positional { .. }, InputToken::Word(_)) => true,
        (
            Matcher::Option {
                id: expected,
                limit,
            },
            InputToken::Option { id, .. },
        ) => expected == id && limit.allows_another(seen_before),
        (Matcher::AnyOption(allowed), InputToken::Option { id, .. }) => flag(allowed, *id),
        (Matcher::BoundedOption(limits), InputToken::Option { id, .. }) => limits
            .get(*id)
            .is_some_and(|limit| limit.allows_another(seen_before)),
        _ => false,
    }
}

fn flag(mask: &[bool], id: usize) -> bool {
    mask.get(id).copied().unwrap_or(false)
}

/// Captures of the path ending at `path`, in argv order.
fn materialize(
    nfa: &Nfa,
    input: &[InputToken],
    arena: &[PathNode],
    path: Option<usize>,
) -> Option<Vec<Capture>> {
    let mut out = Vec::new();
    let mut current = path;
    while let Some(id) = current {
        let node = arena.get(id)?;
        let Node::Consume { matcher, .. } = nfa.nodes.get(node.state)? else {
            return None;
        };
        out.push(capture(matcher, input.get(node.token)?)?);
        current = node.prev;
    }
    out.reverse();
    Some(out)
}

/// Binding of `token`, already accepted by `matcher`.
fn capture(matcher: &Matcher, token: &InputToken) -> Option<Capture> {
    match (matcher, token) {
        (Matcher::Command { key, .. }, InputToken::Word(_)) => Some(Capture::Command(key.clone())),
        (Matcher::Positional { key }, InputToken::Word(value)) => Some(Capture::Positional {
            key: key.clone(),
            value: value.clone(),
        }),
        (_, InputToken::Option { id, value }) => Some(Capture::Option {
            id: *id,
            value: value.clone(),
        }),
        _ => None,
    }
}

#[derive(Default)]
struct Builder {
    nodes: Vec<Node>,
    /// Option occurrence limits of the pattern currently being compiled.
    option_limits: HashMap<usize, Count>,
}

/// Fragment of the NFA: its entry state and its exit state, which is always a split.
type Fragment = (usize, usize);

impl Builder {
    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn split(&mut self) -> usize {
        self.push(Node::Split(Vec::new()))
    }

    /// Append an epsilon edge with lower priority than the existing edges of `from`.
    ///
    /// `from` is always a split: fragment exits are splits, and so are the states the builder
    /// creates to add edges to.
    fn epsilon(&mut self, from: usize, to: usize) {
        let source = self.nodes.get_mut(from);
        debug_assert!(
            matches!(source, Some(Node::Split(_))),
            "epsilon edge from state {from}, which is not a split"
        );
        if let Some(Node::Split(targets)) = source {
            targets.push(to);
        }
    }

    fn consume(&mut self, matcher: Matcher, target: usize) -> usize {
        self.push(Node::Consume { matcher, target })
    }

    fn option_limit(&self, id: usize) -> Count {
        self.option_limits
            .get(&id)
            .copied()
            .unwrap_or(Count::Finite(1))
    }

    /// Compile `expr` into a fragment.
    ///
    /// `shortcut_options` marks the options `[options]` stands for.
    fn compile_expr(&mut self, expr: &Expr, shortcut_options: &[bool]) -> Fragment {
        match expr {
            Expr::Empty => self.empty(),
            Expr::Atom(atom) => self.atom(atom),
            Expr::Sequence(items) => self.sequence(items, shortcut_options),
            Expr::Alternative(branches) => self.alternative(branches, shortcut_options),
            Expr::Optional(inner) => self.optional(inner, shortcut_options),
            Expr::Required(inner) => self.compile_expr(inner, shortcut_options),
            Expr::Repeat(inner) => self.repeat(inner, shortcut_options),
            Expr::OptionsGroup(ids) => self.options_group(ids, shortcut_options.len()),
            Expr::OptionsShortcut => self.options_shortcut(shortcut_options),
        }
    }

    fn empty(&mut self) -> Fragment {
        let end = self.split();
        let start = self.split();
        self.epsilon(start, end);
        (start, end)
    }

    fn atom(&mut self, atom: &Atom) -> Fragment {
        let matcher = match atom {
            Atom::Command { literal, key } => Matcher::Command {
                literal: literal.clone(),
                key: key.clone(),
            },
            Atom::Positional { key } => Matcher::Positional { key: key.clone() },
            Atom::Option(id) => Matcher::Option {
                id: *id,
                limit: self.option_limit(*id),
            },
        };
        let end = self.split();
        (self.consume(matcher, end), end)
    }

    fn sequence(&mut self, items: &[Expr], shortcut_options: &[bool]) -> Fragment {
        let Some((first, rest)) = items.split_first() else {
            return self.empty();
        };
        let (start, mut end) = self.compile_expr(first, shortcut_options);
        for item in rest {
            let (next_start, next_end) = self.compile_expr(item, shortcut_options);
            self.epsilon(end, next_start);
            end = next_end;
        }
        (start, end)
    }

    fn alternative(&mut self, branches: &[Expr], shortcut_options: &[bool]) -> Fragment {
        let start = self.split();
        let end = self.split();
        for branch in branches {
            let (branch_start, branch_end) = self.compile_expr(branch, shortcut_options);
            self.epsilon(start, branch_start);
            self.epsilon(branch_end, end);
        }
        (start, end)
    }

    /// Zero or one occurrence of `inner`, preferring one. The skip edge starts at a fresh state,
    /// because the start state of `inner` may be re-entered by a cycle.
    fn optional(&mut self, inner: &Expr, shortcut_options: &[bool]) -> Fragment {
        let start = self.split();
        let end = self.split();
        let (inner_start, inner_end) = self.compile_expr(inner, shortcut_options);
        self.epsilon(start, inner_start);
        self.epsilon(start, end);
        self.epsilon(inner_end, end);
        (start, end)
    }

    /// One or more occurrences of `inner`, compiled as a cycle preferring another occurrence.
    fn repeat(&mut self, inner: &Expr, shortcut_options: &[bool]) -> Fragment {
        let start = self.split();
        let end = self.split();
        let (inner_start, inner_end) = self.compile_expr(inner, shortcut_options);
        self.epsilon(start, inner_start);
        self.epsilon(inner_end, inner_start);
        self.epsilon(inner_end, end);
        (start, end)
    }

    /// Zero or more tokens accepted by `matcher`, preferring more.
    fn option_loop(&mut self, matcher: Matcher) -> Fragment {
        let start = self.split();
        let end = self.split();
        let consume = self.consume(matcher, start);
        self.epsilon(start, consume);
        self.epsilon(start, end);
        (start, end)
    }

    /// Optional single option, outside any pattern limit.
    fn optional_option(&mut self, id: usize) -> Fragment {
        let start = self.split();
        let end = self.split();
        let consume = self.consume(
            Matcher::Option {
                id,
                limit: Count::Finite(1),
            },
            end,
        );
        self.epsilon(start, consume);
        self.epsilon(start, end);
        (start, end)
    }

    fn options_shortcut(&mut self, shortcut_options: &[bool]) -> Fragment {
        let mut ids = shortcut_options
            .iter()
            .enumerate()
            .filter_map(|(id, allowed)| allowed.then_some(id));
        match (ids.next(), ids.next()) {
            (None, _) => self.empty(),
            (Some(id), None) => self.optional_option(id),
            (Some(_), Some(_)) => self.option_loop(Matcher::AnyOption(shortcut_options.to_vec())),
        }
    }

    /// Unordered option loop where each option of `ids` may match up to its per-pattern limit.
    ///
    /// Known limitation: the limit counts every occurrence in the pattern, not in this group, so
    /// `tool [-a] [-b] cmd [-a] [-b]` accepts `-a -a cmd` while `tool [-a] cmd [-a]` rejects it.
    fn options_group(&mut self, ids: &[usize], option_count: usize) -> Fragment {
        let mut limits = vec![Count::Finite(0); option_count];
        for id in ids {
            if let Some(limit) = limits.get_mut(*id) {
                *limit = self.option_limit(*id);
            }
        }
        self.option_loop(Matcher::BoundedOption(limits))
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
        assert!(nfa.nodes.len() < 10);
    }

    #[test]
    fn nullable_repeat_terminates_and_accepts_zero_or_more_values() {
        let pattern = Expr::Repeat(Box::new(Expr::Optional(Box::new(Expr::Atom(
            Atom::Positional { key: "file".into() },
        )))));
        let nfa = compile(&[pattern], &OptionRegistry::default());
        assert!(execute(&nfa, &[]).is_some());
        let input = [InputToken::Word("a".into()), InputToken::Word("b".into())];
        let captures = execute(&nfa, &input).unwrap();
        assert_eq!(captures.len(), 2);
        assert!(nfa.nodes.len() < 10);
    }

    fn positional(key: &str) -> Expr {
        Expr::Atom(Atom::Positional { key: key.into() })
    }

    fn words(values: &[&str]) -> Vec<InputToken> {
        values
            .iter()
            .map(|value| InputToken::Word((*value).to_owned()))
            .collect()
    }

    fn bound_keys(captures: &[Capture]) -> Vec<String> {
        captures
            .iter()
            .map(|capture| match capture {
                Capture::Command(key) | Capture::Positional { key, .. } => key.clone(),
                Capture::Option { id, .. } => format!("option {id}"),
            })
            .collect()
    }

    #[test]
    fn first_declared_pattern_wins() {
        let patterns = vec![positional("left"), positional("right")];
        let nfa = compile(&patterns, &OptionRegistry::default());
        let captures = execute(&nfa, &words(&["x"])).unwrap();
        assert_eq!(bound_keys(&captures), ["left"]);
    }

    #[test]
    fn optional_prefers_its_content() {
        let pattern = Expr::Sequence(vec![
            Expr::Optional(Box::new(positional("a"))),
            Expr::Optional(Box::new(positional("b"))),
        ]);
        let nfa = compile(&[pattern], &OptionRegistry::default());
        let captures = execute(&nfa, &words(&["x"])).unwrap();
        assert_eq!(bound_keys(&captures), ["a"]);
    }

    #[test]
    fn optional_backtracks_when_its_content_cannot_lead_to_acceptance() {
        let pattern = Expr::Sequence(vec![
            Expr::Optional(Box::new(positional("a"))),
            positional("b"),
        ]);
        let nfa = compile(&[pattern], &OptionRegistry::default());
        let captures = execute(&nfa, &words(&["x"])).unwrap();
        assert_eq!(bound_keys(&captures), ["b"]);
    }

    #[test]
    fn repeat_prefers_more_iterations_and_alternatives_their_written_order() {
        let pattern = Expr::Sequence(vec![
            Expr::Repeat(Box::new(Expr::Alternative(vec![
                positional("a"),
                positional("b"),
            ]))),
            Expr::Optional(Box::new(positional("c"))),
        ]);
        let nfa = compile(&[pattern], &OptionRegistry::default());
        let captures = execute(&nfa, &words(&["x", "y", "z"])).unwrap();
        assert_eq!(bound_keys(&captures), ["a", "a", "a"]);
    }

    #[test]
    fn ambiguous_alternatives_stay_linear() {
        let pattern = Expr::Repeat(Box::new(Expr::Alternative(vec![
            positional("a"),
            positional("b"),
        ])));
        let nfa = compile(&[pattern], &OptionRegistry::default());
        let input = (0..10_000).map(|i| i.to_string()).collect::<Vec<_>>();
        let input = input.iter().map(String::as_str).collect::<Vec<_>>();
        let captures = execute(&nfa, &words(&input)).unwrap();
        assert_eq!(captures.len(), 10_000);
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
        let input = registry
            .normalize_args(&["-b", "-a"])
            .into_result()
            .unwrap();
        assert!(execute(&nfa, &input).is_some());
        let repeated = registry
            .normalize_args(&["-a", "-a"])
            .into_result()
            .unwrap();
        assert_eq!(execute(&nfa, &repeated), None);
    }

    #[test]
    fn option_group_allows_duplicated_option_up_to_its_occurrences() {
        let pattern = Expr::OptionsGroup(vec![0, 0]);
        let mut registry = OptionRegistry::from_doc(
            "Usage: tool [-a] [-a]\n\n-a  a",
            &["tool [-a] [-a]".to_owned()],
        )
        .unwrap();
        registry.set_repeatable(&HashSet::from([0])).unwrap();
        let nfa = compile(&[pattern], &registry);
        let twice = registry
            .normalize_args(&["-a", "-a"])
            .into_result()
            .unwrap();
        assert!(execute(&nfa, &twice).is_some());
        let thrice = registry
            .normalize_args(&["-a", "-a", "-a"])
            .into_result()
            .unwrap();
        assert_eq!(execute(&nfa, &thrice), None);
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
        let once = registry.normalize_args(&["-a"]).into_result().unwrap();
        let twice = registry
            .normalize_args(&["-a", "-a"])
            .into_result()
            .unwrap();
        assert!(execute(&nfa, &once).is_some());
        assert_eq!(execute(&nfa, &twice), None);
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
        let repeated = registry
            .normalize_args(&["-a", "-a"])
            .into_result()
            .unwrap();
        assert!(execute(&nfa, &repeated).is_some());
    }
}
