# ADR-001: Add `become_method` Support for Sudo-Based Privilege Escalation

## Status

Implemented

## Context

Rash currently supports privilege escalation via `become: true`, which uses direct `setuid`/`setgid` syscalls. This approach requires:

1. Running rash as root, OR
2. Setting `CAP_SETUID` and `CAP_SETGID` capabilities on the rash binary

Users with `NOPASSWD` sudoers configuration expect `become: true` to work, but it fails with:

```
[ERROR] gid cannot be changed to 0
[ERROR] child failed with exit_code 1
```

This is because the syscall-based approach requires Linux capabilities, not just sudo permissions.

### Ansible's Approach

Ansible provides `become_method` with multiple options (`sudo`, `su`, `pbrun`, `pfexec`, `doas`, etc.). When using `sudo`, Ansible wraps module execution with `sudo -u <user>`. This works for any user with sudo privileges, without requiring special capabilities.

### Key Differences

| Aspect | Ansible | Rash (current) |
|--------|---------|----------------|
| Module type | Python scripts (copied to remote) | Compiled Rust (in binary) |
| Privilege escalation | Wrap command with sudo | Direct setuid/setgid syscalls |
| Requirement | sudo privileges | CAP_SETUID/CAP_SETGID or root |

## Decision

We will add `become_method` support with two options:

1. **`syscall`** (default): Current behavior using `setuid`/`setgid` syscalls
2. **`sudo`**: Execute tasks via `sudo -u <user> rash`

### Implementation Details

#### New Parameters

| Parameter | Type | Default | CLI Flag | Description |
|-----------|------|---------|----------|-------------|
| `become_method` | `syscall` \| `sudo` | `syscall` | `--become-method` | Privilege escalation method |
| `become_exe` | string | `sudo` | `--become-exe` | Path to sudo executable |
| `become_password` | string | - | - | Password for sudo (can use vault) |
| - | - | - | `--ask-become-pass` / `-K` | Prompt for sudo password |

#### Task YAML Example

```yaml
- name: Install package
  become: true
  become_method: sudo
  become_exe: /usr/bin/sudo
  become_user: root
  command: apt install -y nginx

# With password (from vault)
- name: Install package with password
  become: true
  become_method: sudo
  become_password: "{{ vault_sudo_password }}"
  command: apt install -y nginx
```

#### CLI Usage

```bash
# Prompt for password
rash --become --become-method sudo -K script.rh

# Password via task variable
rash --become --become-method sudo script.rh
```

#### Architecture

##### For `command`/`shell` modules (simple case)

No fork needed - wrap command directly:

```
sudo -H -u <user> -- <cmd> <args>
```

##### For other modules (file, copy, template, etc.)

Re-invoke rash via sudo with serialized task context:

```
Parent Process:
1. Serialize task + context to temp YAML file
2. fork() → exec("sudo", "-H", "-u", user, "rash", "--output", "json", tempfile)
3. Wait for completion
4. Parse JSON result from stdout
5. Delete temp file
```

##### Password handling

When `become_password` is provided or `--ask-become-pass` is used:

```
sudo -H -S -u <user> -- <cmd>
# Password is passed via stdin
```

- `-S` flag tells sudo to read password from stdin
- Password is written to stdin of the sudo process
- Can be set via task YAML or prompted interactively

#### Task Serialization Format

The temp file passed to the child rash process:

```yaml
---
_rash_internal:
  original_path: /original/script.rh
  args: ["arg1", "arg2"]
_vars:
  accumulated_var: value
  register_from_prev: {...}

tasks:
  - name: My task
    command: apt install nginx
```

This allows the child process to:
- Preserve accumulated variable state
- Maintain correct `rash.path` builtin
- Execute the task with proper context

#### New CLI Output Format

Add `--output json` to support machine-readable output:

```bash
rash --output json script.rh
```

Output:
```json
{
  "tasks": [
    {
      "name": "Install package",
      "changed": true,
      "failed": false,
      "output": "...",
      "vars": {"register_name": {...}}
    }
  ],
  "failed": false
}
```

### Files to Modify

| File | Changes |
|------|---------|
| `rash_core/src/context.rs` | Add `BecomeMethod` enum, `become_method`, `become_exe` to `GlobalParams` |
| `rash_core/src/logger.rs` | Add `Json` variant to `Output` enum |
| `rash_core/src/task/mod.rs` | Add `become_method`, `become_exe` fields; implement sudo execution paths |
| `rash_core/src/task/valid.rs` | Parse `become_method` and `become_exe` from task YAML |
| `rash_core/src/bin/rash.rs` | Add CLI args: `--become-method`, `--become-exe`, `--output json` |
| `rash_core/src/lib.rs` | Export function to detect and handle `_rash_internal` task files |

### Known Limitations for `become_method: sudo`

| Feature | Support | Notes |
|---------|---------|-------|
| Handler notifications | Not supported | Handlers defined in original script not available |
| Rescue/Always blocks | Not supported | Block structure spans multiple tasks |
| Variable propagation | Supported | Via `_vars` in serialized task |
| Loops | Supported | Task includes loop definition |
| Builtins (`rash.path`) | Supported | Via `_rash_internal.original_path` |

## Consequences

### Positive

- Users with sudo access (but not capabilities) can now use `become: true`
- Backward compatible - default `syscall` method preserves current behavior
- `--output json` is useful for automation and CI/CD beyond become use case
- Aligns with Ansible's familiar `become_method` pattern

### Negative

- Adds complexity to task execution path
- Some features (handlers, rescue/always) not supported with sudo method
- Temp file creation for task serialization (cleaned up after execution)
- Slight performance overhead for sudo method (process spawning + temp file I/O)

### Neutral

- Two code paths to maintain for privilege escalation
- Security audit needed for temp file handling

## Alternatives Considered

### 1. Require CAP_SETUID/CAP_SETGID only

Rejected: Requires system administrator intervention and doesn't match user expectations from Ansible.

### 2. Always use sudo for become

Rejected: Breaking change; syscall approach is more efficient and works in container environments without sudo.

### 3. Special `--task-exec` hidden CLI mode

Rejected: More complex than reusing full binary with JSON output; harder to debug.

### 4. Support only command/shell modules with sudo

Rejected: Inconsistent user experience; users expect all modules to work with become.

## References

- [Ansible Become Documentation](https://docs.ansible.com/ansible/latest/playbook_guide/playbooks_privilege_escalation.html)
- [Linux Capabilities](https://man7.org/linux/man-pages/man7/capabilities.7.html)
