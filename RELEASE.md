# release workflow

- Bump version in `Cargo.toml`.
- Update lock file: `cargo update -p rash_core -p rash_derive`
- Update `CHANGELOG.md`.
- Merge PR.
- Tag version in master branch: `make tag`
