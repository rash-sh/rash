---
title: Bultins
weight: 5100
indent: true
---

# Bultins

By default, every execution of `rash` exposes two variables to the Context: `{{ rash }}` and
`{{ env }}`.

## rash

`{{ rash }}` variables are builtin values retrieved from execution context.

{$include_doc  {{#include ../../rash_core/src/vars/builtin.rs:examples}}}

`src/vars/builtin.rs`:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/vars/builtin.rs:builtins}}
```

## env

You can access any environment var as `{{ env.MY_ENV_VAR }}`.

Also, you can use command line arguments to pass environment variables:

```bash
rash -e MY_ENV_VAR=foo example.rh
```
