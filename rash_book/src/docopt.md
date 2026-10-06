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
- [Choosing between matches](#choosing-between-matches)
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
- Every help-text line that starts with an option (after indentation) describes it, conventionally
  under an `Options:` heading; a Markdown bullet such as `- note` does not. See
  [Options](syntax.md#options).
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

`rash` consumes the first `--`. A second one reaches the script, where it ends the script options:
`rash tool.rh -- -v -- -x` passes `-x` as a positional value (see [`[--]`](#language-summary)).

## Language summary

| Syntax                  | Meaning                                             | Variable                                                       |
| ----------------------- | --------------------------------------------------- | -------------------------------------------------------------- |
| `name`                  | Command: the literal word `name`                    | `name`: `true`/`false`, or a count if it can repeat            |
| `<name>`, `NAME`        | Positional argument                                 | `name`: string, or list if it can repeat; omitted if not given |
| `-v`, `--verbose`       | Option flag                                         | `options.verbose`: `true`/`false`, or a count if it can repeat |
| `--port=<n>`, `-o FILE` | Option with a value                                 | `options.port`: string, its `[default: ...]`, or `null`        |
| `[ ... ]`               | Optional elements                                   |                                                                |
| `( ... )`               | Required group                                      |                                                                |
| `a \| b`                | Mutually exclusive alternatives                     |                                                                |
| `elem...`               | One or more repetitions of `elem`                   |                                                                |
| `[options]`             | Any described option that no usage pattern names    |                                                                |
| `[--]`                  | Match the `--` that ends the options                | `__`: `true`/`false`                                           |
| `-`                     | Command: a lone `-` (by convention, standard input) | `_`: `true`/`false`                                            |

Command and positional names are ASCII words: a letter, then letters or digits, with words joined
by `-` or `_` (lowercase for `name` and `<name>`, uppercase for `NAME`). `<file1>`, `FILE-2` and
`step2` are valid; `<1st>` and `Run` are not. Variable names replace `-` with `_` and lowercase
`NAME`. Options are stored under `options`, keyed by their long name if they have one, so a usage
that declares options cannot also name a command or positional `options`. Groups can be nested up
to 64 levels deep.

A `--` in the arguments always ends the options: every later argument is a positional value, even
if it starts with `-`. If no usage pattern declares `[--]`, the `--` itself is dropped; if one
does, `--` is kept as a word that fills the `[--]` slot (or a positional where `[--]` cannot be).

A variable has the same type in every pattern: if `<source>` can repeat in one pattern, it is a
list in all of them.

See [Syntax](syntax.md) for the full description of each element, and [Parser](parser.md) for the
variables they produce.

## Choosing between matches

When the arguments fit the declaration in more than one way, `rash` picks one result,
deterministically:

1. Usage patterns are tried in declaration order: the first pattern that matches wins.
2. Alternatives (`a | b`) are tried in the order they are written.
3. Optional elements and repetitions take as many arguments as they can, but give arguments back
   when the rest of the pattern needs them.

| Declaration                                           | Arguments | Result                                                   |
| ----------------------------------------------------- | --------- | -------------------------------------------------------- |
| `tool [<a>] [<b>]`                                    | `x`       | `a = "x"`, `b` omitted                                   |
| `tool (<a> \| <b>)`                                   | `x`       | `a = "x"`, `b` omitted                                   |
| `tool [<a>]... [<b>]`                                 | `x y`     | `a = ["x", "y"]`, `b` omitted                            |
| `tool [<a>] [<b>] <c>`                                | `x`       | `c = "x"`, `a` and `b` omitted                           |
| `cp <source> <dest>` and `cp <source>... <directory>` | `a b`     | first pattern: `source = ["a"]`, `dest = "b"`            |
| (same)                                                | `a b c`   | second pattern: `source = ["a", "b"]`, `directory = "c"` |

## Help and errors

**Help.** `rash` prints the help text, followed by a note about `--`, and exits with status 0
without running any task when:

- The arguments contain a **help option** before any `--`: a declared flag whose long name is
  `--help` (with any short alias, such as `-h --help`), or a `-h` flag without a long name. It
  wins even if the other arguments are invalid or match no pattern (`--unknown --help` shows the
  help). It must itself be written correctly (`--help=yes` is an error), and it is not a help
  request when it is the value of another option (`--port --help`). `-h` is not a help option
  when it takes a value (`-h <host>`) or has another long name (`-h, --human`).
- The arguments match a pattern through a `help` command, such as `tool (help | run <target>)`.

**Usage errors.** When the arguments match no usage pattern, `rash` prints `[ERROR]` and the help
text to stderr and exits with status 1. These more specific errors are reported the same way:

| Error                                    | Cause                                                            |
| ---------------------------------------- | ---------------------------------------------------------------- |
| `Unknown option: --nope`                 | The option is not declared (long options cannot be abbreviated). |
| `Option --port requires a value`         | A value option is the last argument.                             |
| `Option --dry-run does not take a value` | A flag is given a value with `=`.                                |
| `Ambiguous option alias: -u`             | The short alias is declared for two different options.           |
| `Invalid usage identifier: Run`          | The declaration contains an invalid command or positional name.  |
| `Invalid usage grammar at token ...`     | Unbalanced brackets, or groups nested deeper than 64 levels.     |
| `` `options` is a reserved name ... ``   | A command or positional named `options` in a usage with options. |

An ambiguous alias is only an error when it is used: the long aliases of both options keep working.

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
- The `--` separator ends the options: `tool <file>...` accepts `a -- -b` (`file = ["a", "-b"]`),
  and `tool [options] [--] <file>...` accepts `-v -- -x`. `-` and `--` are commands with the
  variables `_` and `__`.
- A help option shows the help wherever it appears and even if other arguments are invalid:
  `tool a b --help` and `tool --unknown --help` show the help instead of an error.
- Digits in names (`<file1>`, `FILE-2`, `step2`) and directly nested brackets (`[[<x>]]`).

**Now rejected** (previously accepted, often with a wrong result):

- An option given more times than the pattern declares it: `tool [-a] [-b]` rejects `-a -a`, and
  `tool [-a] [-b] [-a]` rejects `-a -a -a`.
- A value option without its value fails with `Option -o requires a value` instead of storing
  `"-o"` or swallowing the next option.
- A short alias declared for two options (`-u --sysupgrade` and `-u --upgrades`) fails with
  `Ambiguous option alias: -u` when used, instead of silently picking one. The long aliases work.
- A command or positional named `options` in a usage that declares options.
- `--help=yes` fails with `Option --help does not take a value` instead of showing the help.
- Invalid declarations and option values report the cause (`Invalid usage identifier: Run`,
  `Option --known does not take a value`) instead of the whole help text or an empty message.

**Value changes:**

- When several matches are possible, the result follows [Choosing between
  matches](#choosing-between-matches). Previously the choice was arbitrary and could change from
  one run to the next: `cp <source> <dest>` / `cp <source>... <directory>` with `a b` sometimes set
  `directory`, and `tool (<a> | <b>)` with `x` usually set `b`.
- A command that can occur more than once in a pattern (`go (up | down)...`, `tool a [a]`) is
  always a count: `0` when absent and `1` when given once (previously `false`/`true` until given
  twice).
- An option written more than once in a pattern (`[-a] [-a]`) or in a repeated group
  (`[--quiet | --verbose]...`) is a count (previously `true`, or a mix of boolean and count).
- A positional written more than once in a pattern (`[<x>] [<x>]`) is always a list, even with a
  single value (previously the last value as a string).
- Short options never fill positional slots: `tool [-asoFILE] [INPUT ...]` with `-s` sets
  `options.s` (previously it sometimes gave `input = ["-s"]`).
- A `-h` flag without a long name shows the help (previously it set `options.h` and could stand in
  for a positional), and `--port --help` sets `options.port` to `--help` instead of showing the
  help.
- Help lines starting with `-` that declare no option, such as Markdown bullets (`- note`), no
  longer add an empty `options[""]` key.

## Differences from Docopt

These Docopt behaviours are not supported:

- **Options only match where the pattern declares them.** With
  `tool [--verbose] (start|stop) [--force]`, `start --force` works but `--force start` and
  `start --verbose` are rejected. Adjacent optional options, and the options in `[options]`, can be
  given in any order among themselves.
- **`--` is dropped without `[--]`.** When no usage pattern declares `[--]`, `--` in the
  arguments ends the options and is discarded; Docopt keeps it as a positional value. The
  variables of `--` and `-` are `__` and `_`.
- **No abbreviated long options.** `--verb` does not match `--verbose`.
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
- **Stricter identifiers.** Command and positional names are ASCII, cannot mix case and cannot
  start with a digit (`Run`, `<File>` and `<1st>` are invalid).

Rash also accepts some arguments that Docopt rejects, because it backtracks into optional and
repeated elements: `tool [<a>] [<b>] <c>` with `x`, and the `cp` patterns above with `a b c`.
