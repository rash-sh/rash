# rash changes

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
