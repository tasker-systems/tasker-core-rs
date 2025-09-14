# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core** is a high-performance Rust implementation of workflow orchestration. This is a workspace containing multiple crates that together form a comprehensive orchestration system.

**Architecture**: PostgreSQL message queue (pgmq) based system where Rust handles orchestration coordination and workers process steps autonomously through queue polling.

## Workspace Structure

```
[workspace.members]
- pgmq-notify          # PGMQ wrapper with notification support
- tasker-client        # Client library for external interactions
- tasker-orchestration # Core orchestration logic and executors
- tasker-shared        # Shared types, traits, and utilities
- tasker-worker        # Worker implementation for step processing
- workers/ruby/ext/tasker_core # Ruby FFI bindings
- workers/rust         # Rust worker implementation
```

## Development Commands

### Core Development
```bash
# Build and test (ALWAYS use --all-features for full consistency)
cargo build --all-features
cargo test --all-features
cargo test --all-features --package <package-name>  # Test specific package
cargo test --all-features <test-name>               # Run specific test

# Linting and formatting
cargo clippy --all-targets --all-features
cargo fmt

# Fast compilation check
cargo check --all-features

# Documentation
cargo doc --all-features --open
```

### Database Operations
```bash
# Run migrations (requires DATABASE_URL)
cargo sqlx migrate run

# Test database connectivity
psql $DATABASE_URL -c "SELECT 1"

# Create test database with PGMQ
createdb tasker_rust_test
psql tasker_rust_test -c "CREATE EXTENSION IF NOT EXISTS pgmq;"
```

### Ruby Integration
```bash
cd workers/ruby
bundle install
bundle exec rake compile                    # Compile Ruby extension

# Run integration tests
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec spec/integration/ --format documentation

# Run specific workflow test
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec spec/integration/linear_workflow_integration_spec.rb
```

### Configuration Testing
```bash
# Validate configuration loading for each environment
TASKER_ENV=test cargo run --bin config-validator
TASKER_ENV=development cargo run --bin config-validator
TASKER_ENV=production cargo run --bin config-validator
```

### CI and Testing Commands

# Enhanced testing with nextest (parallel execution)
cargo nextest run --profile default                # Local testing with nextest
cargo nextest run --profile ci                     # Run CI profile tests locally
cargo nextest list                                 # List available tests

# Run tests for specific packages
cargo nextest run --package tasker-shared --package tasker-orchestration

# Integration testing with Docker Compose
docker compose -f docker/docker-compose.test.yml up -d --build
cargo nextest run --package tasker-core --test '*'
docker compose -f docker/docker-compose.test.yml down

# View test results and configuration
cargo nextest show-config version                  # Show nextest configuration
ls target/nextest/ci/junit.xml                    # Check JUnit output

### Coverage and Benchmarking
```bash
cargo llvm-cov --all-features                      # Generate coverage report
cargo llvm-cov --html --all-features              # HTML coverage report
cargo llvm-cov --lcov --output-path lcov.info --all-features  # LCOV for CI

cargo bench --all-features                         # Run benchmarks
```

### Docker Operations
```bash
# Start PostgreSQL with PGMQ
docker-compose up -d postgres

# Start full server (orchestration + API)
docker-compose --profile server up -d

# Clean up
docker-compose down
docker system prune -af                           # Clean all Docker resources
```

## High-Level Architecture

### Hybrid Event-Driven + Command Pattern Architecture

The system implements a sophisticated hybrid architecture combining:

1. **Event-Driven Systems**: Real-time coordination using PostgreSQL LISTEN/NOTIFY and PGMQ notifications
2. **Command Pattern**: Async command processors using tokio mpsc channels for orchestration and worker operations
3. **Deployment Modes**: PollingOnly, EventDrivenOnly, and Hybrid modes with fallback polling
4. **Queue-Based Communication**: PGMQ message queues for reliable step execution and result processing

### Dual State Machine Architecture

The system implements two coordinated state machines:

#### Task State Machine (12 states)
- **Initial**: `Pending`, `Initializing`
- **Active**: `EnqueuingSteps`, `StepsInProcess`, `EvaluatingResults`
- **Waiting**: `WaitingForDependencies`, `WaitingForRetry`, `BlockedByFailures`
- **Terminal**: `Complete`, `Error`, `Cancelled`, `ResolvedManually`

