# syntax=docker/dockerfile:1.7

FROM rust:alpine AS builder
WORKDIR /app

# Install build dependencies for musl targets.
RUN apk add --no-cache build-base musl-dev && \
    rustup target add x86_64-unknown-linux-musl

# Copy manifests first to maximize Docker layer cache usage.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples

# Build the basic example binary for the Cloudflare-required musl/amd64 target.
RUN cargo build --locked --release --target x86_64-unknown-linux-musl --example basic

FROM alpine:latest
WORKDIR /opt/app

RUN addgroup -S containerflare && adduser -S containerflare -G containerflare

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/examples/basic /usr/local/bin/containerflare-basic

EXPOSE 8787
USER containerflare
ENV RUST_LOG=info

CMD ["/usr/local/bin/containerflare-basic"]
