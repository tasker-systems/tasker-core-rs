# Tasker Core Documentation

**Last Updated**: 2025-12-29
**Project Status**: Production Ready
**Version**: 0.1.0

Welcome to the Tasker Core documentation hub. This page provides organized access to all documentation, guides, and reference materials for the Tasker workflow orchestration system.

---

## Quick Start

**New to Tasker Core?** Start here:

1. **[Quick Start Guide](guides/quick-start.md)** - Get running in 5 minutes
2. **[Crate Architecture](architecture/crate-architecture.md)** - Understand the workspace structure
3. **[Use Cases & Patterns](guides/use-cases-and-patterns.md)** - See when and how to use Tasker

**Looking for something specific?** Jump to a section below or use the search (Cmd/Ctrl+F).

**Using AI assistance?** See **[CLAUDE-GUIDE.md](CLAUDE-GUIDE.md)** for efficient navigation patterns, trigger mappings, and investigation strategies designed for Claude sessions.

---

## Documentation Structure

The documentation is organized by cognitive function:

| Directory | Purpose | When to Read |
|-----------|---------|--------------|
| **[principles/](principles/)** | The "why" - Core values and design philosophy | Design decisions, trade-offs |
| **[architecture/](architecture/)** | The "what" - System structure and patterns | Understanding components |
| **[guides/](guides/)** | The "how" - Practical implementation | Implementing features |
| **[workers/](workers/)** | Language-specific handler development | Writing handlers |
| **[reference/](reference/)** | Technical specifications | Precise details, edge cases |
| **[development/](development/)** | Developer tooling | Build system, coding standards |
| **[operations/](operations/)** | Production guidance | Deployment, monitoring |

---

## Documentation by Role

### For Architects

**Making design decisions?** These documents explain architectural patterns and trade-offs:

**Principles & Philosophy**:
- **[Tasker Core Tenets](principles/tasker-core-tenets.md)** - The 10 foundational design principles
- **[Defense in Depth](principles/defense-in-depth.md)** - Multi-layer protection model
- **[Composition Over Inheritance](principles/composition-over-inheritance.md)** - Handler composition patterns
- **[Cross-Language Consistency](principles/cross-language-consistency.md)** - Multi-language API philosophy

**Architecture**:
- **[Crate Architecture](architecture/crate-architecture.md)** - Workspace structure and crate responsibilities
- **[Actor-Based Architecture](architecture/actors.md)** - Lightweight actor pattern for orchestration (TAS-46)
- **[Worker Actor Architecture](architecture/worker-actors.md)** - Actor pattern for worker step execution (TAS-69)
- **[Events and Commands](architecture/events-and-commands.md)** - Event-driven coordination patterns
- **[Domain Events](architecture/domain-events.md)** - Business event publishing (durable, fast, broadcast)
- **[States and Lifecycles](architecture/states-and-lifecycles.md)** - Dual state machine architecture
- **[Idempotency and Atomicity](architecture/idempotency-and-atomicity.md)** - Defense-in-depth guarantees
- **[Circuit Breakers](architecture/circuit-breakers.md)** - Fault isolation and cascade prevention (TAS-75)
- **[Backpressure Architecture](architecture/backpressure-architecture.md)** - Unified resilience strategy
- **[Deployment Patterns](architecture/deployment-patterns.md)** - Hybrid, EventDriven, and PollingOnly modes

**Historical Context**:
- **[CHRONOLOGY](CHRONOLOGY.md)** - Development timeline and lessons learned

### For Developers

**Building with Tasker?** These guides help you be productive:

**Getting Started**:
- **[Quick Start](guides/quick-start.md)** - Get your first workflow running
- **[Crate Architecture](architecture/crate-architecture.md)** - Understand the codebase structure
- **[Use Cases & Patterns](guides/use-cases-and-patterns.md)** - Practical workflow examples

**Workflow Patterns**:
- **[Conditional Workflows](guides/conditional-workflows.md)** - Runtime decision-making and dynamic step creation
- **[Batch Processing](guides/batch-processing.md)** - Parallel processing with cursor-based workers
- **[DLQ System](guides/dlq-system.md)** - Dead letter queue investigation and resolution
- **[Retry Semantics](guides/retry-semantics.md)** - Understanding max_attempts and retryable flags

