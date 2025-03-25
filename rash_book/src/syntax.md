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
- [Argument formatting rules](#argument-formatting-rules)
- [Advanced usage patterns](#advanced-usage-patterns)

## Usage patterns

Text occurring between keyword `usage:` (case-insensitive) and a visibly empty line is interpreted as
a list of usage patterns. The first word after `usage:` is interpreted as the program's name. Here is a
minimal example for a program that takes no command-line arguments:

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
arguments.

```
Usage: my_program <host> <port>
Usage: my_program HOST PORT
```

Both styles are equivalent, though the `<argument-name>` style is recommended for clarity. Positional
arguments are required by default unless placed within optional brackets `[]`.

When used in your program, these positional arguments will be available as variables with their name
in lowercase:

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

**Note**: Shebang line `#!/usr/bin/env rash --` can be used to pass options to the script directly:

## Optional elements

Elements (arguments, commands) enclosed with square brackets `[]` are marked as
optional. It does not matter if elements are enclosed in the same or different pairs of brackets.

The following examples are equivalent:

```
Usage: my_program [command <argument>]
```

```
Usage: my_program [command] [<argument>]
```

Optional elements can be nested:

```
Usage: my_program [command [--option]]
```

In this example, `--option` can only be used if `command` is provided.

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
Usage: my_program run [fast]
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
Usage: my_program open <file>...
       my_program move (<from> <to>)...
```

You can flexibly specify the number of arguments that are required. Here are 3 (redundant) ways
of requiring zero or more arguments:

```
Usage: my_program [<file>...]
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

When parsed, repeatable elements will be available as arrays in your program:

```
# If invoked as: my_program open file1.txt file2.txt file3.txt
# The variables available would be:
file = ["file1.txt", "file2.txt", "file3.txt"]
```

## Argument formatting rules

When writing your usage patterns, follow these formatting rules:

1. Command names should be lowercase words without special characters
2. Positional arguments should be in `<lowercase-with-hyphens>` or `UPPERCASE` format
3. Option flags should begin with `-` or `--`
4. Long option names should use hyphens for spaces (`--long-option`)
5. When option flags accept values, format as `--option=VALUE` or `-o VALUE`

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