#### Workflow Step State Machine (8 states)
- **Pipeline**: `Pending`, `Enqueued`, `InProgress`, `EnqueuedForOrchestration`
- **Terminal**: `Complete`, `Error`, `Cancelled`, `ResolvedManually`

### Core Components

**OrchestrationCore** (`tasker-orchestration/src/core.rs`)
- Unified bootstrap system that initializes all orchestration components
- Manages database pools, circuit breakers, and executor pools
- Single entry point preventing configuration mismatches

**OrchestrationEventSystem** (`tasker-orchestration/src/orchestration/event_systems/`)
- Implements EventDrivenSystem trait for real-time coordination
- Manages queue listeners and fallback pollers
- Sends commands to orchestration command processor

**WorkerEventSystem** (`tasker-worker/src/worker/event_systems/`)
- Monitors namespace-specific queues for step execution
- Handles step claiming, execution, and result submission
- Integrates with Ruby/Rust handlers via FFI

**Command Processors** (TAS-40)
- Replace complex polling coordinators with simple command processing
- ~100 lines vs 1000+ lines of complex systems
- Atomic operations through proper delegation

**Finalization System** (`tasker-orchestration/src/finalization_claimer.rs`)
- Atomic SQL-based claiming via `claim_task_for_finalization` function
- Prevents race conditions when multiple processors attempt to finalize same task
- Comprehensive metrics for observability

**Circuit Breaker System** (`tasker-shared/src/resilience/`)
- Protects database and messaging operations from cascading failures
- Configurable per executor type via TOML configuration
- Automatic recovery with exponential backoff

### Event Flow and Communication

#### Complete Task Execution Flow
1. **Task Initialization**: Client sends TaskRequestMessage via pgmq_send_with_notify
2. **Step Discovery**: Orchestration discovers ready steps using SQL functions
3. **Step Enqueueing**: Ready steps enqueued to namespace queues with notifications
4. **Worker Processing**: Workers claim steps, execute handlers, submit results
5. **Result Processing**: Orchestration processes results, discovers next steps
6. **Task Completion**: All steps complete, task finalized atomically

#### PGMQ Notification Channels
- `pgmq_message_ready.orchestration`: Orchestration queue messages
- `pgmq_message_ready.{namespace}`: Worker namespace queue messages
- `pgmq_message_ready`: Global fallback channel
- `pgmq_queue_created`: Queue creation notifications

### SQL Function Architecture

The system relies heavily on PostgreSQL functions for complex operations:

#### Critical Problem-Solving Functions

**pgmq_read_specific_message()**: Prevents message claiming race conditions
- Atomic claim with visibility timeout
- Returns empty if already claimed
- Eliminates "message not found" errors

**transition_task_state_atomic()**: Enables distributed orchestration
- Compare-and-swap pattern with processor ownership
- Prevents concurrent task processing
- Maintains complete audit trail

**get_step_readiness_status()**: Complex dependency resolution
- Validates parent completion status
- Calculates exponential backoff timing
- Enforces retry limits and conditions

**get_next_ready_tasks()**: Efficient work distribution
- Batch task discovery with priority
- Dynamic priority based on age and retries
- Prevents task starvation

### Configuration System

Component-based TOML configuration with environment-specific overrides:

```
config/tasker/base/
├── auth.toml              # Authentication settings
├── circuit_breakers.toml  # Resilience configuration
├── database.toml          # Connection pool settings
├── engine.toml            # Core engine parameters
├── executor_pools.toml    # Pool sizing and behavior
├── orchestration.toml     # Orchestration settings
├── pgmq.toml             # Message queue configuration
├── query_cache.toml      # Caching configuration
├── system.toml           # System-level settings
└── telemetry.toml        # Metrics and logging

config/tasker/environments/{test,development,production}/
└── <component>.toml      # Environment-specific overrides
```

Environment detection order:
1. `TASKER_ENV` environment variable
2. Default to "development"

### Message Queue Architecture

PostgreSQL-based queues with namespace isolation:
- `orchestration_step_results`: Step completion results from workers
- `orchestration_task_requests`: New task initialization requests
- `orchestration_task_finalization`: Task finalization notifications
- `{namespace}_queue`: Namespace-specific step execution queues

### Key Database Features

**UUID v7 Support**: Time-ordered UUIDs for distributed systems
- Migration: `20250818000001_add_finalization_claiming.sql`
- Atomic operations via SQL functions

