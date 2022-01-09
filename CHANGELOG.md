# rash changes

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
