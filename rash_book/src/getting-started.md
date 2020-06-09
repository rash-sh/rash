# Getting started

Simple YAML declarative shell scripting language based on modules and templates.
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

- command: myapp -u "{{ rash.user.uid }}" -h "{{ env.HOSTNAME }}"
  # transforms process in pid 1 (similar to `exec` in bash)
  transfer_pid_1: true
```

## Syntax

YAML syntax based on [modules](module_index.md).

Besides, `rash` includes [Tera](https://tera.netlify.app/docs/) templates which you can use
anywhere. You can use all its functions and combine them as you want.

`rash` implements custom [builtins](vars.md), too. For example, `{{ rash.path }}` or
`{{ env.MY_ENV_VAR }}`.
