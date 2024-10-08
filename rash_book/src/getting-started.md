---
title: Getting started
weight: 2000
---

# Getting started

Simple YAML declarative shell scripting language based on modules and templates.
`rash` syntax is inspired by [Ansible](https://www.ansible.com/).

## Quickstart

To start using `rash` you just need a container with entrypoint.
For install, add `rash` binary to your Dockerfile:

```dockerfile
{{#include ../../examples/envar-api-gateway/Dockerfile}}
```

Also, you must create your first `entrypoint.rh`:

```yaml
{{#include ../../examples/envar-api-gateway/entrypoint.rh}}
```

Or instead, you could want to use `rash` for local scripting. In that case you can follow
our [installation guide](installation.md).

## Syntax

YAML syntax based on [modules](module_index.md).

Besides, `rash` includes [MiniJinja](https://docs.rs/minijinja/) templates which you can use
anywhere. You can use all its functions and combine them as you want.

`rash` implements custom [builtins](vars.md), too. For example, `{{ rash.path }}` or
`{{ env.MY_ENV_VAR }}`.
