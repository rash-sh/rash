# Tasks

`tasks` are main execution unit. They need a module and admits some optional fields described below.

```yaml
{{#include ../../examples/task.rh:3:}}
```

## Fields

Tasks admits the following keys:

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
