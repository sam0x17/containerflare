# Containerflare Architecture Plan

## Goals
- Provide a first-party Rust crate for building Cloudflare Containers Workers with an
  Axum-first developer experience while also supporting Google Cloud Run without extra glue.
- Hide container-runtime specific details (init, I/O plumbing, request context propagation,
  command protocol).
- Offer ergonomic abstractions for making host-environment calls (KV, D1, Queues, environment
  variables, secrets) via a thin command channel.
- Deliver a lightweight runtime that targets Alpine/musl containers
  (`x86_64-unknown-linux-musl`) per Cloudflare's linux/amd64 requirement for Containers and the
  Cloud Run baseline, while keeping the API surface flexible for future adapters.
- Keep the API surface intentionally small at 0.1.x while leaving room for future adapters
  (tower layers, typed commands, streaming bodies).

## Non-goals (for now)
- Full parity with the entire Cloudflare JS runtime surface area.
- Manage deployment CLIs (users still run `wrangler deploy`).
- Provide auto-magical migrations; we only expose helpers to trigger host commands.

## High-level Architecture
```
====================       =================================
| Axum Router/App  | <---> | Containerflare Runtime Adapter |
====================       =================================
                                   |          ^
                                   v          |
                           ============================
                           | Worker Host Command Bus |
                           ============================
```
1. `ContainerRuntime` bootstraps the process: reads config/environment, wires stdin/stdout to
   host IPC, spawns Axum server inside the container, and translates between Cloudflare/Cloud Run
   host requests and Axum `Request`s (binding to `CF_CONTAINER_*` or `PORT` automatically).
2. `CommandClient` (in the standalone `containerflare-command` crate) pushes structured JSON commands over the same IPC channel (or TCP
   unix-socket when available) and awaits responses. On platforms without a host command bus (Cloud
   Run) the client reports `CommandError::Unavailable` immediately.
3. Axum handlers access host capabilities via `ContainerContext`, injected as an
   extension/state, keeping handler ergonomics idiomatic.

## Modules & Responsibilities
- `containerflare::runtime`
  - Provides `Runtime::new(Config) -> Runtime` and `Runtime::serve(router)`.
  - Handles async executor setup (tokio) and Axum server binding (binds to `PORT` when set,
    otherwise `CF_CONTAINER_PORT`/`0.0.0.0:8787` for the Cloudflare sidecar).
  - Manages graceful shutdown (SIGTERM/SIGINT) triggered by host container.
- `containerflare::config`
  - Parses config from env (e.g., `CF_CONTAINER_PORT`, `CF_CMD_SOCKET`, `PORT`,
    `K_SERVICE`, etc.).
  - Allows override via builder for unit tests.
- `containerflare::platform`
  - Detects whether the process is running under Cloudflare Containers, Google Cloud Run, or a
    generic environment.
  - Carries structured metadata (worker name, service/revision/configuration/project/region) that
    gets injected into every request via Axum extensions.
- `containerflare::context`
  - Defines `ContainerContext` struct containing request metadata + `CommandClient` handle.
    Metadata is populated from the Worker-supplied `x-containerflare-metadata` header (request
    id, colo/region/country, client IP, etc.) or from Cloud Run headers/environment with HTTP
    fallbacks for local testing.
  - Implements `FromRequestParts` for easy injection into Axum handlers.
- `containerflare-command` (workspace crate re-exported by `containerflare`)
  - Owns the low-level IPC transport (stdin/stdout framing while sockets are not GA yet).
  - Provides strongly-typed requests/responses (start with generic
    `CommandRequest`/`CommandResponse`).
  - Exposes async helper methods (`fetch_asset`, `kv_get`, `d1_query`, `queue_send`).
- `containerflare::error`
  - Central error type (enum) convertible to Axum `Response` and `anyhow` compatible.

## Request Handling Flow
1. Cloudflare worker container forwards an HTTP request to the embedded Axum server (loopback
   HTTP or `hyper::server::conn::http1::Builder` via raw streams; MVP uses `TcpListener`). On Cloud
   Run the platform forwards traffic from the global load balancer to the container port.
2. `ContainerRuntime` accepts the request, attaches metadata (worker/colo, account, request id,
   Cloud Run service/revision/project, trace context, etc.) gleaned from headers/environment.
3. Handler receives `ContainerContext` extension to talk back to host (issue commands, access
   secrets, mutate storage) and can branch on `ContainerContext::platform()` when necessary.
4. Responses travel back through Axum/Hyper, and the worker container proxies them to the edge
   client.

## Host Command Channel
All IPC transport code now lives in the standalone `containerflare-command` crate (still
re-exported by `containerflare` for convenience).
- Transport: JSON lines over stdin/stdout for MVP (implemented). Local testing can swap to TCP
  or Unix sockets by setting `CF_CMD_ENDPOINT`.
- `CommandClient` serializes commands sequentially with flush/timeout guarantees and surfaces
  structured errors; follow-up work will add true multiplexing with per-command IDs once
  Cloudflare documents the protocol.
- Platforms without a host bus (e.g., Cloud Run) configure the runtime with a disabled command
  endpoint so `CommandClient` returns `CommandError::Unavailable` immediately while keeping the API
  ergonomics consistent.
- Future extension: feature-flagged advanced transports (shared memory, Unix sockets on Windows
  Subsystem for Linux, etc.) for faster local dev.

## Deployment & Runtime Concerns
- Binary targets must be statically linked with musl (`x86_64-unknown-linux-musl`), matching
  Cloudflare's "containers should be built for the `linux/amd64` architecture" guidance
  (`cloudflare-docs/src/content/docs/containers/platform-details/architecture.mdx:79`); crate
  exposes `bin` example as reference.
- Provide `containerflare::main(router)` helper macro to hide tokio boilerplate.
- Provide `examples/basic` showing builder usage, Cloudflare deployment (wrangler), and Cloud Run
  deployment (single Dockerfile + gcloud instructions) from the same codebase via local scripts
  inside the example directory.

## Iteration Plan
1. Implement config, runtime, and context scaffolding (MVP; ensures requests can reach Axum
   handlers).
2. Add command channel abstraction with no-op stub for now (structure in place, not plumbed to
   real host yet).
3. Publish example demonstrating handler + command call; include integration test using hyper
   to ensure request path works locally.
4. Expand command coverage + typed helpers before 0.2.0.

## Immediate TODOs
- Provide a `containerflare::main` (or attribute macro) that wraps `tokio::main` and
  `RuntimeConfig::from_env`, so users do not need to call `run` manually.
- Flesh out the command protocol contract once Cloudflare publishes it: add request IDs +
  concurrent in-flight handling, plus retries/backoff for transient pipe failures.
- Enhance `examples/basic` (or add a second example) to demonstrate issuing a host command via
  `ContainerContext::invoke` with a mocked transport.
- Add tracing subscriber defaults suitable for Alpine (e.g., emit JSON logs gated by
  `RUST_LOG`).
- Extend the test suite with coverage for `ContainerContext::from_request_parts` and
  integration tests that exercise the Axum server end-to-end via `hyper`.
