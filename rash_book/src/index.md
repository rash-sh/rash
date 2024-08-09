---
title: Rash Book
weight: 000
---

# Rash Book

`rash` solves an optimization problem in the containers ecosystem.

Nowadays, you generally need to write container _entrypoints_ in `bash` or include them in the
binary, i.e. the program itself. This is a trade-off decision between being fast or being reusable,
efficient, flexible...

Besides, _entrypoints_ share use cases between different kinds of applications, e.g.
[databases entrypoints](https://github.com/pando85/entrypoint-examples) are quite similar.
Likewise, you might need to provision your containers between different platforms with the same
tools, paying attention to secrets, configuration management...

`rash` provides:

- A **simple syntax** to maintain low complexity.
- One static binary to be **container oriented**.
- A **declarative** syntax to be idempotent.
- **Clear output** to log properly.
- **Security** by design.
- **Speed and efficiency**.
- **Modular** design.
- Support of [MiniJinja](https://docs.rs/minijinja/latest/minijinja/syntax/index.html) **templates**.
