---
title: Tasks
weight: 4000
---

# Tasks

Tasks are Rash's main execution unit. Every task selects exactly one module and may add execution
keywords such as conditions, registration, retries, privilege escalation, error handling, or output
control.

Upgrading a script written for an older Rash? See [Breaking changes](breaking-changes.md).

```yaml
{{#include ../../examples/task.rh:3:}}
```

## Keywords

| Keyword | Type | Description |
| --- | --- | --- |
| `name` | string | Human-readable task name. |
| `when` | string or list | MiniJinja expression(s), written without `{{ }}`. The task runs only when all expressions are true. |
| `vars` | map | Variables scoped to the task. |
| `environment` | map | Environment variables made available while the task executes. |
| `register` | string | Store the structured task result under this variable name. |
| `changed_when` | string or list | Override the task's changed status with a MiniJinja expression. |
| `failed_when` | string or list | Override the task's failure status with a MiniJinja expression. |
| `ignore_errors` | boolean | Continue execution after a failed task while preserving the failed result. |
| `loop` | list or template | Execute the task for every rendered item. The current item is available as `item`; `register` keeps the last item's result. |
| `until` | string or list | Repeat the task until the expression becomes true. |
| `retries` | integer | Number of retries for `until`; defaults to 3. |
| `delay` | integer | Delay between retries, in seconds; defaults to 0. |
| `async` | integer | Maximum runtime, in seconds, for asynchronous `command`/`shell`/`script` execution. |
| `poll` | integer | Async polling interval in seconds. `0` means fire-and-forget. |
| `rescue` | list | Tasks executed when the main task fails. |
| `always` | list | Tasks executed after the main task regardless of success or failure. |
| `notify` | string or list | Handler name(s) queued when the task reports `changed: true`. |
| `check_mode` | boolean | Execute the task in dry-run/check mode when supported by the module. |
| `quiet` | boolean | Suppress the task's normal module-result output while leaving the task itself visible. |
| `no_log` | boolean | Suppress all logging for the task and redact its name and error. Use for credentials and other sensitive values. |
| `become` | boolean | Run the task with privilege escalation. |
| `become_user` | string | Target user when `become` is enabled. |
| `become_method` | string | Privilege escalation method: `syscall` (default) or `sudo`. |
| `become_exe` | string | Sudo executable used with `become_method: sudo`. |
| `become_password` | string | Password used with sudo become. Prefer a protected value and `no_log: true`. |

Boolean values can be used directly for `when`, `changed_when` and `failed_when`. A list of
expressions is evaluated as a logical AND.

## Registered results

`register` stores the finalized task result, after `changed_when` and `failed_when` have been
evaluated. A registered result always provides these generic fields:

| Field | Meaning |
| --- | --- |
| `changed` | Final changed status. |
| `failed` | Final failure status. |
| `output` | Generic module output, when present. |
| `stdout` | Compatibility alias for `output`. |
| `extra` | Module-specific structured payload. |
| `error` | Failure description when the task failed. |

Non-conflicting keys from `extra` are also exposed at the result's top level. For example,
`command`, `shell` and `script` provide `rc` and `stderr`, while modules such as `stat` can expose
module-specific objects such as `stat`:

```yaml
- command:
    argv: [sh, -c, "printf hello; exit 3"]
  register: command_result
  ignore_errors: true

- debug:
    msg: "rc={{ command_result.rc }}, stdout={{ command_result.stdout }}"

- stat:
    path: /etc/hostname
  register: hostname

- debug:
    msg: "exists={{ hostname.stat.exists }}"
```

The generic `extra` field remains available even when its keys are flattened:

```yaml
- debug:
    msg: "{{ command_result.extra.rc }} == {{ command_result.rc }}"
```

Rash also provides result tests for conditions:

```yaml
- command: "false"
  register: probe
  ignore_errors: true

- debug:
    msg: "The probe failed"
  when: probe is failed

- debug:
    msg: "The probe succeeded"
  when: probe is succeeded
```

Supported result tests are `failed`, `succeeded` (alias `success`) and `changed`.

### Process failures are results

For `command`, `shell` and `script`, a process that starts correctly but exits non-zero still
produces a structured result. By default that result has `failed: true`; `failed_when` can redefine
which exit statuses are considered failures:

