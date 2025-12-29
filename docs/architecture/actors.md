# Actor-Based Architecture

**Last Updated**: 2025-12-04
**Audience**: Architects, Developers
**Status**: Active (TAS-46 Complete - Phases 1-7)
**Related Docs**: [Documentation Hub](README.md) | [Worker Actor Architecture](worker-actors.md) | [Events and Commands](events-and-commands.md) | [States and Lifecycles](states-and-lifecycles.md)

← Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the actor-based architecture in tasker-core, covering the lightweight Actor pattern that formalizes the relationship between Commands and Lifecycle Components. This architecture replaces imperative delegation with message-based actor coordination.

## Overview

The tasker-core system implements a **lightweight Actor pattern** inspired by frameworks like Actix, but designed specifically for our orchestration needs without external dependencies. The architecture provides:

1. **Actor Abstraction**: Lifecycle components encapsulated as actors with clear lifecycle hooks
2. **Message-Based Communication**: Type-safe message handling via Handler<M> trait
3. **Central Registry**: ActorRegistry for managing all orchestration actors
4. **Service Decomposition**: Focused components following single responsibility principle
5. **Direct Integration**: Command processor calls actors directly without wrapper layers

This architecture eliminates inconsistencies in lifecycle component initialization, provides type-safe message handling, and creates a clear separation between command processing and business logic execution.

## Implementation Status

**TAS-46 Complete**: All phases implemented and production-ready

- ✅ **Phase 1**: Core abstractions (traits, registry, lifecycle management)
- ✅ **Phase 2-3**: All 4 primary actors implemented
- ✅ **Phase 4-6**: Message hydration, module reorganization
- ✅ **Phase 7**: Service decomposition into focused components
- ✅ **Cleanup**: Direct actor integration, removed intermediate wrappers

**Current State**: Production-ready actor-based orchestration with 4 actors managing all lifecycle operations.

## Core Concepts

### What is an Actor?

In the tasker-core context, an **Actor** is an encapsulated lifecycle component that:

- **Manages its own state**: Each actor owns its dependencies and configuration
- **Processes messages**: Responds to typed command messages via the Handler<M> trait
- **Has lifecycle hooks**: Initialization (started) and cleanup (stopped) methods
- **Is isolated**: Actors communicate through message passing
- **Is thread-safe**: All actors are Send + Sync + 'static

### Why Actors?

The previous architecture had several inconsistencies:

```rust
// OLD: Inconsistent initialization patterns
pub struct TaskInitializer {
    // Constructor pattern
}

pub struct TaskFinalizer {
    // Builder pattern with new()
}

pub struct StepEnqueuer {
    // Factory pattern with create()
}
```

The actor pattern provides consistency:

```rust
// NEW: Consistent actor pattern
impl OrchestrationActor for TaskRequestActor {
    fn name(&self) -> &'static str { "TaskRequestActor" }
    fn context(&self) -> &Arc<SystemContext> { &self.context }
    fn started(&mut self) -> TaskerResult<()> { /* initialization */ }
    fn stopped(&mut self) -> TaskerResult<()> { /* cleanup */ }
}
```

### Actor vs Service

**Services** (underlying business logic):
- Encapsulate business logic
- Stateless operations on domain models
- Direct method invocation
- Examples: TaskFinalizer, StepEnqueuerService, OrchestrationResultProcessor

**Actors** (message-based coordination):
- Wrap services with message-based interface
- Manage service lifecycle
- Asynchronous message handling
- Examples: TaskRequestActor, ResultProcessorActor, StepEnqueuerActor, TaskFinalizerActor

The relationship:

```rust
pub struct TaskFinalizerActor {
    context: Arc<SystemContext>,
    service: TaskFinalizer,  // Wraps underlying service
}

impl Handler<FinalizeTaskMessage> for TaskFinalizerActor {
    type Response = FinalizationResult;

    async fn handle(&self, msg: FinalizeTaskMessage) -> TaskerResult<Self::Response> {
        // Delegates to service
        self.service.finalize_task(msg.task_uuid).await
    }
}
```

