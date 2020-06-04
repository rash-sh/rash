# Vars <!-- omit in toc -->

`rash` Context has variables associated to use as [Tera](https://tera.netlify.app/) templates.
You can use them everywhere except in Yaml keys and they are going to be rendered at execution time.

There are two kind of variables:
- [Core](#core)
- [Runtime](#runtime)

## Core

By default, every execution of rash expose `{{ rash }}` and `{{ env }}` variables to Context.

### rash <!-- omit in toc -->

`{{ rash }}` variables are builtin values gotten from execution context.

{{#include_doc {{#include ../../rash_core/src/vars/builtin.rs:examples}}}}

`src/vars/builtin.rs`:
```rust,no_run,noplaypen
{{#include ../../rash_core/src/vars/builtin.rs:builtins}}
```


### env <!-- omit in toc -->

Any environment var could be accessed as `{{ env.MY_ENV_VAR }}`.

Also, you can use command line to pass environment variables:
```bash
rash -e MY_ENV_VAR=foo example.rh
```

## Runtime

In runtime, Variables could be added. You can check [module: set_vars](./set_vars.html) or
[Tasks](./tasks.html) fields to get more information.
