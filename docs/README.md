# Tasker Core Documentation

**Last Updated**: 2025-10-10
**Project Status**: Production Ready
**Version**: 0.1.0

Welcome to the Tasker Core documentation hub. This page provides organized access to all documentation, guides, and reference materials for the Tasker workflow orchestration system.

---

## Quick Start

**New to Tasker Core?** Start here:

1. **[Quick Start Guide](quick-start.md)** - Get running in 5 minutes
2. **[Crate Architecture](crate-architecture.md)** - Understand the workspace structure
3. **[Use Cases & Patterns](use-cases-and-patterns.md)** - See when and how to use Tasker

**Looking for something specific?** Jump to a section below or use the search (Cmd/Ctrl+F).

---

## Documentation by Role

### For Architects

**Making design decisions?** These documents explain architectural patterns and trade-offs:

- **[Crate Architecture](crate-architecture.md)** - Workspace structure and crate responsibilities
- **[Events and Commands Architecture](events-and-commands.md)** - Event-driven coordination patterns
- **[States and Lifecycles](states-and-lifecycles.md)** - Dual state machine architecture
- **[Deployment Patterns](deployment-patterns.md)** - Hybrid, EventDriven, and PollingOnly modes
- **[Archive: Architectural Evolution](archive/architectural-evolution.md)** - Historical context and lessons learned
- **[Archive: Orchestration Principles](archive/orchestration-principles.md)** - Fundamental design patterns

**Performance & Scaling**:
- **[Benchmarks Overview](benchmarks/README.md)** - Performance targets and validation
- **[Archive: Performance Optimization](archive/performance-optimization.md)** - Optimization strategies

### For Developers

**Building with Tasker?** These guides help you be productive:

- **[Quick Start](quick-start.md)** - Get your first workflow running
- **[Crate Architecture](crate-architecture.md)** - Understand the codebase structure
- **[Use Cases & Patterns](use-cases-and-patterns.md)** - Practical workflow examples
- **[Task and Step Readiness](task-and-step-readiness-and-execution.md)** - SQL functions and execution logic
- **[Retry Semantics](retry-semantics.md)** - Understanding max_attempts and retryable flags
- **[Archive: Testing Methodologies](archive/testing-methodologies.md)** - Testing patterns and strategies
- **[Archive: Ruby Integration Lessons](archive/ruby-integration-lessons.md)** - FFI integration patterns

**API & Integration**:
- **Orchestration REST API** - See `tasker-orchestration/src/web/` for OpenAPI docs
- **Worker Integration** - See `tasker-worker/` README for handler development

### For Operators

**Running Tasker in production?** These docs cover deployment and monitoring:

- **[Deployment Patterns](deployment-patterns.md)** - Configuration and deployment modes
- **[Observability](observability/README.md)** - Metrics, logging, and monitoring
- **[Benchmarks](benchmarks/README.md)** - Performance validation and targets
- **[Archive: Deployment](archive/deployment.md)** - Production deployment insights

**Configuration Management**:
- Component-based TOML configuration (see `config/tasker/base/`)
- Environment-specific overrides (see `config/tasker/environments/`)
- Configuration validation via `cargo run --bin config-validator`

---

## Documentation by Topic

### Core Architecture

| Document | Description | Audience |
|----------|-------------|----------|
| **[Crate Architecture](crate-architecture.md)** | Workspace structure and crate roles | All |
| **[Events and Commands](events-and-commands.md)** | Event-driven coordination and command patterns | Architects, Developers |
| **[States and Lifecycles](states-and-lifecycles.md)** | Task and step state machines | All |
| **[Task Readiness & Execution](task-and-step-readiness-and-execution.md)** | SQL functions and orchestration logic | Developers, Architects |

### Guides & Patterns

| Document | Description | Audience |
|----------|-------------|----------|
| **[Quick Start](quick-start.md)** | Get running in 5 minutes | Developers |
| **[Use Cases & Patterns](use-cases-and-patterns.md)** | Practical workflow examples | All |
| **[Deployment Patterns](deployment-patterns.md)** | Deployment modes and configuration | Architects, Operators |
| **[Retry Semantics](retry-semantics.md)** | Retry configuration explained | Developers |

