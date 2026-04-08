# ── Stage 1: Build ───────────────────────────────────────────────────────────
# Uses the full Rust toolchain image. git is required by build.rs at compile
# time to embed the version string via `git describe`.
FROM rust:1.85-slim AS builder

RUN apt-get update && apt-get install -y \
    git \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests and lock file first so dependency fetching is a cached layer.
# Actual source is copied after, so editing src/ doesn't re-fetch crates.
COPY Cargo.toml Cargo.lock ./
COPY hotspots-core/Cargo.toml hotspots-core/
COPY hotspots-cli/Cargo.toml hotspots-cli/
COPY hotspots-cli/build.rs hotspots-cli/

# Pre-fetch all dependencies (cached unless Cargo.lock changes)
RUN cargo fetch

# Copy the full source tree and build the release binary
COPY hotspots-core/ hotspots-core/
COPY hotspots-cli/ hotspots-cli/

RUN cargo build --release -p hotspots-cli

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
# Minimal Debian image. Needs:
#   git         — hotspots shells out to git for churn/touch/co-change metrics
#   ca-certs    — git over HTTPS for any network ops
#   procps      — provides /usr/bin/time and ps for memory instrumentation
#   time        — GNU time for peak RSS measurement inside the container
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    git \
    ca-certificates \
    procps \
    time \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/hotspots /usr/local/bin/hotspots

# Entrypoint dispatcher: avoids fly CLI argument-parsing issues when the
# command contains flags (e.g. git -c pack.threads=2). Accepts: analyze,
# probe, clone, clear, stats, or pass-through exec "$@".
COPY benchmarks/memory-crash/benchmark-entrypoint.sh /usr/local/bin/benchmark-entrypoint
RUN chmod +x /usr/local/bin/benchmark-entrypoint

ENTRYPOINT ["/usr/local/bin/benchmark-entrypoint"]
CMD ["analyze"]