**Handler Development**:
- **[Worker Patterns](workers/patterns-and-practices.md)** - Common handler patterns
- **[API Convergence Matrix](workers/api-convergence-matrix.md)** - Cross-language API reference
- **[Ruby Worker](workers/ruby.md)** | **[Python Worker](workers/python.md)** | **[TypeScript Worker](workers/typescript.md)** | **[Rust Worker](workers/rust.md)**

**Reference**:
- **[Task and Step Readiness](reference/task-and-step-readiness-and-execution.md)** - SQL functions and execution logic
- **[FFI Telemetry Pattern](reference/ffi-telemetry-pattern.md)** - Cross-language telemetry

### For Operators

**Running Tasker in production?** These docs cover deployment and monitoring:

- **[Deployment Patterns](architecture/deployment-patterns.md)** - Configuration and deployment modes
- **[Configuration Management](guides/configuration-management.md)** - Complete configuration guide
- **[Circuit Breakers](architecture/circuit-breakers.md)** - Fault isolation and operational monitoring
- **[Backpressure Architecture](architecture/backpressure-architecture.md)** - Unified resilience strategy
- **[DLQ System](guides/dlq-system.md)** - Dead letter queue investigation
- **[Observability](observability/README.md)** - Metrics, logging, and monitoring
- **[Backpressure Monitoring](operations/backpressure-monitoring.md)** - Alerting and incident response
- **[Channel Tuning](operations/mpsc-channel-tuning.md)** - MPSC channel configuration
- **[Checkpoint Operations](operations/checkpoint-operations.md)** - Monitoring and managing batch checkpoints
- **[Benchmarks](benchmarks/README.md)** - Performance validation and targets

---

## Complete Documentation Structure

```
docs/
├── README.md                           # You are here (documentation hub)
├── CLAUDE-GUIDE.md                     # Navigation guide for Claude sessions
├── CHRONOLOGY.md                       # Development timeline and lessons learned
│
├── principles/                         # Core values and design philosophy
│   ├── README.md                       # Principles index
│   ├── tasker-core-tenets.md           # The 10 foundational tenets
│   ├── defense-in-depth.md             # Multi-layer protection model
│   ├── cross-language-consistency.md   # Multi-language API philosophy
│   ├── composition-over-inheritance.md # Handler composition patterns
│   └── zen-of-python-PEP-20.md         # Reference inspiration
│
├── architecture/                       # System structure and patterns
│   ├── README.md                       # Architecture index
│   ├── crate-architecture.md           # Workspace structure
│   ├── actors.md                       # Orchestration actor pattern
│   ├── worker-actors.md                # Worker actor pattern
│   ├── worker-event-systems.md         # Worker event architecture
│   ├── states-and-lifecycles.md        # State machine specifications
│   ├── events-and-commands.md          # Event-driven coordination
│   ├── domain-events.md                # Business event publishing
│   ├── idempotency-and-atomicity.md    # Defense-in-depth guarantees
│   ├── backpressure-architecture.md    # Resilience strategy
│   ├── circuit-breakers.md             # Fault isolation
│   └── deployment-patterns.md          # Deployment modes
│
├── guides/                             # Practical how-to guides
│   ├── README.md                       # Guides index
│   ├── quick-start.md                  # Get running in 5 minutes
│   ├── use-cases-and-patterns.md       # Workflow examples
│   ├── conditional-workflows.md        # Decision points
│   ├── batch-processing.md             # Cursor-based workers
│   ├── dlq-system.md                   # Dead letter queue
│   ├── retry-semantics.md              # Error handling
│   └── configuration-management.md     # TOML configuration
│
├── workers/                            # Language-specific documentation
│   ├── README.md                       # Workers overview
│   ├── patterns-and-practices.md       # Common patterns
│   ├── api-convergence-matrix.md       # Cross-language API reference
│   ├── example-handlers.md             # Code examples
│   ├── memory-management.md            # FFI memory patterns
│   ├── ruby.md                         # Ruby worker
│   ├── python.md                       # Python worker
│   ├── typescript.md                   # TypeScript worker
│   └── rust.md                         # Rust worker
│
├── reference/                          # Technical specifications
│   ├── README.md                       # Reference index
│   ├── task-and-step-readiness-and-execution.md  # SQL functions
│   ├── table-management.md             # Database tables
│   ├── ffi-telemetry-pattern.md        # FFI telemetry
│   ├── library-deployment-patterns.md  # Library distribution
│   └── sccache-configuration.md        # Build caching
│
├── development/                        # Developer tooling
│   ├── tooling.md                      # cargo-make build system
│   ├── development-patterns.md         # Development workflows
│   ├── mpsc-channel-guidelines.md      # Channel usage rules
│   └── ffi-callback-safety.md          # FFI safety patterns
│
├── operations/                         # Production operations
│   ├── mpsc-channel-tuning.md          # Channel configuration
│   └── backpressure-monitoring.md      # Monitoring and alerting
│
├── observability/                      # Metrics and monitoring
│   └── README.md                       # Observability hub
│
├── benchmarks/                         # Performance testing
│   └── README.md                       # Benchmark suite overview
│
├── testing/                            # Testing documentation
│   └── comprehensive-lifecycle-testing-guide.md
│
├── decisions/                          # Architecture Decision Records (ADRs)
│   └── [ADR files]
│
└── ticket-specs/                       # Historical feature specifications
    └── TAS-*/                          # Per-ticket documentation
```

