# syntax=docker/dockerfile:1.7

FROM rust:alpine AS builder
WORKDIR /app

RUN apk add --no-cache build-base musl-dev

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY examples ./examples
COPY containerflare-command ./containerflare-command

RUN rustup update stable && \
    rustup target add x86_64-unknown-linux-musl && \
    cargo build --locked --release \
      --manifest-path examples/cloudrun/Cargo.toml \
      --target x86_64-unknown-linux-musl

FROM alpine:latest
WORKDIR /opt/app

RUN addgroup -S containerflare && adduser -S containerflare -G containerflare

COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/containerflare-cloudrun-example /usr/local/bin/containerflare-cloudrun

EXPOSE 8080
USER containerflare
ENV RUST_LOG=info \
    PORT=8080

CMD ["/usr/local/bin/containerflare-cloudrun"]
