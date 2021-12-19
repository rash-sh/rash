# release workflow

- Bump version in `VERSION` and run `make update-version`.
- Update `CHANGELOG.md`.
- Merge PR.
- Tag version in main branch, add header from changelog and body without `#`: `make tag`
