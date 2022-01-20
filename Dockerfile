FROM scratch
LABEL mantainer pando855@gmail.com

ARG CARGO_TARGET_DIR=target
COPY ${CARGO_TARGET_DIR}-musl/x86_64-unknown-linux-musl/release/rash /bin/rash

ENTRYPOINT [ "/bin/rash" ]
