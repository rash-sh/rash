# Vars <!-- omit in toc -->

The `rash` context has variables associated to use as [Tera](https://tera.netlify.app/) templates.
You can use them everywhere except in Yaml keys. `rash` will render them at execution time.

There are two kind of variables:

- [Core](#core)
- [Runtime](#runtime)

## Core

By default, every execution of `rash` exposes two variables to the Context: `{{ rash }}` and `{{ env }}`.

### rash <!-- omit in toc -->

`{{ rash }}` variables are builtin values retrieved from execution context.

{{#include_doc {{#include ../../rash_core/src/vars/builtin.rs:examples}}}}

`src/vars/builtin.rs`:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/vars/builtin.rs:builtins}}
```

### env <!-- omit in toc -->

You can access any environment var as `{{ env.MY_ENV_VAR }}`.

Also, you can use command line arguments to pass environment variables:

```bash
rash -e MY_ENV_VAR=foo example.rh
```

## Runtime

It's possible to set Variables in runtime, too. Check sections [module: set_vars](./set_vars.html) or
[Tasks](./tasks.html) to get additional information.
