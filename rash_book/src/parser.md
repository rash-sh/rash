---
title: Parser
weight: 10200
indent: true
---

# Parser

## How elements are parsed

Elements are parsed using usages and automatically added to your context variables. Commands and
positional arguments become top-level variables, and options are grouped under `options`.

```yaml
#!/usr/bin/env rash
#
# Usage: ./program [options] (start|stop|restart)
#
# Options:
#   --count=<n>  Number of iterations [default: 1]

- debug:
    msg: "{{ options }}"

- debug:
    msg: "{{ start }}"
```

## Command parsing

Commands are parsed as `false` by default and when they are passed they will appear as `true`:

```json
{
  "command1": true,
  "command2": false,
  "command3": false
}
```

Example: with the usage pattern `./program (start|stop|restart)`, if you call `./program start`, the variables will be:

```json
{
  "start": true,
  "stop": false,
  "restart": false
}
```

**Note**: Command with dashes are converted to underscores in the resulting JSON. E.g., `./program my-command`:

```json
{
  "my_command": true
}
```

A command that can occur more than once in a pattern, such as `./program go (up|down)...`, is a
count instead: `./program go up up` gives `"up": 2, "down": 0`.

## Option parsing

Options are grouped under the `options` key in the resulting JSON:

```json
{
  "options": {
    "apply": false,
    "dry_run": false,
    "help": false,
    "number": "10",
    "timeout": null,
    "version": false,
    "q": true
  },
  "port": "443"
}
```

In the example above:

- Boolean options not provided are `false`
- Options with values have their string value (like `"number": "10"`); if a value option is given
  several times, the last value is kept
- Options without default values that weren't provided will be `null`
- Options are keyed by their long name; short options without a long alias use their
  single-letter key (like `"q": true`)
- Options with dashes are converted to underscores in the resulting JSON
- Flags that can occur more than once (`[-v...]`, `[--verbose]...`) are counts, `0` when not given

**Special case - help**: The `help` option is handled specially. If help is passed as an argument or option, the program
will show all documentation and exit with a status code of 0 without running any task:

```bash
./program --help   # Shows help text and exits, if --help (or an alias like -h) is declared
./program help     # Same behavior if 'help' is defined as a command
```

`--help` must be accepted by a usage pattern, either where it is declared or in place of a
positional argument. See [Help and errors](docopt.md#help-and-errors).

### Default values for options

When you specify a default value for an option in the options section, that value will be used when the option isn't explicitly provided in the command line. For example:

```
# Options:
#   --timeout=<seconds>    Connection timeout [default: 30]
#   --retries=<count>      Number of retry attempts [default: 3]
```

If your script is invoked without these options, the values in your script would be:

```json
{
  "options": {
    "timeout": "30", // Default value applied
    "retries": "3" // Default value applied
  }
}
```

If you provide a different value in the command line, it overrides the default:

```bash
./script.rh --timeout=60
```

Would result in:

```json
{
  "options": {
    "timeout": "60", // Command-line value overrides default
    "retries": "3" // Default value applied
  }
}
```

## Positional argument parsing

Positional arguments are parsed as strings or arrays depending on whether they're repeatable
(followed by `...`, inside a repeated group, or written more than once in a pattern):

```json
{
  "argument": "value",
  "repeating_argument": ["value1", "value2", "value3"]
}
```

If a positional argument isn't provided in the command line, it will be omitted from the variables
JSON, so use the `default` filter for optional ones: `{{ dest | default('.') }}` or
`{{ files | default([]) }}`.

**Note**: Positional arguments with dashes are converted to underscores in the resulting JSON. E.g., `./program <repeating-argument>...`:

```json
{
  "repeating_argument": ["value1", "value2", "value3"]
}
```

Examples:

1. For a usage pattern `./program <file>`:

   ```bash
   ./program document.txt
   ```

   Results in:

   ```json
   {
     "file": "document.txt"
   }
   ```

   **Note**: Uppercase arguments are converted to lowercase in the resulting JSON.

   For usage pattern `./program FILE`:

   ```bash
   ./program document.txt
   ```

   Results in:

   ```json
   {
     "file": "document.txt"
   }
   ```

2. For a usage pattern `./program <file>...`:

   ```bash
   ./program file1.txt file2.txt file3.txt
   ```

   Results in:

   ```json
   {
     "file": ["file1.txt", "file2.txt", "file3.txt"]
   }
   ```

3. For a usage pattern `./program (<source> <dest>)...`:

   ```bash
   ./program file1.txt dir1 file2.txt dir2
   ```

   Results in:

   ```json
   {
     "source": ["file1.txt", "file2.txt"],
     "dest": ["dir1", "dir2"]
   }
   ```

4. For a usage pattern `./program [--verbose] <command>`:
   ```bash
   ./program --verbose start
   ```
   Results in:
   ```json
   {
     "options": {
       "verbose": true
     },
     "command": "start"
   }
   ```

## Accessing parsed values in rash scripts

Within your rash script, you can access these values using standard variable access syntax:

```yaml
#!/usr/bin/env -S rash --
#
# Usage: ./script.rh <name> [--count=<n>]
#
# Options:
#   --count=<n>  Number of iterations [default: 1]

- name: Print a greeting
  debug:
    msg: "Hello, {{ name }}! ({{ options.count }} times)"
```

The comment block must start right after the shebang line. When invoked with
`./script.rh World --count=3`, it prints `Hello, World! (3 times)`. Without `--count`, the default
`1` is used.