**PGMQ Extension**: PostgreSQL message queue for reliable messaging
- Automatic retry and visibility timeout
- Transactional message processing
- Atomic send + notify via pgmq_send_with_notify

**TAS-41 Enhancements**: Processor ownership tracking
- 12 comprehensive task states for fine-grained control
- Atomic state transitions with ownership validation
- Prevents concurrent processing by multiple orchestrators

## Deployment Modes

### PollingOnly Mode
- Traditional polling-based coordination
- No event listeners or real-time notifications
- Reliable fallback for restricted environments
- Higher latency but guaranteed operation

### EventDrivenOnly Mode
- Pure event-driven using PostgreSQL LISTEN/NOTIFY
- Real-time response to database changes
- Lowest latency for step discovery
- Requires reliable PostgreSQL connections

### Hybrid Mode (Recommended)
- Primary event-driven with polling fallback
- Best of both worlds: real-time + reliability
- Automatic fallback during connection issues
- Production-ready with resilience guarantees

## Testing Strategy

### Test Execution Patterns
```bash
# Run all tests with full output
cargo test --all-features -- --nocapture

# Run tests for specific module
cargo test --all-features orchestration::

# Run with specific log level
RUST_LOG=debug cargo test --all-features

# Run integration tests only
cargo test --all-features --test '*integration*'

# Run with release optimizations
cargo test --release --all-features
```

### Integration Testing with Docker Compose

The system uses a simplified Docker Compose-based integration testing approach that assumes services are already running.

#### Prerequisites
```bash
# Start test services before running integration tests
docker-compose -f docker/docker-compose.test.yml up --build -d

# Verify services are healthy
curl http://localhost:8080/health  # Orchestration service
curl http://localhost:8081/health  # Worker service
```

#### Integration Test Types

**Full E2E Tests** - Tests complete workflow execution:
```bash
cargo test --test rust_worker_e2e_integration_tests
```

**API-Only Tests** - Tests orchestration API without worker:
```bash
cargo test --test simple_integration_tests
```

#### Environment Configuration

Control service endpoints via environment variables:
```bash
# Custom service URLs
export TASKER_TEST_ORCHESTRATION_URL="http://localhost:9080"
export TASKER_TEST_WORKER_URL="http://localhost:9081"

# Skip health checks (useful for CI)
export TASKER_TEST_SKIP_HEALTH_CHECK="true"

# Custom health check timeouts
export TASKER_TEST_HEALTH_TIMEOUT="30"
export TASKER_TEST_HEALTH_RETRY_INTERVAL="1"
```

#### Integration Test Manager Usage

```rust
use tasker_core::test_helpers::DockerIntegrationManager;

// Full integration test with worker
let manager = DockerIntegrationManager::setup().await?;

// API-only test (no worker needed)
let manager = DockerIntegrationManager::setup_orchestration_only().await?;

// Access configured clients
let task_response = manager.orchestration_client.create_task(request).await?;
```

### Test Categories
- **Unit Tests**: In-module tests for isolated functionality
- **Integration Tests**: Docker Compose-based integration testing
- **Ruby Integration**: `workers/ruby/spec/integration/` for FFI validation

## Troubleshooting

### Common Issues

**Database Connection Errors**
- Verify PostgreSQL is running: `pg_isready`
- Check PGMQ extension: `psql $DATABASE_URL -c "SELECT * FROM pgmq.meta"`
- Ensure correct DATABASE_URL format: `postgresql://user:pass@localhost/db`

**Configuration Loading Issues**
- Check TASKER_ENV is set correctly
- Verify TOML syntax: `cargo run --bin config-validator`
- Look for environment-specific overrides in `config/tasker/environments/`

**Test Failures**
- Run with `--nocapture` for full output
- Check for stale test database: `cargo sqlx migrate run`
- Verify all features enabled: `--all-features` flag

**Ruby Extension Compilation**
- Ensure Rust toolchain installed: `rustc --version`
- Clean and rebuild: `cd workers/ruby && rake clean && rake compile`
- Check magnus version compatibility in Cargo.toml

**Race Condition Issues**
- Check processor ownership in task_transitions table
- Verify atomic SQL functions are installed
- Look for "unclear state" logs indicating contention

**Event System Issues**
- Verify PostgreSQL LISTEN/NOTIFY is working
- Check deployment mode configuration
- Monitor fallback poller metrics for missed events

