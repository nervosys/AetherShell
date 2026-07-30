# AetherShell Docker Image
# Multi-stage build for minimal final image

# Stage 1: Build
#
# Track the latest 1.x stable rather than pinning a patch release, so this image
# stays in step with the `stable` toolchain the CI and Release workflows use. The
# previous `rust:1.75-slim-bookworm` pin broke once dependencies moved to edition
# 2024 (base64ct, bcrypt, pbkdf2, time, url, home), which Cargo 1.75 cannot parse:
#
#   The package requires the Cargo feature called `edition2024`, but that feature
#   is not stabilized in this version of Cargo (1.75.0)
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Create stubs for EVERY target declared in Cargo.toml, so the dependency-cache
# layer can parse the manifest.
#
# Cargo refuses to parse a manifest whose declared targets do not exist on disk,
# and this layer deliberately copies only Cargo.toml/Cargo.lock — not src/ or
# benches/. It previously stubbed just src/main.rs and src/lib.rs, so the build
# died on the first missing target:
#
#   error: failed to parse manifest at `/app/Cargo.toml`
#   Caused by: can't find `mcp_performance` bench at `benches/mcp_performance.rs`
#
# That is why the Docker workflow had failed on every run since 2026-02-11.
# If a [[bin]] or [[bench]] is added to Cargo.toml, add its stub here too.
RUN mkdir -p src src/bin benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/bin/aimodel.rs && \
    for b in mcp_performance builtin_performance parser_performance \
             eval_performance pipeline_performance; do \
        echo "fn main() {}" > "benches/$b.rs"; \
    done

# Build dependencies only
RUN cargo build --release --features native && \
    rm -rf src benches target/release/deps/aether_shell*

# Copy actual source
COPY src/ src/
COPY examples/ examples/

# Build release binary
RUN cargo build --release --features native --bin ae --bin aimodel

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash aether
USER aether
WORKDIR /home/aether

# Copy binaries from builder
COPY --from=builder /app/target/release/ae /usr/local/bin/ae
COPY --from=builder /app/target/release/aimodel /usr/local/bin/aimodel

# Copy example scripts
COPY --chown=aether:aether examples/ /home/aether/examples/

# Set environment
ENV TERM=xterm-256color
ENV RUST_BACKTRACE=1

# Health check
HEALTHCHECK --interval=30s --timeout=3s \
    CMD ae -c "1 + 1" || exit 1

# Default command: interactive REPL
ENTRYPOINT ["ae"]
CMD []

# Labels
LABEL org.opencontainers.image.title="AetherShell"
LABEL org.opencontainers.image.description="AI-powered typed shell with functional pipelines"
LABEL org.opencontainers.image.url="https://github.com/nervosys/AetherShell"
LABEL org.opencontainers.image.source="https://github.com/nervosys/AetherShell"
LABEL org.opencontainers.image.vendor="Nervosys"
LABEL org.opencontainers.image.licenses="Apache-2.0"
