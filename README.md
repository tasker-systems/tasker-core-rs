# Tasker Core

[![CI](https://github.com/tasker-systems/tasker-core/actions/workflows/ci.yml/badge.svg)](https://github.com/tasker-systems/tasker-core/actions/workflows/ci.yml)
![GitHub](https://img.shields.io/github/license/tasker-systems/tasker-core)
![GitHub release (latest SemVer)](https://img.shields.io/github/v/release/tasker-systems/tasker-core?color=blue&sort=semver)
![Tests](https://img.shields.io/badge/tests-700%2B%20passing-brightgreen)

High-performance Rust implementation of workflow orchestration designed for production-scale deployments. Built on PostgreSQL with PGMQ message queues, Tasker Core provides DAG-based task execution, event-driven coordination, and comprehensive state management for complex workflows.

**Status**: Production Ready | **Version**: 0.1.0

---

## What is Tasker Core?

Tasker Core is a workflow orchestration system that solves the challenge of coordinating complex, multi-step processes across distributed systems. It provides:

- **DAG-Based Workflow Execution** - Define complex workflows as directed acyclic graphs with dependencies
- **PostgreSQL-Native Architecture** - Leverage PostgreSQL for durability, PGMQ for reliable messaging
- **Event-Driven Coordination** - Real-time step discovery using PostgreSQL LISTEN/NOTIFY with polling fallback
- **Dual State Machines** - Comprehensive task and step lifecycle management with atomic transitions
- **Multi-Language Workers** - Rust native workers, Ruby via FFI, Python/WASM planned
- **Production-Ready** - Circuit breakers, health monitoring, comprehensive metrics, zero race conditions

### When to Use Tasker Core

**Ideal Use Cases**:
- **Order Fulfillment Workflows** - Multi-step processes with inventory checks, payment processing, shipping coordination
- **Payment Processing Pipelines** - Retry logic, idempotency, distributed transaction coordination
- **Data Transformation DAGs** - ETL pipelines with complex dependencies and parallel execution
- **Microservices Orchestration** - Coordinate distributed operations across multiple services
- **Event-Driven Systems** - React to database changes and external events with reliable execution

**Not Ideal For**:
- Simple cron jobs (use native cron or systemd timers)
- Single-step operations (overhead not justified)
- Real-time sub-millisecond requirements (architectural overhead ~10-20ms)

---

## Architecture Overview

### Core Components

```
┌─────────────────────────────────────────────────────────────┐
│                     Client Applications                     │
│                   (REST API / CLI / SDK)                    │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│              Tasker Orchestration Server                    │
│  ┌────────────────────────────────────────────────────┐    │
│  │  Task Initialization → Step Discovery →           │    │
│  │  Step Enqueueing → Result Processing →            │    │
│  │  Task Finalization                                 │    │
│  └────────────────────────────────────────────────────┘    │
└──────────┬──────────────────────────────────┬───────────────┘
           │                                  │
           ▼                                  ▼
┌──────────────────────┐          ┌──────────────────────────┐
│   PostgreSQL + PGMQ  │          │  Namespace-Specific      │
│  ┌────────────────┐  │          │  Worker Pools            │
│  │ Task Database  │  │◄────────►│  ┌──────────────────┐   │
│  │ State Machines │  │          │  │ payments_queue   │   │
│  │ SQL Functions  │  │          │  │ inventory_queue  │   │
│  │ Message Queues │  │          │  │ fulfillment_queue│   │
│  └────────────────┘  │          │  └──────────────────┘   │
└──────────────────────┘          └──────────────────────────┘
```

### Key Architectural Patterns

**1. PostgreSQL-Centric Design**
- Database as coordination layer and source of truth
- PGMQ for reliable, durable message queuing
- SQL functions for complex orchestration logic
- PostgreSQL LISTEN/NOTIFY for real-time events

**2. Dual State Machine Architecture**
- **Task State Machine** (12 states): Overall workflow lifecycle
- **Step State Machine** (9 states): Individual step execution
- Atomic transitions with processor ownership tracking
- Complete audit trail for compliance and debugging

**3. Event-Driven with Fallback**
- **Hybrid Mode**: Event-driven primary, polling fallback (recommended for production)
- **EventDrivenOnly Mode**: Real-time coordination via LISTEN/NOTIFY
- **PollingOnly Mode**: Traditional polling for restricted environments

**4. Autonomous Worker Pattern**
- Workers claim steps from namespace-specific queues
- Independent step execution without central coordination
- Language-agnostic via FFI (Rust native, Ruby, Python planned)
- Horizontal scaling per namespace

---

## Workspace Architecture

Tasker Core is organized as a Cargo workspace with 7 specialized crates:

| Crate | Purpose | Key Responsibility |
|-------|---------|-------------------|
| **pgmq-notify** | PGMQ wrapper | Message queuing with atomic notify support |
| **tasker-shared** | Foundation | Core types, SQL functions, state machines |
| **tasker-orchestration** | Orchestration | Task coordination, REST API, lifecycle management |
| **tasker-worker** | Workers | Step execution, handler integration, FFI layer |
| **tasker-client** | Client Library | REST client and CLI tools |
| **workers/ruby/ext** | Ruby Workers | Ruby FFI bindings for Ruby handlers |
| **workers/rust** | Rust Workers | Native Rust worker implementation |

**Architecture Deep Dive**: See **[docs/crate-architecture.md](docs/crate-architecture.md)** for detailed explanations and dependency graphs.

---

## Quick Start

### Prerequisites

- **Rust**: 1.75+ with Cargo
- **PostgreSQL**: 14+ with PGMQ extension
- **Docker**: For local development (optional but recommended)

### Get Running in 5 Minutes

```bash
# 1. Start PostgreSQL with PGMQ using Docker Compose
docker-compose up -d postgres

# 2. Run database migrations
export DATABASE_URL="postgresql://tasker:tasker@localhost/tasker_rust_test"
cargo sqlx migrate run

# 3. Start orchestration server
docker-compose --profile server up -d

# 4. Create your first task
curl -X POST http://localhost:8080/v1/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "template_name": "linear_workflow",
    "namespace": "example",
    "configuration": {}
  }'

# 5. Monitor execution
curl http://localhost:8080/v1/tasks/{task_uuid}
```

**Full Quick Start Guide**: **[docs/quick-start.md](docs/quick-start.md)**

---

## Core Concepts

### Tasks and Steps

```yaml
# Example: Order fulfillment workflow
Task: order_fulfillment_#{order_id}
  │
  ├─ Step: validate_order (pending)
  │   │
  │   └─ Step: check_inventory (waiting_for_dependencies)
  │       │
  │       ├─ Step: reserve_stock (parallel)
  │       └─ Step: process_payment (parallel)
  │           │
  │           └─ Step: ship_order (waiting_for_dependencies)
  │               │
  │               └─ Step: send_confirmation (waiting_for_dependencies)
```

**Tasks**: Overall workflow instances with metadata, priority, and lifecycle
**Steps**: Individual work units with handlers, dependencies, retry logic

### State Machines

**Task States** (12 comprehensive states):
```
Pending → Initializing → EnqueuingSteps → StepsInProcess →
EvaluatingResults → [Complete | WaitingForDependencies | WaitingForRetry]
```

**Step States** (9 states with orchestration queuing):
```
Pending → Enqueued → InProgress → EnqueuedForOrchestration →
[Complete | WaitingForRetry | Error]
```

**Deep Dive**: **[docs/states-and-lifecycles.md](docs/states-and-lifecycles.md)**

### Event-Driven Coordination

```
Worker completes step
  ↓ [pgmq_send_with_notify]
PGMQ: orchestration_step_results queue
  ↓ [pg_notify: 'pgmq_message_ready.orchestration']
Orchestration Event System
  ↓ [Process result, discover next steps]
Step readiness analyzed via SQL functions
  ↓ [pgmq_send_with_notify]
PGMQ: {namespace}_queue
  ↓ [pg_notify: 'pgmq_message_ready.{namespace}']
Worker Event System
  ↓ [Claim and execute step]
```

**Deep Dive**: **[docs/events-and-commands.md](docs/events-and-commands.md)**

---

## Deployment

### Deployment Modes

**Hybrid Mode** (Recommended for Production):
- Event-driven primary with polling fallback
- Real-time when possible, reliable always
- Automatic degradation during issues

**EventDrivenOnly Mode**:
- Pure event-driven via PostgreSQL LISTEN/NOTIFY
- Lowest latency for step discovery
- Requires stable PostgreSQL connections

**PollingOnly Mode**:
- Traditional polling-based coordination
- Higher latency but guaranteed operation
- Useful for restricted network environments

**Configuration Guide**: **[docs/deployment-patterns.md](docs/deployment-patterns.md)**

### Docker Deployment

```bash
# Start database + orchestration + workers
docker-compose --profile server up -d

# Verify health
curl http://localhost:8080/health

# Scale workers per namespace
docker-compose up -d --scale worker=3
```

### Production Considerations

- **Database Pool**: Configure pool size for expected load (default: 20 connections)
- **Circuit Breakers**: Tune thresholds based on failure patterns (default: 5 errors in 60s)
- **Executor Pools**: Set min/max executors per type (default: 2-10 per pool)
- **Health Monitoring**: K8s readiness/liveness probes at `/health`
- **Metrics**: OpenTelemetry-compatible metrics for Prometheus/DataDog

---

## Performance

### Benchmarks (5K tasks, 21K+ steps)

| Category | Metric | Target | Actual | Status |
|----------|--------|--------|--------|--------|
| **API** | Task initialization (p99) | < 100ms | 17.7ms | ✅ 5.6x better |
| **SQL** | get_next_ready_tasks (mean) | < 3ms | 1.75-2.93ms | ✅ Pass |
| **SQL** | get_step_readiness_status (mean) | < 1ms | 440-603µs | ✅ Pass |
| **Events** | NOTIFY round-trip (p95) | < 10ms | 14.1ms | ⚠️ Slightly above |
| **E2E** | 4-step workflow (p99) | < 500ms | 133.5ms | ✅ 3.7x better |

**Full Benchmark Suite**: **[docs/benchmarks/README.md](docs/benchmarks/README.md)**

### Scaling Characteristics

- **Horizontal**: Scale orchestration and worker processes independently
- **Database**: SQL functions maintain sub-3ms performance at 100K+ tasks
- **Throughput**: >100 tasks/second per orchestrator instance
- **Worker Pools**: Linear scaling per namespace, no contention

---

## Development

### Common Commands

```bash
# Build everything
cargo build --all-features

# Run tests (requires DATABASE_URL)
DATABASE_URL="postgresql://..." cargo test --all-features

# Run benchmarks
cargo bench --all-features

# Format and lint
cargo fmt
cargo clippy --all-targets --all-features

# Run orchestration server locally
cargo run --bin tasker-server
```

### Configuration

Component-based TOML configuration with environment overrides:

```
config/tasker/
├── base/                      # Base configuration
│   ├── database.toml
│   ├── orchestration.toml
│   ├── circuit_breakers.toml
│   └── ...
└── environments/              # Environment overrides
    ├── development/
    ├── test/
    └── production/
```

---

## Documentation

### Essential Reading

- **[Documentation Hub](docs/README.md)** - Central navigation for all docs
- **[Crate Architecture](docs/crate-architecture.md)** - Workspace structure explained
- **[Quick Start Guide](docs/quick-start.md)** - Get running in 5 minutes
- **[Use Cases & Patterns](docs/use-cases-and-patterns.md)** - When and how to use Tasker

### Architecture Documentation

- **[Events and Commands](docs/events-and-commands.md)** - Event-driven coordination patterns
- **[States and Lifecycles](docs/states-and-lifecycles.md)** - State machine architecture
- **[Task Readiness & SQL Functions](docs/task-and-step-readiness-and-execution.md)** - Database-level logic

### Operations

- **[Deployment Patterns](docs/deployment-patterns.md)** - Configuration and deployment
- **[Observability](docs/observability/README.md)** - Metrics, logging, monitoring
- **[Benchmarks](docs/benchmarks/README.md)** - Performance validation

### Advanced

- **[CLAUDE.md](CLAUDE.md)** - Complete project context for AI assistants
- **[Ticket Specs](docs/ticket-specs/)** - Feature implementations (TAS-29 through TAS-49)
- **[Archive](docs/archive/)** - Historical insights and lessons learned

---

## Key Features

### Production-Ready Orchestration

- **Zero Race Conditions**: Atomic finalization claiming via SQL functions
- **Circuit Breaker Protection**: Automatic failure detection and recovery
- **Dynamic Executor Pools**: Auto-scaling based on load
- **Comprehensive Observability**: 50+ metrics for monitoring
- **Health Monitoring**: K8s-compatible health checks

### Developer Experience

- **Type-Safe**: Full Rust type safety with SQLx compile-time query verification
- **Testable**: 645+ tests with comprehensive factory system
- **Documented**: Inline documentation and comprehensive guides
- **Extensible**: Plugin system for custom handlers and workers
- **Observable**: Structured logging with correlation IDs

### Enterprise Features

- **Audit Trail**: Complete state transition history
- **Retry Semantics**: Configurable retry with exponential backoff
- **Priority Scheduling**: Dynamic priority with age-based escalation
- **Namespace Isolation**: Logical separation of workflow types
- **Multi-Tenancy**: Support for multiple deployments

---

## Related Projects

- **[tasker-engine](https://github.com/tasker-systems/tasker-engine)**: Production Rails engine for workflow orchestration
- **[tasker-blog](https://github.com/tasker-systems/tasker-blog)**: GitBook documentation with engineering stories

---

## Contributing

We welcome contributions! Please see:

- **[CLAUDE.md](CLAUDE.md)** for complete project context
- **[docs/README.md](docs/README.md)** for documentation structure
- GitHub Issues for bug reports and feature requests

### Development Workflow

1. Clone and set up environment
2. Run tests: `cargo test --all-features`
3. Make changes with tests
4. Run linters: `cargo fmt && cargo clippy`
5. Submit PR with clear description

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Built with ❤️ by the Tasker Systems team**

Production-ready workflow orchestration at scale.

→ **Get Started**: [Quick Start Guide](docs/quick-start.md)
→ **Learn More**: [Documentation Hub](docs/README.md)
→ **See Examples**: [Use Cases & Patterns](docs/use-cases-and-patterns.md)
