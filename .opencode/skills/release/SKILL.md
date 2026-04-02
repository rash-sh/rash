---
name: release
description: Guide the release process for rash, including version bumping, changelog updates, and creating release branches.
---

## Purpose

Provide step-by-step instructions for releasing a new version of rash, ensuring proper versioning, changelog updates, and release branch management.

## When to use

Use this skill when asked to:

- Create a new release
- Bump the version
- Prepare a release PR
- Update the changelog for a release

## Prerequisites

Before starting a release:

1. Ensure you are on the `master` branch
2. Ensure the working tree is clean (no uncommitted changes)
3. Ensure local master is up to date with origin/master

## Version Decision Guide

Use Semantic Versioning (MAJOR.MINOR.PATCH). Determine the bump type by analyzing commits since the last release:

### Major Version (X.0.0)

Bump MAJOR when:

- Breaking changes to the CLI interface (removed/renamed commands or flags)
- Breaking changes to module parameters or behavior
- Breaking changes to the public API
- Commit message contains `BREAKING CHANGE:` or `!` (e.g., `feat!: ...`)

### Minor Version (0.X.0)

Bump MINOR when:

- New features added (`feat:` commits)
- New CLI commands or flags
- New modules or parameters
- Backward-compatible enhancements

### Patch Version (0.0.X)

Bump PATCH when:

- Bug fixes (`fix:` commits)
- Documentation updates (`docs:` commits)
- Internal refactoring (`refactor:` commits)
- Performance improvements without API changes
- Dependency updates

### Decision Process

1. Run: `git log v$(sed -n 's/^version = "\(.*\)"/\1/p' ./Cargo.toml | head -n1)..HEAD --oneline`
2. Check commit messages for:
   - `!` or `BREAKING CHANGE:` -> MAJOR
   - `feat:` -> MINOR
   - `fix:`, `docs:`, `refactor:`, etc. -> PATCH
3. If multiple types, use the highest precedence (MAJOR > MINOR > PATCH)

## Release Process

### Step 1: Verify Clean State

Ensure you're on master with no uncommitted changes and up to date with origin:

```bash
git checkout master
git pull origin master
git status  # Should show "nothing to commit, working tree clean"
```

Check that there are no unmerged commits:

```bash
git rev-list --count origin/master..HEAD
# Should output 0
```

If there are local commits not on origin/master, they must be merged first. The CHANGELOG template needs master commit IDs.

### Step 2: Determine Version

1. Get current version:

   ```bash
   grep '^version =' Cargo.toml
   ```

2. Review commits since last release:

   ```bash
   git log v<CURRENT_VERSION>..HEAD --oneline
   ```

3. Decide on MAJOR, MINOR, or PATCH bump based on the Version Decision Guide above.

### Step 3: Create Release Branch

Create a branch named `release/v{NEW_VERSION}`:

```bash
git checkout -b release/v<NEW_VERSION>
```

Example: `git checkout -b release/v2.19.0`

### Step 4: Update Version in Cargo.toml

Edit `Cargo.toml` and update the version in the `[workspace.package]` section:

```toml
[workspace.package]
version = "<NEW_VERSION>"
```

The version is on line 7 of Cargo.toml.

### Step 5: Propagate Version and Update Lock File

Run make to update version in all workspace member Cargo.toml files and update the lock file:

```bash
make update-version
```

This command:
- Updates `rash_core/Cargo.toml` and `rash_derive/Cargo.toml` with the new version
- Runs `cargo update -p rash_core -p rash_derive`

### Step 6: Update Changelog

Generate the changelog using git-cliff:

```bash
make update-changelog
```

This runs: `git cliff -t v<VERSION> -u -p CHANGELOG.md`

The changelog will be automatically updated with commits since the last release, grouped by type (Added, Fixed, Documentation, etc.).

### Step 7: Commit Changes

Stage and commit all changes:

```bash
git add .
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
git commit -m "release: Version $VERSION"
```

### Step 8: Push Branch and Create PR

Push the release branch:

```bash
git push -u origin release/v<NEW_VERSION>
```

Create a pull request to merge into master.

### Step 9: After Merge

After the PR is merged to master:

1. Create and push a tag: `git tag v<VERSION> && git push origin v<VERSION>`
2. The CI workflow automatically builds release artifacts and creates a GitHub Release

## Quick Reference

| Step | Command |
|------|---------|
| Check current version | `grep '^version =' Cargo.toml` |
| View recent commits | `git log v<CUR>..HEAD --oneline` |
| Create branch | `git checkout -b release/v<VER>` |
| Update versions | `make update-version` |
| Update changelog | `make update-changelog` |
| Commit | `git commit -m "release: Version <VER>"` |

## Checklist

- [ ] On master branch, clean working tree
- [ ] Pulled latest from origin/master
- [ ] No unmerged commits (checked with `git rev-list --count origin/master..HEAD`)
- [ ] Determined version bump type (MAJOR/MINOR/PATCH)
- [ ] Created release branch `release/v<VERSION>`
- [ ] Updated version in Cargo.toml `[workspace.package]`
- [ ] Ran `make update-version`
- [ ] Ran `make update-changelog`
- [ ] Committed with message `release: Version <VERSION>`
- [ ] Pushed branch and created PR
- [ ] After merge: created tag `v<VERSION>`
