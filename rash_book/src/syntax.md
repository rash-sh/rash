---
title: Syntax
weight: 7100
indent: true
---

# Syntax  <!-- omit in toc -->

{% raw %}
- [Usage patterns](#usage-patterns)
- [<argument> ARGUMENT](#argument-argument)
- [-o --option](#-o---option)
- [[optional elements]](#optional-elements)
- [(required elements)](#required-elements)
- [element|another](#elementanother)
- [element...](#element)
{% endraw %}

## Usage patterns

Text occurring between keyword usage: (case-insensitive) and a visibly empty line is interpreted as
list of usage patterns. The first word after usage: is interpreted as the program's name. Here is a
minimal example for program that takes no command-line arguments:

```
Usage: my_program
```

Program can have several patterns listed with various elements used to describe the pattern:

```
  my_program command <argument>
  my_program [<optional-argument>]
  my_program (either-this-command | or-this-other-command)
  my_program <repeating-argument> <repeating-argument>...
```

Each of the elements and constructs is described below. We will use the word _word_ to describe a
sequence of characters delimited by either whitespace, one of `[]()|` characters, or `...`.


{% raw %}
## <argument> ARGUMENT
{% endraw %}

Words starting with "<", ending with ">" and upper-case words are interpreted as positional
arguments.

```
Usage: my_program <host> <port>
```

## -o --option

Words starting with one or two dashes (with exception of `-`, `--` by themselves) are interpreted
as short (one-letter) or long options, respectively.

- Short options can be `stacked` meaning that -abc is equivalent to -a -b -c.
- Long options can have arguments specified after space or equal `=` sign:
       `--input=ARG` is equivalent to `--input ARG`.
- Short options can have arguments specified after optional space:
    `-f FILE` is equivalent to `-fFILE`.

**Note**: writing `--input ARG` (as opposed to `--input=ARG`) is ambiguous, meaning it is not
possible to tell whether `ARG` is option's argument or a positional argument. In usage patterns
this will be interpreted as an option with argument only if a description (covered below) for that
option is provided. Otherwise it will be interpreted as an option and separate positional argument.

There is the same ambiguity with the `-f FILE` and `-fFILE` notation. In the latter case it is not
possible to tell whether it is a number of stacked short options, or an option with an argument.
These notations will be interpreted as an option with argument only if a description for the option
is provided.

**Warning**: options should be passed to rash after `--` to be interpreted as script arguments.
Other way it is to use them after another non option arg.

## [optional elements]

Elements (arguments, commands) enclosed with square brackets `[]` are marked to be
optional. It does not matter if elements are enclosed in the same or different pairs of brackets.

E.g.:

```
Usage: my_program [command <argument>]
```

```
Usage: my_program [command] [<argument>]
```

## (required elements)

All elements are required by default, if not included in brackets `[]`. However, sometimes it is
necessary to mark elements as required explicitly with parentheses `()`. For example, when you
need to group mutually-exclusive elements (see next section):

```
Usage: my_program (--either-this <and-that> | <or-this>)
```

Another use case is when you need to specify that if one element is present, then another one is
required, which you can achieve as:

```
Usage: my_program [(<one-argument> <another-argument>)]
```
In this case, a valid program invocation could be with either no arguments, or with 2 arguments.

## element|another

Mutually-exclusive elements can be separated with a pipe `|` as follows:

```
Usage: my_program go (up | down | left | right)
```

Use parentheses `()` to group elements when one of the mutually exclusive cases is required.
Use brackets `[]` to group elements when none of the mutually exclusive cases is required:

```
Usage: my_program go [up | down | left | right]
```

Note, that specifying several patterns works exactly like pipe "|", that is:

```
Usage: my_program run [fast]
       my_program jump [high]
```

is equivalent to:

```
Usage: my_program (run [fast] | jump [high])
```

## element...

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
