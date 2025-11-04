# Build stage
FROM rust:1.90 AS chef

WORKDIR /app

# Install cargo-chef deterministically
RUN cargo install --locked cargo-chef

# Stage 1: compute dependency recipe (cacheable)
FROM chef AS planner

# Copy everything so Cargo Chef can correctly resolve the graph
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: cook dependencies (cacheable)
FROM chef AS deps
COPY --from=planner /app/recipe.json recipe.json

# Use BuildKit cache mounts for registry/git to speed up network & reuse layers
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

# Stage 3: build the actual binary
FROM chef AS builder

# Reuse cooked deps layer
COPY --from=deps /app/target target
COPY --from=deps /usr/local/cargo /usr/local/cargo

# Now copy real source and build
COPY . .

# Cache cargo artifacts across builds; keep target cache for faster incremental rebuilds
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin offline-election-tool

# Optionally strip symbols to shrink the binary
RUN strip target/release/offline-election-tool || true

# Runtime
FROM debian:bookworm-slim

# Install runtime deps; clean apt lists to keep image slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Run as non-root
RUN useradd -u 10001 -r -s /usr/sbin/nologin appuser
COPY --from=builder /app/target/release/offline-election-tool /app/offline-election-tool
RUN chmod +x /app/offline-election-tool && chown appuser:appuser /app/offline-election-tool
USER appuser

ENTRYPOINT ["/app/offline-election-tool"]
