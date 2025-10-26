# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Last Updated**: 2025-10-26

## Project Overview

**tasker-core** is a high-performance Rust implementation of workflow orchestration. This is a workspace containing multiple crates that together form a comprehensive orchestration system.

**Architecture**: PostgreSQL message queue (pgmq) based system with actor-based coordination. Rust handles orchestration through a lightweight actor pattern, while workers process steps autonomously through queue polling with event-driven coordination.

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

# Tool Installation (using custom install-tools action with hybrid approach)
# CI workflows use: .github/actions/install-tools with proven working commands

# NOTE: sccache temporarily disabled due to GitHub Actions cache service issues
# See docs/sccache-configuration.md for planned sccache configuration

# For local development, install tools using the same hybrid approach:
cargo binstall cargo-nextest --secure                                          # Fast, secure binary install
cargo install sqlx-cli --no-default-features --features native-tls,postgres   # Proven reliable command
cargo install cargo-audit --locked                                             # Reproducible builds

# Enhanced testing with nextest (parallel execution)
cargo nextest run --profile default                # Local testing with nextest
cargo nextest run --profile ci                     # Run CI profile tests locally (generates JUnit XML)
cargo nextest run --profile local                  # Local development (fail-fast enabled)
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

### Actor Pattern Architecture (TAS-46 Complete)

The system implements a lightweight actor pattern for orchestration coordination:

**ActorRegistry** (`tasker-orchestration/src/actors/registry.rs`)
- Central registry managing 4 production-ready actors
- Lifecycle management with `started()` and `stopped()` hooks
- Shared system context with database pools and circuit breakers
- Type-safe message passing via `Handler<M>` trait

**Four Core Actors** (All Production Ready):
1. **TaskRequestActor**: Handles task initialization and namespace validation
2. **ResultProcessorActor**: Processes step execution results and orchestrates next steps
3. **StepEnqueuerActor**: Manages batch enqueueing of ready steps to worker queues
4. **TaskFinalizerActor**: Handles task finalization with atomic claiming and completion logic

**Actor Traits** (`tasker-orchestration/src/actors/traits.rs`):
```rust
pub trait OrchestrationActor: Send + Sync {
    fn started(&self) -> impl Future<Output = TaskerResult<()>> + Send;
    fn stopped(&self) -> impl Future<Output = TaskerResult<()>> + Send;
}

pub trait Handler<M: Message>: OrchestrationActor {
    type Response: Send;
    fn handle(&self, msg: M) -> impl Future<Output = Self::Response> + Send;
}
```

**Service Decomposition** (TAS-46 Phase 7):
- Large services (800-900 lines) decomposed into focused components
- Each service file now <300 lines with single responsibility
- Example: `task_finalization/` split into 6 focused files:
  - `service.rs`: Main TaskFinalizer (~200 lines)
  - `completion_handler.rs`: Task completion logic
  - `event_publisher.rs`: Lifecycle event publishing
  - `execution_context_provider.rs`: Context fetching
  - `state_handlers.rs`: State-specific handling

### Core Components

**OrchestrationCore** (`tasker-orchestration/src/core.rs`)
- Unified bootstrap system that initializes all orchestration components
- Builds ActorRegistry with all 4 actors
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

**Command Processors** (TAS-40, TAS-46)
- Pure routing to actors without complex logic
- Direct actor calls via message passing
- ~100 lines vs 1000+ lines of complex systems
- Atomic operations through actor delegation

**Finalization System** (`tasker-orchestration/src/finalization_claimer.rs`)
- Atomic SQL-based claiming via `claim_task_for_finalization` function
- Prevents race conditions when multiple processors attempt to finalize same task
- Comprehensive metrics for observability

**Circuit Breaker System** (`tasker-shared/src/resilience/`)
- Protects database and messaging operations from cascading failures
- Configurable per executor type via TOML configuration
- Automatic recovery with exponential backoff

### Event Flow and Communication

#### Complete Task Execution Flow (Actor-Based)
1. **Task Initialization**: Client sends TaskRequestMessage via pgmq_send_with_notify
   - Command processor routes to **TaskRequestActor**
   - Actor delegates to TaskInitializer service for initialization
2. **Step Discovery**: Orchestration discovers ready steps using SQL functions
   - **StepEnqueuerActor** coordinates batch processing
3. **Step Enqueueing**: Ready steps enqueued to namespace queues with notifications
   - Actor delegates to StepEnqueuerService for batch operations
4. **Worker Processing**: Workers claim steps, execute handlers, submit results
   - Results sent back via orchestration_step_results queue
5. **Result Processing**: Orchestration processes results, discovers next steps
   - **ResultProcessorActor** handles step result processing
   - Actor delegates to OrchestrationResultProcessor service
6. **Task Completion**: All steps complete, task finalized atomically
   - **TaskFinalizerActor** handles finalization with atomic claiming
   - Actor delegates to TaskFinalizer service components

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