---

## Finding Documentation

### By Use Case

- **"I want to build a workflow"** -> [Use Cases & Patterns](guides/use-cases-and-patterns.md)
- **"I need dynamic step creation"** -> [Conditional Workflows](guides/conditional-workflows.md)
- **"I need to process large datasets"** -> [Batch Processing](guides/batch-processing.md)
- **"I have a stuck task"** -> [DLQ System](guides/dlq-system.md)
- **"I need to understand the architecture"** -> [Crate Architecture](architecture/crate-architecture.md)
- **"I want to publish business events"** -> [Domain Events](architecture/domain-events.md)
- **"I'm debugging concurrency issues"** -> [Idempotency and Atomicity](architecture/idempotency-and-atomicity.md)
- **"I'm configuring a deployment"** -> [Configuration Management](guides/configuration-management.md)
- **"I'm writing a custom handler"** -> [Worker Patterns](workers/patterns-and-practices.md)
- **"Why was this designed this way?"** -> [Tasker Core Tenets](principles/tasker-core-tenets.md), [CHRONOLOGY](CHRONOLOGY.md)

### By Technology

- **PostgreSQL/SQL** -> [Task Readiness & Execution](reference/task-and-step-readiness-and-execution.md)
- **Event Systems** -> [Events and Commands](architecture/events-and-commands.md)
- **State Machines** -> [States and Lifecycles](architecture/states-and-lifecycles.md)
- **Configuration (TOML)** -> [Configuration Management](guides/configuration-management.md)
- **Ruby/Python/TypeScript FFI** -> [Workers](workers/)
- **Circuit Breakers** -> [Circuit Breakers](architecture/circuit-breakers.md)

---

## Key Concepts

### What is Tasker Core?

Tasker Core is a high-performance workflow orchestration system built in Rust, designed for:
- **DAG-based workflow execution** with complex dependencies
- **PostgreSQL-native architecture** using PGMQ message queues
- **Event-driven coordination** with polling fallback for reliability
- **Multi-language worker support** (Rust native, Ruby, Python, TypeScript via FFI)

### Core Components

| Component | Purpose |
|-----------|---------|
| **Tasks** | Overall workflow instances with lifecycle management |
| **Workflow Steps** | Individual units of work with dependencies |
| **State Machines** | Dual state machines (Task + Step) for atomic transitions |
| **Event Systems** | Real-time coordination via PostgreSQL LISTEN/NOTIFY |
| **Message Queues** | PGMQ-based reliable message delivery |
| **Workers** | Autonomous step processors (Rust, Ruby, Python, TypeScript) |

---

## Contributing to Documentation

Found an issue or want to improve the docs?

1. **Quick fixes**: Edit the file directly and submit a PR
2. **Larger changes**: Open an issue first to discuss the approach
3. **New documentation**: Follow the structure above
4. **Updates**: Keep the "Last Updated" date current

---

## Getting Help

- **Code Issues**: See GitHub Issues
- **Architecture Questions**: Review [CHRONOLOGY](CHRONOLOGY.md) for historical context
- **Design Philosophy**: Check [Tasker Core Tenets](principles/tasker-core-tenets.md)
- **General Questions**: See [CLAUDE.md](../CLAUDE.md) for project context

---

**Ready to get started?** -> [Quick Start Guide](guides/quick-start.md)

**Need architectural context?** -> [Crate Architecture](architecture/crate-architecture.md)

**Looking for examples?** -> [Use Cases & Patterns](guides/use-cases-and-patterns.md)
