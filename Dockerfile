# ============================================================================
# NoString Server — Multi-stage Docker build
# ============================================================================
# Stage 1: Build dependencies (cached layer)
# Stage 2: Build the binary (fast rebuild on source changes)
# Stage 3: Minimal runtime image
# ============================================================================

# ---------------------------------------------------------------------------
# Builder stage
# ---------------------------------------------------------------------------
FROM rust:1.93-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# --- Layer 1: Cache workspace manifests + Cargo.lock ---
# Only changes to Cargo.toml/Cargo.lock invalidate this layer
COPY Cargo.toml Cargo.lock ./

# Copy all crate manifests
COPY crates/nostring-core/Cargo.toml crates/nostring-core/Cargo.toml
COPY crates/nostring-email/Cargo.toml crates/nostring-email/Cargo.toml
COPY crates/nostring-electrum/Cargo.toml crates/nostring-electrum/Cargo.toml
COPY crates/nostring-inherit/Cargo.toml crates/nostring-inherit/Cargo.toml
COPY crates/nostring-notify/Cargo.toml crates/nostring-notify/Cargo.toml
COPY crates/nostring-server/Cargo.toml crates/nostring-server/Cargo.toml
COPY crates/nostring-shamir/Cargo.toml crates/nostring-shamir/Cargo.toml
COPY crates/nostring-watch/Cargo.toml crates/nostring-watch/Cargo.toml
COPY tauri-app/src-tauri/Cargo.toml tauri-app/src-tauri/Cargo.toml
COPY tests/e2e/Cargo.toml tests/e2e/Cargo.toml

# --- Layer 2: Create minimal stubs so cargo can resolve the workspace ---
RUN set -e && \
    mkdir -p crates/nostring-core/src      && echo "pub fn stub(){}" > crates/nostring-core/src/lib.rs && \
    mkdir -p crates/nostring-email/src     && echo "pub fn stub(){}" > crates/nostring-email/src/lib.rs && \
    mkdir -p crates/nostring-electrum/src  && echo "pub fn stub(){}" > crates/nostring-electrum/src/lib.rs && \
    mkdir -p crates/nostring-inherit/src   && echo "pub fn stub(){}" > crates/nostring-inherit/src/lib.rs && \
    mkdir -p crates/nostring-notify/src    && echo "pub fn stub(){}" > crates/nostring-notify/src/lib.rs && \
    mkdir -p crates/nostring-server/src    && echo "fn main(){}"     > crates/nostring-server/src/main.rs && \
    mkdir -p crates/nostring-shamir/src    && echo "pub fn stub(){}" > crates/nostring-shamir/src/lib.rs && \
    mkdir -p crates/nostring-watch/src     && echo "pub fn stub(){}" > crates/nostring-watch/src/lib.rs && \
    mkdir -p tauri-app/src-tauri/src       && echo "fn main(){}"     > tauri-app/src-tauri/src/main.rs && \
    mkdir -p tests/e2e/src                 && echo "pub fn stub(){}" > tests/e2e/src/lib.rs

# --- Layer 3: Build all dependencies (this is the expensive cached layer) ---
# Build just the server target — pulls in all transitive deps
# The `|| true` handles the expected stub compilation failures,
# but all 609 dependency crates get compiled and cached here
RUN cargo build --release -p nostring-server 2>&1 || true

# Verify dependency cache worked (ring, bitcoin, nostr-sdk are the heavy ones)
RUN ls target/release/deps/libbitcoin-*.rlib 2>/dev/null && echo "✅ Dep cache OK" || echo "⚠️ Dep cache may be incomplete"

# --- Layer 4: Copy real source and rebuild (fast — only our crates) ---
COPY crates/ crates/

# We don't need Tauri or e2e tests in Docker, but the workspace requires them.
# The stubs from layer 2 are fine — we only build nostring-server.

# Invalidate the build cache for OUR crates only (not deps)
RUN find crates/ -name "*.rs" -newer Cargo.toml -exec touch {} + 2>/dev/null; \
    rm -f target/release/nostring-server target/release/deps/libnostring_* target/release/deps/nostring_*

# Final build — only recompiles our ~8 crates, not all 609 deps
RUN cargo build --release -p nostring-server

# Verify the binary exists and runs
RUN target/release/nostring-server --help || true

# ---------------------------------------------------------------------------
# Runtime stage — minimal image
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd -r nostring && useradd -r -g nostring -m nostring

# Data and config directories
RUN mkdir -p /data /config && chown nostring:nostring /data /config

# Copy the binary
COPY --from=builder /build/target/release/nostring-server /usr/local/bin/nostring-server

USER nostring

VOLUME ["/data", "/config"]

HEALTHCHECK --interval=60s --timeout=10s --retries=3 \
    CMD nostring-server --version || exit 1

ENTRYPOINT ["nostring-server"]
CMD ["--config", "/config/nostring-server.toml"]
