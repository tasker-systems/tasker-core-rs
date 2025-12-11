# Tasker Core

[![CI](https://github.com/tasker-systems/tasker-core-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/tasker-systems/tasker-core-rs/actions/workflows/ci.yml)
![GitHub](https://img.shields.io/github/license/tasker-systems/tasker-core-rs)
![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/tasker-systems/tasker-core-rs?color=blue&sort=semver)
![Tests](https://img.shields.io/badge/tests-1400%2B%20passing-brightgreen)

High-performance Rust workflow orchestration built on PostgreSQL with PGMQ message queues. Tasker Core provides DAG-based task execution, event-driven coordination, and comprehensive state management for complex workflows.

**Status**: Production Ready | **Version**: 0.1.0

---

## Why Tasker Core?

- **DAG-Based Workflows** - Define complex workflows as directed acyclic graphs with dependencies
- **PostgreSQL-Native** - Database as coordination layer with PGMQ for reliable messaging
- **Event-Driven** - Real-time step discovery via LISTEN/NOTIFY with polling fallback
- **Multi-Language Workers** - Rust native, Ruby via FFI, Python/WASM planned
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

- **Rust** 1.75+ | **PostgreSQL** 14+ with PGMQ | **Docker** (recommended)

### Get Running

```bash
# Start PostgreSQL with PGMQ
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

**Full Guide**: [docs/quick-start.md](docs/quick-start.md)

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
│   PostgreSQL + PGMQ  │◄────────►│  Namespace Worker Pools  │
│   (State + Queues)   │          │  (Horizontal Scaling)    │
└──────────────────────┘          └──────────────────────────┘
```

### Key Patterns

| Pattern | Description |
|---------|-------------|
| **Dual State Machines** | 12 task states + 9 step states with atomic transitions |
| **Event-Driven + Fallback** | LISTEN/NOTIFY primary, polling fallback (Hybrid mode) |
| **Autonomous Workers** | Claim steps from namespace queues, scale independently |
| **PostgreSQL-Centric** | Database as source of truth, SQL functions for orchestration |

**Deep Dive**: [docs/crate-architecture.md](docs/crate-architecture.md) | [docs/states-and-lifecycles.md](docs/states-and-lifecycles.md)

---

## Core Concepts

### Tasks and Steps

```yaml
Task: order_fulfillment_#{order_id}
  ├─ validate_order
  │   └─ check_inventory
  │       ├─ reserve_stock (parallel)
  │       └─ process_payment (parallel)
  │           └─ ship_order
  │               └─ send_confirmation
```

**Tasks**: Workflow instances with metadata, priority, lifecycle
**Steps**: Work units with handlers, dependencies, retry logic

### Workspace Structure

| Crate | Purpose |
|-------|---------|
| `pgmq-notify` | PGMQ wrapper with atomic notify |
| `tasker-shared` | Core types, state machines, SQL functions |
| `tasker-orchestration` | Task coordination, REST API |
| `tasker-worker` | Step execution, FFI layer |
| `tasker-client` | REST client and CLI |
| `workers/rust` | Native Rust workers |
| `workers/ruby` | Ruby FFI bindings |

---

## Performance

| Metric | Target | Actual |
|--------|--------|--------|
| Task initialization (p99) | < 100ms | 17.7ms |
| SQL functions (mean) | < 3ms | 1.75-2.93ms |
| 4-step workflow (p99) | < 500ms | 133.5ms |

- Horizontal scaling: orchestration and workers scale independently
- SQL performance: sub-3ms at 100K+ tasks
- Throughput: >100 tasks/second per orchestrator

**Full Benchmarks**: [docs/benchmarks/README.md](docs/benchmarks/README.md)

---

## Documentation

### Getting Started
- **[Quick Start](docs/quick-start.md)** - Get running in 5 minutes
- **[Use Cases & Patterns](docs/use-cases-and-patterns.md)** - When and how to use Tasker

### Architecture
- **[Crate Architecture](docs/crate-architecture.md)** - Workspace structure
- **[States & Lifecycles](docs/states-and-lifecycles.md)** - State machine design
- **[Events & Commands](docs/events-and-commands.md)** - Event-driven coordination

### Operations
- **[Configuration](docs/configuration-management.md)** - TOML architecture, CLI tools
- **[Deployment Patterns](docs/deployment-patterns.md)** - Hybrid, EventDriven, Polling modes
- **[Observability](docs/observability/README.md)** - Metrics, logging, monitoring

### Reference
- **[Documentation Hub](docs/README.md)** - Complete index
- **[CLAUDE.md](CLAUDE.md)** - AI assistant context

---

## Development

```bash
# Build and test
cargo build --all-features
cargo test --all-features

# Lint
cargo fmt && cargo clippy --all-targets --all-features

# Run server
cargo run --bin tasker-server
```

See **[CLAUDE.md](CLAUDE.md)** for complete development context.

---

## Related Projects

- **[tasker-engine](https://github.com/tasker-systems/tasker-engine)** - Production Rails engine
- **[tasker-blog](https://github.com/tasker-systems/tasker-blog)** - Documentation and engineering stories

---

## Contributing

1. Review [CLAUDE.md](CLAUDE.md) for project context
2. Run tests: `cargo test --all-features`
3. Format and lint before PR
4. See [docs/README.md](docs/README.md) for documentation structure

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Production-ready workflow orchestration at scale.**

[Quick Start](docs/quick-start.md) | [Documentation](docs/README.md) | [Examples](docs/use-cases-and-patterns.md)
