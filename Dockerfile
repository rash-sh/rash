ARG BASE_IMAGE=rust:1.78.0
FROM ${BASE_IMAGE} AS builder
LABEL mantainer pando855@gmail.com

WORKDIR /usr/src/rash
COPY . .
RUN cargo build --locked --release --bin rash

FROM debian:bookworm-20230904-slim
LABEL mantainer pando855@gmail.com

COPY --from=builder /usr/src/rash/target/release/rash /bin/rash

ENTRYPOINT [ "/bin/rash" ]
