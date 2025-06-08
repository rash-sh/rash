# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.9.12](https://github.com/rash-sh/rash/tree/v2.9.12) - 2025-06-08

### Fixed

- task: Include task vars on module exec

### Documentation

- Fix index ref in Rash book

### Build

- deps: Update Rust crate nix to 0.30
- deps: Update Rust crate nix to v0.30.1
- deps: Update Rust crate minijinja to v2.10.1
- deps: Update Rust crate minijinja to v2.10.2
- deps: Update Rust crate clap to v4.5.38
- deps: Update rust Docker tag to v1.87.0
- deps: Update Rust crate criterion to 0.6.0
- deps: Update Rust crate clap to v4.5.39
- Update nix package to 0.30 and Cargo.lock

## [v2.9.11](https://github.com/rash-sh/rash/tree/v2.9.11) - 2025-04-30

### Documentation

- Revert rename index to introduction

### Build

- deps: Update Rust crate chrono to v0.4.41

## [v2.9.10](https://github.com/rash-sh/rash/tree/v2.9.10) - 2025-04-27

### Documentation

- Rename index to introduction

### Build

- ci: Migrate config renovate.json5
- ci: Update ubuntu runners
- deps: Update rust Docker tag to v1.86.0
- deps: Update Rust crate clap to v4.5.36
- deps: Bump crossbeam-channel from 0.5.13 to 0.5.15
- deps: Update Rust crate clap to v4.5.37
- deps: Update Rust crate proc-macro2 to v1.0.95
- deps: Update Rust crate syn to v2.0.101
- Update `ipc-channel` to `82f6c49`

### Testing

- ci: Remove body max line length limitation in commitlint

## [v2.9.9](https://github.com/rash-sh/rash/tree/v2.9.9) - 2025-04-01

### Build

- ci: Fix curl rash installation
- deps: Update Rust crate mdbook to v0.4.48
- deps: Update Rust crate env_logger to v0.11.8

## [v2.9.8](https://github.com/rash-sh/rash/tree/v2.9.8) - 2025-04-01

### Build

- ci: Add sha256sum to release binaries
- ci: Add AUR bin package
- ci: Fix sha256 calc on macOs and add them to gitignore
- deps: Update Rust crate minijinja to v2.9.0
- deps: Update Rust crate clap to v4.5.35

### Refactor

- ci: Format YAML

## [v2.9.7](https://github.com/rash-sh/rash/tree/v2.9.7) - 2025-03-26

### Fixed

- core: Add hashset in expand_usages to avoid re-analyzing candidates
- core: Allow targets and option params with `=` in docopt

### Documentation

- Update README with basic example

### Build

- deps: Update Rust crate log to v0.4.27

## [v2.9.6](https://github.com/rash-sh/rash/tree/v2.9.6) - 2025-03-25

### Fixed

- core: Improve docopt parsing performance pruning option usages
- core: Replace smallest regex by ordering matches in docopt
- core: Support option params with `=` in docopt

### Documentation

- core: Fix options usage and add a test

### Testing

- core: Replace `.py` with `.rh` in docopt

## [v2.9.5](https://github.com/rash-sh/rash/tree/v2.9.5) - 2025-03-23

### Fixed

- core: Change iter to VecDeque to avoid stack overflow calc usages

## [v2.9.4](https://github.com/rash-sh/rash/tree/v2.9.4) - 2025-03-22

### Fixed

- core: Docopt args replace `-` with `_` in vars

## [v2.9.3](https://github.com/rash-sh/rash/tree/v2.9.3) - 2025-03-20

### Fixed

- core: Docopt edge cases with multiple options and commands
- core: Now docopt positional arguments in uppercase are supported

### Documentation

- core: Enhance docopt section with examples and clarifications

### Build

- deps: Update rust Docker tag to v1.85.1
- deps: Update Rust crate tempfile to v3.19.1

## [v2.9.2](https://github.com/rash-sh/rash/tree/v2.9.2) - 2025-03-18

### Fixed

- core: Fixes docopt command that contains dashes

### Build

