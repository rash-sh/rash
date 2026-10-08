---
title: Breaking changes
weight: 4100
indent: true
---

# Breaking changes

Behaviour changes that can affect existing scripts, with how to migrate them. New features
(`failed_when`, `quiet`, `no_log`, `meta: exit`, include `export`, `defaults`, stdio modes, `pause`
input) are described in [Tasks](tasks.md) and the module pages.

## Changes since 2.21.0

### Results and failures

- **Non-zero exits are results.** When `command`, `shell` or `script` exits non-zero, the task now
  produces a result with `rc`, `stdout`/`output`, `stderr`, `failed: true` and an `error`
  (`command exited with code 3: <stderr>`). Before, the task returned an error with only the
  stderr text and registered nothing. A process killed by a signal reports `rc = 128 + signal`.
  To accept some exit codes, use `failed_when` (for example `failed_when: result.rc not in [0, 1]`)
  instead of `ignore_errors`. See [Process failures are results](tasks.md#process-failures-are-results).
- **`ignore_errors` registers the failed result.** The registered variable keeps `failed: true`,
  `rc`, `stderr` and `error`, and an ignored failure reports `changed: true` when the module did.
  Before, nothing was registered, so `{{ result }}` was undefined. Test it with `result is failed`.
- **`ignore_errors` also covers template and condition errors.** An undefined variable in the
  params, `when`, `changed_when` or `failed_when` of a task with `ignore_errors: true` is now
  ignored like any other failure instead of stopping the script.
- **`until` retries failed attempts.** An attempt that fails (for example a non-zero exit) is
  retried instead of failing the task at once. When retries run out, the task fails with
  `until condition not satisfied after N retries`, which `ignore_errors` can ignore; the registered
  result has `failed: true`. `until` sees the task `vars` like `when` does, and a task skipped by
  `when` is not retried.
- **`result` in conditions.** `changed_when` and `failed_when` see the current result as `result`
  and under the `register` name, shadowing variables with those names.
- **Registered result shape.** Every registered result has `changed`, `failed`, `output`, `stdout`
  (alias of `output`), `extra` and `error`, plus the non-conflicting keys of `extra` at the top
  level. Code that dumps or iterates a whole result sees the new keys.
- **Loops register every item.** The registered variable is the last item's result, with `changed`
  and `failed` true when any item changed or failed, the first item's `error`, and a `results` list
  with each executed item's result (including its `item`), so `result is failed` and
  `result.results | map(attribute='rc')` work. Before, the first item's result was kept and there
  was no `results`. `set_vars` inside a loop keeps the last item's value.
- **`--output json` results have `failed`**, next to `changed`, `output` and `extra`.
- **A failing `command`, `shell` or `script` prints its result line** (with the process output)
  before the error, like a successful one.
- **Async results** contain stdout only (stdout and stderr were merged before), plus `rc` and
  `stderr`. A job exiting non-zero gives a failed result.
- **Async loops with a positive `poll`** register the same shape as synchronous loops (last item
  plus `results`, see above): `rash_job_ids` and the `job_id` of each item are no longer included,
  and `changed_when`/`failed_when` are evaluated per item with `item` in scope. `poll: 0` still
  registers `rash_job_ids`.
- **`async_status` and `async_poll` fail when the job failed**, with the job error as task error.
  Before they reported success with `failed: true` inside the result. Use `ignore_errors: true`
  or `failed_when` to inspect a failed job.
- **`rescue` and `always` changes count.** A task with a `rescue` or `always` section reports
  `changed: true` when the section changed something, so it can now `notify` handlers. A `block`
  itself never reports `changed`: `register` and `notify` on a block see no change, use them on its
  child tasks.

### Modules

- **`pause`** always reports `changed: false`. With no seconds or minutes it outputs nothing
  instead of `"0"`, and it prints its `prompt` even then.
- **`script`** parses `args` and `executable` with shell-like quoting: `args: "'a b' c"` passes
  two arguments (it was split on whitespace before) and `executable: "python3 -u"` is split into
  program and arguments (it was used verbatim before). A word starting with `#` begins a comment,
  in `args`, `executable`, `cmd` with `transfer_pid` and shebang lines: quote it
  (`args: "'#channel'"`). The whole shebang line is honored (`#!/usr/bin/env sh` works) and check
  mode is supported: under `--check` the script is no longer run, but a missing file is still an
  error.
- **`no_log` on an `async` task** covers the task that starts the job only: the `async_status` or
  `async_poll` task that reports its output needs its own `no_log`.
- **`command` and `script` inherit stdin** like `shell` already did (it was `/dev/null`). A
  process reading stdin can now consume Rash's input or wait for terminal input; pass `stdin: ""`
  to give it an empty stdin.
- **Async jobs get an empty stdin** unless `stdin` is given. Before, they inherited Rash's stdin,
  so on a terminal they could be stopped waiting for input until their timeout.
- **`command` with `transfer_pid`** splits `cmd` with shell-like quoting instead of whitespace,
  rejects `stdin` and runs with `stdout`/`stderr: capture` or `tee` as `inherit`: nothing is left
  to capture once Rash is replaced. When the program cannot be executed (missing or not
  executable), Rash exits with status 1 and an error naming it, also under `ignore_errors` or
  inside `rescue`, with or without `become`. Before, it was an ordinary task failure.
- **`meta`** rejects unknown parameters.

### Parsing and validation

- **Invalid `become_method`** is an error. Before, it logged a warning and fell back to the global
  method.
- **Unknown top-level keys** of the mapping script form (anything but `tasks`, `handlers` and
  `defaults`) are errors. Before, they were ignored.
- **`rescue` and `always` must be lists**, checked when the task is parsed instead of when the
  section runs: for top-level tasks when the script is parsed; for tasks inside a `block`, an
  included file or a `rescue`/`always` section when that block, file or section runs.
- **Typed task attributes must have the right YAML type.** `become`, `check_mode`,
  `ignore_errors`, `quiet` and `no_log` must be booleans, and `retries`, `delay`, `async` and
  `poll` integers. A string, including a template (`retries: "{{ n }}"`), is now a parse error.
  Before, it was silently ignored and the default used.

### Privilege escalation and check mode

- **`become` and `check_mode` on `block` and `include`** apply to every child task, and each child
  escalates on its own. Before, `check_mode: true` on a block or include was ignored and its
  children ran for real, and `become` ran the whole block as the become user. Control-flow
  modules (`block`, `include`, `meta`, `set_vars`, `debug`, `assert`, `fail`, `pause`,
  `async_status`, `async_poll` and custom modules) always run in the Rash process. A child cannot
  disable an inherited `become` or `check_mode`.
- **`rescue` and `always` run as part of their task.** Their tasks see the task `vars` and inherit
  its `become` and `check_mode` (before, they ran for real under `check_mode: true`). With
  `ignore_errors: true` a failure is no longer rescued: `always` runs and the task is reported as an
  ignored failure instead of a success that notified handlers.
- **`become_method: syscall` runs the task in a new child Rash process** started as Rash's user,
  which switches to the become user before running the task, taking that user's supplementary
  groups, on Linux and macOS alike. Before, Rash forked itself and the child kept Rash's
  supplementary groups (all of root's groups when running as root).
- **`command` with `transfer_pid` and `become_method: syscall`** switches user in Rash itself and
  exits Rash when the program cannot be executed (see [Modules](#modules)). Before, Rash had
  already switched to the become user and went on running the remaining tasks as that user.
- **`become_method: sudo` to a non-root user other than the current one** is refused unless Rash
  runs as root: task data is exchanged through private files that user could not read.
- **Async tasks with `become`** run the job as the become user, with that user's supplementary
  groups (it was ignored before); `become_method: sudo` is rejected for async tasks.
- **Async tasks in check mode** do not start the job; they report the change they would make.

### Signals

- **Ctrl-C, SIGTERM and SIGHUP stop the script** with exit status `128 + signal` (130, 143, 129)
  and kill running async jobs. Before, Rash died from the default signal action, leaving its child
  process and async jobs running.
- While a `command`, `shell` or `script` process runs, signals sent with `kill` are forwarded to it,
  Rash waits for it, and then stops the script: `ignore_errors`, `failed_when`, `rescue` and loops
  do not swallow the interruption, but `always` sections run. A terminal Ctrl-C that the process
  handles itself (it exits normally) does not stop Rash.
- Between tasks or during an in-process module (`pause`, `copy`, async polling, ...) Rash exits
  immediately, without running `always` sections.

See [Signals and interactive commands](tasks.md#signals-and-interactive-commands).
