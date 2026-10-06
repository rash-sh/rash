---
title: Syntax
weight: 10100
indent: true
---

# Syntax <!-- omit in toc -->

- [Usage patterns](#usage-patterns)
- [Positional arguments](#positional-arguments)
- [Options](#options)
- [Optional elements](#optional-elements)
- [Required groups](#required-groups)
- [Mutually exclusive elements](#mutually-exclusive-elements)
- [Repeatable elements](#repeatable-elements)
- [The `[options]` shortcut](#the-options-shortcut)
- [Argument formatting rules](#argument-formatting-rules)
- [Advanced usage patterns](#advanced-usage-patterns)

## Usage patterns

The keyword `usage:` (case-insensitive) introduces the usage patterns. A pattern on the same line
as `usage:` is the only pattern. To declare several patterns, put `usage:` on its own line and
write one indented pattern per line; the list ends at an empty line or a non-indented line. The first
word of each pattern is the program's name. Here is a minimal example for a program that takes no
command-line arguments:

```
Usage: my_program
```

Programs can have several patterns listed with various elements used to describe the pattern:

```
Usage:
  my_program command <argument>
  my_program [<optional-argument>]
  my_program (either-this-command | or-this-other-command)
  my_program <repeating-argument> <repeating-argument>...
```

Each of the elements and constructs is described below. We will use the word _word_ to describe a
sequence of characters delimited by either whitespace, one of `[]()|` characters, or `...`.

## Positional arguments

Words starting with "<", ending with ">" or words in UPPER-CASE are interpreted as positional
arguments. Any other word is a command, which must be given literally.

```
Usage: my_program <host> <port>
Usage: my_program HOST PORT
```

Both styles are equivalent, though the `<argument-name>` style is recommended for clarity. Positional
arguments are required by default unless placed within optional brackets `[]`.

Names are ASCII words: a letter, then letters or digits (lowercase inside `<>`, uppercase for
`NAME`), with words joined by `-` or `_`, such as `<file1>` or `FILE-2`. When used in your program,
these positional arguments will be available as variables with their name in lowercase and `-`
replaced by `_`:

```
# If invoked as: my_program example.com 8080
# The variables available would be:
host = "example.com"
port = "8080"
```

## Options

Words starting with one or two dashes (with exception of `-`, `--` by themselves) are interpreted
as short (one-letter) or long options, respectively.

- Short options can be `stacked` meaning that `-abc` is equivalent to `-a -b -c`.
- Long options can have arguments specified after space or equal `=` sign:
  `--input=ARG` is equivalent to `--input ARG`.
- Short options can have arguments specified after optional space:
  `-f FILE` is equivalent to `-fFILE`.

Examples:

```
Usage: my_program -o
Usage: my_program --output=FILE
Usage: my_program -i INPUT
```

Options are described in the help text, conventionally under `Options:`. Every line that starts
with an option declares it: its short and/or long aliases (separated by a space or `,`), an
optional value placeholder, then **at least two spaces** and a description. Lines starting with a
bare `-`, such as Markdown bullets (`- note`), are not option descriptions. A `[default: value]` in
the description sets the value used when the option is not given:

```
Options:
  -v --verbose            Enable verbose output
  -o FILE, --output=FILE  Write output to FILE
  --port=<port>           Port to listen on [default: 8080]
```

Aliases resolve to one option, stored under its long name (`options.output`, whether `-o` or
`--output` is used). An option can also appear only in a usage pattern; it then has no description
and no default.

Options are matched at the position where the pattern declares them. Adjacent optional options
(`[-v] [-q]`) and the options in [`[options]`](#the-options-shortcut) can be given in any order
among themselves, but `my_program [--verbose] <file>` does not accept `my_program file --verbose`.
Long options must be spelled in full.

**Note**: Writing `--input ARG` (as opposed to `--input=ARG`) is ambiguous, meaning it is not
possible to tell whether `ARG` is option's argument or a positional argument. In usage patterns
this will be interpreted as an option with argument only if a description (covered below) for that
option is provided. Otherwise, it will be interpreted as an option and a separate positional argument.

There is the same ambiguity with the `-f FILE` and `-fFILE` notation. In the latter case, it is not
possible to tell whether it is a number of stacked short options, or an option with an argument.
These notations will be interpreted as an option with argument only if a description for the option
is provided.

**Warning**: Options should be passed to rash after `--` to be interpreted as script arguments.
Otherwise, they will be treated as options for rash itself:

```bash
# Correct (using --):
rash script.rh -- --option value

# Incorrect (option passed to rash, not your script):
rash script.rh command --option value

# Incorrect (option passed to rash, not your script):
rash script.rh --option value
```

**Note**: The shebang line `#!/usr/bin/env -S rash --` passes every argument to the script, so
`./script.rh --option value` works.

### The `--` separator and `-`

A `--` in the arguments ends the options: every later argument is a positional value, even if it
starts with `-`. A lone `-` and `--` in a usage pattern are commands, stored as `_` and `__`;
declare `[--]` to record whether `--` was given:

```
Usage: my_program [options] [--] <file>...

# my_program -v -- -x.txt  ->  options.verbose = true, __ = true, file = ["-x.txt"]
```

If no pattern declares `--`, the `--` is dropped: with `my_program <file>...`, `a -- -b` gives
`file = ["a", "-b"]`. Remember that `rash` consumes the first `--` itself:
`rash my_program.rh -- a -- -b`.

## Optional elements

Elements (arguments, commands) enclosed with square brackets `[]` are marked as
optional. It does not matter if elements are enclosed in the same or different pairs of brackets.

The following examples are equivalent:

```
Usage: my_program [command --option]
```

```
Usage: my_program [command] [--option]
```

Optional elements can be nested:

```
Usage: my_program [command [--option]]
```

In this example, `--option` can only be used if `command` is provided.

Optional elements take arguments greedily, left to right: with `my_program [<source>] [<dest>]`, a
single argument fills `<source>`. They give arguments back when a later required element needs
them, so `my_program [<source>] <dest>` with one argument fills `<dest>`. See
[Choosing between matches](docopt.md#choosing-between-matches).

## Required groups

All elements are required by default if not included in brackets `[]`. However, sometimes it is
necessary to mark elements as required explicitly with parentheses `()`. For example, when you
need to group mutually-exclusive elements:

```
Usage: my_program (--either-this <and-that> | <or-this>)
```

Another use case is when you need to specify that if one element is present, then another one is
required, which you can achieve as:

```
Usage: my_program [(<one-argument> <another-argument>)]
```

In this case, a valid program invocation could be with either no arguments, or with both arguments together.

## Mutually exclusive elements

Mutually-exclusive elements can be separated with a pipe `|` as follows:

```
Usage: my_program go (up | down | left | right)
```

Use parentheses `()` to group elements when one of the mutually exclusive cases is required.
Use brackets `[]` to group elements when none of the mutually exclusive cases is required:

```
Usage: my_program go [up | down | left | right]
```

Note that specifying several patterns works exactly like pipe "|", that is:

```
Usage:
  my_program run [fast]
  my_program jump [high]
```

is equivalent to:

```
Usage: my_program (run [fast] | jump [high])
```

## Repeatable elements

Use ellipsis `...` to specify that the argument (or group of arguments) to the left could be
repeated one or more times:

```
Usage:
  my_program open <file>...
  my_program move (<from> <to>)...
```

You can flexibly specify the number of arguments that are required. Here are 3 (redundant) ways
of requiring zero or more arguments:

```
Usage:
  my_program [<file>...]
  my_program [<file>]...
  my_program [<file> [<file> ...]]
```

One or more arguments:

```
Usage: my_program <file>...
```

Two or more arguments (and so on):

```
Usage: my_program <file> <file>...
```

When parsed, repeatable positional arguments will be available as arrays in your program:

```
# If invoked as: my_program open file1.txt file2.txt file3.txt
# The variables available would be:
file = ["file1.txt", "file2.txt", "file3.txt"]
```

Commands and option flags that can occur more than once are counted instead:

```
Usage:
  my_program go (up | down)...
  my_program [-v...]

# my_program go up up down  ->  up = 2, down = 1
# my_program -vvv           ->  options.v = 3
```

An element written more than once in a pattern counts as repeatable too: `[<x>] [<x>]` produces a
list and `[-v] [-v]` a count. An option cannot be given more times than its pattern allows:
`my_program [-v]` rejects `-v -v`.

## The `[options]` shortcut

`[options]` stands for every option described in the help text that no usage pattern writes
explicitly. The options can be given in any order:

```
Usage: my_program [options] <file>

Options:
  -v --verbose  Enable verbose output
  -n --dry-run  Do not change anything
```

Here `my_program -n -v file.txt` and `my_program --verbose file.txt` are valid, but options after
`<file>` are not. An option written explicitly in any pattern is left out of `[options]` in every
pattern: with `my_program run [-v]` and `my_program list [options]`, `list -v` is rejected.

## Argument formatting rules

When writing your usage patterns, follow these formatting rules:

1. Command names are lowercase ASCII words of letters and digits, starting with a letter and
   joined by `-` or `_` (`my-command`, `step2`)
2. Positional arguments follow the same rules, written as `<lowercase-with-hyphens>` or `UPPERCASE`
3. Option flags begin with `-` or `--`
4. Long option names use hyphens for spaces (`--long-option`)
5. When option flags accept values, format as `--option=VALUE` or `-o VALUE`, and describe them in
   the options section

## Advanced usage patterns

Complex command-line interfaces can combine all the elements described above:

```
Usage:
  program ship new <name>...
  program ship <name> move <x> <y> [--speed=<kn>]
  program ship shoot <x> <y>
  program mine (set|remove) <x> <y> [--moored|--drifting]
  program -h | --help
  program --version
```

Options can be described in a separate section:

```
Usage: my_program [options] <command>

Options:
  -h --help         Show this help message
  --version         Show version information
  -v --verbose      Enable verbose output
  -o FILE, --output=FILE  Write output to FILE
```

This defines which flags are available and how they should be parsed, especially for options that take arguments.
