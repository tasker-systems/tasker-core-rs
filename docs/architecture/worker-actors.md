# Worker Actor-Based Architecture

**Last Updated**: 2025-12-04
**Audience**: Architects, Developers
**Status**: Active (TAS-69 Complete)
**Related Docs**: [Documentation Hub](README.md) | [Actor-Based Architecture](actors.md) | [Events and Commands](events-and-commands.md)

<- Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the worker actor-based architecture in tasker-worker, covering the lightweight Actor pattern that mirrors the orchestration architecture (TAS-46) for step execution and worker coordination.

## Overview

The tasker-worker system implements a **lightweight Actor pattern** that mirrors the orchestration architecture, providing:

1. **Actor Abstraction**: Worker components encapsulated as actors with clear lifecycle hooks
2. **Message-Based Communication**: Type-safe message handling via `Handler<M>` trait
3. **Central Registry**: `WorkerActorRegistry` for managing all worker actors
4. **Service Decomposition**: Focused services following single responsibility principle
5. **Lock-Free Statistics**: AtomicU64 counters for hot-path performance
6. **Direct Integration**: Command processor routes to actors without wrapper layers

This architecture provides consistency between orchestration and worker systems, enabling clearer code organization and improved maintainability.

## Implementation Status

**TAS-69 Complete**: All phases implemented and production-ready

- **Phase 1**: Core abstractions (traits, registry, lifecycle management)
- **Phase 2**: Service decomposition from 1575 LOC command_processor.rs
- **Phase 3**: All 5 primary actors implemented
- **Phase 4**: Command processor refactored to pure routing (~200 LOC)
- **Phase 5**: Stateless service design eliminating lock contention
- **Cleanup**: Lock-free AtomicU64 statistics, shared event system

**Current State**: Production-ready actor-based worker with 5 actors managing all step execution operations.

## Core Concepts

### What is a Worker Actor?

In the tasker-worker context, a **Worker Actor** is an encapsulated step execution component that:

- **Manages its own state**: Each actor owns its dependencies and configuration
- **Processes messages**: Responds to typed command messages via the `Handler<M>` trait
- **Has lifecycle hooks**: Initialization (`started`) and cleanup (`stopped`) methods
- **Is isolated**: Actors communicate through message passing
- **Is thread-safe**: All actors are `Send + Sync + 'static`

### Why Actors for Workers?

The previous architecture had a monolithic command processor:

```rust
// OLD: 1575 LOC monolithic command processor
pub struct WorkerProcessor {
    // All logic mixed together
    // RwLock contention on hot path
    // Two-phase initialization complexity
}
```

The actor pattern provides:

```rust
// NEW: Pure routing command processor (~200 LOC)
impl ActorCommandProcessor {
    async fn handle_command(&self, command: WorkerCommand) -> bool {
        match command {
            WorkerCommand::ExecuteStep { message, queue_name, resp } => {
                let msg = ExecuteStepMessage { message, queue_name };
                let result = self.actors.step_executor_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }
            // ... pure routing, no business logic
        }
    }
}
```

### Actor vs Service

**Services** (underlying business logic):
- Encapsulate step execution logic
- Stateless operations on step data
- Direct method invocation
- Examples: `StepExecutorService`, `FFICompletionService`, `WorkerStatusService`

**Actors** (message-based coordination):
- Wrap services with message-based interface
- Manage service lifecycle
- Asynchronous message handling
- Examples: `StepExecutorActor`, `FFICompletionActor`, `WorkerStatusActor`

The relationship:

```rust
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<StepExecutorService>,  // Wraps underlying service
}

#[async_trait]
impl Handler<ExecuteStepMessage> for StepExecutorActor {
    async fn handle(&self, msg: ExecuteStepMessage) -> TaskerResult<bool> {
        // Delegates to stateless service
        self.service.execute_step(msg.message, &msg.queue_name).await
    }
}
```

## Worker Actor Traits

### WorkerActor Trait

The base trait for all worker actors, defined in `tasker-worker/src/worker/actors/traits.rs`:

```rust
/// Base trait for all worker actors
///
/// Provides lifecycle management and context access for all actors in the
/// worker system. All actors must implement this trait to participate
/// in the actor registry and lifecycle management.
pub trait WorkerActor: Send + Sync + 'static {
    /// Returns the unique name of this actor
    fn name(&self) -> &'static str;

    /// Returns a reference to the system context
    fn context(&self) -> &Arc<SystemContext>;

    /// Called when the actor is started
    fn started(&mut self) -> TaskerResult<()> {
        tracing::info!(actor = %self.name(), "Actor started");
        Ok(())
    }

    /// Called when the actor is stopped
    fn stopped(&mut self) -> TaskerResult<()> {
        tracing::info!(actor = %self.name(), "Actor stopped");
        Ok(())
    }
}
```

