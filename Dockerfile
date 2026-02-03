# ============================================================================
# NoString Server — Multi-stage Docker build
# ============================================================================
# Stage 1: Build the Rust binary
# Stage 2: Minimal runtime image
# ============================================================================

# ---------------------------------------------------------------------------
# Builder stage
# ---------------------------------------------------------------------------
FROM rust:1.83-bookworm AS builder

# Install build dependencies for SQLite (bundled) and OpenSSL
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifests first for dependency caching
COPY Cargo.toml Cargo.lock* ./
COPY crates/nostring-core/Cargo.toml crates/nostring-core/Cargo.toml
COPY crates/nostring-email/Cargo.toml crates/nostring-email/Cargo.toml
COPY crates/nostring-electrum/Cargo.toml crates/nostring-electrum/Cargo.toml
COPY crates/nostring-inherit/Cargo.toml crates/nostring-inherit/Cargo.toml
COPY crates/nostring-notify/Cargo.toml crates/nostring-notify/Cargo.toml
COPY crates/nostring-server/Cargo.toml crates/nostring-server/Cargo.toml
COPY crates/nostring-shamir/Cargo.toml crates/nostring-shamir/Cargo.toml
COPY crates/nostring-watch/Cargo.toml crates/nostring-watch/Cargo.toml

# Create stub source files so cargo can resolve dependencies
RUN mkdir -p crates/nostring-core/src && echo "// stub" > crates/nostring-core/src/lib.rs && \
    mkdir -p crates/nostring-email/src && echo "// stub" > crates/nostring-email/src/lib.rs && \
    mkdir -p crates/nostring-electrum/src && echo "// stub" > crates/nostring-electrum/src/lib.rs && \
    mkdir -p crates/nostring-inherit/src && echo "// stub" > crates/nostring-inherit/src/lib.rs && \
    mkdir -p crates/nostring-notify/src && echo "// stub" > crates/nostring-notify/src/lib.rs && \
    mkdir -p crates/nostring-server/src && echo "fn main() {}" > crates/nostring-server/src/main.rs && \
    mkdir -p crates/nostring-shamir/src && echo "// stub" > crates/nostring-shamir/src/lib.rs && \
    mkdir -p crates/nostring-watch/src && echo "// stub" > crates/nostring-watch/src/lib.rs

# Also create stubs for the Tauri app and e2e tests (workspace members)
COPY tauri-app/src-tauri/Cargo.toml tauri-app/src-tauri/Cargo.toml
RUN mkdir -p tauri-app/src-tauri/src && echo "fn main() {}" > tauri-app/src-tauri/src/main.rs
COPY tests/e2e/Cargo.toml tests/e2e/Cargo.toml
RUN mkdir -p tests/e2e/src && echo "// stub" > tests/e2e/src/lib.rs

# Pre-build dependencies (this layer is cached until Cargo.toml changes)
RUN cargo build --release -p nostring-server 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/
COPY tauri-app/src-tauri/src/ tauri-app/src-tauri/src/
COPY tauri-app/src-tauri/build.rs tauri-app/src-tauri/build.rs

# Touch source files to invalidate the stub build cache
RUN find crates/nostring-server/src -name "*.rs" -exec touch {} +

# Build the real binary
RUN cargo build --release -p nostring-server

# ---------------------------------------------------------------------------
# Runtime stage — minimal Debian bookworm-slim
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies (OpenSSL, CA certificates for TLS)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN groupadd -r nostring && useradd -r -g nostring -m nostring

# Create directories for data and config
RUN mkdir -p /data /config && chown nostring:nostring /data /config

# Copy the binary from builder
COPY --from=builder /build/target/release/nostring-server /usr/local/bin/nostring-server

# Switch to non-root user
USER nostring

# Volumes for persistent data and configuration
VOLUME ["/data", "/config"]

# Health check — verify the binary runs
HEALTHCHECK --interval=60s --timeout=10s --retries=3 \
    CMD nostring-server --version || exit 1

# Default command
ENTRYPOINT ["nostring-server"]
CMD ["--config", "/config/nostring-server.toml"]
