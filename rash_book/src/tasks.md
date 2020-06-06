# Tasks

`tasks` are the main execution unit. They need a module and admit some optional fields described below.

```yaml
{{#include ../../examples/copy.rh:3:}}
```

## Fields

Tasks admit the following keys:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/task/mod.rs:task}}
```

### Register structure

Register saves in a variable a modules result structure like this one:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/modules/mod.rs:module_result}}
```

For example:

```yaml
{{#include ../../examples/register.rh:3:}}
```
