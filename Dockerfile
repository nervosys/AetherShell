# AetherShell Docker Image
# Multi-stage build for minimal final image

# Stage 1: Build
FROM rust:1.75-slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Create dummy main to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies only
RUN cargo build --release --features native && \
    rm -rf src target/release/deps/aether_shell*

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