### Handler<M> Trait

The message handling trait, enabling type-safe message processing:

```rust
/// Message handler trait for specific message types
#[async_trait]
pub trait Handler<M: Message>: WorkerActor {
    /// Handle a message asynchronously
    async fn handle(&self, msg: M) -> TaskerResult<M::Response>;
}
```

### Message Trait

The marker trait for command messages:

```rust
/// Marker trait for command messages
pub trait Message: Send + 'static {
    /// The response type for this message
    type Response: Send;
}
```

## WorkerActorRegistry

The central registry managing all worker actors, defined in `tasker-worker/src/worker/actors/registry.rs`:

### Structure

```rust
/// Registry managing all worker actors
#[derive(Clone)]
pub struct WorkerActorRegistry {
    /// System context shared by all actors
    context: Arc<SystemContext>,

    /// Worker ID for this registry
    worker_id: String,

    /// Step executor actor for step execution (TAS-69)
    pub step_executor_actor: Arc<StepExecutorActor>,

    /// FFI completion actor for handling step completions (TAS-69)
    pub ffi_completion_actor: Arc<FFICompletionActor>,

    /// Template cache actor for template management (TAS-69)
    pub template_cache_actor: Arc<TemplateCacheActor>,

    /// Domain event actor for event dispatching (TAS-69)
    pub domain_event_actor: Arc<DomainEventActor>,

    /// Worker status actor for health and status (TAS-69)
    pub worker_status_actor: Arc<WorkerStatusActor>,
}
```

### Initialization

All dependencies required at construction time (no two-phase initialization):

```rust
impl WorkerActorRegistry {
    pub async fn build(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
    ) -> TaskerResult<Self> {
        // Create actors with all dependencies upfront
        let mut step_executor_actor = StepExecutorActor::new(
            context.clone(),
            worker_id.clone(),
            task_template_manager.clone(),
            event_publisher,
            domain_event_handle,
        );

        // Call started() lifecycle hook
        step_executor_actor.started()?;

        // ... create other actors ...

        Ok(Self {
            context,
            worker_id,
            step_executor_actor: Arc::new(step_executor_actor),
            // ...
        })
    }
}
```

## Implemented Actors

### StepExecutorActor

Handles step execution from PGMQ messages and events.

**Location**: `tasker-worker/src/worker/actors/step_executor_actor.rs`

**Messages**:
- `ExecuteStepMessage` - Execute step from raw data
- `ExecuteStepWithCorrelationMessage` - Execute with FFI correlation
- `ExecuteStepFromPgmqMessage` - Execute from PGMQ message
- `ExecuteStepFromEventMessage` - Execute from event notification

**Delegation**: Wraps `StepExecutorService` (stateless, no locks)

**Purpose**: Central coordinator for all step execution, handles claiming, handler invocation, and result construction.

### FFICompletionActor

Handles step completion results from FFI handlers.

**Location**: `tasker-worker/src/worker/actors/ffi_completion_actor.rs`

**Messages**:
- `SendStepResultMessage` - Send result to orchestration
- `ProcessStepCompletionMessage` - Process completion with correlation

**Delegation**: Wraps `FFICompletionService`

**Purpose**: Forwards step execution results to orchestration queue, manages correlation for async FFI handlers.

### TemplateCacheActor

Manages task template caching and refresh.

**Location**: `tasker-worker/src/worker/actors/template_cache_actor.rs`

**Messages**:
- `RefreshTemplateCacheMessage` - Refresh cache for namespace

**Delegation**: Wraps `TaskTemplateManager`

**Purpose**: Maintains handler template cache for efficient step execution.

### DomainEventActor

Dispatches domain events after step completion.

**Location**: `tasker-worker/src/worker/actors/domain_event_actor.rs`

**Messages**:
- `DispatchDomainEventsMessage` - Dispatch events for completed step

**Delegation**: Wraps `DomainEventSystemHandle`

**Purpose**: Fire-and-forget domain event dispatch (never blocks step completion).

### WorkerStatusActor

Provides worker health and status reporting.

**Location**: `tasker-worker/src/worker/actors/worker_status_actor.rs`

**Messages**:
- `GetWorkerStatusMessage` - Get current worker status
- `HealthCheckMessage` - Perform health check
- `GetEventStatusMessage` - Get event integration status
- `SetEventIntegrationMessage` - Enable/disable event integration

**Features**:
- **Lock-free statistics** via `AtomicStepExecutionStats`
- AtomicU64 counters for `total_executed`, `total_succeeded`, `total_failed`
- Average execution time computed on read from `sum / count`

