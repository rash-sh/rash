---
title: Filters
weight: 9000
---

# Filters and tests

Rash expressions are evaluated by MiniJinja with strict undefined-variable handling. Rash enables
MiniJinja's `builtins`, `unicode`, `json`, and `urlencode` features, so the standard built-in filters
from that environment can be used in task parameters, `when`, `changed_when`, `failed_when`, and
other rendered expressions.

Rash does not maintain a separate Ansible-compatible filter namespace. If an Ansible filter is not a
MiniJinja built-in, do not assume it exists in Rash.

## Common filters

The following filters are particularly useful in Rash scripts:

| Filter | Example | Purpose |
| --- | --- | --- |
| `default` / `d` | `{{ value | default('fallback') }}` | Supply a value when an expression is undefined. |
| `trim` | `{{ command.stdout | trim }}` | Remove leading/trailing whitespace. |
| `lower` / `upper` | `{{ name | lower }}` | Change text case. |
| `replace` | `{{ path | replace(' ', '_') }}` | Replace text. |
| `length` / `count` | `{{ items | length }}` | Return collection/string length. |
| `join` | `{{ items | join(',') }}` | Join an iterable into text. |
| `split` | `{{ value | split(':') }}` | Split text into values. |
| `int` | `{{ value | int }}` | Convert a value to an integer. |
| `float` | `{{ value | float }}` | Convert a value to a floating-point number. |
| `round` | `{{ value | round }}` | Round a number. |
| `abs` | `{{ value | abs }}` | Absolute value. |
| `reverse` | `{{ items | reverse }}` | Reverse an iterable or string. |
| `items` | `{{ mapping | items }}` | Iterate over mapping key/value pairs. |
| `dictsort` | `{{ mapping | dictsort }}` | Sort mapping entries. |
| `map` | `{{ users | map(attribute='name') | join(', ') }}` | Transform an iterable. |
| `tojson` | `{{ data | tojson }}` | Serialize a value as JSON. |
| `urlencode` | `{{ query | urlencode }}` | Percent-encode a string or query mapping. |

This is not intended to duplicate MiniJinja's complete filter reference; it documents the surface
that Rash exposes and the filters most relevant to local automation.

## Strict undefined variables and `omit`

Undefined variables are errors by default:

```yaml
- debug:
    msg: "{{ missing_variable }}"  # error
```

Use `default` when a fallback value is appropriate:

```yaml
- debug:
    msg: "{{ optional_name | default('anonymous') }}"
```

Rash also exposes the special global value `omit`. When a rendered module parameter evaluates to
`omit`, Rash removes that parameter instead of passing it to the module:

```yaml
- some_module:
    optional_parameter: "{{ optional_value | default(omit) }}"
```

This is useful when the difference between "parameter absent" and "parameter present with an empty
value" matters.

## Tests

MiniJinja tests use the `is` syntax and are especially useful in `when` expressions:

```yaml
- debug:
    msg: "value is available"
  when: value is defined
```

Rash adds task-result tests on top of MiniJinja's standard tests:

| Test | Meaning |
| --- | --- |
| `result is failed` | The registered result contains `failed: true`. |
| `result is succeeded` | The registered result contains `failed: false`. |
| `result is success` | Alias for `succeeded`. |
| `result is changed` | The registered result contains `changed: true`. |

Example:

```yaml
- command: test -f /optional/file
  register: probe
  ignore_errors: true
  changed_when: false

- debug:
    msg: "optional file is absent"
  when: probe is failed
```

These tests inspect the structured result; they do not change task failure semantics. Use
`failed_when` when you want to redefine whether a task itself is considered failed.
