# Getting started

Simple YAML declarative shell scripting language based in modules and templates.
`rash` syntax is inspired by [Ansible](https://www.ansible.com/).

## Quickstart

To start using `rash` you just need a container with entrypoint.
For install, add `rash` binary to your Dockerfile:

```dockerfile
FROM rustagainshell/rash AS rash

FROM base_image

COPY --from=rash /bin/rash /bin

RUN my app things...

COPY entrypoint.rh /
ENTRYPOINT ["/entrypoint.rh"]
```

Also, create your first `entrypoint.rh`:

```yaml
#!/bin/rash

- command: myapp -u "{{ rash.user }}" -h "{{ env.HOSTNAME }}"
  # transforms process in pid 1 (similar to `exec` in bash)
  transfer_pid_1: true
```

## Who is using `rash`

- A production ready [php-fpm](https://github.com/dcarrillo/docker-phpfpm) docker image
