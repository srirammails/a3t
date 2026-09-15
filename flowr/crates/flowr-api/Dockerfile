# Build stage
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Copy all workspace members
COPY iggy-poc ./iggy-poc
COPY dragonfly-poc ./dragonfly-poc
COPY flowr-poc ./flowr-poc
COPY iggy-pg-poc ./iggy-pg-poc
COPY dragonfly-connector ./dragonfly-connector
COPY flowr-core ./flowr-core
COPY flowr-expr ./flowr-expr
COPY flowr-tasks ./flowr-tasks
COPY flowr-registry ./flowr-registry
COPY flowr-api ./flowr-api
COPY flowr-suspend ./flowr-suspend

# Build the release binary
RUN cargo build --release -p flowr-api

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/flowr-server /usr/local/bin/

ENV RUST_LOG=info
EXPOSE 8080

CMD ["flowr-server"]