- core: Update to 2024 edition
- deps: Update Rust crate clap to v4.5.29
- deps: Update Rust crate tempfile to v3.17.1
- deps: Update Rust crate clap to v4.5.30
- deps: Update strum monorepo to v0.27.1
- deps: Update Rust crate mdbook to v0.4.45
- deps: Update KSXGitHub/github-actions-deploy-aur action to v4
- deps: Update Rust crate serde_json to v1.0.139
- deps: Update Rust crate serde to v1.0.218
- deps: Update Rust crate clap to v4.5.31
- deps: Update Rust crate proc-macro2 to v1.0.94
- deps: Update Rust crate serde_json to v1.0.140
- deps: Update Rust crate quote to v1.0.39
- deps: Update Rust crate syn to v2.0.99
- deps: Update Rust crate semver to v1.0.26
- deps: Update Rust crate log to v0.4.26
- deps: Update Rust crate schemars to v0.8.22
- deps: Update Rust crate chrono to v0.4.40
- deps: Update Rust crate console to v0.15.11
- deps: Update Rust crate minijinja to v2.8.0
- deps: Update Rust crate tempfile to v3.18.0
- deps: Update Rust crate serde to v1.0.219
- deps: Update Rust crate syn to v2.0.100
- deps: Update Rust crate clap to v4.5.32
- deps: Update Rust crate quote to v1.0.40
- deps: Update Rust crate mdbook to v0.4.47
- deps: Update Rust crate env_logger to v0.11.7
- deps: Update Rust crate tempfile to v3.19.0
- deps: Update rust Docker tag to v1.85.0

## [v2.9.1](https://github.com/rash-sh/rash/tree/v2.9.1) - 2025-02-09

### Fixed

- ci: Clean `release.sh` duplicated steps
- Cargo clippy errors

### Documentation

- lookup: Update find example with new minijinja sintax

### Build

- ci: Use mdbook version from `Cargo.lock`
- ci: Disables concurrent builds in pages deploy
- deps: Update Rust crate clap to v4.5.21
- deps: Update Rust crate serde_json to v1.0.133
- deps: Update Rust crate byte-unit to v5.1.6
- deps: Update Rust crate mdbook to v0.4.42
- deps: Update Rust crate serde to v1.0.215
- deps: Update Rust crate syn to v2.0.87
- deps: Update Rust crate tempfile to v3.14.0
- deps: Update Rust crate prs-lib to v0.5.2
- deps: Update Rust crate syn to v2.0.89
- deps: Update Rust crate proc-macro2 to v1.0.92
- deps: Update Rust crate mdbook to v0.4.43
- deps: Update Rust crate syn to v2.0.90
- deps: Update rust Docker tag to v1.83.0
- deps: Update Rust crate clap to v4.5.22
- deps: Update Rust crate clap to v4.5.23
- deps: Update Rust crate serde to v1.0.216
- deps: Update Rust crate chrono to v0.4.39
- deps: Update Rust crate semver to v1.0.24
- deps: Update Rust crate fern to v0.7.1
- deps: Update Rust crate console to v0.15.10
- deps: Update wagoid/commitlint-github-action action to v6.2.0
- deps: Update Rust crate serde_json to v1.0.134
- deps: Update Rust crate syn to v2.0.91
- deps: Update Rust crate quote to v1.0.38
- deps: Update Rust crate syn to v2.0.92
- deps: Update Rust crate serde to v1.0.217
- deps: Update Rust crate syn to v2.0.93
- deps: Update Rust crate syn to v2.0.94
- deps: Update Rust crate syn to v2.0.95
- deps: Update Rust crate serde_json to v1.0.135
- deps: Update Rust crate clap to v4.5.24
- deps: Update Rust crate minijinja to v2.6.0
- deps: Update Rust crate clap to v4.5.26
- deps: Update Rust crate syn to v2.0.96
- deps: Update Rust crate proc-macro2 to v1.0.93
- deps: Update Rust crate env_logger to v0.11.6
- deps: Update Rust crate serde_with to v3.12.0
- deps: Update Rust crate itertools to 0.14
- deps: Update Rust crate tempfile to v3.15.0
- deps: Update rust Docker tag to v1.84.0
- deps: Update clechasseur/rs-clippy-check action to v4
- deps: Update wagoid/commitlint-github-action action to v6.2.1
- deps: Update Rust crate serde_json to v1.0.136
- deps: Update Rust crate semver to v1.0.25
- deps: Update Rust crate serde_json to v1.0.137
- deps: Update Rust crate clap to v4.5.27
- deps: Update Rust crate log to v0.4.25
- deps: Update Rust crate similar to v2.7.0
- deps: Update Rust crate serde_json to v1.0.138
- deps: Update rust Docker tag to v1.84.1
- deps: Update Rust crate syn to v2.0.97
- deps: Update Rust crate syn to v2.0.98
- deps: Update Rust crate clap to v4.5.28
- Compile just rash bin in make target build and release
- Filter rash binary in AUR packages

