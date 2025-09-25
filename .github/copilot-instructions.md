# Rash – High-Signal Instructions

## Core

Declarative YAML scripting (Ansible-like) in a single Rust binary (`rash`). Workspace crates:
`rash_core` (engine + CLI entry `rash_core/src/bin/rash.rs`), `rash_derive` (proc macros),
`mdbook_rash` (docs preprocessor). Targets: Linux GNU & MUSL. MSRV 1.88. Use Make targets—never raw
cargo except for ad‑hoc exploration.

## Layout (anchor paths)

```
ENTRYPOINT=rash_core/src/bin/rash.rs
MODULES=rash_core/src/modules
JINJA=rash_core/src/jinja
DOCOPT=rash_core/src/docopt
ERROR=rash_core/src/error
LOGGER=rash_core/src/logger.rs
TEST_SCRIPTS=test/**/*.rh
EXAMPLES=examples/*.rh
DOCS_SRC=rash_book/src
DOCS_OUT=rash_book/rash-sh.github.io/docs/rash/<VERSION>
```

## Build & Validate

```
Lint              : make lint            # fmt --check + clippy -D warnings
Lint (auto-fix)   : make lint-fix
Test (all)        : make test            # runs lint + cargo tests + each test/*.rh
Debug build       : make build           # → target/<triple>/debug/rash
Release (native)  : make release [CARGO_TARGET=...]
Cross MUSL        : make release CARGO_TARGET=x86_64-unknown-linux-musl
Images            : make images          # smoke: make test-images
Docs              : make book VERSION=master
Version propagate : make update-version
Changelog         : make update-changelog
```

## Conventions & Style

- Root Cargo.toml owns version; member crates reference it.
- Zero clippy warnings; no #[allow] unless truly justified.
- No unwrap()/expect() outside tests—use ? with contextual errors.
- Keep functions < ~60 lines; favor iterators / itertools.
- Add new module: new file in MODULES + doc entry + example.
- Preserve backward compatibility of .rh YAML; gate breaking changes.
- Logging only through fern (logger.rs); prefix task names; avoid direct println.
- Keep binary size lean; justify heavy deps; prefer workspace dependency unification.

## Examples (make test-examples)

```
Purpose     : fast smoke via direct exec of examples/*.rh
Exclusions  : envar-api-gateway/, diff.rh, dotfiles/
New example : chmod +x; shebang '#!/usr/bin/env rash'; deterministic; ≤ ~40 lines; ≥1 assert; cleans up temp
Skip if     : needs network/service; non-deterministic diff; large/private assets
```

Minimal template:

```yaml
#!/usr/bin/env rash
- name: example
  command: echo hello
- assert:
    that:
      - lookup('command', 'echo hello').stdout contains 'hello'
```

## Testing Strategy

Unit tests inline or colocated; integration semantics covered by test/\*.rh; examples provide
feature smoke. Every added module: (1) unit tests where logic isolated, (2) example script if
user-facing behavior.

## Release / Cross

```
Pre-tag check : make lint test (gnu)
MUSL build    : CARGO_TARGET=x86_64-unknown-linux-musl make release
Artifact name : rash-<target>.tar.gz
Allocator     : jemallocator only on 64-bit musl
```

## Performance

```
Release profile : LTO=fat; panic=abort; strip symbols; 1 codegen-unit
Benchmarks      : cargo bench -p rash_core docopt
```

## Security / Safety

```
Validate inputs : command, get_url, template
Privilege        : honor 'become' semantics
Network          : no new hot-path I/O unless feature-gated
```

## Proc Macros & Docs

```
rash_derive : keep API minimal; bump minor on expansion
mdbook_rash : installed via make book; update docs on behavior change
```

## PR Checklist

```
[ ] make lint test
[ ] Docs & example added/updated for new module
[ ] No unwrap()/expect() outside tests code
[ ] Examples executable + deterministic
[ ] Version propagated if bumped
```

## Search Shortcuts

```
ENTRYPOINT MODULES JINJA DOCOPT ERROR LOGGER TEST_SCRIPTS EXAMPLES DOCS_SRC DOCS_OUT
```

Prefer these anchors before broad tree search.

## Guideline

Assume these instructions are authoritative; search only when implementing something not covered
here.