```yaml
- command:
    argv: [grep, -q, needle, file.txt]
  register: grep_result
  changed_when: false
  failed_when: result.rc not in [0, 1]

- debug:
    msg: "needle was not present"
  when: grep_result.rc == 1
```

Both `result` and the task's `register` name are available while Rash evaluates `changed_when` and
`failed_when`.

`ignore_errors: true` changes control flow, not the result: execution continues, but the registered
value remains `failed: true`. This makes it possible to inspect the exact failure later. It also
ignores errors rendering the task (undefined variables in params or conditions).

## Error handling

### `rescue`

`rescue` executes when the main task has a semantic or execution failure:

```yaml
- name: Update application
  command: ./update
  rescue:
    - debug:
        msg: "Update failed; running recovery"
    - command: ./recover
```

If rescue completes successfully, the task is considered recovered and execution continues.

### `always`

`always` is a finally-style section and runs after the main task and any rescue section:

```yaml
- name: Work with temporary state
  command: ./work
  always:
    - file:
        path: /tmp/work.lock
        state: absent
```

`rescue` and `always` wrap the **whole task execution**. When the task contains a loop, the loop is
executed as one aggregate operation: rescue runs once if that operation fails, and always runs once
after it completes. They are not independently invoked for every loop item.

Section tasks see the task `vars` and inherit its `become` and `check_mode` settings. With
`ignore_errors: true`, a failure is not rescued: `always` still runs and the task is reported as an
ignored failure (it does not notify handlers).

### Explicit script exit

`meta: exit` terminates a Rash script with an application-defined status code:

```yaml
- name: Stop with a CLI-style usage status
  meta:
    action: exit
    code: 2
  always:
    - debug:
        msg: "cleanup still runs"
```

An explicit exit is control flow rather than a failure: it is not swallowed by `ignore_errors` and
does not trigger `rescue`, but an `always` section still executes before Rash exits. The code must be
between 0 and 255 and defaults to 0.

## Retries

`until` repeats a task until its expression is true. The current zero-based retry count is available
as `retries` while evaluating the condition:

```yaml
- command: test -S /run/app.sock
  register: socket_check
  changed_when: false
  failed_when: false
  until: socket_check.rc == 0
  retries: 10
  delay: 1
```

Failed attempts are retried too. When the retries run out the task fails with `until condition not
satisfied` and the registered result reports `failed: true`.

## Asynchronous commands

`async` applies to `command`, `shell` and `script` tasks. Rash starts the process in a managed
process group so timeouts can terminate the process tree rather than only the immediate child.
Without `stdin` data the job gets an empty stdin: a background job never reads Rash's input or the
terminal.

```yaml
- command:
    argv: [./long-build]
    stdout: tee
    stderr: tee
  async: 600
  poll: 2
  register: build
```

With `poll: 0`, Rash returns immediately and the registered result contains `rash_job_id` (or
`rash_job_ids` for an asynchronous loop). With a positive polling interval, Rash waits and returns
the final structured process result. `transfer_pid` and async execution are intentionally
incompatible.

Async tasks accept exactly the same module parameters as synchronous ones (unknown fields are
rejected, `shell` honors `creates`/`removes`). In check mode no job is started and the task reports
the change it would make. With `become`, the job runs directly as the become user
(`become_method: syscall`); `become_method: sudo` is rejected for async tasks.

## Signals and interactive commands

Synchronous `command`, `shell` and `script` processes run in Rash's own process group, like the
foreground job of a shell: they can prompt on the terminal (`read`, `ssh`, `sudo`, `$EDITOR`) and
receive Ctrl-C directly.

When Rash receives SIGINT, SIGTERM or SIGHUP while such a process runs, it forwards signals sent
with `kill` (for example by `docker stop`) to the process, waits for it, and then stops the script:
the interruption is not swallowed by `ignore_errors`, `failed_when`, `rescue` or loops, but `always`
sections still run. A terminal Ctrl-C that the process handles itself (it exits normally, like an
editor or a REPL) does not stop Rash.

