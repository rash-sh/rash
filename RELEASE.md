# release workflow

- Bump version in `Cargo.toml` and run `make update-version`.
- Update lock file: `cargo update -p rash_core -p rash_derive`.
- Update `CHANGELOG.md` with `make update-changelog`.
- Merge PR.

## Upgrade dependencies

Requirements:

- `cargo-edit`: `cargo install cargo-edit`

Upgrade dependencies:

- `cargo upgrade` or `cargo upgrade --incompatible`

Update cargo lock dependencies:

- `cargo update`