**Purpose**: Real-time health monitoring and statistics without lock contention.

## Lock-Free Statistics

The `WorkerStatusActor` uses atomic counters for lock-free statistics on the hot path:

```rust
/// Lock-free step execution statistics using atomic counters
#[derive(Debug)]
pub struct AtomicStepExecutionStats {
    total_executed: AtomicU64,
    total_succeeded: AtomicU64,
    total_failed: AtomicU64,
    total_execution_time_ms: AtomicU64,
}

impl AtomicStepExecutionStats {
    /// Record a successful step execution (lock-free)
    #[inline]
    pub fn record_success(&self, execution_time_ms: u64) {
        self.total_executed.fetch_add(1, Ordering::Relaxed);
        self.total_succeeded.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_ms.fetch_add(execution_time_ms, Ordering::Relaxed);
    }

    /// Record a failed step execution (lock-free)
    #[inline]
    pub fn record_failure(&self) {
        self.total_executed.fetch_add(1, Ordering::Relaxed);
        self.total_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current statistics
    pub fn snapshot(&self) -> StepExecutionStats {
        let total_executed = self.total_executed.load(Ordering::Relaxed);
        let total_time = self.total_execution_time_ms.load(Ordering::Relaxed);
        let average_execution_time_ms = if total_executed > 0 {
            total_time as f64 / total_executed as f64
        } else {
            0.0
        };
        StepExecutionStats {
            total_executed,
            total_succeeded: self.total_succeeded.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            average_execution_time_ms,
        }
    }
}
```

**Benefits**:
- Zero lock contention on step completion (every step calls `record_success` or `record_failure`)
- Sub-microsecond overhead per operation
- Consistent averages computed from totals

## Integration with Commands

### ActorCommandProcessor

The command processor provides pure routing to actors:

```rust
impl ActorCommandProcessor {
    async fn handle_command(&self, command: WorkerCommand) -> bool {
        match command {
            // Step Execution Commands -> StepExecutorActor
            WorkerCommand::ExecuteStep { message, queue_name, resp } => {
                let msg = ExecuteStepMessage { message, queue_name };
                let result = self.actors.step_executor_actor.handle(msg).await;
                let _ = resp.send(result);
                true
            }

            // Completion Commands -> FFICompletionActor
            WorkerCommand::SendStepResult { result, resp } => {
                let msg = SendStepResultMessage { result };
                let send_result = self.actors.ffi_completion_actor.handle(msg).await;
                let _ = resp.send(send_result);
                true
            }

            // Status Commands -> WorkerStatusActor
            WorkerCommand::HealthCheck { resp } => {
                let result = self.actors.worker_status_actor.handle(HealthCheckMessage).await;
                let _ = resp.send(result);
                true
            }

            WorkerCommand::Shutdown { resp } => {
                let _ = resp.send(Ok(()));
                false  // Exit command loop
            }
        }
    }
}
```

### FFI Completion Flow

Domain events are dispatched **after** successful orchestration notification:

```rust
async fn handle_ffi_completion(&self, step_result: StepExecutionResult) {
    // Record stats (lock-free)
    if step_result.success {
        self.actors.worker_status_actor
            .record_success(step_result.metadata.execution_time_ms as f64).await;
    } else {
        self.actors.worker_status_actor.record_failure().await;
    }

    // Send to orchestration FIRST
    let msg = SendStepResultMessage { result: step_result.clone() };
    match self.actors.ffi_completion_actor.handle(msg).await {
        Ok(()) => {
            // Domain events dispatched AFTER successful orchestration notification
            // Fire-and-forget - never blocks the worker
            self.actors.step_executor_actor
                .dispatch_domain_events(step_result.step_uuid, &step_result, None).await;
        }
        Err(e) => {
            // Don't dispatch domain events - orchestration wasn't notified
            tracing::error!("Failed to forward step completion to orchestration");
        }
    }
}
```

## Service Decomposition

Large services were decomposed from the monolithic command processor:

### StepExecutorService

```
services/step_execution/
├── mod.rs                  # Public API
├── service.rs              # StepExecutorService (~250 lines)
├── step_claimer.rs         # Step claiming logic
├── handler_invoker.rs      # Handler invocation
└── result_builder.rs       # Result construction
```

**Key Design**: Completely stateless service using `&self` methods. Wrapped in `Arc<StepExecutorService>` without any locks.

### FFICompletionService

```
services/ffi_completion/
├── mod.rs                  # Public API
├── service.rs              # FFICompletionService
└── result_sender.rs        # Orchestration result sender
```