### MPSC Channel Strategy (TAS-51)

The system uses bounded MPSC channels with configuration-driven capacity management for all async communication.

**Golden Rules**:
1. **All channels MUST be bounded** - `unbounded_channel()` is forbidden
2. **All channels MUST be configured via TOML** - No hard-coded buffer sizes
3. **All channels MUST have monitoring** - Use ChannelMonitor for observability
4. **All channels MUST handle backpressure** - Document overflow behavior
5. **All configurations MUST have environment overrides** - Test with small buffers

**Configuration Structure**:
```
config/tasker/base/mpsc_channels.toml              # Base channel buffer sizes
config/tasker/environments/test/mpsc_channels.toml # Small buffers (100-500)
config/tasker/environments/development/mpsc_channels.toml # Medium buffers (500-1000)
config/tasker/environments/production/mpsc_channels.toml  # Large buffers (2000-50000)
```

**Channel Categories**:
- **Orchestration Channels**: Command processing (1000-5000), PGMQ notifications (10000-50000)
- **Task Readiness Channels**: Event coordination (1000-5000)
- **Worker Channels**: Command processing (1000-2000), FFI events (1000-2000)
- **Shared Channels**: Event publishing (5000-10000), Ruby FFI (1000-2000)

**Critical Implementation Detail**:
Environment override files MUST use full `[mpsc_channels.*]` prefix:
```toml
# ✅ CORRECT
[mpsc_channels.orchestration.command_processor]
command_buffer_size = 5000

# ❌ WRONG - creates conflicting top-level key
[orchestration.command_processor]
command_buffer_size = 5000
```

**Channel Monitoring**:
- `ChannelMonitor` tracks real-time usage and saturation
- Warns at 80% capacity by default
- Exposes OpenTelemetry metrics for production monitoring
- All channel creates must integrate monitoring

**Backpressure Strategies**:
- **Block**: Default for critical messages (task results, commands)
- **Drop with metrics**: For non-critical events (internal notifications)
- **Timeout**: Balance between blocking and dropping
- **Retry with backoff**: Important messages with temporary saturation

**Documentation**:
- **ADR**: `docs/architecture-decisions/TAS-51-bounded-mpsc-channels.md` - Design decisions and rationale
- **Operations**: `docs/operations/mpsc-channel-tuning.md` - Monitoring, alerting, and capacity planning
- **Development**: `docs/development/mpsc-channel-guidelines.md` - Step-by-step creation guide and patterns

**Quick Start for New Channels**:
1. Add configuration to `config/tasker/base/mpsc_channels.toml`
2. Add environment overrides with `[mpsc_channels.*]` prefix
3. Add Rust type in `tasker-shared/src/config/mpsc_channels.rs`
4. Create channel using configuration: `mpsc::channel(config.buffer_size)`
5. Integrate ChannelMonitor for observability
6. Implement backpressure strategy
7. Add tests covering backpressure behavior

### Actor-Based Module Organization

The orchestration crate is organized around the actor pattern (TAS-46):

```
tasker-orchestration/src/orchestration/
├── actors/                          # Actor pattern implementation
│   ├── traits.rs                    # OrchestrationActor, Handler<M>, Message traits
│   ├── registry.rs                  # ActorRegistry with lifecycle management
│   ├── task_request_actor.rs        # Task initialization actor
│   ├── result_processor_actor.rs    # Result processing actor
│   ├── step_enqueuer_actor.rs       # Step enqueueing actor
│   └── task_finalizer_actor.rs      # Task finalization actor
│
├── hydration/                       # Message hydration layer (Phase 4)
│   ├── step_result_hydrator.rs      # PGMQ message → StepExecutionResult
│   ├── task_request_hydrator.rs     # PGMQ message → TaskRequestMessage
│   └── finalization_hydrator.rs     # PGMQ message → task_uuid
│
├── lifecycle/                       # Decomposed services (Phase 7)
│   ├── task_initialization/         # Task init service components
│   ├── result_processing/           # Result processing components
│   ├── step_enqueuer_services/      # Step enqueuer components
│   └── task_finalization/           # Task finalization components
│       ├── service.rs                # Main TaskFinalizer (~200 lines)
│       ├── completion_handler.rs
│       ├── event_publisher.rs
│       ├── execution_context_provider.rs
│       └── state_handlers.rs
│
├── event_systems/                   # Event-driven coordination
├── command_processor.rs             # Pure routing to actors
└── core.rs                          # Bootstrap with ActorRegistry
```

**Key Architectural Principles**:
- **Actors**: Message-based coordination with type-safe interfaces
- **Hydration**: PGMQ message transformation layer
- **Services**: Focused components with single responsibility (<300 lines)
- **Command Processor**: Pure routing without business logic
- **No Wrapper Layers**: Direct actor calls from command processor

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

