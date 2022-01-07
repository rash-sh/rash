---
title: Modules
weight: 5000
---

# Modules

Modules are operations executed by tasks. They require parameters for its execution.

E.g.:

```yaml
{{#include ../../examples/builtins.rh:3:}}
```

## Modules index

{$include_module_index}

## Omitting parameters

By default all parameters defined in yaml are passed to the module. However, you can
omit them programmatically.

E.g.:

```
"{{ env.MY_PASSWORD_MODE | default(value=omit()) }}"
```

Furthermore, if you are chaining additional filters after the `default(value=omit())`, you should instead
do something like this: `"{{ foo | default(value=None) | some_filter or omit() }}"`.
In this example, the default `None` value will cause the later filters to fail, which will trigger
the `or omit` portion of the logic. Using `omit` in this manner is very specific to the later
filters you are chaining though, so be prepared for some trial and error if you do this.
