# How to Contribute

The Rash project in under [The GNU General Public License v3.0](LICENSE). We accept contributions
via GitHub pull requests. This document outlines some of the conventions related to development
workflow, commit message formatting, contact points and other resources to make it easier to get
your contribution accepted.

## Certificate of Origin

By contributing to this project you agree to the Developer Certificate of Origin (DCO). This
document was created by the Linux Kernel community and is a simple statement that you, as a
contributor, have the legal right to make the contribution. See the [DCO](DCO) file for details.

Contributors sign-off that they adhere to these requirements by adding a Signed-off-by line to
commit messages. For example:

```text
This is my commit message

Signed-off-by: Random J Developer <random@developer.example.org>
```

Git even has a -s command line option to append this automatically to your commit message:

```shell
git commit -s -m 'This is my commit message'
```

If you have already made a commit and forgot to include the sign-off, you can amend your last commit
to add the sign-off with the following command, which can then be force pushed.

```shell
git commit --amend -s
```

We use a [DCO bot](https://github.com/apps/dco) to enforce the DCO on each pull request and branch
commits.

## Getting Started

1. Fork the repository on GitHub
1. Read the [install](INSTALL.md) for build and test instructions
1. Play with the project, submit bugs, submit patches!

## Pre-commit Hooks

We use [pre-commit](https://pre-commit.com/) to ensure code quality and consistency. The hooks will:

- Remove trailing whitespace from all files
- Ensure files end with a newline
- Check YAML syntax and formatting
- Check JSON syntax
- Detect merge conflict markers
- Prevent adding large files
- **Format Rust code with `cargo fmt`**
- **Lint Rust code with `cargo clippy`**
- Validate commit messages with commitlint
- Validate Renovate configuration

And more.

### Installing pre-commit

To install pre-commit hooks locally:

```shell
# Install pre-commit (if not already installed)
pip install pre-commit

# Install the hooks
pre-commit install

# Optionally, run against all files to check current state
pre-commit run --all-files
```

Once installed, the hooks will run automatically on each commit. If any hook fails, the commit will
be rejected and you'll need to fix the issues before committing again.

### Manual execution

You can also run the hooks manually:

```shell
# Run all hooks on all files
pre-commit run --all-files

# Run all hooks on staged files only
pre-commit run

# Run specific hook
pre-commit run trailing-whitespace
```

### Rust-specific Checks

In addition to the general pre-commit hooks, we have specific Rust tooling that can be run manually:

```shell
# Format Rust code
make fmt

# Check if Rust code is properly formatted (without modifying files)
make fmt-check

# Run clippy linter
make clippy

# Run clippy with automatic fixes
make clippy-fix

# Run all linting checks (fmt-check + clippy)
make lint

# Run all linting with automatic fixes (fmt + clippy-fix)
make lint-fix
```

These checks are automatically run by pre-commit, but you can also run them manually during
development.

## Contribution Flow

This is a rough outline of what a contributor's workflow looks like:

1. Create a branch from where you want to base your work (usually master).
1. Make your changes and arrange them in readable commits.
1. Make sure your commit messages are in the proper format (see below).
1. Push your changes to the branch in your fork of the repository.
1. Make sure all tests pass, and add any new tests as appropriate.
1. Submit a pull request to the original repository.

## Coding Style

Rash projects are written in Rust and follow a functional style trying to keep code simple.

## Comments

You should add appropriate comments to all new methods and structures. Additionally, if an existing
method or structure is sufficiently modified, you should add comments to it if it doesn't have any
already or update them if they do.

The goal of comments is to make the code more readable and grokkable by future developers. Once you
have made your code as understandable as possible, add comments to make sure future developers can
understand (A) the responsibility of piece of code within Rash's architecture and (B) why it was
written as it was.

The blog entry below explains more the whys and hows of this guideline.
<https://blog.codinghorror.com/code-tells-you-how-comments-tell-you-why/>

## Commit Messages

We follow a rough convention for commit messages that is designed to answer two questions: what
changed and why. The subject line should feature the what and the body of the commit should describe
the why.

```text
doc: Add issue templates

I thought it would be nice to have templates for issues. This way, bug reports
or new requests have a normalized pattern and they'll become easier to process.
```

We can define the format more formally as follows:

```text
<type>(<scope>): <what changed>
<BLANK LINE>
<why this change was made>
<BLANK LINE>
<footer>
{% if issue related %}
Resolves: #{issue.id}
{%- endif %}
```

You can find what values area could take see
[rash/.commitlintrc.json](https://github.com/rash-sh/rash/blob/master/.commitlintrc.json).

The first line is the subject and should be no longer than 70 characters. The second line is always
blank, and other lines should be wrapped at 80 characters. This allows the message to be easier to
read on GitHub as well as in various git tools.

**Important!** Any submitted pull request needs to have commit messages validated according to that
specification. To avoid nasty surprises, we set up a `commit-msg` hook that validates your commit
message before the commit actually takes place.

You need to install [Docker](https://docs.docker.com/engine/install/) for this hook to work. If
you're working on Linux, make sure you can run it with non-root permissions! More info
[here](https://docs.docker.com/engine/install/linux-postinstall/).