### Reference Documentation

| Document | Description | Audience |
|----------|-------------|----------|
| **[Observability](observability/README.md)** | Metrics, logging, and monitoring | Operators |
| **[Benchmarks](benchmarks/README.md)** | Performance testing and targets | Architects, Developers |
| **[Bug Reports](bug-reports/)** | Known issues and resolutions | All |
| **[Ticket Specs](ticket-specs/)** | Feature implementations and RFCs | Developers |

### Testing Documentation

| Document | Description | Audience |
|----------|-------------|----------|
| **[Testing Guide](testing/comprehensive-lifecycle-testing-guide.md)** | Lifecycle testing patterns | Developers |
| **[TAS-42 Implementation](testing/TAS-42-implementation-summary.md)** | Ruby worker testing approach | Developers |

---

## Complete Documentation Structure

```
tasker-core/
  README.md                          # Project overview (architecture & use cases)
  CLAUDE.md                          # AI assistant context and project guide

  docs/
    README.md                        # You are here (documentation hub)

    Quick Start & Guides
      quick-start.md                 # Get running in 5 minutes
      crate-architecture.md          # Workspace structure explained
      use-cases-and-patterns.md      # Practical workflow examples
      deployment-patterns.md         # Deployment modes and configuration

    Core Architecture
      events-and-commands.md         # Event-driven coordination
      states-and-lifecycles.md       # State machine architecture
      task-and-step-readiness-and-execution.md  # SQL functions

    Reference
      retry-semantics.md             # Retry configuration
      sccache-configuration.md       # Build caching

    Observability
      observability/
        README.md                    # Observability hub
        metrics-reference.md         # Complete metrics catalog
        metrics-verification.md
        logging-standards.md

      benchmarks/
        README.md                    # Benchmark suite overview
        api-benchmarks.md
        sql-benchmarks.md
        event-benchmarks.md
        e2e-benchmarks.md

    Testing
      testing/
        comprehensive-lifecycle-testing-guide.md
        TAS-42-implementation-summary.md

    Bug Reports
      bug-reports/
        2025-10-05-retry-eligibility-bug.md

    Feature Specifications
      ticket-specs/
        TAS-29/  # Observability & Benchmarking
        TAS-31/  # Production Resilience
        TAS-32/  # Unified Configuration
        TAS-33/  # UUID v7 Migration
        TAS-34/  # Component-based Config
        TAS-37/  # Race Condition Elimination
        TAS-40/  # Command Pattern
        TAS-41/  # Enhanced State Machines
        TAS-42/  # Ruby Worker Testing
        TAS-43/  # Worker Event System
        TAS-48/  # Task Staleness Relief
        TAS-49/  # Future enhancements

    Archive
      archive/
        README.md                    # Archive guide
        architectural-evolution.md   # Design decisions history
        orchestration-principles.md  # Fundamental patterns
        testing-methodologies.md     # Testing strategies
        performance-optimization.md  # Optimization patterns
        ruby-integration-lessons.md  # FFI patterns
```

---

## Key Concepts

### What is Tasker Core?

Tasker Core is a high-performance workflow orchestration system built in Rust, designed for:
- **DAG-based workflow execution** with complex dependencies
- **PostgreSQL-native architecture** using PGMQ message queues
- **Event-driven coordination** with polling fallback for reliability
- **Multi-language worker support** (Rust native, Ruby via FFI, Python/WASM planned)

### Core Components

| Component | Purpose |
|-----------|---------|
| **Tasks** | Overall workflow instances with lifecycle management |
| **Workflow Steps** | Individual units of work with dependencies |
| **State Machines** | Dual state machines (Task + Step) for atomic transitions |
| **Event Systems** | Real-time coordination via PostgreSQL LISTEN/NOTIFY |
| **Message Queues** | PGMQ-based reliable message delivery |
| **Workers** | Autonomous step processors (Rust, Ruby, etc.) |

### Workspace Crates

