# FlowR Agent Controller Dockerfile
# Build from flowr-system directory:
#   docker build -f flowr/crates/flowr-canvas/Dockerfile .

FROM rust:1.88-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y \
    cmake protobuf-compiler libssl-dev pkg-config clang git \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace
COPY flowr/Cargo.toml flowr/Cargo.lock /build/flowr/
COPY flowr/crates/flowr-canvas /build/flowr/crates/flowr-canvas
# Stub out flowr-server so workspace compiles
RUN mkdir -p /build/flowr/crates/flowr-server/src && \
    echo 'fn main() {}' > /build/flowr/crates/flowr-server/src/main.rs && \
    cp /build/flowr/crates/flowr-canvas/Cargo.toml /tmp/canvas-cargo.toml

# We only need flowr-canvas, not flowr-server deps
# Build just the controller
WORKDIR /build/flowr
RUN cargo build --release -p flowr-canvas 2>&1 || true

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/flowr/target/release/flowr-controller /usr/local/bin/flowr-controller

ENTRYPOINT ["/usr/local/bin/flowr-controller"]
