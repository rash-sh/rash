---
title: CLI Reference
weight: 1500
---

# CLI Reference <!-- omit in toc -->

This section describes the command-line arguments and environment variables that the `rash` binary accepts.

## Usage

```bash
rash [OPTIONS] <SCRIPT_FILE> [SCRIPT_ARGS]...
```

or

```bash
rash [OPTIONS] --script <SCRIPT> [SCRIPT_ARGS]...
```

## Arguments

### `<SCRIPT_FILE>`

Path to the script file to be executed. This file should contain a valid rash script in YAML format.

If provided, this file will be read and used as the script content.

### `[SCRIPT_ARGS]...`

Additional arguments to be accessible to rash scripts.

These arguments can be accessed from the builtin `{{ rash.args }}` as a list of strings. If a usage pattern is defined in your script using the [docopt](docopt.md) format, they will be parsed and added as variables too.

## Options

### `-b, --become`

Run operations with become (privilege escalation).

This enables privilege escalation for all tasks in the script. Note that this does not imply password prompting.

**Example:**
```bash
rash --become my-script.rh
```

### `-u, --become-user <USER>`

Run operations as this user.

This option only works when `--become` is enabled. Specifies which user to become when executing tasks.

**Default:** `root`

**Example:**
```bash
rash --become --become-user www-data my-script.rh
```

### `-c, --check`

Execute in dry-run mode without modifications.

When this flag is enabled, rash will simulate the execution without making any actual changes to the system. This is useful for testing scripts before running them.

**Example:**
```bash
rash --check my-script.rh
```

### `-d, --diff`

Show the differences.

This option displays the differences that would be made by the script execution, similar to a unified diff format.

**Example:**
```bash
rash --diff my-script.rh
```

### `-e, --environment <KEY=VALUE>`

Set environment variables.

Environment variables set this way can be accessed from the builtin `{{ env }}`. For example, if you set `KEY=VALUE`, you can access it as `{{ env.KEY }}`.

This option can be used multiple times to set multiple environment variables.

**Example:**
```bash
rash -e USER=john -e ENV=production my-script.rh
```

**Template usage:**
```yaml
- name: Print environment variable
  debug:
    msg: "User is {{ env.USER }}"
```

### `-o, --output <FORMAT>`

Set the output format.

**Available formats:**
- `ansible` (default): Ansible-style output
- Other formats may be available depending on your rash version

**Default:** `ansible`

**Example:**
```bash
rash --output ansible my-script.rh
```

### `-v, --verbose`

Verbose mode.

Increase the verbosity of the output. This option can be specified multiple times to increase verbosity further:
- `-v`: INFO level logging (DEBUG level internally)
- `-vv`: TRACE level logging

**Example:**
```bash
rash -v my-script.rh
rash -vv my-script.rh  # Even more verbose
```

### `-s, --script <SCRIPT>`

Inline script to be executed.

Instead of reading from a file, you can provide the script content directly as a string. If this option is provided, `<SCRIPT_FILE>` will be used as the filename in the `rash.path` builtin variable.

**Example:**
```bash
rash --script '#!/usr/bin/env rash
- name: Hello
  debug:
    msg: "Hello World"' my-script-name.rh
```

## Environment Variables

### `RASH_LOG_LEVEL`

Set the log level for rash execution.

This environment variable provides an alternative way to set the verbosity level when the `-v` flag is not used. If `-v` is specified, it takes precedence over this environment variable.

**Accepted values:**
- `DEBUG`: Equivalent to `-v`
- `TRACE`: Equivalent to `-vv`

**Example:**
```bash
RASH_LOG_LEVEL=DEBUG rash my-script.rh
RASH_LOG_LEVEL=TRACE rash my-script.rh
```

## Examples

### Basic script execution

```bash
rash my-script.rh
```

### Dry-run with diff

```bash
rash --check --diff my-script.rh
```

### Execute with privilege escalation

```bash
rash --become my-script.rh
```

### Execute as specific user with environment variables

```bash
rash --become --become-user www-data -e APP_ENV=production my-script.rh
```

### Verbose execution with inline script

```bash
rash -vv --script '#!/usr/bin/env rash
- name: Debug example
  debug:
    msg: "This is a test"'
```

### Pass arguments to script

```bash
rash my-script.rh install package1 package2
```

In your script, you can access these arguments:

```yaml
#!/usr/bin/env rash
#
# Usage:
#   my-script.rh <command> <packages>...
#

- name: Print command
  debug:
    msg: "Command: {{ command }}, Packages: {{ packages }}"
```

## See Also

- [Command-line interfaces](docopt.md) - For parsing arguments within your rash scripts
- [Vars](vars.md) - For understanding how to use variables in your scripts
- [Builtins](builtins.md) - For details on builtin variables like `{{ rash.args }}` and `{{ env }}`
