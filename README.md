<img src="https://raw.githubusercontent.com/rash-sh/rash/master/artwork/shelly.svg" width="20%" height="auto" />

# rash

![Build status](https://img.shields.io/github/actions/workflow/status/rash-sh/rash/rust.yml?branch=master)
[![Documentation](https://docs.rs/rash_core/badge.svg)](https://docs.rs/rash_core)
[![crates.io](https://img.shields.io/crates/v/rash_core)](https://crates.io/crates/rash_core)
[![concept-map](https://img.shields.io/badge/design-concept--map-blue)](https://mind42.com/mindmap/f299679e-8dc5-48d8-b0f0-4d65235cdf56)
![Rash license](https://img.shields.io/github/license/rash-sh/rash)
[![Rash Aur package](https://img.shields.io/aur/version/rash)](https://aur.archlinux.org/packages/rash)

Rash is a **declarative local automation tool** for the work that too often ends up in large shell
scripts: validating inputs, rendering configuration, preparing a container, bootstrapping an
environment, or keeping a workstation setup reproducible.

It combines YAML tasks with [MiniJinja](https://github.com/mitsuhiko/minijinja) templating and ships
as a single Rust binary with no runtime dependencies. Its syntax is intentionally familiar to
[Ansible](https://www.ansible.com/) users, but Rash is designed to execute locally as a scripting
tool rather than as an inventory-driven orchestration system.

## Where Rash Fits

Rash is designed for automation that runs **here**, in a container, VM, CI job, developer machine,
or other local execution environment. Typical use cases include:

- **Container entrypoints and init containers**: validate environment, render runtime configuration,
  prepare local state, and hand off execution to the application
- **Bootstrap and setup scripts**: make repeatable machine, development environment, or CI setup
  easier to read and maintain than equivalent shell
- **Workstation and dotfile automation**: express local desired state with templates, conditions,
  loops, privilege escalation, and reusable modules
- **Operational glue**: replace increasingly complex shell scripts with structured tasks and
  explicit change/error handling

Rash borrows syntax and concepts from Ansible because they work well for declarative tasks. That
familiarity is an ergonomic choice, not a compatibility target: Rash does not aim to implement
Ansible inventories, remote fleet orchestration, or module parity.

## Why Rash?

- **Local-First**: Designed for scripts that execute on the machine or container where they run
- **Declarative**: Describe the state or outcome you want instead of encoding every shell step
- **Self-Contained**: A single Rust binary with no runtime dependencies
- **Container-Friendly**: Well suited to minimal images, entrypoints, and init containers
- **Template-Powered**: Uses [MiniJinja](https://github.com/mitsuhiko/minijinja) for powerful
  templating capabilities
- **Script-Friendly Interfaces**: Built-in [docopt](http://docopt.org) parsing for clean command-line
  interfaces
- **Familiar, Not Coupled**: Ansible-inspired YAML without requiring Ansible's runtime or execution
  model
- **Useful Local Primitives**: Modules provide structured building blocks for common automation
  tasks without forcing everything through shell commands

## Example: Imperative vs Declarative

### Bash (Imperative)

```bash
#!/bin/bash
set -e

# Validate required environment variables
REQUIRED_PARAMS="
DATABASE_URL
DATABASE_USER
DATABASE_PASSWORD
LOG_LEVEL
"

for required in $REQUIRED_PARAMS ; do
  [[ -z "${!required}" ]] && echo "$required IS NOT DEFINED" && exit 1
done

# Configure the application
echo "[$0] Configuring application..."
CONFIG_FILE="/app/config.json"
cat > $CONFIG_FILE << EOF
{
  "database": {
    "url": "$DATABASE_URL",
    "user": "$DATABASE_USER",
    "password": "$DATABASE_PASSWORD"
  },
  "server": {
    "port": "${SERVER_PORT:-8080}",
    "log_level": "$LOG_LEVEL"
  }
}
EOF

# Set correct permissions
chmod 0600 $CONFIG_FILE

echo "[$0] Starting application..."
exec "$@"
```

### Rash (Declarative)

```yaml
#!/usr/bin/env rash

- name: Verify input parameters
  assert:
    that:
      - env.DATABASE_URL is defined
      - env.DATABASE_USER is defined
      - env.DATABASE_PASSWORD is defined
      - env.LOG_LEVEL is defined

- name: Configure application
  template:
    src: config.j2
    dest: /app/config.json
    mode: "0600"
  vars:
    server_port: "{{ env.SERVER_PORT | default('8080') }}"

- name: Launch command
  command:
    cmd: "{{ rash.argv }}"
    transfer_pid: yes
```

## Installation

### Binary (Linux/macOS)

```bash
curl -s https://api.github.com/repos/rash-sh/rash/releases/latest \
    | grep browser_download_url \
    | grep -v sha256 \
    | grep $(uname -m) \
    | grep $(uname | tr '[:upper:]' '[:lower:]') \
    | grep -v musl \
    | cut -d '"' -f 4 \
    | xargs curl -s -L \
    | sudo tar xvz -C /usr/local/bin
```

### Arch Linux (AUR)

```bash
yay -S rash
```

### Cargo

```bash
cargo install rash_core
```

### Docker

```bash
docker run --rm -v /usr/local/bin/:/output --entrypoint /bin/cp ghcr.io/rash-sh/rash:latest /bin/rash /output/
```

## Key Features

### Built-in Command-Line Interface Parser

```yaml
#!/usr/bin/env -S rash --
#
# Copy files from source to dest dir
#
# Usage:
#   copy.rh [options] <source>... <dest>
#   copy.rh
#
# Options:
#   -h --help    show this help message and exit
#   --mode MODE  dest file permissions [default: 0644]

- copy:
    src: "{{ item }}"
    dest: "{{ dest }}/{{ item | split('/') | last }}"
    mode: "{{ options.mode }}"
  loop: "{{ source | default([]) }}"
```

### Container Entrypoints

Perfect for creating maintainable container entrypoints that validate the environment, render
runtime configuration, perform local initialization, and hand off execution to the application:

```dockerfile
FROM alpine:3.16

# Install rash binary
ADD https://github.com/rash-sh/rash/releases/download/v0.6.0/rash-x86_64-unknown-linux-musl.tar.gz /tmp/
RUN tar xvzf /tmp/rash-x86_64-unknown-linux-musl.tar.gz -C /usr/local/bin && \
    rm /tmp/rash-x86_64-unknown-linux-musl.tar.gz

# Add entrypoint script
COPY entrypoint.rh /entrypoint.rh
RUN chmod +x /entrypoint.rh

ENTRYPOINT ["/entrypoint.rh"]
```

### Templating System

Access environment variables and use powerful filters:

```yaml
- name: Configure application
  template:
    src: config.j2
    dest: /etc/app/config.json
  vars:
    app_port: "{{ env.PORT | default('8080') }}"
    app_log_level: "{{ env.LOG_LEVEL | default('info') }}"
    database_url: "{{ env.DATABASE_URL }}"
```

### Privilege Escalation

Run commands as different users with the built-in `become` functionality:

```yaml
- name: Configure system DNS
  become: true
  copy:
    dest: /etc/resolv.conf
    content: |
      nameserver 208.67.222.222
      nameserver 208.67.220.220
```

## Documentation

For comprehensive documentation, visit:
[https://rash-sh.github.io/docs/rash/master/](https://rash-sh.github.io/docs/rash/master/)

## Community

- GitHub: [https://github.com/rash-sh/rash](https://github.com/rash-sh/rash)
- Report Issues: [https://github.com/rash-sh/rash/issues](https://github.com/rash-sh/rash/issues)

## License

Rash is distributed under the
[GPL-3.0 License](https://github.com/rash-sh/rash/blob/master/LICENSE).
