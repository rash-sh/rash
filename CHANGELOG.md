# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.17.4](https://github.com/rash-sh/rash/tree/v2.17.4) - 2025-12-24

### Build

- ci: Fix release race condition with dedicated publish ([26f4b99](https://github.com/rash-sh/rash/commit/26f4b99a04b13559541fb860e188a5d8db837e02))

## [v2.17.3](https://github.com/rash-sh/rash/tree/v2.17.3) - 2025-12-24

### Added

- jinja: Add minijinja unicode, urlencode and builtins features ([8e9a58d](https://github.com/rash-sh/rash/commit/8e9a58d38585b86ecbed08245fcf75e02551f180))
- module: Add dconf ([433cf6e](https://github.com/rash-sh/rash/commit/433cf6e34ef2be997cd84ba8bd9e7f5a6c29d3ba))
- task: Add environment variable support ([cff2133](https://github.com/rash-sh/rash/commit/cff21339222792af31aeea20f3000fc5ef351677))

### Fixed

- module: Handle symlinks in copy module ([ca13add](https://github.com/rash-sh/rash/commit/ca13add368c76b024464b5e8d1dc0cbaabc12764))
- module: Skip usermod when appending groups user already has in user module ([f5decda](https://github.com/rash-sh/rash/commit/f5decdad2a63490d1d4bbcf86b2762ff2234b4d8))
- Ensure CHANGELOG commit IDs are correct on release process ([4ee202d](https://github.com/rash-sh/rash/commit/4ee202d35edd24acf74c30d2b4bf9dda5250d303))
- Make clippy happy ([87e6d8b](https://github.com/rash-sh/rash/commit/87e6d8b37028219182c0303da80998e56e157efd))

### Documentation

- Add comprehensive CLI reference documentation ([e389d71](https://github.com/rash-sh/rash/commit/e389d71b1b9f2bf21b883fdedf0707c9f27be2f3))

### Build

- deps: Update Rust crate syn to v2.0.109 ([0071d14](https://github.com/rash-sh/rash/commit/0071d1424219a482490e594cd41f7077e90f2dc5))
- deps: Update Rust crate schemars to v1.1.0 ([70923d6](https://github.com/rash-sh/rash/commit/70923d6634ae7ae75012108755b72464bb0e2c5c))
- deps: Update Rust crate quote to v1.0.42 ([f7b3246](https://github.com/rash-sh/rash/commit/f7b3246aaea712c8147d18763c07d5f3b14c196a))
- deps: Update Rust crate syn to v2.0.110 ([3ad6cd6](https://github.com/rash-sh/rash/commit/3ad6cd602871d6f23a8f7be08389cc01c94d872c))
- deps: Update rust Docker tag to v1.91.1 ([cac9fb2](https://github.com/rash-sh/rash/commit/cac9fb2ea2a654ce35e657267d7f413871f518cb))
- deps: Update Rust crate clap to v4.5.52 ([594b37b](https://github.com/rash-sh/rash/commit/594b37b653fb20b68c1056d09d9a326b0a39983e))
- deps: Update Rust crate serde_with to v3.16.0 ([5948636](https://github.com/rash-sh/rash/commit/594863645fe9705e704e13fa8d10eac8323b108a))
- deps: Upgrade mdbook to 0.5 ([eb89556](https://github.com/rash-sh/rash/commit/eb895563c2f1ca458057f693995de340790fa57a))
- deps: Update Rust crate clap to v4.5.53 ([3661f5e](https://github.com/rash-sh/rash/commit/3661f5ec8e63d3a6faf9df3e247423447e0b6f26))
- deps: Update actions/checkout action to v6 ([95e1d47](https://github.com/rash-sh/rash/commit/95e1d477ebdc9768c20fb4931a79d3e756d0a7e5))
- deps: Update Rust crate syn to v2.0.111 ([9f63991](https://github.com/rash-sh/rash/commit/9f639910a2b1b60534a34b1ac03af836eeed3d72))
- deps: Update Rust crate serde_with to v3.16.1 ([22851f6](https://github.com/rash-sh/rash/commit/22851f6e3dd79f47cf53b97ae36ccbe110660996))
- deps: Update Rust crate minijinja to v2.13.0 ([1853875](https://github.com/rash-sh/rash/commit/185387553376a0df58fbd9f75a24ae0596adcc1b))
- deps: Update Rust crate criterion to 0.8.0 ([7fbe8cd](https://github.com/rash-sh/rash/commit/7fbe8cd9a85c0494a6623d3357a7c18192ac3575))
- deps: Update Rust crate byte-unit to v5.2.0 ([2b0595b](https://github.com/rash-sh/rash/commit/2b0595bfb4c5fda73afb6499cb17f1f753772664))
- deps: Update Rust crate mdbook-driver to v0.5.1 ([6781027](https://github.com/rash-sh/rash/commit/6781027e9ebfa2ba58150d76fa944d354f425c52))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v42 ([a609c07](https://github.com/rash-sh/rash/commit/a609c07eb36913df97665ce2302199ec6fcd42d3))
- deps: Update Rust crate reqwest to v0.12.25 ([1d9d5a5](https://github.com/rash-sh/rash/commit/1d9d5a5eae0c6d92ebcf880618ae8dd55bc65f8d))
- deps: Update Rust crate criterion to v0.8.1 ([2c3abf5](https://github.com/rash-sh/rash/commit/2c3abf5fb4294a1c85092c664f01d0742e646d1a))
- deps: Update Rust crate log to v0.4.29 ([c1fc352](https://github.com/rash-sh/rash/commit/c1fc352185c826ff899cd9a69ead5ee818e2541f))
- deps: Update rust Docker tag to v1.92.0 ([94829a3](https://github.com/rash-sh/rash/commit/94829a3768bb165981dab59a83f9f39d33473f97))
- deps: Update actions/cache action to v5 ([4f41923](https://github.com/rash-sh/rash/commit/4f41923d16e5c8f88ded9739598c652736e07223))
- deps: Update Rust crate mdbook-driver to v0.5.2 ([76c4612](https://github.com/rash-sh/rash/commit/76c46124e82efedaeb7022551b007a9abecbfb65))
- deps: Update Rust crate minijinja to v2.14.0 ([0ffcca2](https://github.com/rash-sh/rash/commit/0ffcca27cae72ef898baf3bcea79a4b38f30d41b))
- deps: Update Rust crate reqwest to v0.12.26 ([4c6e9ca](https://github.com/rash-sh/rash/commit/4c6e9caa07874d31f3f353836c429357d2038c04))
- deps: Update Rust crate console to v0.16.2 ([31e19e7](https://github.com/rash-sh/rash/commit/31e19e715e3eb9377ee9e212da4d701f047249be))
- deps: Update Rust crate serde_json to v1.0.146 ([df37a23](https://github.com/rash-sh/rash/commit/df37a23d2511c978d94cbb9a8c5ea57a099e34ed))
- deps: Update Rust crate serde_json to v1.0.147 ([6a99218](https://github.com/rash-sh/rash/commit/6a992186193d02c63d810815ab7d1ab608677fb5))
- deps: Update Rust crate tempfile to v3.24.0 ([4e161ce](https://github.com/rash-sh/rash/commit/4e161ce5e073a8dc9a65ce2c427a346cae07a48c))
- deps: Update Rust crate reqwest to v0.12.28 ([c625710](https://github.com/rash-sh/rash/commit/c625710b0e6c9a36e3a8d5a4a306f29bb8012b3b))

## [v2.17.2](https://github.com/rash-sh/rash/tree/v2.17.2) - 2025-11-02

### Documentation

- Remove unnecessary changelog header and intro to avoid repetition ([066b822](https://github.com/rash-sh/rash/commit/066b822874912deded2aeaff625564ae39d5e487))
- Fix weights for correct TOC rendering ([5b70763](https://github.com/rash-sh/rash/commit/5b707634a01051a313eca5dfe21d04874c5ea3eb))

## [v2.17.1](https://github.com/rash-sh/rash/tree/v2.17.1) - 2025-11-02

### Fixed

- ci: Re-enable integration tests for MacOS ([398f9bd](https://github.com/rash-sh/rash/commit/398f9bd1259550657afc4ff82c874d202dbefb01))

## [v2.17.0](https://github.com/rash-sh/rash/tree/v2.17.0) - 2025-11-02

### Added

- module: Add user ([73b1cdf](https://github.com/rash-sh/rash/commit/73b1cdf2c661f81e8ce248cf046a58d14c5da133))
- module: Add group ([cd87762](https://github.com/rash-sh/rash/commit/cd8776260a7529f571b295e2648c42c3a441762e))

### Fixed

- task: Remove sum logic for number fields in var merge ([5f0cb7e](https://github.com/rash-sh/rash/commit/5f0cb7e1400ef4e5bdbdeeb98de67ea3614c4805))

### Documentation

- module: Add chars `%^?` to match regex in include_docs ([fed4e2f](https://github.com/rash-sh/rash/commit/fed4e2f6dbd0dea53a0241c0257bbf2e423c5bac))
- Add commit ID links in CHANGELOG.md ([1289df2](https://github.com/rash-sh/rash/commit/1289df296784b4531d019bb109fc0ac7f1548064))

### Build

- ci: Change Apple build to arm64 and update to macos-15 ([e87238a](https://github.com/rash-sh/rash/commit/e87238a08e2f3d027446cfa0c8085ba238048f5d))
  - **BREAKING**: Apple x86_64 binary is deprecated.
- deps: Update Rust crate tokio to v1.48.0 ([9253965](https://github.com/rash-sh/rash/commit/9253965fc1aafcc7151c7b3e97e1f5012fd8de87))
- deps: Update Rust crate reqwest to v0.12.24 ([88a4047](https://github.com/rash-sh/rash/commit/88a40475e53cb6f2a4a1088b76b753036c48b62a))
- deps: Update Rust crate ignore to v0.4.24 ([5b02011](https://github.com/rash-sh/rash/commit/5b020118dc506e224cd0b47cc143c336d41fd0a8))
- deps: Update Rust crate syn to v2.0.107 ([e88e7d1](https://github.com/rash-sh/rash/commit/e88e7d1f912af820338c728cf7dd3857efb99e41))
- deps: Update Rust crate clap to v4.5.50 ([8aae268](https://github.com/rash-sh/rash/commit/8aae2687ddb680b64a46e7097260cab0b4f81212))
- deps: Update Rust crate serde_with to v3.15.1 ([1231b26](https://github.com/rash-sh/rash/commit/1231b2676142e6f308e4aeb538514f244232dc61))
- deps: Update Rust crate syn to v2.0.108 ([00835aa](https://github.com/rash-sh/rash/commit/00835aac5b6c531e4dedd8c5f8bc3f53318c0cbf))
- deps: Update Rust crate proc-macro2 to v1.0.102 ([c330722](https://github.com/rash-sh/rash/commit/c3307225c46d46efd9c44035b87909a023c6ed0e))
- deps: Update Rust crate proc-macro2 to v1.0.103 ([13c8a2e](https://github.com/rash-sh/rash/commit/13c8a2e9660c8dea2da8b720b2dac68c5f8aa151))
- deps: Update Rust crate clap to v4.5.51 ([e646bca](https://github.com/rash-sh/rash/commit/e646bca2f6e030887be2415dc5eca71ffcfc0a91))
- deps: Update Rust crate ignore to v0.4.25 ([29777d9](https://github.com/rash-sh/rash/commit/29777d9ba2a9f04a26dde36a754c408f3d2069a6))
- deps: Update rust Docker tag to v1.91.0 ([7c58d08](https://github.com/rash-sh/rash/commit/7c58d0899a5cf380590a27ac83690e0d9d22da58))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.168.1 ([27100f9](https://github.com/rash-sh/rash/commit/27100f95a47cffeb74ed02cd8a20dac6b05bbeef))
- deps: Update Rust crate prs-lib to v0.5.5 ([ff4bf4e](https://github.com/rash-sh/rash/commit/ff4bf4e4d52aa2ecb9b693c35bb3fd917ed583a6))
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.168.3 ([39405b6](https://github.com/rash-sh/rash/commit/39405b63c14c40c111f268a876bac404f2128e9f))

## [v2.16.2](https://github.com/rash-sh/rash/tree/v2.16.2) - 2025-10-13

### Fixed

- ci: Add fmt and clippy for build tests
- Make clippy 1.89 happy
- Make clippy 1.90 happy

### Documentation

- Add copilot instructions

### Build

- ci: Fix cargo login token
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.62.1
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41
- deps: Update Rust crate schemars to v1.0.4
- deps: Update Rust crate clap to v4.5.41
- deps: Update Rust crate serde_json to v1.0.141
- deps: Update Rust crate rand to v0.9.2
- deps: Update Rust crate reqwest to v0.12.22
- deps: Update Rust crate serde_with to v3.14.0
- deps: Update Rust crate tokio to v1.46.1
- deps: Update Rust crate mdbook to v0.4.52
- deps: Update strum monorepo to v0.27.2
- deps: Update Rust crate criterion to 0.7.0
- deps: Update Rust crate tokio to v1.47.0
- deps: Update Rust crate ipc-channel to v0.20.1
- deps: Update Rust crate clap to v4.5.42
- deps: Update Rust crate serde_json to v1.0.142
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v41.43.0
- deps: Update Rust crate tokio to v1.47.1
- deps: Update Rust crate clap to v4.5.43
- deps: Update Rust crate clap to v4.5.44
- deps: Update Rust crate proc-macro2 to v1.0.97
- deps: Update Rust crate clap to v4.5.45
- deps: Bump slab from 0.4.10 to 0.4.11
- deps: Update Rust crate reqwest to v0.12.23
- deps: Update actions/checkout action to v5
- deps: Update pre-commit hook pre-commit/pre-commit-hooks to v6
- deps: Update rust Docker tag to v1.89.0
- deps: Update Rust crate syn to v2.0.105
- deps: Update Rust crate syn to v2.0.106
- deps: Update Rust crate proc-macro2 to v1.0.98
- deps: Update Rust crate proc-macro2 to v1.0.101
- deps: Update Rust crate serde_json to v1.0.143
- deps: Update Rust crate prs-lib to v0.5.3
- deps: Update Rust crate tempfile to v3.21.0
- deps: Update Rust crate minijinja to v2.12.0
- deps: Update Rust crate regex to v1.11.2
- deps: Update Rust crate clap to v4.5.46
- deps: Update Rust crate clap to v4.5.47
- deps: Update Rust crate log to v0.4.28
- deps: Update actions/setup-python action to v6
- deps: Update clechasseur/rs-clippy-check action to v5
- deps: Update Rust crate tempfile to v3.22.0
- deps: Update Rust crate chrono to v0.4.42
- deps: Update Rust crate console to v0.16.1
- deps: Update Rust crate prs-lib to v0.5.4
- deps: Update Cargo.lock
- deps: Update Rust crate serde to v1.0.220
- deps: Update Rust crate serde_json to v1.0.144
- deps: Update Rust crate serde to v1.0.221
- deps: Update Rust crate semver to v1.0.27
- deps: Update Rust crate serde_json to v1.0.145
- deps: Update Rust crate serde to v1.0.223
- deps: Update Rust crate serde to v1.0.224
- deps: Update Rust crate serde to v1.0.225
- deps: Update Rust crate ipc-channel to v0.20.2
- deps: Update rust Docker tag to v1.90.0
- deps: Update Rust crate clap to v4.5.48
- deps: Update Rust crate serde_with to v3.14.1
- deps: Update Rust crate serde to v1.0.226
- deps: Update Rust crate regex to v1.11.3
- deps: Update Rust crate tempfile to v3.23.0
- deps: Update pre-commit hook alessandrojcm/commitlint-pre-commit-hook to v9.23.0
- deps: Update Rust crate serde to v1.0.228
- deps: Update Rust crate quote to v1.0.41
- deps: Update Rust crate serde_with to v3.15.0
- deps: Update Rust crate regex to v1.12.1
- deps: Update Rust crate clap to v4.5.49
- deps: Update Rust crate regex to v1.12.2

## [v2.16.1](https://github.com/rash-sh/rash/tree/v2.16.1) - 2025-06-30

### Fixed

- module: Improve cmd error and remove Pacman executable detection
- Cargo clippy errors 1.88

### Build

- deps: Update Rust crate schemars to v1.0.3
- deps: Update rust Docker tag to v1.88.0
- deps: Update Rust crate console to 0.16
- deps: Update Rust crate minijinja to v2.11.0

## [v2.16.0](https://github.com/rash-sh/rash/tree/v2.16.0) - 2025-06-25

### Added

- module: Add dereference param to copy

### Fixed

- module: Improve error message on exec not found for pacman module

### Documentation

- ci: Replace master with latest on pages

### Build

- core: Replace serde-yaml with serde-norway
- deps: Update Rust crate syn to v2.0.104
- deps: Update Rust crate schemars to v1
- deps: Fix schemars import on rash_derive
- deps: Update Rust crate schemars to v1.0.1

## [v2.15.0](https://github.com/rash-sh/rash/tree/v2.15.0) - 2025-06-19

### Added

- lookup: Add password
- lookup: Add pipe
- lookup: Add vault
- lookup: Add file

### Fixed

- jinja: Render for invalid string

### Build

- deps: Update Rust crate rand to 0.9

### Refactor

- core: Remove term_size dependency

## [v2.14.2](https://github.com/rash-sh/rash/tree/v2.14.2) - 2025-06-17

### Fixed

- core: Keep `vars` scoped to block execution
- jinja: Improve error message on error

## [v2.14.1](https://github.com/rash-sh/rash/tree/v2.14.1) - 2025-06-16

### Fixed

- module: Propagate variables to parent scope in module block
- task: Render name for `always` and `rescue` tasks

### Build

- ci: Fix package URL on AUR description and use uri module
- ci: Auto update pre-commit once a month automatically
- deps: Update Rust crate sha2 to v0.10.9
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.57.1
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.59.0

### Testing

- ci: Skip Rust hooks on pre-commit workflow
- ci: Deprecate commitlint workflow
- ci: Remove uri and get_url examples
- module: Simplify httpbin.org dependand examples

## [v2.14.0](https://github.com/rash-sh/rash/tree/v2.14.0) - 2025-06-15

### Added

- module: Add uri module
- module: Add get_url module

## [v2.13.0](https://github.com/rash-sh/rash/tree/v2.13.0) - 2025-06-15

### Added

- ci: Add pre-commit and deprecate cargo-husky
- module: Add lineinfile

### Fixed

- module: Show diff on permissions change for copy module

### Documentation

- Update concept map
- Fix changelog of v2.12.0

### Build

- deps: Update Rust crate ipc-channel to v0.20.0
- deps: Update pre-commit hook renovatebot/pre-commit-hooks to v40.56.3

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v2.12.0](https://github.com/rash-sh/rash/tree/v2.12.0) - 2025-06-15

### Added

- task: Add block module
- task: Add `always` and `rescue` attributes

### Fixed

- module: Make clearer messages on diff for systemd module

### Documentation

- task: Add always and rescue attributes with examples

## [v2.11.0](https://github.com/rash-sh/rash/tree/v2.11.0) - 2025-06-15

### Added

- module: Add systemd

## [v2.10.0](https://github.com/rash-sh/rash/tree/v2.10.0) - 2025-06-15

### Added

- module: Add setup module for loading variables from config files

### Build

- deps: Update Rust crate tempfile to v3.20.0
- deps: Update Rust crate mdbook to v0.4.51
- deps: Update Rust crate schemars to 0.9
- deps: Update Rust crate clap to v4.5.40
- deps: Update Rust crate syn to v2.0.102
- deps: Update Rust crate syn to v2.0.103
- deps: Update Rust crate serde_with to v3.13.0

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