## Key Implementation Details

### State Machine Integration Points

**Task <-> Step Coordination**
1. Task initialization discovers ready steps
2. Task enqueues discovered steps to worker queues
3. Task monitors step completion via events
4. Task processes step results and discovers next steps
5. Task completes when all steps are complete

**Processor Ownership (TAS-41)**
- States requiring ownership: `Initializing`, `EnqueuingSteps`, `StepsInProcess`, `EvaluatingResults`
- Processor UUID stored in `tasker_task_transitions.processor_uuid`
- Atomic ownership claiming prevents concurrent processing
- Ownership validated on each transition attempt

### Command Types

**Orchestration Commands**
- Task lifecycle: `InitializeTask`, `ProcessStepResult`, `FinalizeTask`
- Message processing: `ProcessStepResultFromMessage`, `InitializeTaskFromMessage`
- Event processing: `ProcessStepResultFromMessageEvent`, `ProcessTaskReadiness`
- System operations: `GetProcessingStats`, `HealthCheck`, `Shutdown`

**Worker Commands**
- Step execution: `ExecuteStep`, `ExecuteStepWithCorrelation`
- Result processing: `SendStepResult`, `ProcessStepCompletion`
- Event integration: `ExecuteStepFromMessage`, `ExecuteStepFromEvent`
- System operations: `GetWorkerStatus`, `SetEventIntegration`, `HealthCheck`

### Critical SQL Functions

**Step Readiness Analysis**
- `get_step_readiness_status()`: Comprehensive dependency analysis
- `calculate_backoff_delay()`: Exponential backoff calculation
- `check_step_dependencies()`: Parent completion validation
- `get_ready_steps()`: Parallel execution candidate discovery

**DAG Operations**
- `detect_cycle()`: Cycle detection using recursive CTEs
- `calculate_dependency_levels()`: Topological depth calculation
- `get_step_transitive_dependencies()`: Full dependency tree traversal

**State Management**
- `transition_task_state_atomic()`: Atomic transitions with ownership
- `get_current_task_state()`: Current state resolution
- `finalize_task_completion()`: Task completion orchestration

**Analytics and Monitoring**
- `get_analytics_metrics()`: Comprehensive system analytics
- `get_system_health_counts()`: System-wide health metrics
- `get_slowest_steps()`: Performance optimization analysis

### Completed Features (Production Ready)

**TAS-41**: Enhanced state machines with processor ownership
- 12 task states for fine-grained control
- Atomic state transitions preventing race conditions
- Comprehensive audit trail with transition metadata

**TAS-40**: Command pattern replacement
- Simplified architecture replacing complex coordinators
- Async command processing via tokio mpsc channels
- Preserved observability through delegated components

**TAS-37**: Race condition elimination with dynamic orchestration
- Atomic finalization claiming prevents duplicate processing
- Auto-scaling executor pools based on load
- Context-aware health monitoring

**TAS-34**: Component-based configuration architecture
- 10 focused TOML files replacing monolithic config
- Environment-specific overrides with validation
- Type-safe configuration management

**TAS-33**: Circuit breaker integration
- Configurable protection for all critical paths
- Seamless fallback between protected/unprotected clients
- Comprehensive metrics for monitoring

### Available Specifications

Future implementation opportunities with detailed specs:
- **TAS-39**: Health check endpoint integration (`docs/ticket-specs/TAS-39.md`)

## Performance Targets

Achieved performance improvements:
- 10-100x faster dependency resolution vs PostgreSQL functions
- <1ms overhead per step coordination
- >10k events/sec cross-language processing
- Zero race conditions via atomic claiming

## Binary Targets

### Available Binaries
- `config-validator`: Validates configuration for all environments
- `tasker-server`: Combined orchestration + web API server (Docker deployment)

### Running Binaries
```bash
# Configuration validator
cargo run --bin config-validator

# Server (requires Docker)
docker build -t tasker-server .
docker run -p 3000:8080 -e DATABASE_URL=$DATABASE_URL tasker-server
```

## Architecture Documentation

For deep dives into specific architectural aspects:
- **Event Systems**: `docs/events-and-commands.md` - Event-driven architecture and command patterns
- **State Machines**: `docs/states-and-lifecycles.md` - Task and step state transitions
- **SQL Functions**: `docs/task-and-step-readiness-and-execution.md` - Database-level orchestration logic
