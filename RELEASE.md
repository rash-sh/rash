# release workflow

```bash
# bump version
vim Cargo.toml
make update-version

# update lock file
cargo update -p rash_core -p rash_derive

# update CHANGELOG.md
make update-changelog

# merge PR
git add .
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
git commit -m "release: Version $VERSION"
```

## Upgrade dependencies

Requirements:

- `cargo-edit`: `cargo install cargo-edit`

Upgrade dependencies:

- `cargo upgrade` or `cargo upgrade --incompatible`

Update cargo lock dependencies:

- `cargo update`
