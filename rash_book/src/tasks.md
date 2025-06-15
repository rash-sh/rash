---
title: Tasks
weight: 4000
---

# Tasks

`tasks` are the main execution unit. They need a module and admit some optional fields described
below.

```yaml
{{#include ../../examples/task.rh:3:}}
```

## Keywords

Tasks admit the following optional keys:

| Keyword       | Type    | Description                                                                     |
| ------------- | ------- | ------------------------------------------------------------------------------- |
| become        | boolean | run operations with become (does not imply password prompting)                  |
| become_user   | string  | run operations as this user (just works with become enabled)                    |
| check_mode    | boolean | Run task in dry-run mode without modifications                                  |
| changed_when  | string  | Template expression passed directly without `{{ }}`; Overwrite change status    |
| ignore_errors | string  | Template expression passed directly without `{{ }}`; if true errors are ignored |
| name          | string  | Task name                                                                       |
| loop          | array   | `loop` receives a Template (with `{{ }}`) or a list to iterate over it          |
| register      | string  | Variable name to store module result                                            |
| vars          | map     | Define variables in task scope. Does not support own reference variables.       |
| when          | string  | Template expression passed directly without {{ }}; if false skip task execution |
| rescue        | array   | List of tasks to execute when the main task fails                               |
| always        | array   | List of tasks to execute regardless of success or failure                       |

### Registering variables

Use the Register field to define the name of the variable in which you wish to save the module
result. Its value will conform to the following structure:

```rust
{{#include ../../rash_core/src/modules/mod.rs:module_result}}
```

For example:

```yaml
{{#include ../../examples/register.rh:3:}}
```

### Using become

First of all, to use become you will need to execute rash with a user with `CAP_SETUID` and
`CAP_SETGID` capabilities (e.g.: `root`).

You can enable become from multiple places: If you want to activate it for all tasks you can pass it
as an execution arg (`-b/--become`). Or you can enable it per tasks using the `become` keyword.

For example, to configure `resolv.conf` (which requires `root` privileges), you can use the default
value of `become_user`(`root`):

```yaml
- name: Configure OpenDNS as resolvers
  become: true
  copy:
    dest: /etc/resolv.conf
    content: |
      nameserver 208.67.222.222
      nameserver 208.67.220.220
```

In the other hand, if you want to run `rash` with become, you are most likely already running it as
`root` so you will use it to change to other uses. E.g.:

```yaml
- command: some-unprivileged-command
  become: true
  become_user: foo
```

### Error handling with rescue

The `rescue` attribute allows you to define tasks that will execute only when the main task fails.
This provides a way to handle errors gracefully and perform cleanup or recovery operations.

```yaml
- name: Task that might fail
  command:
    cmd: "risky-command"
  rescue:
    - name: Handle the failure
      debug:
        msg: "Main task failed, running recovery"
    - name: Cleanup after failure
      file:
        path: "/tmp/cleanup_needed"
        state: absent
```

Key points about `rescue`:

- Rescue tasks only execute if the main task fails
- Multiple rescue tasks can be defined and will execute in order
- Rescue tasks have access to the same variables as the main task
- If a rescue task fails, it can have its own rescue block

### Cleanup with always

The `always` attribute defines tasks that will execute regardless of whether the main task succeeds
or fails. This is useful for cleanup operations that must always run.

```yaml
- name: Task with cleanup
  copy:
    content: "Important data"
    dest: "/tmp/important_file.txt"
  always:
    - name: Log operation
      debug:
        msg: "File operation completed"
    - name: Set permissions
      file:
        path: "/tmp/important_file.txt"
        mode: "0644"
```

Key points about `always`:

- Always tasks execute regardless of main task success or failure
- Multiple always tasks can be defined and will execute in order
- Always tasks run after rescue tasks (if any)
- Always tasks have access to the same variables as the main task

### Combining rescue and always

You can use both `rescue` and `always` attributes on the same task for comprehensive error handling
and cleanup:

```yaml
- name: Complex task with error handling and cleanup
  command:
    cmd: "complex-operation"
  rescue:
    - name: Handle failure
      debug:
        msg: "Operation failed, attempting recovery"
    - name: Recovery action
      command:
        cmd: "recovery-command"
  always:
    - name: Cleanup temporary files
      file:
        path: "/tmp/temp_files"
        state: absent
    - name: Log completion
      debug:
        msg: "Task sequence completed"
```

Execution order:

1. Main task executes
2. If main task fails, rescue tasks execute
3. Always tasks execute (regardless of main task or rescue task outcomes)

### rescue and always with loops

When used with loops, `rescue` and `always` apply to each iteration:

```yaml
- name: Process multiple items
  debug:
    msg: "Processing: {{ item }}"
  loop:
    - item1
    - item2
    - item3
  rescue:
    - name: Handle item failure
      debug:
        msg: "Failed to process: {{ item }}"
  always:
    - name: Log item completion
      debug:
        msg: "Completed processing: {{ item }}"
```
