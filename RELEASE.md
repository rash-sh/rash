# release workflow

- Bump version in `Cargo.toml` and run `make update-version`.
- Update lock file: `cargo update -p rash_core -p rash_derive`.
- Update `CHANGELOG.md` with `make update-changelog`.
- Merge PR.
- Tag version in master branch: `make tag`.