## Actor Traits

### OrchestrationActor Trait

The base trait for all orchestration actors, defined in `tasker-orchestration/src/actors/traits.rs`:

```rust
/// Base trait for all orchestration actors
///
/// Provides lifecycle management and context access for all actors in the
/// orchestration system. All actors must implement this trait to participate
/// in the actor registry and lifecycle management.
///
/// # Lifecycle
///
/// 1. **Construction**: Actor is created by ActorRegistry
/// 2. **Initialization**: `started()` is called during registry build
/// 3. **Operation**: Actor processes messages via Handler<M> implementations
/// 4. **Shutdown**: `stopped()` is called during registry shutdown
pub trait OrchestrationActor: Send + Sync + 'static {
    /// Returns the unique name of this actor
    ///
    /// Used for logging, metrics, and debugging. Should be a static string
    /// that clearly identifies the actor's purpose.
    fn name(&self) -> &'static str;

    /// Returns a reference to the system context
    ///
    /// Provides access to database pool, configuration, and other
    /// framework-level resources.
    fn context(&self) -> &Arc<SystemContext>;

    /// Called when the actor is started
    ///
    /// Perform any initialization work here, such as:
    /// - Setting up database connections
    /// - Loading configuration
    /// - Initializing caches
    ///
    /// # Errors
    ///
    /// Return an error if initialization fails. The actor will not be
    /// registered and the system will fail to start.
    fn started(&mut self) -> TaskerResult<()> {
        tracing::info!(actor = %self.name(), "Actor started");
        Ok(())
    }

    /// Called when the actor is stopped
    ///
    /// Perform any cleanup work here, such as:
    /// - Closing database connections
    /// - Flushing caches
    /// - Releasing resources
    ///
    /// # Errors
    ///
    /// Return an error if cleanup fails. Errors are logged but do not
    /// prevent other actors from shutting down.
    fn stopped(&mut self) -> TaskerResult<()> {
        tracing::info!(actor = %self.name(), "Actor stopped");
        Ok(())
    }
}
```

**Key Design Decisions**:

1. **Send + Sync + 'static**: Enables actors to be shared across threads
2. **Default lifecycle hooks**: Actors only override when needed
3. **Context injection**: All actors have access to SystemContext
4. **Error handling**: Lifecycle failures are TaskerResult for proper error propagation

### Handler<M> Trait

The message handling trait, enabling type-safe message processing:

```rust
/// Message handler trait for specific message types
///
/// Actors implement Handler<M> for each message type they can process.
/// This provides type-safe, asynchronous message handling with clear
/// input/output contracts.
#[async_trait]
pub trait Handler<M: Message>: OrchestrationActor {
    /// The response type returned by this handler
    type Response: Send;

    /// Handle a message asynchronously
    ///
    /// Process the message and return a response. This method should be
    /// idempotent where possible and handle errors gracefully.
    async fn handle(&self, msg: M) -> TaskerResult<Self::Response>;
}
```

**Key Design Decisions**:

1. **async_trait**: All message handling is asynchronous
2. **Type safety**: Message and Response types are checked at compile time
3. **Multiple implementations**: Actor can implement Handler<M> for multiple message types
4. **Error propagation**: TaskerResult ensures proper error handling

### Message Trait

The marker trait for command messages:

```rust
/// Marker trait for command messages
///
/// All messages sent to actors must implement this trait. The associated
/// `Response` type defines what the handler will return.
pub trait Message: Send + 'static {
    /// The response type for this message
    type Response: Send;
}
```

**Key Design Decisions**:

1. **Marker trait**: No methods, just type constraints
2. **Associated type**: Response type is part of the message definition
3. **Send + 'static**: Enables messages to cross thread boundaries

## ActorRegistry

The central registry managing all orchestration actors, defined in `tasker-orchestration/src/actors/registry.rs`:

### Purpose

The ActorRegistry serves as:

1. **Single Source of Truth**: All actors are registered here
2. **Lifecycle Manager**: Handles initialization and shutdown
3. **Dependency Injection**: Provides SystemContext to all actors
4. **Type-Safe Access**: Strongly-typed access to each actor

### Structure

```rust
/// Registry managing all orchestration actors
///
/// The ActorRegistry holds Arc references to all actors in the system,
/// providing centralized access and lifecycle management.
#[derive(Clone)]
pub struct ActorRegistry {
    /// System context shared by all actors
    context: Arc<SystemContext>,

    /// Task request actor for processing task initialization requests (TAS-46 Phase 2)
    pub task_request_actor: Arc<TaskRequestActor>,

    /// Result processor actor for processing step execution results (TAS-46 Phase 2)
    pub result_processor_actor: Arc<ResultProcessorActor>,

    /// Step enqueuer actor for batch processing ready tasks (TAS-46 Phase 3)
    pub step_enqueuer_actor: Arc<StepEnqueuerActor>,

    /// Task finalizer actor for task finalization with atomic claiming (TAS-46 Phase 3)
    pub task_finalizer_actor: Arc<TaskFinalizerActor>,
}
```

### Initialization

The `build()` method creates and initializes all actors:

```rust
impl ActorRegistry {
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self> {
        tracing::info!("Building ActorRegistry with actors");

        // Create shared StepEnqueuerService (used by multiple actors)
        let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
        let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

        // Create TaskRequestActor and its dependencies
        let task_initializer = Arc::new(TaskInitializer::new(
            context.clone(),
            task_claim_step_enqueuer.clone(),
        ));

        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            context.message_client.clone(),
            context.task_handler_registry.clone(),
            task_initializer,
            TaskRequestProcessorConfig::default(),
        ));

        let mut task_request_actor = TaskRequestActor::new(context.clone(), task_request_processor);
        task_request_actor.started()?;
        let task_request_actor = Arc::new(task_request_actor);

        // Create ResultProcessorActor and its dependencies
        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer.clone());
        let result_processor = Arc::new(OrchestrationResultProcessor::new(
            task_finalizer,
            context.clone(),
        ));

        let mut result_processor_actor =
            ResultProcessorActor::new(context.clone(), result_processor);
        result_processor_actor.started()?;
        let result_processor_actor = Arc::new(result_processor_actor);

        // Create StepEnqueuerActor using shared StepEnqueuerService
        let mut step_enqueuer_actor =
            StepEnqueuerActor::new(context.clone(), task_claim_step_enqueuer.clone());
        step_enqueuer_actor.started()?;
        let step_enqueuer_actor = Arc::new(step_enqueuer_actor);

        // Create TaskFinalizerActor using shared StepEnqueuerService
        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer.clone());
        let mut task_finalizer_actor = TaskFinalizerActor::new(context.clone(), task_finalizer);
        task_finalizer_actor.started()?;
        let task_finalizer_actor = Arc::new(task_finalizer_actor);

        tracing::info!("✅ ActorRegistry built successfully with 4 actors");

        Ok(Self {
            context,
            task_request_actor,
            result_processor_actor,
            step_enqueuer_actor,
            task_finalizer_actor,
        })
    }
}
```

### Shutdown

The `shutdown()` method gracefully stops all actors:

```rust
impl ActorRegistry {
    pub async fn shutdown(&mut self) {
        tracing::info!("Shutting down ActorRegistry");

        // Call stopped() on all actors in reverse initialization order
        if let Some(actor) = Arc::get_mut(&mut self.task_finalizer_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop TaskFinalizerActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.step_enqueuer_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop StepEnqueuerActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.result_processor_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop ResultProcessorActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.task_request_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop TaskRequestActor");
            }
        }

        tracing::info!("✅ ActorRegistry shutdown complete");
    }
}
```

## Implemented Actors

### TaskRequestActor

Handles task initialization requests from external clients.

**Location**: `tasker-orchestration/src/actors/task_request_actor.rs`

**Message**: `ProcessTaskRequestMessage`
- Input: `TaskRequestMessage` with task details
- Response: `Uuid` of created task

