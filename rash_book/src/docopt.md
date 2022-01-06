---
title: Command-line interfaces
weight: 7000
---

# Command-line interfaces <!-- omit in toc -->

`rash` has an integrated command-line parser based in the documentation of your script.

This is an ad-hoc implementation based in [Docopt](http://docopt.org/). The main idea
behind is to write the documentation and `rash` automatically parses arguments based on it.

E.g.:

```yaml
#!/usr/bin/env rash
#
# Copy files from source to dest dir
#
# Usage: copy.rh <source>... <dest>
#

- copy:
    src: "{{ item }}"
    dest: "{{ dest }}/{{ item | split(pat='/') | last }}"
  loop: "{{ source }}"
```

## Syntax

### Usage patterns

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

### [optional elements]

Elements (arguments, commands) enclosed with square brackets `[]` are marked to be
optional. It does not matter if elements are enclosed in the same or different pairs of brackets.

E.g.:

```
Usage: my_program [command <argument>]
```

```
Usage: my_program [command] [<argument>]
```

### (required elements)

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

### element|another

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

### element...

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

## Parser

Elements are parsed using usages and automatically added to your `rash` variables.

Commands are parsed as fault by default and when are passed they will appear as true:

```json
{
    "run": true,
    "fast": true,
    "jump": false,
    "high": false,
}
```

**Note**: `help` is a special case because if help is passed as argument, the program will show
all documentation and after that exit 0.

Positional arguments are parsed like this:

```json
{
    "argument": "value",
    "repeating-argument": ["value1", "value2"...],
}
```
