ARG BASE_IMAGE=rust:1.58.0
FROM ${BASE_IMAGE} AS builder
LABEL mantainer pando855@gmail.com

WORKDIR /usr/src/rash
COPY . .
RUN cargo install cross \
    && cross build --target=x86_64-unknown-linux-musl --release

FROM scratch
LABEL mantainer pando855@gmail.com

COPY --from=builder /usr/src/rash/target/release/rash /bin/rash

ENTRYPOINT [ "/bin/rash" ]

