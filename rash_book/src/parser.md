---
title: Parser
weight: 7200
indent: true
---

# Parser

Elements are parsed using usages and automatically added to your `rash` variables.

Commands are parsed as `false` by default and when are passed they will appear as `true`:

```json
{
    "options": {
        "apply": false,
        "help": false,
        "number": "10",
        "timeout": null,
        "version": false,
       "q": true,
    },
    "port": "443"
}
```

**Note**: `help` is a special case because if help is passed as argument or option, the program
will show all documentation and after that exit 0.

Positional arguments, if exists, they are parsed as arrays:

```json
{
    "argument": "value",
    "repeating-argument": ["value1", "value2"...],
}
```

If they don't appear they will be omitted from vars.
