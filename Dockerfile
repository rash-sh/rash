ARG BASE_IMAGE=ekidd/rust-musl-builder:1.57.0
FROM ${BASE_IMAGE} AS builder
LABEL mantainer pando855@gmail.com

ADD --chown=rust:rust . ./
RUN cargo build --release

FROM scratch
LABEL mantainer pando855@gmail.com

COPY --from=builder \
    /home/rust/src/target/x86_64-unknown-linux-musl/release/rash \
    /bin/
ENTRYPOINT [ "/bin/rash" ]