### Refactor

- derive: Simplify returns with `.into()` method
- derive: Remove dead code imports
- derive: Reuse imports

## [v2.9.0](https://github.com/rash-sh/rash/tree/v2.9.0) - 2024-11-11

### Build

- ci: Change images to GitHub registry
- deps: Update Rust crate minijinja to v2.5.0
  - Added a `lines` filter to split a string into lines.
  - Added the missing `string` filter from Jinja2. mitsuhiko/minijinja#617
  - and more: [2.5.0](https://github.com/mitsuhiko/minijinja/releases/tag/2.5.0) and
    [2.4.0](https://github.com/mitsuhiko/minijinja/releases/tag/2.5.0)

### Testing

- cli: Disable e2e tests for ARM

## [v2.8.0](https://github.com/rash-sh/rash/tree/v2.8.0) - 2024-11-07

### Added

- cli: Add `script` argument for inline script
- deps: Enable `loop_controls` feature in minijinja

### Build

- deps: Update Rust crate serde-error to v0.1.3
- deps: Update Rust crate serde to v1.0.214

## [v2.7.6](https://github.com/rash-sh/rash/tree/v2.7.6) - 2024-10-24

### Fixed

- book: Change static to const
- ci: Clippy Github Action name typo
- task: Delete `special.rs` file not in use
- Formatting issues

### Build

- deps: Update Rust crate proc-macro2 to v1.0.87
- deps: Update Rust crate clap to v4.5.20
- deps: Update Rust crate proc-macro2 to v1.0.88
- deps: Update Rust crate ipc-channel to 0.19
- deps: Update Rust crate serde_json to v1.0.129
- deps: Update Rust crate serde_json to v1.0.130
- deps: Update Rust crate serde_json to v1.0.131
- deps: Update Rust crate serde_json to v1.0.132
- deps: Update Rust crate syn to v2.0.80
- deps: Update Rust crate syn to v2.0.81
- deps: Update Rust crate syn to v2.0.82
- deps: Update rust Docker tag to v1.82.0
- deps: Update Rust crate serde to v1.0.211
- deps: Update Rust crate proc-macro2 to v1.0.89
- deps: Update Rust crate serde to v1.0.212
- deps: Update Rust crate serde to v1.0.213
- deps: Update Rust crate syn to v2.0.83
- deps: Update Rust crate syn to v2.0.84
- deps: Update Rust crate syn to v2.0.85
- deps: Update Rust crate regex to v1.11.1
- deps: Update Rust crate fern to 0.7.0

### Refactor

- core: Remove String from function arg
- Refactored get_module_name method

## [v2.7.5](https://github.com/rash-sh/rash/tree/v2.7.5) - 2024-10-06

### Build

- Add jemalloc for musl

## [v2.7.4](https://github.com/rash-sh/rash/tree/v2.7.4) - 2024-10-06

### Documentation

- vars: Fix debug function call

### Build

- deps: Update Rust crate clap to v4.5.18
- deps: Update Rust crate syn to v2.0.79
- deps: Update Rust crate tempfile to v3.13.0
- deps: Update Rust crate regex to v1.11.0
- deps: Update Rust crate clap to v4.5.19
- deps: Update Rust crate serde_with to v3.10.0
- deps: Update Rust crate serde_with to v3.11.0
- deps: Update Rust crate ipc-channel to v0.18.3
- Optimize release binary

## [v2.7.3](https://github.com/rash-sh/rash/tree/v2.7.3) - 2024-09-18

### Added

- ci: Add release.sh script

### Fixed

- vars: Make `rash.path` canonical for coherence with `rash.dir`

### Build

- deps: Update Rust crate **minijinja** to v2.3.1
- deps: Update Rust crate clap to v4.5.17
- deps: Update Rust crate serde_json to v1.0.128
- deps: Update Rust crate serde to v1.0.210
- deps: Update rust to v1.81
- deps: Update Rust crate syn to v2.0.77
- deps: Update Rust crate ignore to v0.4.23
- deps: Remove pinned versions from `Cargo.toml`
- docker: Update target base image version to trixie-20240904-slim
- Remove death code

### Testing

- module: Add e2e for include

## [v2.7.2](https://github.com/rash-sh/rash/tree/v2.7.2) - 2024-09-16

### Fixed

- task: Add serde to handle result from fork in become tasks

### Documentation

- lookup: Add example and comments to passwordstore examples
- Add to changelog missing info for v2.7.1

### Refactor

- vars: Simplify the builtin vars implementation

## [v2.7.1](https://github.com/rash-sh/rash/tree/v2.7.1) - 2024-09-15

### Fixed

- core: Add script path to task name output
- module: Include continue workflow in the previous context

## [v2.7.0](https://github.com/rash-sh/rash/tree/v2.7.0) - 2024-09-15

### Added

- lookup: Add `subkey` option to passwordstore

### Build

- deps: Change clippy to clechasseur/rs-clippy-check action to v3

## [v2.6.0](https://github.com/rash-sh/rash/tree/v2.6.0) - 2024-09-15

### Added

- module: Add include

### Documentation

- Update dotfiles example refactorized

## [v2.5.0](https://github.com/rash-sh/rash/tree/v2.5.0) - 2024-09-10

### Added

- lookup: Add `returnall` option to passwordstore

## [v2.4.0](https://github.com/rash-sh/rash/tree/v2.4.0) - 2024-09-10

### Added

- module: Make `render_params` force string optional

### Fixed

- ci: Remove `fetch-depth: 0` to get just last commit on commitlint
- ci: Add permissions to commitlint action

### Documentation

- lookup: Remove TODO as completed
- Add find lookup example and update dots script
- Update dots example

### Build

- deps: Update Rust crate syn to v2.0.75
- deps: Update wagoid/commitlint-github-action action to v6.1.0
- deps: Update wagoid/commitlint-github-action action to v6.1.1
- deps: Update KSXGitHub/github-actions-deploy-aur action to v3
- deps: Update Rust crate quote to v1.0.37
- deps: Update Rust crate serde_json to v1.0.127
- deps: Update Rust crate serde to v1.0.209
- deps: Update Rust crate syn to v2.0.76
- deps: Update Rust crate minijinja to v2.2.0
- deps: Update KSXGitHub/github-actions-deploy-aur action to v3.0.1
- deps: Update wagoid/commitlint-github-action action to v6.1.2
- deps: Update rust Docker tag to v1.81.0

### Refactor

- core: Merge `minijinja::Value` instead of using json
- core: Replace minijinja value by serde_json in docopt
- core: Improbe `merge_json` performance
- core: Small tweak in parse function in docopt
- jinja: Expose render with `force_string` functions
- jinja: Improve `Value` transformations
- lookup: Direct serde between `Params` and `minijinja::Value`

### Testing

- module: Add `set_vars.rh` to examples

## [v2.3.1](https://github.com/rash-sh/rash/tree/v2.3.1) - 2024-08-15

### Fixed

- task: Render iterator when item used in vars

### Documentation

- Order changelog groups

## [v2.3.0](https://github.com/rash-sh/rash/tree/v2.3.0) - 2024-08-15

### Added

- lookup: Add find reusing module logic

### Build

- deps: Update Rust crate serde_json to v1.0.125
- deps: Update Rust crate serde to v1.0.208

### Fixed

- task: Support `omit` in `vars`
- task: Render params recursivey and respect omit
- task: Use vars to render iterator loop

## [v2.2.0](https://github.com/rash-sh/rash/tree/v2.2.0) - 2024-08-14

### Build

- deps: Update Rust crate serde to v1.0.207

### Fixed

- jinja: Omit not trigger error when default variable exists
  - **BREAKING**: use `default(omit)` instead of `default(omit())`.

## [v2.1.1](https://github.com/rash-sh/rash/tree/v2.1.1) - 2024-08-11

### Build

- deps: Update Rust crate serde_json to v1.0.123

### Fixed

- task: Render vars recursively

## [v2.1.0](https://github.com/rash-sh/rash/tree/v2.1.0) - 2024-08-11

### Added

- jinja: Enable `tojson` filter from minijinja
- lookup: Add passwordstore

### Build

- deps: Update Rust crate clap to v4.5.15
- deps: Update Rust crate syn to v2.0.73
- deps: Update Rust crate serde to v1.0.206
- deps: Update Rust crate syn to v2.0.74

### Documentation

- jinja: Add lookups programmatically to Rash book
- jinja: Add section with lookups and filters
- Replace Tera doc with MiniJinja
- Add debug vars and context info
- Fix index

### Fixed

- module: `set_vars` overwrites previous variables

### Refactor

- jinja: Add macro for generating add lookup function
- module: Move module::utils to utils
- task: Change `test_render_params_with_vars_array_concat`
- Create jinja module

### Testing

- task: Add vars concat arrays test

## [v2.0.1](https://github.com/rash-sh/rash/tree/v2.0.1) - 2024-08-09

### Build

- Remove armhf build

### Documentation

- Update examples with MiniJinja breacking changes

### Fixed

- Minor docs and refactors

### Refactor

- Use minijinja::Value instead of Vars abstraction

### Testing

- task: Check item is removed from vars after execute loop task

## [v2.0.0](https://github.com/rash-sh/rash/tree/v2.0.0) - 2024-08-09

### **BREAKING**

Replaced Tera with Minijinja, enhancing the project's versatility and bringing near-complete
compatibility with Jinja2 syntax. This upgrade resolves several critical issues, including improved
handling of `()` in expressions.

With Minijinja, Rash now overcomes the limitations previously imposed by the Jinja2 engine.

### Build

- deps: Update Rust crate serde to v1.0.204
- deps: Update Rust crate syn to v2.0.69
- deps: Update Rust crate syn to v2.0.70
- deps: Update Rust crate clap to v4.5.9
- deps: Update Rust crate syn to v2.0.71
- deps: Update Rust crate syn to v2.0.72
- deps: Update Rust crate clap to v4.5.10
- deps: Update Rust crate similar to v2.6.0
- deps: Update Rust crate serde_with to v3.9.0
- deps: Update Rust crate env_logger to v0.11.4
- deps: Update Rust crate clap to v4.5.11
- deps: Update Rust crate serde_json to v1.0.121
- deps: Update Rust crate clap to v4.5.12
- deps: Update Rust crate clap to v4.5.13
- deps: Update Rust crate serde_json to v1.0.122
- deps: Update Rust crate regex to v1.10.6
- deps: Update wagoid/commitlint-github-action action to v6.0.2
- deps: Update Rust crate tempfile to v3.12.0
- deps: Update Rust crate serde to v1.0.205
- deps: Update Rust crate clap to v4.5.14
- deps: Update rust Docker tag to v1.80.1

### Documentation

- Change from list to script in release workflow

### Refactor

- tera: Change Jinja2 engine for minijinja
- Replace lazy_static with std from 1.80

## [v1.10.5](https://github.com/rash-sh/rash/tree/v1.10.5) - 2024-07-04

### Fixed

- module: Not display for Content::Bytes in Copy

### Refactor

- module: Improve readalability in Copy

## [v1.10.4](https://github.com/rash-sh/rash/tree/v1.10.4) - 2024-07-04

### Build

- deps: Update Rust crate serde_json to v1.0.118
- deps: Update Rust crate log to v0.4.22
- deps: Update Rust crate clap to v4.5.8
- deps: Update Rust crate serde_json to v1.0.119
- deps: Update Rust crate serde_with to v3.8.2
- deps: Update Rust crate serde_json to v1.0.120
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.2
- deps: Update Rust crate serde_with to v3.8.3

### Fixed

- module: Copy binary data

## [v1.10.3](https://github.com/rash-sh/rash/tree/v1.10.3) - 2024-06-24

### Build

- deps: Update Rust crate lazy_static to v1.5.0
- deps: Update Rust crate syn to v2.0.68
- deps: Update Rust crate strum to v0.26.3
- Fix AUR gpg key fingerprint

## [v1.10.2](https://github.com/rash-sh/rash/tree/v1.10.2) - 2024-06-21

### Added

- ci: Add automerge in patch versions for renovate
- ci: Add autotag workflow

### Build

- deps: Update Rust crate nix to 0.28
- deps: Update softprops/action-gh-release action to v2
- deps: Update mindsers/changelog-reader-action action to v2.2.3
- deps: Bump mio from 0.8.10 to 0.8.11
- deps: Update Rust crate serde_with to 3.7
- deps: Update wagoid/commitlint-github-action action to v6
- deps: Update Rust crate similar to 2.5
- deps: Update rust Docker tag to v1.77.0
- deps: Update rust Docker tag to v1.77.1
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.1
- deps: Update wagoid/commitlint-github-action action to v6.0.1
- deps: Update Rust crate serde_with to 3.8
- deps: Update Rust crate clap to 4.5.4
- deps: Update Rust crate criterion to 0.5.1
- deps: Update rust Docker tag to v1.77.2
- deps: Update Rust crate byte-unit to 5.1.4
- deps: Update Rust crate cargo-husky to 1.5.0
- deps: Update Rust crate chrono to 0.4.38
- deps: Update Rust crate fern to 0.6.2
- deps: Update Rust crate env_logger to 0.11.3
- deps: Update Rust crate ignore to 0.4.22
- deps: Update Rust crate itertools to 0.12.1
- deps: Update Rust crate log to 0.4.21
- deps: Update Rust crate proc-macro2 to 1.0.81
- deps: Update Rust crate regex to 1.10.4
- deps: Update Rust crate semver to 1.0.22
- deps: Update Rust crate serde to 1.0.200
- deps: Update Rust crate serde_with to 3.8.1
- deps: Update Rust crate serde_json to 1.0.116
- deps: Update Rust crate strum to 0.26.2
- deps: Update Rust crate quote to 1.0.36
- deps: Update Rust crate schemars to 0.8.17
- deps: Update Rust crate serde-error to 0.1.2
- deps: Update Rust crate tempfile to 3.10.1
- deps: Update rust Docker tag to v1.78.0
- deps: Update Rust crate strum_macros to 0.26.2
- deps: Update Rust crate tera to 1.19.1
- deps: Update Rust crate syn to 2.0.60
- deps: Update Rust crate serde_yaml to v0.9.34
- deps: Update Rust crate schemars to v0.8.18
- deps: Update Rust crate semver to v1.0.23
- deps: Update Rust crate proc-macro2 to v1.0.82
- deps: Update Rust crate serde to v1.0.201
- deps: Update Rust crate serde_json to v1.0.117
- deps: Update Rust crate syn to v2.0.62
- deps: Update Rust crate syn to v2.0.63
- deps: Update Rust crate serde to v1.0.202
- deps: Update Rust crate schemars to v0.8.19
- deps: Update peaceiris/actions-gh-pages action to v4
- deps: Update Rust crate mdbook to v0.4.38
- deps: Update Rust crate syn to v2.0.64
- deps: Update Rust crate itertools to 0.13.0
- deps: Update Rust crate mdbook to v0.4.40
- deps: Update Rust crate proc-macro2 to v1.0.83
- deps: Update Rust crate syn to v2.0.65
- deps: Update Rust crate schemars to v0.8.20
- deps: Update Rust crate schemars to v0.8.21
- deps: Update Rust crate syn to v2.0.66
- deps: Update Rust crate nix to 0.29
- deps: Update Rust crate proc-macro2 to v1.0.84
- deps: Update Rust crate serde to v1.0.203
- deps: Update Rust crate ipc-channel to v0.18.1
- deps: Update Rust crate proc-macro2 to v1.0.85
- deps: Update Rust crate strum_macros to v0.26.3
- deps: Update Rust crate clap to v4.5.6
- deps: Update Rust crate strum_macros to v0.26.4
- deps: Update Rust crate regex to v1.10.5
- deps: Update Rust crate clap to v4.5.7
- deps: Update Rust crate syn to v2.0.67
- deps: Update Rust crate proc-macro2 to v1.0.86
- deps: Update rust Docker tag to v1.79.0
- deps: Update Rust crate tera to v1.20.0

### Fixed

- ci: Automerge all patches
- Cargo clippy warnings

## [v1.10.1](https://github.com/rash-sh/rash/tree/v1.10.1) - 2024-02-23

### Added

- ci: Add renovate
- module: Add pacman
- module: Check pacman upgrades before execution

### Build

- book: Update mdbook to 0.4.34
- deps: Bump rustix from 0.37.23 to 0.37.25
- deps: Bump unsafe-libyaml from 0.2.9 to 0.2.10
- deps: Bump shlex from 1.2.0 to 1.3.0
- deps: Update Rust crate mdbook to 0.4.37
- deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.0
- deps: Update Rust crate term_size to 1.0.0-beta1
- deps: Update Rust crate itertools to 0.12
- deps: Update Rust crate regex to 1.10
- deps: Update Rust crate serde_with to 3.6
- deps: Update Rust crate strum to 0.26
- deps: Update Rust crate console to 0.15.8
- deps: Update Rust crate term_size to 1.0.0-beta.2
- deps: Update wagoid/commitlint-github-action action to v5
- deps: Update docker/setup-qemu-action action to v3
- deps: Update docker/setup-buildx-action action to v3
- deps: Update actions/checkout action to v4
- deps: Update rust Docker tag to v1.76.0
- deps: Update mindsers/changelog-reader-action action to v2.2.2
- deps: Update Rust crate env_logger to 0.11
- deps: Update Rust crate ipc-channel to 0.18
- deps: Update Rust crate similar to 2.4
- deps: Update Rust crate strum_macros to 0.26
- deps: Update Rust crate clap to 4.5
- deps: Update Rust crate byte-unit to v5
- deps: Update lock file
- docker: Update debian to latest bookworm version
- Compress binary with upx
- Fix macOS and push images
- Increase min rust version to 1.74

### Documentation

- ci: Remove patch versions from web page
- core: Add comment about tera bug
- module: Include pacman examples and remove new lines in params
- vars: Add debug command to show all vars in current context

### Fixed

- ci: Fix strip ref prefix from version in github pages action
- core: Log errors instead of trace
- core: Enable vars in when param
- core: Add log trace for extend vars
- core: Allow module log for empty output
- core: Log with colors just if terminal
- docker: Update to rust 1.72.0
- docker: Update to rust 1.75.0

### Refactor

- core: Replace match with and_then for readibility
- module: Add run_test function for pacman integration tests
- Replace to_string to to_owner when possible
- Remove match in favor of map if possible
- Remove some match statements

### Testing

- Add docopt benches

## [v1.10.0](https://github.com/rash-sh/rash/tree/v1.10.0) - 2023-09-12

### Added

- core: Add output option to print log raw mode

### Fixed

- ci: Run jobs just in PR or master branch
- deps: Remove users crate dependency

## [v1.9.0](https://github.com/rash-sh/rash/tree/v1.9.0) - 2023-09-07

### Added

- task: Add `vars` optional field

### Build

- Upgrade to Rust 1.70 and fix new clippy warnings
- Update compatible versions
- Upgrade incompatible versions
- Add memfd feature to ipc-channel
- Disable memfd for ipc-channel
- Set resolver = "2"

### Documentation

- Add dotfile description
- Fix readme typo

### Fixed

- ci: Update workers to latest versions
- ci: Upgrade cache action version to v2
- ci: Update to node16 github actions
- ci: Replace `actions-rs/toolchain` with `dtolnay/rust-toolchain`
- ci: Change dtolnay/rust-toolchaint to stable
- ci: Remove container and downgrade to ubuntu 20
- core: Improve docopt performance prefiltering possible options
- core: Handle docopt edge cases with optiona arguments
- task: Improve error message when become fails
- Cargo clippy errors

### Removed

- Command module: `transfer_pid_1` (use `transfer_pid` instead)

## [v1.8.6](https://github.com/rash-sh/rash/tree/v1.8.6) - 2023-01-27

### Added

- module: Support `chdir` in command module

### Build

- book: Update mdbook to 0.4.25
- deps: Bump prettytable-rs from 0.8.0 to 0.10.0
- Upgrade to Rust 1.67 and fix new clippy warnings

### Fixed

- ci: Remove build scope from commitlintrc
- core: Set up to current dir parent path when empty
- module: Add trace for command exec

## [v1.8.5](https://github.com/rash-sh/rash/tree/v1.8.5) - 2022-12-20

### Added

- Add `git-cliff` to update CHANGELOG automatically

### Build

- Upgrade to Rust 1.66 and fix new clippy warnings
- Add arm64 docker images

### Documentation

- Fix build status badget

### Fixed

- ci: Add local versions in dependencies
- cli: Change skipping log to debug

### Refactor

- module: Implement trait Module

## [v1.8.4](https://github.com/rash-sh/rash/tree/v1.8.4) (2022-10-24)

### Fixed

- ci: Read version from `Cargo.toml`

## [v1.8.3](https://github.com/rash-sh/rash/tree/v1.8.3) (2022-10-24) [YANKED]

### Fixed

- cli: Support repeated arguments in docopt (#281)
- cli: Help not ignored when positional required in docopt (#283)
- cli: Improve tera error handling and add a trace all verbose option (#287)
- docs: Add default values and fix examples (#285)

## [v1.8.2](https://github.com/rash-sh/rash/tree/v1.8.2) (2022-08-15)

### Fixed

- Fix multi-word variable repr for options when true in docopt (#274)

## [v1.8.1](https://github.com/rash-sh/rash/tree/v1.8.1) (2022-08-15)

### Fixed

- Fix multi-word variable repr for options in docopt (#273)

## [v1.8.0](https://github.com/rash-sh/rash/tree/v1.8.0) (2022-06-30)

### Added

- Support all data structures in loops (#263)

## [v1.7.1](https://github.com/rash-sh/rash/tree/v1.7.1) (2022-06-13)

### Fixed

- Update Debian image to bullseye and Rust to 1.61.0
- Bumps [regex](https://github.com/rust-lang/regex) from 1.5.4 to 1.5.5.
  - [Release notes](https://github.com/rust-lang/regex/releases)
  - [Changelog](https://github.com/rust-lang/regex/blob/master/CHANGELOG.md)
  - [Commits](https://github.com/rust-lang/regex/compare/1.5.4...1.5.5)
- Update ipc-channel to 0.16 and run `cargo update`

## [v1.7.0](https://github.com/rash-sh/rash/tree/v1.7.0) (2022-01-26)

### Added

- Rename `transfer_pid_1` to `transfer_pid` in command module
- Add module debug (#241)

## [v1.6.1](https://github.com/rash-sh/rash/tree/v1.6.1) (2022-01-22)

### Fixed

- Options variables are now accessible (#236)
- Update to Rust 1.58.1

## [v1.6.0](https://github.com/rash-sh/rash/tree/v1.6.0) (2022-01-20)

### Added

- Add parse options to docopt implementation (#232)
- Use `cross` for musl docker image (#232)

## [v1.5.0](https://github.com/rash-sh/rash/tree/v1.5.0) (2022-01-09)

### Added

- Add become (#220)
- Add `omit()` for omitting parameters programmatically (#70)
- Add preserve mode to copy module (#214)
- Add docopt to `rash` files (#212)

### Fixed

- Format mode in diff as octal in File module

## [v1.4.1](https://github.com/rash-sh/rash/tree/v1.4.1) (2021-12-24)

### Fixed

- Fix log with print in normal diff

## [v1.4.0](https://github.com/rash-sh/rash/tree/v1.4.0) (2021-12-22)

### Added

- Add find module

### Fixed

- Fix `rash.dir` as absolute according with docs
- Fix publish packages to crates.io

## [v1.3.1](https://github.com/rash-sh/rash/tree/v1.3.1) (2021-12-19)

### Added

- Automatically added body to GitHub release

### Fixed

- Update rash package versions in Cargo.lock (missing in 1.3.0)

## [v1.3.0](https://github.com/rash-sh/rash/tree/v1.3.0) (2021-12-19)

### Added

- Add `changed_when` optional field in task
- Add support for arrays in `when` and `changed_when`
- Add clearer logger for diff files
- Add src option to copy module
- Add `check` mode

### Fixed

- Parsed `when` and `changed_when` when they are booleans
- Builtin dir when current dir returns `.`
- Check `when` for each different item in loop
- Remove vendor on release

## [v1.2.0](https://github.com/rash-sh/rash/tree/v1.2.0) (2021-12-17)

### Added

- Add diff param and apply in file, copy and template modules (#190)
- Get params doc from struct (#189)

### Fixed

- Add warn and error to stderr instead of stdout
- Remove `--all-features` from release

## [v1.1.0](https://github.com/rash-sh/rash/tree/v1.1.0) (2021-12-12)

### Added

- Add file module (#180)

## [v1.0.2](https://github.com/rash-sh/rash/tree/v1.0.2) (2021-12-07)

### Added

- Add AUR packages automatic build and publish
- Release with signed tags
- Add releases binaries with Linux Glib >= 2.17 support and macOS

## [v1.0.1](https://github.com/rash-sh/rash/tree/v1.0.1) (2021-12-03)

### Bug fixes

- Remove duplicate error messages

## [v1.0.0](https://github.com/rash-sh/rash/tree/v1.0.0) (2020-06-11)

First stable version released:

### modules

- assert
- command
- copy
- template
- set_vars

### tasks

- when
- register
- ignore_errors
- loop

### vars

- rash
  - args
  - dir
  - path
  - user.uid
  - user.gid
- env

## v0.1.0

Core version released:

- data structure
- error management
- log
- execution
- cli

### modules

- add command (basic functionality)