Between tasks, or while an in-process module such as `pause`, `copy` or async polling runs, Rash
stops immediately and `always` sections do not run. In every case running async jobs are killed and
Rash exits with `128 + signal` (130 for SIGINT, 143 for SIGTERM, 129 for SIGHUP).

## Script and block defaults

For larger local scripts, the top-level mapping form can define defaults shared by tasks and
handlers:

```yaml
defaults:
  environment:
    APP_ENV: production
  changed_when: false

tasks:
  - command: ./inspect
  - command: ./inspect-other
    environment:
      APP_DEBUG: "1"

handlers:
  - name: reload application
    command: ./reload
```

A task overrides a scalar default. `vars` and `environment` are merged key-by-key so a task can add
or replace individual entries. The traditional top-level sequence of tasks remains supported.

A `block` has the same optional defaults mechanism while preserving its original sequence form:

```yaml
- block:
    tasks:
      - command: ./migrate
      - command: ./verify
    defaults:
      environment:
        APP_ENV: production
      become: true
```

## Output and secrets

Use `quiet: true` when an internal task should not contribute its normal result to script output.
This is useful with `--output raw` when the script should behave like a Unix command and emit only a
final selected value.

Use `no_log: true` for sensitive tasks. It suppresses task logging rather than merely hiding the
final result, so rendered credentials are not exposed by normal debug/trace output: the task name
is shown as `<redacted>` and a failure is reported without its details. The registered result still
holds the real values, and output a process writes itself (`stdout: tee` or `inherit`) still reaches
the terminal.

```yaml
{{#include ../../examples/no_log.rh:5:}}
```

`command`, `shell` and `script` choose per stream what happens with process output: `capture`
(default, registered), `tee` (streamed live and registered), `inherit` (streamed live, not
registered) or `null` (discarded; quote it in YAML). `stdin` feeds data to the process; without it,
the process inherits Rash's stdin (async jobs get an empty one).

```yaml
{{#include ../../examples/stdio.rh:6:}}
```

## Using become

### Syscall method (default)

The syscall method starts a child Rash process that switches to the become user (its supplementary
groups from `initgroups(3)`, on Linux and macOS alike, its GID and UID) before running the task, and
requires `CAP_SETUID` and `CAP_SETGID` (or root):

```bash
sudo setcap cap_setgid,cap_setuid+ep $(which rash)
```

```yaml
- name: Configure a root-owned file
  become: true
  copy:
    dest: /etc/example.conf
    content: enabled=true
```

### Sudo method

The sudo method delegates privilege escalation to a sudo-compatible executable:

```yaml
- name: Install package
  become: true
  become_method: sudo
  become_user: root
  command:
    argv: [apt, install, -y, nginx]
```

A password can be supplied through `become_password`, or requested with `--ask-become-pass` / `-K`:

```bash
rash --become --become-method sudo -K script.rh
```

| Aspect | `syscall` | `sudo` |
| --- | --- | --- |
| Requires UID/GID capabilities | Yes | No |
| Requires sudo executable | No | Yes |
| Password support | No | Yes |
| Extra child Rash process | Yes (not for the current user or with `transfer_pid`) | Yes (started by sudo) |

### Control-flow modules

Modules that only drive Rash itself (`block`, `include`, `meta`, `set_vars`, `debug`, `assert`,
`fail`, `pause`, `async_status`, `async_poll` and custom modules) always run in the Rash process,
never as the become user, so `meta: exit` keeps its exit code and vars stay in scope. When
`become` (or `check_mode`) is set on a `block` or `include`, every child task inherits it and
escalates on its own; a child cannot turn it off.

With both methods, task data is exchanged with the child through private (`0600`) temporary files
that are removed afterwards, also when the script is interrupted. The syscall child opens them
before switching user; with the sudo method, becoming a non-root user other than the current one
requires running Rash as root. Signals sent to Rash while a become task runs are forwarded to the
child, like for any other process (see [Signals](#signals-and-interactive-commands)).

The temporary directory may be writable by other users (e.g. a `TMPDIR` kept by `sudo -E`), so the
child only gets the user to switch to from Rash's command line, never from those files, and refuses
a task or result file that is not a regular, single-link, owner-only file of the expected owner;
Rash reads the result through the file it created instead of reopening its path.
