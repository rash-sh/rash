# Rash Book

`rash` solves an optimization problem in containers ecosystem. Nowadays, container *entrypoints*
must be written in `bash` or included in the binary (the program itself). This is a trade off
decision between being fast or being reusable, efficient, flexible...

Beside, *entrypoints* share use cases between kind of applications (for example
[databases entrypoints](https://github.com/pando85/entrypoint-examples) are quite similar).
Or between platforms you have to provision your containers with same tools to care about
secrets, configuration management...

`rash` provides:
- **simple syntax** to maintain low complexity.
- static binary to be **container oriented**.
- **declarative** syntax to be idempotent.
- **clear output** to log properly.
- **secure** by design.
- **fast and efficient** (TODO: performance tests versus `bash`).
- **modular** design.
- support [Tera](https://tera.netlify.app/) **templates**.
