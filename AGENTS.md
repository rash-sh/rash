# AGENTS.md

## Ephemeral Environment Rules

This agent runs in a short‑lived environment. Any uncommitted or unpushed work will be permanently
lost.

### Mandatory

1. **Commit frequently** — after every logical step.
2. **Push immediately after every commit** (retry up to 3 times).
3. **Never leave unpushed commits.**
4. **Use the correct branch**:
   - PR work → PR head branch
   - Issue work → `fix/<id>-short-desc` or `feat/<id>-short-desc`
   - Otherwise → current branch unless instructed

5. **Keep commits small and atomic.** Prefer multiple small commits.
6. **Do not commit secrets.**
7. **Before ending work:** run `git status` and ensure everything is committed and pushed.

**Rule of thumb:** If runner stops right now, no meaningful work should be lost.

## Project Overview

Rash is a declarative shell scripting language using Ansible-like YAML syntax, compiled to a single
Rust binary. It's designed for container entrypoints, IoT devices, and local scripting with zero
dependencies.

## Build Commands

```bash
make build              # Debug build
make test               # Lint + unit tests + integration tests
make lint               # fmt --check + clippy -D warnings
make lint-fix           # Auto-fix formatting and clippy issues
```

## Code Conventions

- Zero clippy warnings enforced with `-D warnings`
- No `unwrap()`/`expect()` outside of tests - use `?` with contextual errors
- Function size: Keep under ~60 lines
- Every new module feature needs both unit tests and an example

## Testing

```bash
# Run specific Rust test
cargo test -p rash_core test_name

# Run integration test script
cargo run --bin rash test/path/to/test.rh
```
