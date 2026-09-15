# Flowr System - POC

Proof of Concept for the Flowr workflow runtime.

## Overview

Flowr is an open-source workflow runtime built for latency-sensitive, event-driven infrastructure. This POC validates the core infrastructure concepts before full implementation.

## Goals

1. **Validate Iggy** - Deploy and verify queue behavior matches Flowr spec requirements
2. **Validate Dragonfly** - Deploy and explore as flow registry/hot state store
3. **Validate Rust Stack** - Basic Tokio/Kameo POC proving the execution model

## Quick Start

### Local Development (Docker Compose)

```bash
# Start Iggy and Dragonfly
cd deployment
docker-compose up -d

# Verify services
docker-compose ps
```

### Run POC Tests

```bash
cd poc

# Run Iggy validation
cargo run -p iggy-poc

# Run Dragonfly validation
cargo run -p dragonfly-poc

# Run Flowr core POC
cargo run -p flowr-poc
```

## Directory Structure

```
flowr-system/
├── deployment/           # Docker and K8s deployment files
│   ├── docker-compose.yml
│   ├── iggy/            # Iggy K8s manifests
│   └── dragonfly/       # Dragonfly K8s manifests
├── poc/                  # Proof of Concept code
│   ├── iggy-poc/        # Iggy validation
│   ├── dragonfly-poc/   # Dragonfly exploration
│   └── flowr-poc/       # Core Flowr POC
└── docs/                 # Documentation
```

## Infrastructure Dependencies

| Component | Purpose | Port |
|-----------|---------|------|
| Apache Iggy | Audit log, checkpoints, durability | 8090 (TCP), 3000 (HTTP) |
| Dragonfly | Flow registry, hot state, signal bus | 6379 |

## Success Criteria

- [ ] Iggy async write latency < 10μs
- [ ] Iggy sync write latency < 5ms
- [ ] Dragonfly read latency < 1ms
- [ ] Tokio mpsc transition latency < 1μs
- [ ] Kameo actor suspend/resume works
