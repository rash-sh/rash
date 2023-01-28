---
title: Tasks
weight: 4000
---

# Tasks

`tasks` are the main execution unit. They need a module and admit some optional fields described below.

```yaml
{{#include ../../examples/task.rh:3:}}
```

## Keywords

Tasks admit the following optional keys:

| Keyword | Type   | Description |
|---------|--------|-------------|
| become | boolean | run operations with become (does not imply password prompting) |
| become_user | string | run operations as this user (just works with become enabled) |
| check_mode | boolean | Run task in dry-run mode without modifications |
| changed_when | string | Template expression passed directly without `{{ }}`; Overwrite change status |
| ignore_errors | string | Template expression passed directly without `{{ }}`; if true errors are ignored |
| name | string | Task name |
| loop | array | `loop` receives a Template (with `{{ }}`) or a list to iterate over it |
| register | string | Variable name to store module result |
| vars | map | Define variables in task scope. Does not support own reference variables. |
| when | string | Template expression passed directly without {{ }}; if false skip task execution |

### Registering variables

Use the Register field to define the name of the variable in which you wish to save
the module result. Its value will conform to the following structure:

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

You can enable become from multiple places: If you want to activate it for all tasks you can
pass it as an execution arg (`-b/--become`). Or you can enable it per tasks using the `become`
keyword.

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
