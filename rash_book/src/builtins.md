---
title: Bultins
weight: 6100
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

### check_mode

The `rash.check_mode` variable is a boolean that indicates whether rash is running in check mode
(dry-run mode). This is useful for conditionally executing tasks or validating behavior when
`--check` flag is passed.

Example:
```yaml
- name: Skip in check mode
  debug:
    msg: "Running in check mode, skipping actual changes"
  when: rash.check_mode
```

## env

You can access any environment var as `{{ env.MY_ENV_VAR }}`.

Also, you can use command line arguments to pass environment variables:

```bash
rash -e MY_ENV_VAR=foo example.rh
```
