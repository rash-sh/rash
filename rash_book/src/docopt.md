---
title: Command-line interfaces
weight: 10000
---

# Command-line interfaces <!-- omit in toc -->

- [Why use document-based argument parsing?](#why-use-document-based-argument-parsing)
- [Basic example](#basic-example)
- [Where the declaration is read from](#where-the-declaration-is-read-from)
- [Passing arguments to a script](#passing-arguments-to-a-script)
- [Language summary](#language-summary)
- [Help and errors](#help-and-errors)
- [Compatibility notes](#compatibility-notes)
- [Differences from Docopt](#differences-from-docopt)

`rash` includes a command-line argument parser driven by the documentation in your script.
Rather than writing argument parsing code, you document how your script should be used, and `rash`
parses the arguments according to that specification.

The usage language is inspired by [Docopt](http://docopt.org/): "the program's help message is the
source of truth for command-line argument parsing logic". Rash implements its own parser. It follows
Docopt for the constructs described in this book, but it is not a complete Docopt implementation;
see [Differences from Docopt](#differences-from-docopt).

## Why use document-based argument parsing?

1. **Documentation first**: You write the help text that users will see, not abstract parsing rules
2. **Single source of truth**: No risk of documentation being out of sync with the code
3. **Declarative**: Describe what the interface looks like, not how to parse it

## Basic example

```yaml
{{#include ../../examples/copy.rh}}
```

```bash
./copy.rh --mode 0600 a.txt b.txt /tmp/dest
```

In this example:

1. The usage patterns describe the command-line interface
2. The arguments are parsed and made available as variables (`source`, `dest`, `options.mode`)
3. No additional parsing code is needed

## Where the declaration is read from

- The **help text** is the comment block that starts on the line right after the shebang. It ends at
  the first line without `#`, so **do not leave a blank line after the shebang**: the comments below
  it would not be read. The first space after each `#` is removed.
- `Usage:` (case-insensitive) declares the usage patterns, in one of two forms:
  - **One line**: `# Usage: my_script.rh <name>`. This form holds exactly **one** pattern.
  - **Block**: `# Usage:` on its own line, followed by one indented pattern per line. The block ends
    at an empty comment line or at a non-indented line such as `Options:`.
- The first word of each pattern is the program name and is ignored.
- Every help-text line that starts with `-` (after indentation) describes an option, conventionally
  under an `Options:` heading. See [Options](syntax.md#options).
- If there is no `Usage:`, the arguments are not parsed and no variables are added. They are
  still available as a list of strings in [`{{ rash.args }}`](builtins.md).

## Passing arguments to a script

Arguments after the script path are passed to the script, but arguments starting with `-` are
read by `rash` itself unless they come after `--`:

```bash
rash copy.rh -- --mode 0600 a.txt /tmp/dest
```

A script using the shebang `#!/usr/bin/env -S rash --` receives every argument directly, so it can
be called as `./copy.rh --mode 0600 a.txt /tmp/dest`.

## Language summary

| Syntax                  | Meaning                                                | Variable                                                       |
| ----------------------- | ------------------------------------------------------ | -------------------------------------------------------------- |
| `name`                  | Command: the literal word `name`                       | `name`: `true`/`false`, or a count if it can repeat            |
| `<name>`, `NAME`        | Positional argument                                    | `name`: string, or list if it can repeat; omitted if not given |
| `-v`, `--verbose`       | Option flag                                            | `options.verbose`: `true`/`false`, or a count if it can repeat |
| `--port=<n>`, `-o FILE` | Option with a value                                    | `options.port`: string, its `[default: ...]`, or `null`        |
| `[ ... ]`               | Optional elements                                      |                                                                |
| `( ... )`               | Required group                                         |                                                                |
| `a \| b`                | Mutually exclusive alternatives                        |                                                                |
| `elem...`               | One or more repetitions of `elem`                      |                                                                |
| `[options]`             | Any described option not used elsewhere in the pattern |                                                                |

Commands and positional names use ASCII letters only (lowercase for `name` and `<name>`, uppercase
for `NAME`), with words joined by `-` or `_`. Variable names replace `-` with `_` and lowercase
`NAME`. Options are stored under `options`, keyed by their long name if they have one.

See [Syntax](syntax.md) for the full description of each element, and [Parser](parser.md) for the
variables they produce.

## Help and errors

**Help.** If the matched arguments include a `help` command or an option whose long name is
`--help` (or one of its aliases, such as `-h` in `-h --help`), `rash` prints the help text followed by
a note about `--`, and exits with status 0 without running any task. The help request must still fit
a usage pattern: `--help` may take the place of a positional argument (`tool run --help` with
`tool run <target>`), but it is not accepted at a position that no pattern allows.

**Usage errors.** When the arguments match no usage pattern, `rash` prints `[ERROR]` and the help
text to stderr and exits with status 1. These more specific errors are reported the same way:

| Error                                    | Cause                                                                |
| ---------------------------------------- | -------------------------------------------------------------------- |
| `Unknown option: --nope`                 | The option is not declared (long options cannot be abbreviated).     |
| `Option --port requires a value`         | A value option is the last argument.                                 |
| `Option --dry-run does not take a value` | A flag is given a value with `=`.                                    |
| `Ambiguous option alias: -u`             | The short alias is declared for two different options.               |
| `Ambiguous usage declaration.`           | The arguments can be matched in two ways that give different values. |
| `Invalid usage identifier: Run`          | The declaration contains an invalid command or positional name.      |

An ambiguous alias is only an error when it is used: the long aliases of both options keep working.
An ambiguous declaration is only reported for the arguments that trigger it; for example
`tool <source> <dest>` and `tool <input> <output>` as two patterns cannot bind `a b` unambiguously.

## Compatibility notes

This release replaced the argument parser. Most interfaces behave as before, but these declarations
behave differently:

**Now accepted** (previously rejected or broken):

- Optional options around a required command: `tool [--verbose] (start|stop) [--force]` accepts a
  bare `start`.
- Groups inside brackets are independently optional: `[(a | b) (c | d)]` accepts `c` alone, and
  `[(--aa | --bb) (--cc | --dd)]` no longer rejects every call.
- An option opening a group is an option: `(--either-this <and-that> | <or-this>)` no longer fails
  with `Unknown option: --either-this`.
- An option before `)` is parsed correctly: `(<key> | --all)` accepts `--all` and sets
  `options.all` (no stray `options["all)"]` key).
- Repeatable flags: `[--quiet | --verbose]...`, `[--verbose]... [--quiet]... <file>` and `[-a...]`
  accept repetitions and count them.
- `[--tag]...` with `--tag=VALUE` described under `Options:` works like `[--tag=<value>]...` (the
  last value wins).
- `[-o FILE] [--sorted | --quiet]` accepts an empty call, and declarations that combine a short
  option cluster such as `[-hsoFILE]` with repeatable options no longer reject every call.
- `[options]` stands for every described option not explicit in **its own** pattern. Previously
  an option written explicitly in one pattern was also excluded from `[options]` in later patterns.

**Now rejected** (previously accepted, often with a wrong result):

- An option given more times than the pattern allows: `tool [-a] [-b]` rejects `-a -a`, and
  `tool [options] [-a]` rejects `-a -a`.
- A value option without its value fails with `Option -o requires a value` instead of storing
  `"-o"` or swallowing the next option.
- Short options never fill positional slots. With some short option clusters, such as
  `[-hsoFILE] ... [INPUT ...]`, an option like `-s` could previously end up as an `INPUT` value.
- A short alias declared for two options (`-u --sysupgrade` and `-u --upgrades`) fails with
  `Ambiguous option alias: -u` when used, instead of silently picking one. The long aliases work.
- Arguments that the declaration can bind in two different ways fail with
  `Ambiguous usage declaration.` instead of picking one result. For example `tool [<a>] [<b>]`
  called with one argument; write `tool [<a> [<b>]]` instead.
- Invalid declarations and option values report the cause (`Invalid usage identifier: Run`,
  `Option --known does not take a value`) instead of the whole help text or an empty message.

**Value-type changes:**

- A command that can occur more than once in a pattern (`go (up | down)...`, `tool a [a]`) is
  always a count: `0` when absent and `1` when given once (previously `false`/`true` until given
  twice).
- An option written more than once in a pattern (`[-a] [-a]`) or in a repeated group
  (`[--quiet | --verbose]...`) is a count (previously `true`, or a mix of boolean and count).
- A positional written more than once in a pattern (`[<x>] [<x>]`) is always a list, even with a
  single value (previously the last value as a string).

## Differences from Docopt

These Docopt behaviours are not supported:

- **Options only match where the pattern declares them.** With
  `tool [--verbose] (start|stop) [--force]`, `start --force` works but `--force start` and
  `start --verbose` are rejected. Adjacent optional options, and the options in `[options]`, can be
  given in any order among themselves.
- **No end-of-options marker.** `--` cannot be used inside the script arguments; it fails with
  `Unknown option: --`.
- **No abbreviated long options.** `--verb` does not match `--verbose`.
- **No preference between ambiguous matches.** `tool [<source>] [<dest>]` rejects a single argument
  as ambiguous, where Docopt fills `<source>`. Nest the brackets to express the dependency:
  `tool [<source> [<dest>]]`. The same applies to `[cmd <arg>]` called with the word `cmd`.
- **One-line usage holds one pattern.** Continuation lines after `Usage: tool ...` are ignored; use
  the block form for several patterns.
- **Bare `[options]` accepts a repeated flag** when it stands for two or more options:
  `tool [options]` accepts `-a -a` and reports `true`. Docopt rejects it.
- **Repeated value options keep the last value.** `tool [--tag=<value>]...` with
  `--tag=one --tag=two` gives `"two"`; Docopt gives `["one", "two"]`.
- **Missing positionals are omitted.** An optional positional that was not given is not defined
  (Docopt sets it to `null`, or `[]` if repeatable); use `{{ name | default(...) }}`.
- **A usage reference to a value option may omit the value.** `tool [--tag]` with `--tag=VALUE`
  under `Options:` is accepted; Docopt rejects the declaration.
- **Stricter identifiers.** Command and positional names cannot contain digits or mix case
  (`<file1>` and `Run` are invalid).
