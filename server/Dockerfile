# syntax=docker/dockerfile:1
#
# Public, bank-free omni-me-server image (Phase 2 task 2.1).
#
# Builds ONLY the `omni-me-server` workspace member. The wasm frontend is
# `exclude`d from the workspace and `tauri-app/src-tauri` is never compiled by
# `-p omni-me-server`, so this image carries no Tauri/Android/wasm toolchain.
#
# The private overlay (omni-me-private) reuses this exact pattern for the
# personal image in the deploy-pipeline session — it just swaps the path-deps
# for a pinned git rev and builds its own binary + ships its subprocess helper.

# ---- builder ----
FROM rust:1-bookworm AS builder

# native-tls (server reqwest default-tls + core `auto-import` IMAP) links system
# OpenSSL; surrealdb uses kv-surrealkv (pure embedded), so no clang/rocksdb here.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Release profile (LTO/strip/panic=abort) comes from the root Cargo.toml.
# Cache the cargo registry + target dir across builds; copy the binary OUT of
# the cache mount while it's still mounted so it lands in the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p omni-me-server \
    && cp target/release/omni-me-server /omni-me-server

# ---- runtime ----
FROM debian:bookworm-slim AS runtime

# libssl3 = native-tls runtime; ca-certificates = outbound TLS (Gemini/IMAP/sync);
# curl = HEALTHCHECK probe.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root. Home = /data so the named volume mount inherits `app` ownership.
RUN useradd --system --uid 10001 --create-home --home-dir /data app

COPY --from=builder /omni-me-server /usr/local/bin/omni-me-server

USER app
WORKDIR /data

# DB lands at /data/surreal_data/server.db (relative to WORKDIR); blobs + the
# credentials secret are pointed at fixed paths the compose file mounts.
ENV BLOB_DIR=/data/blobs \
    XDG_CONFIG_HOME=/config

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://localhost:3000/health || exit 1

ENTRYPOINT ["/usr/local/bin/omni-me-server"]
