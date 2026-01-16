# Tasker Core

[![CI](https://github.com/tasker-systems/tasker-core-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/tasker-systems/tasker-core-rs/actions/workflows/ci.yml)
![GitHub](https://img.shields.io/github/license/tasker-systems/tasker-core-rs)
![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/tasker-systems/tasker-core-rs?color=blue&sort=semver)
![Tests](https://img.shields.io/badge/tests-1400%2B%20passing-brightgreen)

High-performance Rust workflow orchestration with PostgreSQL-native messaging. Tasker Core provides DAG-based task execution, event-driven coordination, and comprehensive state management for complex workflows. Supports PGMQ (default, single-dependency) or RabbitMQ (high-throughput) backends.

**Status**: Production Ready | **Version**: 0.1.0

---

## Why Tasker Core?

- **DAG-Based Workflows** - Define complex workflows as directed acyclic graphs with dependencies
- **Flexible Messaging** - PGMQ (PostgreSQL-native, zero extra dependencies) or RabbitMQ (high-throughput)
- **Event-Driven** - Real-time step discovery with push notifications and polling fallback
- **Multi-Language Workers** - Rust native, Ruby via FFI, Python via PyO3, TypeScript FFI
- **Production-Ready** - Circuit breakers, health monitoring, zero race conditions, 50+ metrics

### Ideal For

- Order fulfillment workflows with inventory, payment, and shipping coordination
- Payment processing pipelines with retry logic and idempotency
- ETL pipelines with complex dependencies and parallel execution
- Microservices orchestration across distributed systems

### Not Ideal For

- Simple cron jobs (use native cron)
- Single-step operations (overhead not justified)
- Sub-millisecond requirements (~10-20ms architectural overhead)

---

## Quick Start

### Prerequisites

- **Rust** 1.75+ | **PostgreSQL** 17+ (uuidv7 support) | **Docker** (recommended)
- Optional: RabbitMQ (for high-throughput messaging)

### Get Running

```bash
# Start PostgreSQL (includes PGMQ extension)
docker-compose up -d postgres

# Run migrations
export DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test"
cargo sqlx migrate run

# Start orchestration server
docker-compose --profile server up -d

# Create a task
curl -X POST http://localhost:8080/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{"template_name": "linear_workflow", "namespace": "example"}'

# Check health
curl http://localhost:8080/health
```

**Full Guide**: [docs/guides/quick-start.md](docs/guides/quick-start.md)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Client Applications                     │
│                   (REST API / CLI / SDK)                    │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│              Tasker Orchestration Server                    │
│    Task Init → Step Discovery → Enqueueing → Finalization  │
└──────────┬──────────────────────────────────┬───────────────┘
           │                                  │
           ▼                                  ▼
┌──────────────────────┐          ┌──────────────────────────┐
│  PostgreSQL + Broker │◄────────►│  Namespace Worker Pools  │
│ (PGMQ or RabbitMQ)   │          │  (Horizontal Scaling)    │
└──────────────────────┘          └──────────────────────────┘
```

**Key Patterns**: Dual state machines (12 task + 9 step states), event-driven with polling fallback, autonomous workers, PostgreSQL as source of truth. Messaging via PGMQ (default) or RabbitMQ.

**Deep Dive**: [Architecture Documentation](docs/architecture/)

---

## Workspace Structure

| Crate | Purpose |
|-------|---------|
| `pgmq-notify` | PGMQ wrapper with atomic notify (PostgreSQL backend) |
| `tasker-shared` | Core types, state machines, messaging abstraction |
| `tasker-orchestration` | Task coordination, REST API |
| `tasker-worker` | Step execution, FFI layer |
| `tasker-client` | REST client and CLI |
| `workers/rust` | Native Rust workers |
| `workers/ruby` | Ruby FFI bindings |
| `workers/python` | Python FFI bindings |
| `workers/typescript` | TypeScript FFI bindings |

---

## Performance

| Metric | Target | Actual |
|--------|--------|--------|
| Task initialization (p99) | < 100ms | 17.7ms |
| SQL functions (mean) | < 3ms | 1.75-2.93ms |
| 4-step workflow (p99) | < 500ms | 133.5ms |

Horizontal scaling for orchestration and workers. SQL performance at sub-3ms with 100K+ tasks. Throughput >100 tasks/second per orchestrator.

**Full Benchmarks**: [docs/benchmarks/README.md](docs/benchmarks/README.md)

---

## Documentation

All documentation is organized in the **[Documentation Hub](docs/README.md)**:

| Section | Description |
|---------|-------------|
| **[Quick Start](docs/guides/quick-start.md)** | Get running in 5 minutes |
| **[Architecture](docs/architecture/)** | System design, state machines, event systems |
| **[Guides](docs/guides/)** | Workflows, batch processing, configuration |
| **[Workers](docs/workers/)** | Ruby, Python, TypeScript, Rust handler development |
| **[Operations](docs/operations/)** | Deployment, monitoring, tuning |
| **[Principles](docs/principles/)** | Design philosophy and tenets |
| **[Decisions](docs/decisions/)** | Architecture Decision Records (ADRs) |

**AI Assistant Context**: [CLAUDE.md](CLAUDE.md) | **Development Guide**: [docs/development/](docs/development/)

---

## Development

```bash
# Build and test (always use --all-features)
cargo build --all-features
cargo test --all-features

# Lint
cargo fmt && cargo clippy --all-targets --all-features

# Run server
cargo run --bin tasker-server
```

See **[CLAUDE.md](CLAUDE.md)** for complete development context.

---

## Contributing

1. Review [CLAUDE.md](CLAUDE.md) for project context
2. Run tests: `cargo test --all-features`
3. Format and lint before PR
4. See [Documentation Hub](docs/README.md) for documentation structure

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Production-ready workflow orchestration at scale.**

[Quick Start](docs/guides/quick-start.md) | [Documentation](docs/README.md) | [Examples](docs/guides/use-cases-and-patterns.md)
