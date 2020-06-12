---
title: Tasks
weight: 3000
---

# Tasks

`tasks` are the main execution unit. They need a module and admit some optional fields described below.

```yaml
{{#include ../../examples/task.rh:3:}}
```

## Fields

Tasks admit the following keys:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/task/mod.rs:task}}
```

### Register structure

Use the Register field to define the name of the variable in which you wish to save
the module result. Its value will conform to the following structure:

```rust,no_run,noplaypen
{{#include ../../rash_core/src/modules/mod.rs:module_result}}
```

For example:

```yaml
{{#include ../../examples/register.rh:3:}}
```
