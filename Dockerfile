# Build stage
FROM rust:1.90 as builder

WORKDIR /app

# Copy dependency files and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies, remove apt lists for smaller image size
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/offline-election-tool /app/offline-election-tool

# Make it executable
RUN chmod +x /app/offline-election-tool

ENTRYPOINT ["/app/offline-election-tool"]

