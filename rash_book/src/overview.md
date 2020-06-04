# Overview

Rash is a declarative shell scripting tool oriented to build containers entrypoints.
`rash` syntax is inspired in [Ansible](https://www.ansible.com/).


## Quickstart

To start using `rash` you just need a container with entrypoint.
For installation, add to Dockerfile `rash` binary and enjoy it:

```dockerfile

FROM pando85/rash AS rash

FROM base_image

COPY --from=rash /bin/rash /bin

RUN my app things...

COPY entrypoint.rh /
ENTRYPOINT ["/entrypoint.rh"]

```

Also, create your first `entrypoint.rh`:

```yaml
#!/bin/rash

- command: 'myapp -u {{ rash.user }} -h {{ env.HOSTNAME }} {{ rash.args | join(sep=" ") }}'
  # transforms process in pid 1 (similar to `exec` in bash)
  transfer_pid_1: true
```
