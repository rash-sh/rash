---
title: Lookups
weight: 8000
---

# Lookups

Lookups are MiniJinja functions that obtain or derive values while Rash renders a task. They execute
locally in the Rash process, as Rash's user; task keywords such as `check_mode`, `async`, `quiet`,
and `environment` do not change how a lookup runs. The exception is `become`: the `changed_when`,
`failed_when` and `environment` of a become task are rendered in the become child, so a lookup
there runs as the become user.

Use a lookup when a value is needed **during rendering**. Use a module when the operation is itself a
task whose changed/failed state, result, retries, privilege escalation, or output handling matters.

## Available lookups

| Lookup | Purpose |
| --- | --- |
| `file(...)` | Read a local text file, with optional leading/trailing whitespace stripping. |
| `find(...)` | Run Rash's structured `find` implementation and return its result data. |
| `password(...)` | Generate and persist an idempotent local password, or read an existing password file. |
| `passwordstore(...)` | Read a value from Password Store when the `passwordstore` feature is enabled. |
| `pipe(...)` | Run `/bin/sh -c` locally and return stdout, similar to shell command substitution. |
| `vault(...)` | Retrieve a secret from HashiCorp Vault. |

The generated reference below contains the exact parameters and examples for each lookup.

## Command substitution with `pipe`

`pipe()` is the direct equivalent of the common Bash `$(...)` pattern:

```yaml
- set_vars:
    kernel_release: "{{ pipe('uname -r') }}"
    first_disk: "{{ pipe('find /dev -name \"sd*\" | head -n1') }}"
```

`pipe()`:

- executes through `/bin/sh -c`, so shell pipelines/redirections work;
- returns stdout as a string with trailing whitespace (newlines, spaces, tabs) removed;
- fails rendering if the command cannot be started or exits non-zero;
- does not participate in task `failed_when`, `register`, or async semantics, and runs as Rash's
  user except in the become task conditions described above.

Because it invokes a shell, do not concatenate untrusted input into the command. When command
execution itself is the operation you want to model, prefer a `command` or `shell` task instead.

## File and structured lookups

`file()` reads text during rendering:

```yaml
- debug:
    msg: "hostname={{ file('/etc/hostname') }}"
```

`find()` accepts the same structured query used by Rash's `find` module and is useful when the
returned collection feeds a loop:

```yaml
- copy:
    src: "{{ item }}"
    dest: "/tmp/archive/{{ item | split('/') | last }}"
  loop: "{{ find({'paths': '/tmp/input', 'recurse': true}) }}"
```

## Secret-producing lookups

Values returned by `password`, `passwordstore`, and `vault` are ordinary template values. Rash
cannot infer that a rendered value is secret after it is inserted into a task, so mark tasks that
handle credentials with `no_log: true` where their parameters or results could otherwise be logged.

```yaml
- uri:
    url: https://example.invalid/private
    headers:
      Authorization: "Bearer {{ vault('apps/example:token') }}"
  no_log: true
```

## Lookups index

{$include_lookup_index}
