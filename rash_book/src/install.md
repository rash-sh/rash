# Building Rash

`rash` is a Rust entiretly project and can be built directly with
[cargo](https://doc.rust-lang.org/cargo/) tool.

## Build requirements

The following tools are need:
- docker
- make
- rustc
- cargo

## Build

You can build `rash` images with the binary inside by simply running the command below.

```bash
make build-images
```

Developers may often wish to make only one image or just test it in local.
You can do one of both:

```bash
# docker image
DOCKERFILES=Dockerfile make build-images

# binary
make build
echo rash binary is under ${CARGO_TARGET_DIR}/release/rash
```
