---
title: Vars
weight: 6000
---

# Vars <!-- omit in toc -->

The `rash` context has variables associated to use as [Tera](https://tera.netlify.app/) templates.
You can use them everywhere except in Yaml keys. `rash` will render them at execution time.

There are two kinds of variables:

- [Builtins](builtins.md)
- [Runtime](runtime.md)

## debug

To show all variables in current context:

```yaml
- debug:
    var: __tera_context
```
