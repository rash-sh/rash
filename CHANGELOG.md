# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.10.1](https://github.com/rash-sh/rash/tree/v1.10.1) - 2024-02-23

### Added

* ci: Add renovate
* module: Add pacman
* module: Check pacman upgrades before execution

### Build

* book: Update mdbook to 0.4.34
* deps: Bump rustix from 0.37.23 to 0.37.25
* deps: Bump unsafe-libyaml from 0.2.9 to 0.2.10
* deps: Bump shlex from 1.2.0 to 1.3.0
* deps: Update Rust crate mdbook to 0.4.37
* deps: Update KSXGitHub/github-actions-deploy-aur action to v2.7.0
* deps: Update Rust crate term_size to 1.0.0-beta1
* deps: Update Rust crate itertools to 0.12
* deps: Update Rust crate regex to 1.10
* deps: Update Rust crate serde_with to 3.6
* deps: Update Rust crate strum to 0.26
* deps: Update Rust crate console to 0.15.8
* deps: Update Rust crate term_size to 1.0.0-beta.2
* deps: Update wagoid/commitlint-github-action action to v5
* deps: Update docker/setup-qemu-action action to v3
* deps: Update docker/setup-buildx-action action to v3
* deps: Update actions/checkout action to v4
* deps: Update rust Docker tag to v1.76.0
* deps: Update mindsers/changelog-reader-action action to v2.2.2
* deps: Update Rust crate env_logger to 0.11
* deps: Update Rust crate ipc-channel to 0.18
* deps: Update Rust crate similar to 2.4
* deps: Update Rust crate strum_macros to 0.26
* deps: Update Rust crate clap to 4.5
* deps: Update Rust crate byte-unit to v5
* deps: Update lock file
* docker: Update debian to latest bookworm version
* Compress binary with upx
* Fix macOS and push images
* Increase min rust version to 1.74

### Documentation

* ci: Remove patch versions from web page
* core: Add comment about tera bug
* module: Include pacman examples and remove new lines in params
* vars: Add debug command to show all vars in current context

### Fixed

* ci: Fix strip ref prefix from version in github pages action
* core: Log errors instead of trace
* core: Enable vars in when param
* core: Add log trace for extend vars
* core: Allow module log for empty output
* core: Log with colors just if terminal
* docker: Update to rust 1.72.0
* docker: Update to rust 1.75.0

### Refactor

* core: Replace match with and_then for readibility
* module: Add run_test function for pacman integration tests
* Replace to_string to to_owner when possible
* Remove match in favor of map if possible
* Remove some match statements

### Testing

* Add docopt benches

## [v1.10.0](https://github.com/rash-sh/rash/tree/v1.10.0) - 2023-09-12

### Added

* core: Add output option to print log raw mode

### Fixed

* ci: Run jobs just in PR or master branch
* deps: Remove users crate dependency

## [v1.9.0](https://github.com/rash-sh/rash/tree/v1.9.0) - 2023-09-07

### Added

* task: Add `vars` optional field

### Build

* Upgrade to Rust 1.70 and fix new clippy warnings
* Update compatible versions
* Upgrade incompatible versions
* Add memfd feature to ipc-channel
* Disable memfd for ipc-channel
* Set resolver = "2"

### Documentation

* Add dotfile description
* Fix readme typo

### Fixed

* ci: Update workers to latest versions
* ci: Upgrade cache action version to v2
* ci: Update to node16 github actions
* ci: Replace `actions-rs/toolchain` with `dtolnay/rust-toolchain`
* ci: Change dtolnay/rust-toolchaint to stable
* ci: Remove container and downgrade to ubuntu 20
* core: Improve docopt performance prefiltering possible options
* core: Handle docopt edge cases with optiona arguments
* task: Improve error message when become fails
* Cargo clippy errors

### Removed

* Command module: `transfer_pid_1` (use `transfer_pid` instead)

## [v1.8.6](https://github.com/rash-sh/rash/tree/v1.8.6) - 2023-01-27

### Added

* module: Support `chdir` in command module

### Build

* book: Update mdbook to 0.4.25
* deps: Bump prettytable-rs from 0.8.0 to 0.10.0
* Upgrade to Rust 1.67 and fix new clippy warnings

### Fixed

* ci: Remove build scope from commitlintrc
* core: Set up to current dir parent path when empty
* module: Add trace for command exec

## [v1.8.5](https://github.com/rash-sh/rash/tree/v1.8.5) - 2022-12-20

### Added

* Add `git-cliff` to update CHANGELOG automatically

### Build

* Upgrade to Rust 1.66 and fix new clippy warnings
* Add arm64 docker images

### Documentation

* Fix build status badget

### Fixed

* ci: Add local versions in dependencies
* cli: Change skipping log to debug

### Refactor

* module: Implement trait Module

## [v1.8.4](https://github.com/rash-sh/rash/tree/v1.8.4) (2022-10-24)

### Fixed

