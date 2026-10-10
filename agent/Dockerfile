# syntax=docker/dockerfile:1
#
# Public, bank-free omni-me-agent image — the resident assistant.
#
# Its own image rather than a second binary in server/Dockerfile, which was the
# alternative. The agent drags in the embedding stack (fastembed, ort, tokenizers,
# a statically linked onnxruntime) that the server has no use for, it wants a
# volume of its own rather than a share of the server's, and it needs no port at
# all. Keeping them apart also means the private overlay's server image stays
# what it is: the agent is bank-free, so it runs from this public image even on
# the box, beside the private server, and nothing has to build across two
# workspaces to produce it.
#
# They are not version-locked, and that is deliberate: the agent reaches the
# server over the same sync API a phone uses, so it is a peer rather than a
# sidecar.

# ---- builder ----
# Pinned, never the rolling `rust:1`, and moved in step with CI's toolchain pin.
# Trixie, not bookworm: the prebuilt onnxruntime `ort` links needs glibc 2.38+ and
# a GCC 13+ libstdc++ (`__isoc23_strtoll`), and bookworm's fail at link time.
FROM rust:1.99.0-trixie AS builder

# No apt build deps, same as server/Dockerfile: the workspace is rustls
# throughout and surrealdb is kv-surrealkv. `ort` ships a prebuilt onnxruntime
# and links it statically, so nothing here needs clang or cmake either —
# re-derive with `ldd` on the binary before adding any.

WORKDIR /app
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p omni-me-agent \
    && cp target/release/omni-me-agent /omni-me-agent

# ---- runtime ----
# Must match the builder's distro, for the same glibc reason.
FROM debian:trixie-slim AS runtime

ARG GIT_SHA=dev
LABEL org.opencontainers.image.title="omni-me-agent" \
      org.opencontainers.image.revision="${GIT_SHA}"

# ca-certificates = outbound TLS to the LLM endpoint and to the sync server.
#
# libstdc++6 is what the bundled onnxruntime needs: `ldd` on the release binary
# lists libstdc++, libgcc_s, libm and libc and nothing else, and a slim image is
# not guaranteed to carry the first. Missing it fails at exec with a loader
# error, before any log line exists to explain it.
#
# poppler-utils supplies `pdftotext`. The resident loop never ingests bytes, but
# the reading and transcription benches in this binary do exec it, and the server
# image already paid for this omission once: every spawn failed ENOENT quietly
# enough to read as "no statements arrived".
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libstdc++6 poppler-utils \
    && rm -rf /var/lib/apt/lists/*

# Non-root. Home = /data so the named volume mount inherits `app` ownership.
RUN useradd --system --uid 10001 --create-home --home-dir /data app

COPY --from=builder /omni-me-agent /usr/local/bin/omni-me-agent

USER app
WORKDIR /data

# One volume carries everything the agent must not lose: its own SurrealDB (with
# the vector index) and the embedding model cache, which `model_cache_dir`
# defaults to `<data>/models`.
#
# ⚠️ The index is the reason the volume is not optional. A first boot sweeps the
# corpus for over 24 minutes and cannot answer until it finishes, so an agent
# whose data dir is ephemeral pays that on every redeploy.
#
# OMNI_AGENT_DATA also flips the non-production posture, which makes the agent
# skip any persisted server URL — so OMNI_AGENT_SERVER_URL is not optional here
# either. The compose file supplies it.
ENV OMNI_AGENT_DATA=/data \
    XDG_CONFIG_HOME=/config

# No EXPOSE and no HEALTHCHECK, both on purpose. The agent serves nothing, and it
# is PID 1 — if it dies the container exits, which `restart: unless-stopped`
# already answers. A healthcheck with a start period shorter than that first index
# sweep would kill a working boot.

ENTRYPOINT ["/usr/local/bin/omni-me-agent"]
