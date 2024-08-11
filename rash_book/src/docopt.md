---
title: Command-line interfaces
weight: 8000
---

# Command-line interfaces <!-- omit in toc -->

`rash` has an integrated command-line parser based in the documentation of your script.

This is an ad-hoc implementation based in [Docopt](http://docopt.org/). The main idea
behind is to write the documentation and `rash` automatically parses arguments based on it.

E.g.:

```yaml
{{#include ../../examples/copy.rh}}
```
