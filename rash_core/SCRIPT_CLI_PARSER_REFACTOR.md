# Script CLI parser refactor plan

This document is the execution plan and progress log for replacing Rash's current usage-expansion
based script CLI parser with a compiled matcher.

The refactor is intentionally staged. The legacy parser remains available as an oracle until the new
implementation has demonstrated compatibility, bounded complexity, and acceptable performance.

## Objectives

1. Preserve the existing Rash script CLI contract for valid, deterministic interfaces.
2. Remove argument-count-driven and combinatorial usage expansion from the parsing architecture.
3. Make ambiguous declarations deterministic failures instead of relying on collection iteration order.
4. Keep long repeated argument lists and large option sets cheap enough for entrypoint/bootstrap use.
5. Make the supported grammar explicit and maintainable instead of inheriting accidental Docopt behavior.
6. Switch production only after differential tests and benchmarks provide evidence that the replacement is safe.

## Non-goals

- Full Docopt compatibility.
- Redesigning Rash's top-level interpreter/script argument boundary.
- Changing the JSON/template variable contract as part of the parser rewrite.
- Adding new CLI syntax solely because the new grammar can represent it.
- Removing the legacy parser before it has finished serving as the differential-test and benchmark oracle.

## Architectural invariants

These invariants are release gates, not preferences.

- **No concrete usage expansion.** Grammar size must depend on the declaration, not on `argv` length.
- **No factorial option permutation generation.** Unordered optional options must be represented structurally.
- **Repetition is represented by graph cycles.** `<arg>...` must not duplicate grammar nodes per argument.
- **Matching is deterministic.** If two successful paths produce different bindings, parsing fails as ambiguous.
- **Equivalent paths are allowed.** Multiple paths producing identical bindings are not considered ambiguous.
- **Normalized options preserve aliases.** Short and long forms must resolve to one logical option and output key.
- **Help still participates in grammar matching.** A declared logical help option may satisfy a positional slot, matching legacy Rash behavior, but it must not globally make an otherwise impossible argv valid.
- **The replacement does not silently widen the grammar.** Legacy-invalid command, positional, and usage-option identifiers remain invalid unless an intentional syntax change is reviewed separately.
- **Production switchover is last.** `rash` keeps calling the legacy parser until compatibility and performance gates pass.

## Phase 0 — Contract capture and baseline

**Goal:** establish what must be preserved before changing production behavior.

### Work

- [x] Keep `docopt::parse` intact as the reference implementation.
- [x] Add a public `script_cli::parse` entry point beside it.
- [x] Build differential-test helpers that compare values and error kinds.
- [x] Capture representative real-world grammar shapes:
  - command alternatives;
  - repeatable positional arguments;
  - repeated groups;
  - short and long option aliases;
  - short option clusters;
  - options carrying values;
  - defaults;
  - `[options]`;
  - dashed command/argument names;
  - help behavior.
- [x] Extend Criterion so old and new implementations can be measured side by side.

### Exit criteria

- Legacy parser remains untouched and callable.
- Differential tests cover the main currently-supported syntax classes.
- Benchmarks can invoke both implementations with identical inputs.

**Status:** complete.

## Phase 1 — Grammar model

**Goal:** replace string rewriting with an explicit parser grammar.

### Work

- [x] Tokenize usage declarations.
- [x] Introduce an AST for:
  - sequence;
  - alternative;
  - optional;
  - required grouping;
  - repetition;
  - command literals;
  - positional arguments;
  - option references;
  - `[options]`.
- [x] Normalize adjacent optional options into an unordered option group instead of generating permutations.
- [x] Analyze symbol multiplicity without expanding patterns.
- [x] Preserve normalized variable names for dashed commands/positionals.
- [x] Preserve the legacy lexical domain for command and positional identifiers instead of accepting broader arbitrary atoms.

### Correctness checks

- [x] `[a b]` follows Rash's existing independently-optional behavior.
- [x] `[(a b)]` remains an atomic optional group.
- [x] `(<a> <b>)...` repeats the complete group and rejects incomplete tails.
- [x] uppercase positional notation is preserved.

### Exit criteria

- Every supported declaration is represented as a bounded AST.
- AST size is independent of runtime argument count.
- No option ordering permutations are materialized.

**Status:** complete, pending full CI confirmation.

## Phase 2 — Option registry and argv normalization

**Goal:** separate lexical option handling from grammar matching.

### Work

- [x] Build a single registry for short/long aliases.
- [x] Parse option defaults and value arity from declarations/help text and inline option syntax.
- [x] Allow a description to define value arity even when `Usage:` omits the value placeholder, preserving legacy behavior such as `[--type]` plus `--type=TYPE` in the option description.
- [x] Do **not** infer new value arity merely from an adjacent non-option word in `Usage:` when no declaration supplies it; legacy Rash does not do that.
- [x] Normalize:
  - `--name=value`;
  - `--name value` when the option is known to take a value;
  - short aliases;
  - short clusters;
  - a value-bearing option at the end of a short cluster.