**Delegation**: Wraps `TaskRequestProcessor` service

**Purpose**: Entry point for new workflow instances, coordinates task creation and initial step discovery.

### ResultProcessorActor

Processes step execution results from workers.

**Location**: `tasker-orchestration/src/actors/result_processor_actor.rs`

**Message**: `ProcessStepResultMessage`
- Input: `StepExecutionResult` with execution outcome
- Response: `()` (unit type)

**Delegation**: Wraps `OrchestrationResultProcessor` service

**Purpose**: Handles step completion, coordinates task finalization when appropriate.

### StepEnqueuerActor

Manages batch processing of ready tasks.

**Location**: `tasker-orchestration/src/actors/step_enqueuer_actor.rs`

**Message**: `ProcessBatchMessage`
- Input: Empty (uses system state)
- Response: `StepEnqueuerServiceResult` with batch stats

**Delegation**: Wraps `StepEnqueuerService`

**Purpose**: Discovers ready tasks and enqueues their executable steps.

### TaskFinalizerActor

Handles task finalization with atomic claiming.

**Location**: `tasker-orchestration/src/actors/task_finalizer_actor.rs`

**Message**: `FinalizeTaskMessage`
- Input: `task_uuid` to finalize
- Response: `FinalizationResult` with action taken

**Delegation**: Wraps `TaskFinalizer` service (Phase 7 decomposed)

**Purpose**: Completes or fails tasks based on step execution results, prevents race conditions through atomic claiming.

## Integration with Commands

### Command Processor Integration

The command processor calls actors directly without intermediate wrapper layers:

```rust
// From: tasker-orchestration/src/orchestration/command_processor.rs

/// Handle task initialization using TaskRequestActor directly (TAS-46)
async fn handle_initialize_task(
    &self,
    request: TaskRequestMessage,
) -> TaskerResult<TaskInitializeResult> {
    // TAS-46: Direct actor-based task initialization
    let msg = ProcessTaskRequestMessage { request };
    let task_uuid = self.actors.task_request_actor.handle(msg).await?;

    Ok(TaskInitializeResult::Success {
        task_uuid,
        message: "Task initialized successfully".to_string(),
    })
}

/// Handle step result processing using ResultProcessorActor directly (TAS-46)
async fn handle_process_step_result(
    &self,
    step_result: StepExecutionResult,
) -> TaskerResult<StepProcessResult> {
    // TAS-46: Direct actor-based step result processing
    let msg = ProcessStepResultMessage {
        result: step_result.clone(),
    };

    match self.actors.result_processor_actor.handle(msg).await {
        Ok(()) => Ok(StepProcessResult::Success {
            message: format!(
                "Step {} result processed successfully",
                step_result.step_uuid
            ),
        }),
        Err(e) => Ok(StepProcessResult::Error {
            message: format!("Failed to process step result: {e}"),
        }),
    }
}

/// Handle task finalization using TaskFinalizerActor directly (TAS-46)
async fn handle_finalize_task(&self, task_uuid: Uuid) -> TaskerResult<TaskFinalizationResult> {
    // TAS-46: Direct actor-based task finalization
    let msg = FinalizeTaskMessage { task_uuid };

    let result = self.actors.task_finalizer_actor.handle(msg).await?;

    Ok(TaskFinalizationResult::Success {
        task_uuid: result.task_uuid,
        final_status: format!("{:?}", result.action),
        completion_time: Some(chrono::Utc::now()),
    })
}
```

**Design Evolution**: Initially planned to use `lifecycle_services/` as a wrapper layer between command processor and actors. After implementing Phase 7 service decomposition, we found direct actor calls were simpler and cleaner, so we removed the intermediate layer.

## Service Decomposition (Phase 7)

Large services (800-900 lines) were decomposed into focused components following single responsibility principle:

### TaskFinalizer Decomposition