```
pgmq-notify/          # PGMQ wrapper with notification support
tasker-shared/        # Shared types, SQL functions, and utilities
tasker-orchestration/ # Task coordination and lifecycle management
tasker-worker/        # Step execution and handler integration
tasker-client/        # API client library and CLI tools
workers/ruby/ext/     # Ruby FFI bindings for Ruby workers
workers/rust/         # Rust native worker implementation
```

See **[Crate Architecture](crate-architecture.md)** for detailed explanations.

---

## Finding Documentation

### By Use Case

- **"I want to build a workflow"** -> [Use Cases & Patterns](use-cases-and-patterns.md)
- **"I need to understand the architecture"** -> [Crate Architecture](crate-architecture.md), [Events and Commands](events-and-commands.md)
- **"I'm debugging a state machine issue"** -> [States and Lifecycles](states-and-lifecycles.md)
- **"I need to optimize performance"** -> [Benchmarks](benchmarks/README.md), [Archive: Performance Optimization](archive/performance-optimization.md)
- **"I'm deploying to production"** -> [Deployment Patterns](deployment-patterns.md), [Observability](observability/README.md)
- **"I'm writing a custom handler"** -> [Use Cases & Patterns](use-cases-and-patterns.md), [Archive: Ruby Integration](archive/ruby-integration-lessons.md)
- **"I need to understand SQL functions"** -> [Task Readiness & Execution](task-and-step-readiness-and-execution.md)

### By Technology

- **PostgreSQL/SQL** -> [Task Readiness & Execution](task-and-step-readiness-and-execution.md)
- **Event Systems** -> [Events and Commands](events-and-commands.md)
- **State Machines** -> [States and Lifecycles](states-and-lifecycles.md)
- **PGMQ** -> [Events and Commands](events-and-commands.md), [Task Readiness & Execution](task-and-step-readiness-and-execution.md)
- **Ruby FFI** -> [Archive: Ruby Integration Lessons](archive/ruby-integration-lessons.md)
- **Testing** -> [Testing Guide](testing/comprehensive-lifecycle-testing-guide.md), [Archive: Testing Methodologies](archive/testing-methodologies.md)

---

## Work in Progress

The following documentation is planned or in progress:

- **Development Guide** - Contributing guidelines and development setup
- **API Reference** - Complete REST API documentation (see OpenAPI in code)
- **Migration Guides** - Upgrading between versions
- **Production Playbooks** - Operational runbooks for common scenarios
- **Worker Development Guide** - Complete guide to writing custom workers

---

## Documentation Standards

All Tasker Core documentation follows these standards:

### Document Headers
```markdown
# Document Title

**Last Updated**: YYYY-MM-DD
**Audience**: [Architects/Developers/Operators/All]
**Status**: [Active/Archived/Deprecated]
**Related Docs**: [Links to related documentation]
```

### Navigation
- All docs link back to this hub: `<- Back to [Documentation Hub](README.md)`
- Related documentation sections at bottom of each doc
- Consistent cross-referencing using relative paths

### Code Examples
- All code examples are tested and functional
- Include prerequisite setup steps
- Show expected output

---

## Contributing to Documentation

Found an issue or want to improve the docs?

1. **Quick fixes**: Edit the file directly and submit a PR
2. **Larger changes**: Open an issue first to discuss the approach
3. **New documentation**: Follow the standards above
4. **Updates**: Keep the "Last Updated" date current

### Documentation Principles

- **Audience-first**: Know who you're writing for
- **Example-driven**: Show, don't just tell
- **Maintain continuity**: Update related docs when making changes
- **Link liberally**: Help readers navigate to related content

---

## Getting Help

- **Code Issues**: See GitHub Issues
- **Architecture Questions**: Review [Archive: Architectural Evolution](archive/architectural-evolution.md)
- **Performance Issues**: Check [Benchmarks](benchmarks/README.md) and [Archive: Performance Optimization](archive/performance-optimization.md)
- **General Questions**: See [CLAUDE.md](../CLAUDE.md) for project context

---

**Ready to get started?** -> [Quick Start Guide](quick-start.md)

**Need architectural context?** -> [Crate Architecture](crate-architecture.md)

**Looking for examples?** -> [Use Cases & Patterns](use-cases-and-patterns.md)