* ci: Read version from `Cargo.toml`

## [v1.8.3](https://github.com/rash-sh/rash/tree/v1.8.3) (2022-10-24) [YANKED]

### Fixed

* cli: Support repeated arguments in docopt (#281)
* cli: Help not ignored when positional required in docopt (#283)
* cli: Improve tera error handling and add a trace all verbose option (#287)
* docs: Add default values and fix examples (#285)

## [v1.8.2](https://github.com/rash-sh/rash/tree/v1.8.2) (2022-08-15)

### Fixed

* Fix multi-word variable repr for options when true in docopt (#274)

## [v1.8.1](https://github.com/rash-sh/rash/tree/v1.8.1) (2022-08-15)

### Fixed

* Fix multi-word variable repr for options in docopt (#273)

## [v1.8.0](https://github.com/rash-sh/rash/tree/v1.8.0) (2022-06-30)

### Added

* Support all data structures in loops (#263)

## [v1.7.1](https://github.com/rash-sh/rash/tree/v1.7.1) (2022-06-13)

### Fixed

* Update Debian image to bullseye and Rust to 1.61.0
* Bumps [regex](https://github.com/rust-lang/regex) from 1.5.4 to 1.5.5.
  * [Release notes](https://github.com/rust-lang/regex/releases)
  * [Changelog](https://github.com/rust-lang/regex/blob/master/CHANGELOG.md)
  * [Commits](https://github.com/rust-lang/regex/compare/1.5.4...1.5.5)
* Update ipc-channel to 0.16 and run `cargo update`

## [v1.7.0](https://github.com/rash-sh/rash/tree/v1.7.0) (2022-01-26)

### Added

* Rename `transfer_pid_1` to `transfer_pid` in command module
* Add module debug (#241)

## [v1.6.1](https://github.com/rash-sh/rash/tree/v1.6.1) (2022-01-22)

### Fixed

* Options variables are now accessible (#236)
* Update to Rust 1.58.1

## [v1.6.0](https://github.com/rash-sh/rash/tree/v1.6.0) (2022-01-20)

### Added

* Add parse options to docopt implementation (#232)
* Use `cross` for musl docker image (#232)

## [v1.5.0](https://github.com/rash-sh/rash/tree/v1.5.0) (2022-01-09)

### Added

* Add become (#220)
* Add `omit()` for omitting parameters programmatically (#70)
* Add preserve mode to copy module (#214)
* Add docopt to `rash` files (#212)

### Fixed

* Format mode in diff as octal in File module

## [v1.4.1](https://github.com/rash-sh/rash/tree/v1.4.1) (2021-12-24)

### Fixed

* Fix log with print in normal diff

## [v1.4.0](https://github.com/rash-sh/rash/tree/v1.4.0) (2021-12-22)

### Added

* Add find module

### Fixed

* Fix `rash.dir` as absolute according with docs
* Fix publish packages to crates.io

## [v1.3.1](https://github.com/rash-sh/rash/tree/v1.3.1) (2021-12-19)

### Added

* Automatically added body to GitHub release

### Fixed

* Update rash package versions in Cargo.lock (missing in 1.3.0)

## [v1.3.0](https://github.com/rash-sh/rash/tree/v1.3.0) (2021-12-19)

### Added

* Add `changed_when` optional field in task
* Add support for arrays in `when` and `changed_when`
* Add clearer logger for diff files
* Add src option to copy module
* Add `check` mode

### Fixed

* Parsed `when` and `changed_when` when they are booleans
* Builtin dir when current dir returns `.`
* Check `when` for each different item in loop
* Remove vendor on release

## [v1.2.0](https://github.com/rash-sh/rash/tree/v1.2.0) (2021-12-17)

### Added

* Add diff param and apply in file, copy and template modules (#190)
* Get params doc from struct (#189)

### Fixed

* Add warn and error to stderr instead of stdout
* Remove `--all-features` from release

## [v1.1.0](https://github.com/rash-sh/rash/tree/v1.1.0) (2021-12-12)

### Added

* Add file module (#180)

## [v1.0.2](https://github.com/rash-sh/rash/tree/v1.0.2) (2021-12-07)

### Added

* Add AUR packages automatic build and publish
* Release with signed tags
* Add releases binaries with Linux Glib >= 2.17 support and macOS

## [v1.0.1](https://github.com/rash-sh/rash/tree/v1.0.1) (2021-12-03)

### Bug fixes

* Remove duplicate error messages

## [v1.0.0](https://github.com/rash-sh/rash/tree/v1.0.0) (2020-06-11)

First stable version released:

### modules

* assert
* command
* copy
* template
* set_vars

### tasks

* when
* register
* ignore_errors
* loop

### vars

* rash
  * args
  * dir
  * path
  * user.uid
  * user.gid
* env

## v0.1.0

Core version released:

* data structure
* error management
* log
* execution
* cli

### modules

* add command (basic functionality)