```
task_finalization/ (848 lines → 6 files)
├── mod.rs                          # Public API and types
├── service.rs                      # Main TaskFinalizer service (~200 lines)
├── completion_handler.rs           # Task completion logic
├── event_publisher.rs              # Lifecycle event publishing
├── execution_context_provider.rs   # Context fetching
└── state_handlers.rs               # State-specific handling
```

### StepEnqueuerService Decomposition

```
step_enqueuer_services/ (781 lines → 3 files)
├── mod.rs                          # Public API
├── service.rs                      # Main service (~250 lines)
├── batch_processor.rs              # Batch processing logic
└── state_handlers.rs               # State-specific processing
```

### ResultProcessor Decomposition

```
result_processing/ (889 lines → 4 files)
├── mod.rs                          # Public API
├── service.rs                      # Main processor
├── metadata_processor.rs           # Metadata handling
├── error_handler.rs                # Error processing
└── result_validator.rs             # Result validation
```

## Actor Lifecycle

### Lifecycle Phases

```
┌─────────────────┐
│  Construction   │  ActorRegistry::build() creates actor instances
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Initialization  │  started() hook called on each actor
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Operation     │  Actors process messages via Handler<M>::handle()
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Shutdown     │  stopped() hook called on each actor (reverse order)
└─────────────────┘
```

### Example Actor Implementation

```rust
use tasker_orchestration::actors::{OrchestrationActor, Handler, Message};

// Define the actor
pub struct TaskFinalizerActor {
    context: Arc<SystemContext>,
    service: TaskFinalizer,
}

// Implement base actor trait
impl OrchestrationActor for TaskFinalizerActor {
    fn name(&self) -> &'static str {
        "TaskFinalizerActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        tracing::info!("TaskFinalizerActor starting");
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        tracing::info!("TaskFinalizerActor stopping");
        Ok(())
    }
}

// Define message type
pub struct FinalizeTaskMessage {
    pub task_uuid: Uuid,
}

impl Message for FinalizeTaskMessage {
    type Response = FinalizationResult;
}

// Implement message handler
#[async_trait]
impl Handler<FinalizeTaskMessage> for TaskFinalizerActor {
    type Response = FinalizationResult;

    async fn handle(&self, msg: FinalizeTaskMessage) -> TaskerResult<Self::Response> {
        tracing::debug!(
            actor = %self.name(),
            task_uuid = %msg.task_uuid,
            "Processing FinalizeTaskMessage"
        );

        // Delegate to service
        self.service.finalize_task(msg.task_uuid).await
            .map_err(|e| e.into())
    }
}
```

## Benefits

### 1. Consistency

All lifecycle components follow the same pattern:
- Uniform initialization via `started()`
- Uniform cleanup via `stopped()`
- Uniform message handling via `Handler<M>`

### 2. Type Safety

Messages and responses are type-checked at compile time:
```rust
// Compile error if message/response types don't match
impl Handler<WrongMessage> for TaskFinalizerActor {
    type Response = WrongResponse;  // ❌ Won't compile
    // ...
}
```

### 3. Testability

- Clear message boundaries for mocking
- Isolated actor lifecycle for unit tests
- Type-safe message construction

### 4. Maintainability

- Clear separation of concerns
- Explicit message contracts
- Centralized lifecycle management
- Decomposed services (<300 lines per file)

### 5. Simplicity

- Direct actor calls (no wrapper layers)
- Pure routing in command processor
- Easy to trace message flow

## Summary

The actor-based architecture provides a consistent, type-safe foundation for lifecycle component management in tasker-core. Key takeaways:

1. **Lightweight Pattern**: Actors wrap decomposed services, providing message-based interface
2. **Lifecycle Management**: Consistent initialization and shutdown via traits
3. **Type Safety**: Compile-time verification of message contracts
4. **Service Decomposition**: Focused components following single responsibility principle
5. **Direct Integration**: Command processor calls actors directly without intermediate wrappers
6. **Production Ready**: All phases complete, zero breaking changes, full test coverage

The architecture provides a solid foundation for future scalability and maintainability while maintaining the proven reliability of existing orchestration logic.

---

← Back to [Documentation Hub](README.md)
