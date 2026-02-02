# NoString Docker Build
# 
# NOTE: NoString is primarily a desktop application (Tauri).
# This Dockerfile is for building the Rust libraries for custom integrations
# or CI/CD pipelines. For the desktop app, build locally with `cargo tauri build`.

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

# Copy source
COPY Cargo.toml ./
COPY crates ./crates

# Generate Cargo.lock and build
RUN cargo generate-lockfile && \
    cargo build --release --workspace --exclude nostring-app

# Run tests to verify build
RUN cargo test --release --workspace --exclude nostring-app

# =============================================================================
# Stage 2: Runtime (minimal image for CI artifacts)
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

# Copy documentation
COPY --chown=nostring:nostring docs /home/nostring/docs
COPY --chown=nostring:nostring README.md /home/nostring/

# NoString libraries are Rust crates - link against them in your Rust project
# or use the Tauri desktop application for end-user usage.
CMD ["cat", "/home/nostring/README.md"]

# =============================================================================
# Labels
# =============================================================================
LABEL org.opencontainers.image.title="NoString"
LABEL org.opencontainers.image.description="Sovereign Bitcoin inheritance"
LABEL org.opencontainers.image.source="https://github.com/RenAndKiwi/nostring"
LABEL org.opencontainers.image.licenses="BSD-3-Clause"
