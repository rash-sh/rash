# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## About Rash

Rash is a declarative shell scripting language using Ansible-like YAML syntax, compiled to a single Rust binary. It's designed for container entrypoints, IoT devices, and local scripting with zero dependencies. Scripts use `.rh` extension and are executable with `#!/usr/bin/env rash` shebang.

## Essential Commands

### Building and Testing

```bash
# IMPORTANT: Always use Make targets, never raw cargo commands
make build              # Debug build (uses cargo or cross based on target)
make release            # Release build for current platform
make test               # Lint + unit tests + integration tests (*.rh scripts)
make lint               # fmt --check + clippy -D warnings
make lint-fix           # Auto-fix formatting and clippy issues

# Cross-compilation
make release CARGO_TARGET=x86_64-unknown-linux-musl  # MUSL build
```

### Testing Specific Components

```bash
# Run specific Rust test
cargo test -p rash_core test_name

# Run specific integration test script
cargo run --bin rash test/path/to/test.rh

# Run examples (smoke tests)
make test-examples

# Single example
./examples/builtins.rh
```

### Documentation

```bash
make book VERSION=master  # Build mdbook docs
```

### Environment Variables

```bash
RASH_LOG_LEVEL=DEBUG    # Set log level (DEBUG, TRACE)
```

## Architecture Overview

### Workspace Structure

- **rash_core**: Main crate containing the engine and CLI binary
  - `src/bin/rash.rs`: CLI entry point
  - `src/modules/`: All built-in modules (assert, command, copy, file, etc.)
  - `src/jinja/`: MiniJinja templating integration
  - `src/docopt/`: Docopt parser for script CLI interfaces
  - `src/context.rs`: Execution context managing tasks and variables
  - `src/task/`: Task parsing and execution
  - `src/vars/`: Variable management and builtins
  - `tests/cli/`: Integration tests
  - `tests/mocks/`: Mock commands for testing (e.g., mock dconf)
- **rash_derive**: Procedural macros
- **mdbook_rash**: Documentation preprocessor

### Execution Flow

1. **CLI Parsing** (`src/bin/rash.rs`): Parse args via clap, read script file
2. **Docopt Parsing** (`src/docopt/`): Extract Usage block from script, parse script_args
3. **Task Parsing** (`src/task/`): Parse YAML tasks from script file
4. **Context Creation** (`src/context.rs`): Combine tasks, env vars, and builtins
5. **Task Execution**: Loop through tasks, executing modules with Jinja2 templating
6. **Module Execution** (`src/modules/`): Each module implements `Module` trait

### Key Concepts

- **Tasks**: YAML blocks defining operations (similar to Ansible tasks)
- **Modules**: Rust structs implementing the `Module` trait (in `src/modules/`)
- **Context**: Maintains task queue and variable state during execution
- **Builtins**: Special variables accessible in templates (`{{ env }}`, `{{ rash.path }}`, etc.)
- **Global Params**: `become`, `become_user`, `check_mode` affect all tasks
- **Check Mode**: Dry-run mode (`--check` flag) where modules report changes without modifying

## Module System

### Adding a New Module

1. Create `rash_core/src/modules/mymodule.rs`:
   ```rust
   use crate::modules::{Module, ModuleResult, parse_params};
   use serde::Deserialize;

   #[derive(Debug)]
   pub struct MyModule;

   #[derive(Deserialize)]
   struct Params {
       // Define parameters
   }

   impl Module for MyModule {
       fn get_name(&self) -> &str { "mymodule" }

       fn exec(&self, global_params: &GlobalParams, params: YamlValue,
               vars: &Value, check_mode: bool) -> Result<(ModuleResult, Option<Value>)> {
           let params: Params = parse_params(params)?;
           // Implement logic
           Ok((ModuleResult::new(changed, extra, output), updated_vars))
       }
   }
   ```

2. Register in `rash_core/src/modules/mod.rs`:
   - Add `mod mymodule;`
   - Add `use crate::modules::mymodule::MyModule;`
   - Add to `MODULES` HashMap: `(MyModule.get_name(), Box::new(MyModule))`

3. Add integration tests in `rash_core/tests/cli/modules/mymodule.rs`

4. Create example in `examples/mymodule.rh`

### Module Traits

- `force_string_on_params()`: Default `true` - override if module needs non-string types
- `get_json_schema()`: For docs feature, return JSON schema

### Testing Modules

- **Unit tests**: Inline or in module file
- **Integration tests**: `rash_core/tests/cli/modules/` using actual `.rh` scripts or Rust test functions
- **Mocks**: Create mock commands in `tests/mocks/` (injected via PATH) for external tools

## Code Conventions

### Strict Standards

- **Zero clippy warnings**: Enforced with `-D warnings`
- **No `unwrap()`/`expect()`**: Use `?` with contextual errors (except in tests)
- **Function size**: Keep under ~60 lines; prefer iterators/itertools
- **Logging**: Only through `fern` (logger.rs); use `info!`, `debug!`, `trace!` macros
- **Binary size**: Justify heavy dependencies; prefer workspace dependency unification

### Error Handling

- Use `Result<T>` from `crate::error`
- Create errors with `Error::new(ErrorKind::*, inner_error)`
- Provide context: `map_err(|e| Error::new(ErrorKind::InvalidData, e))`

### Testing Philosophy

- Unit tests for isolated logic
- Integration tests (`test/*.rh` + `tests/cli/`) for end-to-end semantics
- Examples provide feature smoke tests
- Every new module needs both unit tests and an example

## Jinja Templating

- Uses MiniJinja with features: `loader`, `json`, `loop_controls`
- Available in all module parameters
- Builtins accessible: `{{ env.USER }}`, `{{ rash.path }}`, `{{ rash.argv }}`
- Filters supported: `default`, `split`, `last`, etc.

## Release Process

```bash
make lint test                          # Verify on GNU target
make release CARGO_TARGET=x86_64-unknown-linux-musl  # MUSL build
make update-version                     # Propagate version changes
make update-changelog                   # Update CHANGELOG.md
```

## Commit Message Format

Required format (enforced by pre-commit hook):

```
<type>(<scope>): <what changed>

<why this change was made>

Signed-off-by: Name <email>
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`

## Pre-commit Hooks

Install with:
```bash
pre-commit install
pre-commit install --hook-type commit-msg
```

Hooks run `cargo fmt` and `cargo clippy` automatically.

## Performance Notes

- Release profile: `LTO=fat`, `panic=abort`, `strip=symbols`, 1 codegen-unit
- Benchmarks: `cargo bench -p rash_core docopt`
- jemalloc used on 64-bit MUSL targets for better performance

## Module Examples Format

Keep examples:
- Executable with `#!/usr/bin/env rash` shebang
- Deterministic (no network/random behavior)
- Under ~40 lines
- Include at least one `assert` to verify behavior
- Clean up temp files/state

Excluded from `make test-examples`: `envar-api-gateway/`, `diff.rh`, `dotfiles/`, `user.rh`, `group.rh`

## Important Files

- `Cargo.toml`: Workspace root, defines version for all crates
- `Makefile`: All build/test commands
- `cliff.toml`: Changelog generator config
- `.pre-commit-config.yaml`: Pre-commit hooks
- `.commitlintrc.json`: Commit message validation

## CI/CD Notes

- GitHub Actions workflow in `.github/workflows/rust.yml`
- Tests run on push to master and PRs
- Docker images built for releases
- MUSL target used for container images

## Target Platforms

Primary: Linux (GNU and MUSL)
MSRV: Rust 1.88
