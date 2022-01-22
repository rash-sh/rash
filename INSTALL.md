# Building Rash

`rash` is a project written in Rust in its entirety and can be built directly with
[cargo](https://doc.rust-lang.org/cargo/) tool.

## Build requirements

The following tools are needed:

- docker
- make
- rustc
- cargo

## Build

You can build `rash` images with the binary inside by simply running the command below.

```bash
make images
```

Developers may often wish to make only one image or just test it in local.
You can do one of both:

```bash
# docker image
DOCKERFILES=Dockerfile make images

# binary
make build
echo rash binary is under ${CARGO_TARGET_DIR}/release/rash
```