- [x] Preserve values containing `=`.
- [x] Keep repeated simple flags available to the grammar instead of rejecting them globally.
- [x] Scope `[options]` per usage pattern.
- [x] Track the logical help option identity without bypassing matching.
- [x] Reject usage option identifiers outside the legacy lowercase ASCII word grammar.

### Exit criteria

- Matching consumes normalized logical option tokens, not raw spelling variants.
- Alias spelling does not change output shape.
- Unknown options produce the same error class as the legacy parser.
- Registry discovery does not silently reinterpret legacy-simple options as value-bearing options.

**Status:** complete, pending differential results.

## Phase 3 — Compiled matcher

**Goal:** make runtime matching proportional to input and active grammar states rather than generated usage combinations.

### Work

- [x] Compile the AST to an epsilon-NFA.
- [x] Compile repetition to cycles.
- [x] Compile unordered optional option groups to masked option loops.
- [x] Compile `[options]` to a pattern-scoped option loop.
- [x] Keep captures in a persistent arena to avoid cloning the complete binding vector per consumed token.
- [x] Detect different successful binding sets as ambiguity.
- [x] Deduplicate identical successful binding sets.
- [x] Preserve legacy help semantics by allowing a logical help option to be consumed through a positional matcher when that positional path is otherwise valid.
- [ ] Bound active candidates more aggressively by equivalent matcher state where this can be proven semantics-preserving.
- [ ] Audit epsilon-closure behavior for nullable repeated expressions and reject/normalize grammars that could create zero-consumption cycles.

### Complexity targets

- Grammar compilation: `O(grammar size)` apart from registry lookups.
- Repeatable argv: no grammar growth with argument count.
- Matching: target `O(argv * active states)` with bounded state/capture bookkeeping.
- No factorial or exponential pre-expansion as an implementation strategy.

### Exit criteria

- Pathological repeated-input tests remain bounded.
- No zero-consumption infinite loops are possible.
- Ambiguity behavior is deterministic and tested.

**Status:** core implementation complete; hardening remains.

## Phase 4 — Differential compatibility campaign

**Goal:** characterize every semantic delta before production switch.

### Test matrix

- [x] basic commands and alternatives;
- [x] repeated positionals;
- [x] repeated groups;
- [x] optional sequences and grouped optional sequences;
- [x] option aliases;
- [x] short clusters;
- [x] option values and defaults;
- [x] values containing `=`;
- [x] unordered adjacent options;
- [x] `[options]` combinations;
- [x] mixed option/command positions;
- [x] help command, explicit help option, positional-help substitution, and impossible-extra-help cases;
- [x] unknown options;
- [x] overlapping usages with identical bindings;
- [x] repeated commands/count output parity;
- [x] optional repeat forms (`[<x>...]` and `[<x>]...`);
- [x] bounded exhaustive generated argv spaces for representative small grammars;
- [x] malformed command/positional/usage-option identifier parity cases;
- [ ] all legacy parser unit-test scenarios mirrored or exercised through the differential suite;
- [ ] remaining malformed declarations and error-path parity where compatibility matters;
- [ ] broader fuzz/property-generated grammars if the bounded exhaustive suite reveals gaps worth generalizing.

### Delta policy

Every mismatch must be placed in one of three categories:

1. **Compatibility bug in the new parser** — fix before switchover.
2. **Nondeterministic/incorrect legacy behavior** — document and add an explicit regression test for the intended correction.
3. **Unsupported accidental Docopt behavior** — do not silently drop it; document the decision and keep production on legacy until the compatibility policy is accepted.

### Exit criteria

- Differential suite is green for all behavior intended to remain compatible.
- Any intentional differences are explicitly listed in the PR and covered by tests.

**Status:** in progress; the compatibility corpus is broad enough that the next full semantic CI run is the primary gate.

## Phase 5 — Performance validation and optimization

**Goal:** prove the new architecture is at least suitable for Rash's local automation workloads and materially better on pathological grammar shapes.

### Benchmarks

- [x] repeatable arguments at 10 / 100 / 1,000 / 10,000 elements;
- [x] large Pacman-style option declaration;
- [x] `[options]` with larger option sets;
- [x] nested alternatives;
- [ ] compile-only cost separated from match cost where useful;
- [ ] success and failure paths;
- [ ] ambiguous grammar path;
- [ ] repeated option clusters;
- [ ] capture-heavy long argv.

### Optimization order

Only optimize from measurements, in this order:

1. eliminate accidental allocations/clones in the hot match loop;
2. merge equivalent active states when capture semantics permit it;
3. replace general-purpose hash structures with indexed/bitset structures where stable IDs already exist;
4. cache epsilon closures or precompute closure transitions if profiling shows closure traversal is significant;
5. consider a denser compiled instruction representation only if the NFA object model itself is measurable overhead.

