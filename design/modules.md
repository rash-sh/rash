# modules

Modules priority:

- command
- cp
- replace
- request

## command

Extension:

```yaml
options:
    pid1:
        description:
        - Wrap process and launch as PID 1 to catch process signals.
        type: bool
    service:
        description:
        - Run in second plane in infinite loop.
        - Useful for healthchecks or multiple codependent services.
        type: bool
```
