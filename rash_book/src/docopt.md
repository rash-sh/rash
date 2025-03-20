---
title: Command-line interfaces
weight: 10000
---

# Command-line interfaces <!-- omit in toc -->

`rash` includes a powerful command-line argument parser based on the documentation in your script.
Rather than writing complex argument parsing code, you simply document how your script should be used,
and `rash` handles parsing the arguments according to that specification.

This implementation is inspired by [Docopt](http://docopt.org/), a command-line interface description language.
The core philosophy is: "Program's help message is the source of truth for command-line argument parsing logic."

## Why use document-based argument parsing?

1. **Documentation first**: You write the help text that users will see, not abstract parsing rules
2. **Single source of truth**: No risk of documentation being out of sync with the code
3. **Declarative**: Describe what the interface looks like, not how to parse it
4. **Complete**: Supports complex command-line interfaces with commands, options, and arguments

## Basic example

Here's a simple example of how it works:

```yaml
{{#include ../../examples/copy.rh}}
```

In this example:

1. The usage pattern describes the command-line interface
2. The arguments are automatically parsed and made available as variables
3. No additional parsing code is needed

## Format specification

The docopt format uses a specific syntax to define your interface:

1. Begin with a `#!/usr/bin/env rash` shebang line
2. Include a comment section starting with `#`
3. Provide a `Usage:` section that defines the command-line patterns
4. Optionally include `Options:`, `Arguments:`, or other sections for more details

When users invoke your script, `rash` will:

1. Parse the command-line arguments according to your usage patterns
2. Make the parsed values available as variables in your script
3. Display the help text if requested with `--help` or if invalid arguments are provided

For a complete reference of the syntax, see the [Syntax](syntax.md) section, and for details on
how arguments are parsed and made available to your script, see the [Parser](parser.md) section.
