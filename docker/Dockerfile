# NoString Docker Build
# Multi-stage build for minimal production image

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM rust:1.75-bookworm AS builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binaries
RUN cargo build --release --workspace --exclude nostring-app

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --shell /bin/bash nostring
USER nostring
WORKDIR /home/nostring

# Copy built libraries (for custom integrations)
COPY --from=builder /app/target/release/libnostring*.so /usr/local/lib/ 2>/dev/null || true
COPY --from=builder /app/target/release/libnostring*.rlib /usr/local/lib/ 2>/dev/null || true

# Copy documentation
COPY docs /home/nostring/docs
COPY README.md /home/nostring/

# Default command (informational)
CMD ["echo", "NoString libraries built. Use as base image or mount your integration."]

# =============================================================================
# Labels
# =============================================================================
LABEL org.opencontainers.image.title="NoString"
LABEL org.opencontainers.image.description="Sovereign Bitcoin inheritance"
LABEL org.opencontainers.image.source="https://github.com/nostring/nostring"
LABEL org.opencontainers.image.licenses="BSD-3-Clause"
