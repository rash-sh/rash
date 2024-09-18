---
title: Vars
weight: 6000
---

# Vars <!-- omit in toc -->

The `rash` context has variables associated to use as [MiniJinja](https://docs.rs/minijinja/) templates.
You can use them everywhere except in Yaml keys. `rash` will render them at execution time.

There are two kinds of variables:

- [Builtins](builtins.md)
- [Runtime](runtime.md)

## debug

To show all variables in current context:

```yaml
- debug:
    msg: "{{ debug() }}"
```