### Performance gates

- No regression large enough to matter on normal small Rash scripts without a documented reason.
- Clear improvement over legacy behavior on combinatorial option/alternative cases.
- Near-linear behavior for long repeated argv.
- 10,000 repeated arguments must remain practical and must not trigger grammar expansion or quadratic capture copying.

### Exit criteria

- Criterion numbers are recorded in the PR.
- Any regression is explained and either fixed or consciously accepted.

**Status:** benchmark harness present; measurements/optimization pending.

## Phase 6 — Production integration

**Goal:** switch Rash only after the replacement has earned it.

### Work

- [ ] Change production script parsing from `docopt::parse` to `script_cli::parse`.
- [ ] Run all existing examples using the new parser.
- [ ] Run integration tests that exercise scripts through the actual `rash` binary.
- [ ] Confirm error/help output remains usable at the CLI boundary.
- [ ] Keep the legacy parser available for differential tests and benchmarks for the remainder of this PR unless retaining it creates production coupling.

### Rollback condition

If the production switch exposes an unresolved compatibility or performance regression, revert only the switchover commit and continue development with both implementations side by side.

### Exit criteria

- Actual Rash execution uses the compiled parser.
- Full behavior suite remains green.

**Status:** not started by design.

## Phase 7 — Documentation and supported-language definition

**Goal:** stop describing Rash as if it inherits the complete Docopt language by implication.

### Work

- [ ] Document Rash's supported usage grammar explicitly.
- [ ] Keep Docopt described as inspiration/history, not compatibility commitment.
- [ ] Document option ordering, repetition, grouping, aliases, defaults, and help semantics.
- [ ] Document ambiguity errors.
- [ ] Add examples for non-trivial supported grammar.
- [ ] Update terminology from implementation-specific `docopt` wording where appropriate.

### Exit criteria

- Users can understand the supported syntax without consulting Docopt internals.
- Intentional incompatibilities are discoverable.

**Status:** not started.

## Phase 8 — Removal/cleanup decision

**Goal:** decide what happens to the legacy implementation after confidence is established.

### Options

- Keep legacy only behind tests/benchmarks for one release cycle.
- Move selected legacy fixtures into permanent compatibility tests and remove the implementation.
- Remove legacy immediately only if the differential corpus is comprehensive enough and keeping it meaningfully increases maintenance cost.

### Work

- [ ] Make the decision after the production parser is green across CI and benchmark results are known.
- [ ] Remove dead expansion helpers if the legacy implementation is removed.
- [ ] Rename modules/files only after behavior has stabilized; avoid mixing large naming churn with semantic debugging.

**Status:** deferred.

## Phase 9 — Final release gate

The PR must not leave draft until all applicable items below are true.

- [ ] `cargo fmt --check` green.
- [ ] Clippy green with warnings denied.
- [ ] MSRV (`1.94.1` at the start of this refactor) green.
- [ ] Linux GNU tests green.
- [ ] Linux musl build/tests green where exercised by CI.
- [ ] AArch64 Linux green.
- [ ] Apple Silicon/macOS green.
- [ ] FreeBSD build green.
- [ ] Pre-commit green.
- [ ] Release-image workflow green or any unrelated failure explicitly identified.
- [ ] Differential compatibility suite green.
- [ ] Benchmarks reviewed and summarized in the PR.
- [ ] Production switch completed and tested.
- [ ] Documentation updated.
- [ ] Final diff reviewed for accidental scope creep.

## Current progress

Updated: 2026-08-28

| Phase | State | Notes |
| --- | --- | --- |
| 0. Contract/baseline | Complete | Legacy kept as oracle; differential and Criterion harnesses exist. |
| 1. Grammar model | Complete / CI validation | AST, multiplicity analysis, and legacy identifier grammar are implemented. |
| 2. Options/argv normalization | Complete / parity validation | Includes aliases, clusters, declaration-driven value arity, `[options]`, logical help identity, and legacy usage-option identifier validation. |
| 3. Compiled matcher | In progress | Epsilon-NFA, persistent captures, ambiguity detection, and positional help semantics implemented; candidate/nullable-cycle hardening remains. |
| 4. Differential compatibility | In progress | Legacy matrices, malformed-grammar cases, and bounded exhaustive argv enumeration are committed; full semantic CI is the immediate gate. |
| 5. Performance | In progress | Side-by-side benchmarks added; numbers and optimizations pending. |
| 6. Production integration | Blocked on phases 4–5 | No runtime switch yet. |
| 7. Documentation | Not started | Starts after semantics stabilize. |
| 8. Legacy cleanup | Deferred | Decision after production validation. |
| 9. Final release gate | Not started | PR remains draft. |

## Progress-update policy

This file is the durable progress record for the refactor. Update the table and relevant phase checkboxes
when a phase gate changes materially, not for every tiny implementation commit. The PR body should stay
high-level and link to this document rather than duplicating every detail.