### WorkerStatusService

```
services/worker_status/
├── mod.rs                  # Public API
└── service.rs              # WorkerStatusService
```

## Key Architectural Decisions

### 1. Stateless Services

Services use `&self` methods with no mutable state:

```rust
impl StepExecutorService {
    pub async fn execute_step(
        &self,  // Immutable reference
        message: PgmqMessage<SimpleStepMessage>,
        queue_name: &str,
    ) -> TaskerResult<bool> {
        // Stateless execution - no mutable state
    }
}
```

**Benefits**:
- Zero lock contention
- Maximum concurrency per worker
- Simplified reasoning about state

### 2. Constructor-Based Dependency Injection

All dependencies required at construction time:

```rust
pub async fn new(
    context: Arc<SystemContext>,
    worker_id: String,
    task_template_manager: Arc<TaskTemplateManager>,
    event_publisher: WorkerEventPublisher,        // Required
    domain_event_handle: DomainEventSystemHandle, // Required
) -> TaskerResult<Self>
```

**Benefits**:
- Compiler enforces complete initialization
- No "partially initialized" states
- Clear dependency graph

### 3. Shared Event System

Event publisher and subscriber share the same `WorkerEventSystem`:

```rust
let shared_event_system = event_system
    .unwrap_or_else(|| Arc::new(WorkerEventSystem::new()));
let event_publisher =
    WorkerEventPublisher::with_event_system(worker_id.clone(), shared_event_system.clone());

// Enable subscriber with same shared system
processor.enable_event_subscriber(Some(shared_event_system)).await;
```

**Benefits**:
- FFI handlers reliably receive step execution events
- No isolated event systems causing silent failures

### 4. Graceful Degradation

Domain events never fail step completion:

```rust
// dispatch_domain_events returns () not TaskerResult<()>
// Errors logged but never propagated
pub async fn dispatch_domain_events(
    &self,
    step_uuid: Uuid,
    result: &StepExecutionResult,
    metadata: Option<HashMap<String, serde_json::Value>>,
) {
    // Fire-and-forget with error logging
    // Channel full? Log and continue
    // Dispatch error? Log and continue
}
```

## Comparison with Orchestration Actors

| Aspect | Orchestration (TAS-46) | Worker (TAS-69) |
|--------|----------------------|-----------------|
| **Actor Count** | 4 actors | 5 actors |
| **Registry** | `ActorRegistry` | `WorkerActorRegistry` |
| **Base Trait** | `OrchestrationActor` | `WorkerActor` |
| **Message Trait** | `Handler<M>` | `Handler<M>` (same) |
| **Service Design** | Decomposed | Stateless |
| **Statistics** | N/A | Lock-free AtomicU64 |
| **LOC Reduction** | ~800 -> ~200 | 1575 -> ~200 |

## Benefits

### 1. Consistency with Orchestration

Same patterns and traits as TAS-46:
- Identical `Handler<M>` trait interface
- Similar registry lifecycle management
- Consistent message-based communication

### 2. Zero Lock Contention

- Stateless services eliminate RwLock on hot path
- AtomicU64 counters for statistics
- Maximum concurrent step execution

### 3. Type Safety

Messages and responses checked at compile time:
```rust
// Compile error if types don't match
impl Handler<ExecuteStepMessage> for StepExecutorActor {
    async fn handle(&self, msg: ExecuteStepMessage) -> TaskerResult<bool> {
        // Must return bool, not something else
    }
}
```

### 4. Testability

- Clear message boundaries for mocking
- Isolated actor lifecycle for unit tests
- 119 unit tests, 73 E2E tests passing

### 5. Maintainability

- 1575 LOC -> ~200 LOC command processor
- Focused services (<300 lines per file)
- Clear separation of concerns

## Detailed Analysis

For historical implementation details, see [TAS-69](https://linear.app/tasker-systems/issue/TAS-69) and the [TAS-69 ADR](../decisions/TAS-69-worker-decomposition.md).

## Summary

The worker actor-based architecture provides a consistent, type-safe foundation for step execution in tasker-worker. Key takeaways:

1. **Mirrors Orchestration**: Same patterns as TAS-46 for consistency
2. **Lock-Free Performance**: Stateless services and AtomicU64 counters
3. **Type Safety**: Compile-time verification of message contracts
4. **Pure Routing**: Command processor delegates without business logic
5. **Graceful Degradation**: Domain events never fail step completion
6. **Production Ready**: 119 unit tests, 73 E2E tests, full regression coverage

The architecture provides a solid foundation for high-throughput step execution while maintaining the proven reliability of the orchestration system.

---

<- Back to [Documentation Hub](README.md)
