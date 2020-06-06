# Rash Book

`rash` solves an optimization problem in the containers ecosystem.

Nowadays, container *entrypoints* must be written in `bash` or included in the binary
(the program itself). This is a trade-off decision between being fast or being reusable,
efficient, flexible...

Besides, *entrypoints* share use cases between different kinds of applications, e.g.
[databases entrypoints](https://github.com/pando85/entrypoint-examples) are quite similar.
Likewise, you might need to provision your containers between different platforms with the same
tools, paying attention to secrets, configuration management...

`rash` provides:

- A **simple syntax** to maintain low complexity.
- One static binary to be **container oriented**.
- A **declarative** syntax to be idempotent.
- **Clear output** to log properly.
- **Security** by design.
- **Speed and efficiency** (TODO: performance tests versus `bash`).
- **Modular** design.
- Support of [Tera](https://tera.netlify.app/) **templates**.
