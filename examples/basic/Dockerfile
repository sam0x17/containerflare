# syntax=docker/dockerfile:1.7

FROM rust:alpine AS builder
WORKDIR /app

# Install build dependencies for musl targets.
RUN apk add --no-cache build-base musl-dev

# Copy manifests first to maximize Docker layer cache usage.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples
COPY containerflare-command ./containerflare-command

# Build the basic example crate (which depends on containerflare via a path dependency) for musl/amd64.
RUN rustup update stable && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo build --locked --release \
      --manifest-path examples/basic/Cargo.toml \
      --target x86_64-unknown-linux-musl

FROM alpine:latest
WORKDIR /opt/app

RUN addgroup -S containerflare && adduser -S containerflare -G containerflare

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/containerflare-basic-example /usr/local/bin/containerflare-basic

EXPOSE 8787
USER containerflare
ENV RUST_LOG=info \
    CF_CONTAINER_ADDR=0.0.0.0

CMD ["/usr/local/bin/containerflare-basic"]
