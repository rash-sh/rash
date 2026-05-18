#!/bin/bash
set -e

ensure_full_history() {
    if git rev-parse --is-shallow-repository 2>/dev/null | grep -q "true"; then
        echo "Shallow clone detected. Fetching full history and tags..."
        git fetch --unshallow --quiet
    fi
    if [ -z "$(git tag -l)" ]; then
        echo "No tags found. Fetching tags..."
        git fetch --tags --quiet
    fi
}

ensure_full_history

if ! [ "$(git rev-list --count origin/master..HEAD 2>/dev/null || echo 1)" -eq 0 ]; then
    echo "There are commits in this branch. Please merge them first."
    echo "CHANGELOG template needs master commit ID."
    exit 1
fi

# bump version
vim Cargo.toml
make update-version

make update-changelog

git add .
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
git commit -m "release: Version $VERSION"

echo "After merging the PR, tag and release are automatically done"