**TAS-41/TAS-54 State Management**: Processor tracking and automatic recovery
- 12 comprehensive task states for fine-grained control
- Atomic state transitions with audit trail (TAS-41)
- Processor UUID tracked for debugging, not enforced (TAS-54)
- Tasks automatically recover after orchestrator crashes

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

**Processor Ownership (TAS-41, TAS-54)**
- **TAS-54 Update**: Processor ownership enforcement removed for stale task recovery
- Processor UUID stored in `tasker_task_transitions.processor_uuid` for audit trail only
- No longer enforces ownership - any orchestrator can process any task
- Idempotency guaranteed by state guards, transaction atomicity, and atomic claiming
- Tasks automatically recover when orchestrator restarts with different UUID

### Command Types and Actor Mapping

**Orchestration Commands** (Routed to Actors):

Task lifecycle commands → Actor message handlers:
- `InitializeTask` → **TaskRequestActor** via `ProcessTaskRequestMessage`
- `ProcessStepResult` → **ResultProcessorActor** via `ProcessStepResultMessage`
- `FinalizeTask` → **TaskFinalizerActor** via `FinalizeTaskMessage`

Message processing commands:
- `ProcessStepResultFromMessage` → Hydration layer + ResultProcessorActor
- `InitializeTaskFromMessage` → Hydration layer + TaskRequestActor

Event processing commands:
- `ProcessStepResultFromMessageEvent` → Event system integration
- `ProcessTaskReadiness` → StepEnqueuerActor coordination

System operations (direct handling):
- `GetProcessingStats`, `HealthCheck`, `Shutdown`

**Example Actor Integration**:
```rust
// Command processor routes to actors
async fn handle_finalize_task(&self, task_uuid: Uuid)
    -> TaskerResult<TaskFinalizationResult> {
    // Direct actor call with typed message
    let msg = FinalizeTaskMessage { task_uuid };
    let result = self.actors.task_finalizer_actor.handle(msg).await?;

    Ok(TaskFinalizationResult::Success {
        task_uuid: result.task_uuid,
        final_status: format!("{:?}", result.action),
        completion_time: Some(chrono::Utc::now()),
    })
}
```

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

**TAS-54**: Processor ownership removal for automatic stale task recovery (October 2025)
- Removed processor UUID enforcement from TaskStateMachine transitions
- Changed to audit-only mode - processor UUID tracked but not enforced
- Tasks automatically recover when orchestrator crashes and restarts with different UUID
- Idempotency guaranteed by state guards, transaction atomicity, unique constraints, and atomic claiming
- Comprehensive audit trail maintained via processor UUID in transitions
- All 377 tests passing after ownership removal
- Zero manual intervention needed for stale task recovery

**TAS-51**: Bounded MPSC channels with configuration-driven capacity (October 2025)
- Migrated all unbounded channels to bounded with configurable capacities
- Configuration-driven buffer sizing with environment-specific overrides
- Separation of concerns: infrastructure (sizing) vs business logic (retry behavior)
- ChannelMonitor integration for real-time observability
- Explicit backpressure strategies for all channel overflow scenarios
- Comprehensive documentation: ADR, operational runbook, developer guidelines
- Zero memory growth risk from unbounded channels

**TAS-46**: Actor pattern implementation (Phases 1-7)
- Lightweight actor pattern with 4 production-ready actors
- Message-based type-safe communication via `Handler<M>` trait
- Service decomposition: 800-900 line files → focused <300 line components
- Direct actor integration in command processor (no wrapper layers)
- Zero breaking changes: all refactoring internal, full test coverage
- Clean architecture: pure routing, hydration layer, single responsibility

**TAS-41**: Enhanced state machines with processor tracking
- 12 task states for fine-grained control
- Atomic state transitions preventing race conditions
- Comprehensive audit trail with transition metadata
- **Note**: Processor ownership enforcement removed in TAS-54 (audit-only mode)

**TAS-40**: Command pattern replacement
- Simplified architecture replacing complex coordinators
- Async command processing via tokio mpsc channels
- Preserved observability through delegated components
- Enhanced by TAS-46 actor pattern for cleaner delegation

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
- **Actor Pattern**: `docs/actors.md` - Actor-based architecture, TAS-46 implementation details
- **Event Systems**: `docs/events-and-commands.md` - Event-driven architecture and command patterns
- **Crate Structure**: `docs/crate-architecture.md` - Workspace organization and public APIs
- **State Machines**: `docs/states-and-lifecycles.md` - Task and step state transitions
- **SQL Functions**: `docs/task-and-step-readiness-and-execution.md` - Database-level orchestration logic
- **MPSC Channels**:
  - `docs/architecture-decisions/TAS-51-bounded-mpsc-channels.md` - ADR with design decisions and rationale
  - `docs/operations/mpsc-channel-tuning.md` - Operational runbook for monitoring and capacity planning
  - `docs/development/mpsc-channel-guidelines.md` - Developer guidelines for creating and using channels
