# Actor-Based Architecture

**Last Updated**: 2025-10-10
**Audience**: Architects, Developers
**Status**: Active (TAS-46 Phase 1 Complete)
**Related Docs**: [Documentation Hub](README.md) | [Events and Commands](events-and-commands.md) | [States and Lifecycles](states-and-lifecycles.md)

← Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the actor-based architecture in tasker-core, covering the lightweight Actor pattern that formalizes the relationship between Commands and Lifecycle Components. This architecture replaces imperative delegation with message-based actor coordination while maintaining full backward compatibility.

## Overview

The tasker-core system implements a **lightweight Actor pattern** inspired by frameworks like Actix, but designed specifically for our orchestration needs without external dependencies. The architecture provides:

1. **Actor Abstraction**: Lifecycle components encapsulated as actors with clear lifecycle hooks
2. **Message-Based Communication**: Type-safe message handling via Handler<M> trait
3. **Central Registry**: ActorRegistry for managing all orchestration actors
4. **Test Infrastructure**: Dedicated test harnesses for actor-based testing
5. **Backward Compatibility**: Gradual migration path from imperative to actor-based patterns

This architecture eliminates inconsistencies in lifecycle component initialization, provides type-safe message handling, and creates a clear separation between command processing and business logic execution.

## Core Concepts

### What is an Actor?

In the tasker-core context, an **Actor** is an encapsulated lifecycle component that:

- **Manages its own state**: Each actor owns its dependencies and configuration
- **Processes messages**: Responds to typed command messages via the Handler<M> trait
- **Has lifecycle hooks**: Initialization (started) and cleanup (stopped) methods
- **Is isolated**: Actors communicate only through message passing, never direct method calls
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
impl OrchestrationActor for TaskInitializerActor {
    fn name(&self) -> &'static str { "TaskInitializerActor" }
    fn context(&self) -> &Arc<SystemContext> { &self.context }
    fn started(&mut self) -> TaskerResult<()> { /* initialization */ }
    fn stopped(&mut self) -> TaskerResult<()> { /* cleanup */ }
}
```

### Actor vs Service

**Services** (existing lifecycle components):
- Encapsulate business logic
- Stateless operations on domain models
- Direct method invocation
- Examples: TaskInitializer, StepEnqueuer

**Actors** (new wrapper pattern):
- Wrap services with message-based interface
- Manage service lifecycle
- Asynchronous message handling
- Examples: TaskInitializerActor, StepEnqueuerActor

The relationship:

```rust
pub struct TaskInitializerActor {
    context: Arc<SystemContext>,
    service: TaskInitializer,  // Wraps existing service
}

impl Handler<InitializeTaskMessage> for TaskInitializerActor {
    type Response = TaskInitializationResult;

    async fn handle(&self, msg: InitializeTaskMessage) -> TaskerResult<Self::Response> {
        // Delegates to service
        self.service.create_task_from_request(msg.request).await
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
///
/// # Type Parameters
///
/// * `M` - The message type this handler processes (must implement Message trait)
///
/// # Example
///
/// ```rust
/// struct InitializeTaskMessage {
///     request: TaskRequest,
/// }
///
/// impl Message for InitializeTaskMessage {
///     type Response = TaskInitializationResult;
/// }
///
/// impl Handler<InitializeTaskMessage> for TaskInitializerActor {
///     type Response = TaskInitializationResult;
///
///     async fn handle(&self, msg: InitializeTaskMessage) -> TaskerResult<Self::Response> {
///         self.service.create_task_from_request(msg.request).await
///     }
/// }
/// ```
#[async_trait]
pub trait Handler<M: Message>: OrchestrationActor {
    /// The response type returned by this handler
    type Response: Send;

    /// Handle a message asynchronously
    ///
    /// Process the message and return a response. This method should be
    /// idempotent where possible and handle errors gracefully.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to process
    ///
    /// # Returns
    ///
    /// A `TaskerResult` containing the response or an error
    ///
    /// # Errors
    ///
    /// Return an error if message processing fails. The error will be
    /// propagated to the caller (typically the command processor).
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
///
/// # Example
///
/// ```rust
/// struct EnqueueStepsMessage {
///     task_info: ReadyTaskInfo,
/// }
///
/// impl Message for EnqueueStepsMessage {
///     type Response = StepEnqueueResult;
/// }
/// ```
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

    // Phase 2-3: Actor fields will be added here
    // pub task_request_actor: Arc<TaskRequestActor>,
    // pub result_processor_actor: Arc<ResultProcessorActor>,
    // pub step_enqueuer_actor: Arc<StepEnqueuerActor>,
    // pub task_finalizer_actor: Arc<TaskFinalizerActor>,
    // pub task_initializer_actor: Arc<TaskInitializerActor>,
}
```

### Initialization

The `build()` method creates and initializes all actors:

```rust
impl ActorRegistry {
    /// Build a new ActorRegistry with all actors initialized
    ///
    /// This performs the following steps:
    /// 1. Creates all actor instances
    /// 2. Calls started() lifecycle hook on each actor
    /// 3. Returns the registry if all actors initialize successfully
    ///
    /// # Errors
    ///
    /// Returns an error if any actor fails to construct or initialize.
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self> {
        // Phase 1: Just create the basic structure
        let registry = Self { context };

        // Phase 2-3: Call started() on all actors
        // registry.task_request_actor.started()?;
        // registry.result_processor_actor.started()?;
        // etc.

        Ok(registry)
    }
}
```

### Shutdown

The `shutdown()` method gracefully stops all actors:

```rust
impl ActorRegistry {
    /// Shutdown all actors gracefully
    ///
    /// Calls the stopped() lifecycle hook on each actor in reverse
    /// initialization order. Errors are logged but don't prevent
    /// other actors from shutting down.
    pub async fn shutdown(&mut self) {
        // Phase 2-3: Call stopped() on all actors in reverse order
        // if let Err(e) = self.task_finalizer_actor.stopped() {
        //     tracing::error!(error = %e, "Failed to stop TaskFinalizerActor");
        // }
        // etc.

        tracing::info!("ActorRegistry shutdown complete");
    }
}
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
pub struct TaskInitializerActor {
    context: Arc<SystemContext>,
    service: TaskInitializer,
}

// Implement base actor trait
impl OrchestrationActor for TaskInitializerActor {
    fn name(&self) -> &'static str {
        "TaskInitializerActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        tracing::info!("TaskInitializerActor starting");
        // Initialize any resources
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        tracing::info!("TaskInitializerActor stopping");
        // Clean up resources
        Ok(())
    }
}

// Define message type
pub struct InitializeTaskMessage {
    pub request: TaskRequest,
}

impl Message for InitializeTaskMessage {
    type Response = TaskInitializationResult;
}

// Implement message handler
#[async_trait]
impl Handler<InitializeTaskMessage> for TaskInitializerActor {
    type Response = TaskInitializationResult;

    async fn handle(&self, msg: InitializeTaskMessage) -> TaskerResult<Self::Response> {
        tracing::debug!(
            actor = %self.name(),
            task = %msg.request.name,
            "Processing InitializeTaskMessage"
        );

        // Delegate to service
        self.service.create_task_from_request(msg.request).await
    }
}
```

## Integration with Commands

### Command Processor Evolution

The actor pattern integrates seamlessly with the existing command processor:

**Phase 1 (Current)**: Command processor calls lifecycle components directly

```rust
// OLD: Imperative delegation
pub async fn process_command(&self, cmd: OrchestrationCommand) -> TaskerResult<CommandResponse> {
    match cmd {
        OrchestrationCommand::InitializeTask { request } => {
            let result = self.task_initializer.create_task_from_request(request).await?;
            Ok(CommandResponse::TaskInitialized(result))
        }
        // ...
    }
}
```

**Phase 2 (Future)**: Command processor sends messages to actors

```rust
// NEW: Message-based delegation
pub async fn process_command(&self, cmd: OrchestrationCommand) -> TaskerResult<CommandResponse> {
    match cmd {
        OrchestrationCommand::InitializeTask { request } => {
            let msg = InitializeTaskMessage { request };
            let result = self.actors.task_initializer_actor.handle(msg).await?;
            Ok(CommandResponse::TaskInitialized(result))
        }
        // ...
    }
}
```

### Migration Strategy

1. **Phase 1** (Complete): Actor infrastructure and test harness
2. **Phase 2**: Implement first actor (TaskRequestActor) with dual support
3. **Phase 3**: Migrate remaining actors one by one
4. **Phase 4**: Remove legacy direct access, complete migration

Each phase maintains full backward compatibility through the LifecycleTestManager dual implementation pattern.

## Test Infrastructure

### ActorTestHarness

Dedicated test infrastructure for actor-based testing, defined in `tests/common/actor_test_harness.rs`:

```rust
/// Test harness for actor-based testing
///
/// Provides:
/// - Initialized ActorRegistry with all actors
/// - Helper methods for sending messages to actors
/// - Lifecycle management for test setup/teardown
pub struct ActorTestHarness {
    /// Database connection pool
    pub pool: PgPool,

    /// System context for framework operations
    pub context: Arc<SystemContext>,

    /// Actor registry with all orchestration actors
    pub actors: ActorRegistry,
}

impl ActorTestHarness {
    /// Create a new ActorTestHarness with all actors initialized
    pub async fn new(pool: PgPool) -> Result<Self> {
        tracing::info!("🔧 Initializing ActorTestHarness");

        // Create SystemContext from pool
        let context = Arc::new(SystemContext::with_pool(pool.clone()).await?);

        // Build ActorRegistry with all actors
        let actors = ActorRegistry::build(context.clone()).await?;

        tracing::info!("✅ ActorTestHarness initialized successfully");

        Ok(Self {
            pool,
            context,
            actors,
        })
    }

    /// Shutdown all actors gracefully
    pub async fn shutdown(&mut self) {
        tracing::info!("🛑 Shutting down ActorTestHarness");
        self.actors.shutdown().await;
        tracing::info!("✅ ActorTestHarness shutdown complete");
    }
}
```

### LifecycleTestManager Integration

The LifecycleTestManager now supports both legacy and actor-based testing:

```rust
pub struct LifecycleTestManager {
    // ========== Legacy Fields ==========
    /// Task initializer (legacy - prefer actor_harness)
    pub task_initializer: TaskInitializer,

    // ========== Actor-Based Fields (TAS-46) ==========
    /// Actor test harness for actor-based testing
    pub actor_harness: ActorTestHarness,

    // ... other fields
}

impl LifecycleTestManager {
    /// Initialize a task using TaskInitializer (legacy method)
    pub async fn initialize_task(&self, task_request: TaskRequest) -> Result<TaskInitializationResult> {
        self.task_initializer.create_task_from_request(task_request).await
    }

    /// Initialize a task using actor-based approach (Phase 2)
    pub async fn initialize_task_via_actor(&self, task_request: TaskRequest) -> Result<TaskInitializationResult> {
        let msg = InitializeTaskMessage { request: task_request };
        self.actor_harness.actors.task_initializer_actor.handle(msg).await
    }
}
```

### Writing Actor Tests

Example test using the actor harness:

```rust
use common::actor_test_harness::ActorTestHarness;

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_task_initialization_via_actor(pool: PgPool) -> Result<()> {
    let harness = ActorTestHarness::new(pool).await?;

    // Create request
    let request = TaskRequest {
        name: "test_task".to_string(),
        namespace: "test_namespace".to_string(),
        // ... other fields
    };

    // Send message to actor
    let msg = InitializeTaskMessage { request };
    let result = harness.actors.task_initializer_actor.handle(msg).await?;

    // Assert result
    assert!(result.task_uuid.is_some());

    Ok(())
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
impl Handler<WrongMessage> for TaskInitializerActor {
    type Response = WrongResponse;  // ❌ Won't compile
    // ...
}
```

### 3. Testability

- Dedicated test harness for actor testing
- Clear message boundaries for mocking
- Isolated actor lifecycle for unit tests

### 4. Maintainability

- Clear separation of concerns
- Explicit message contracts
- Centralized lifecycle management

### 5. Evolvability

- Easy to add new actors
- Easy to add new message handlers
- Gradual migration path

## Future Enhancements

### Phase 2: TaskRequestActor Implementation

The first actor to be fully implemented:

```rust
pub struct TaskRequestActor {
    context: Arc<SystemContext>,
    processor: TaskRequestProcessor,
}

// Handle task request messages
impl Handler<ProcessTaskRequestMessage> for TaskRequestActor {
    type Response = TaskRequestResult;

    async fn handle(&self, msg: ProcessTaskRequestMessage) -> TaskerResult<Self::Response> {
        self.processor.process_request(msg.request).await
    }
}
```

### Phase 3: Remaining Actors

- ResultProcessorActor
- StepEnqueuerActor
- TaskFinalizerActor
- TaskInitializerActor

### Phase 4: Advanced Features

- **Actor Supervision**: Restart failed actors automatically
- **Actor Metrics**: Per-actor performance monitoring
- **Actor Routing**: Dynamic message routing based on load
- **Actor Persistence**: Save/restore actor state

## Summary

The actor-based architecture provides a consistent, type-safe foundation for lifecycle component management in tasker-core. Key takeaways:

1. **Lightweight Pattern**: Actors wrap services, providing message-based interface
2. **Lifecycle Management**: Consistent initialization and shutdown via traits
3. **Type Safety**: Compile-time verification of message contracts
4. **Test Infrastructure**: Dedicated harnesses for actor and integration testing
5. **Backward Compatible**: Gradual migration without breaking changes

The architecture sets the foundation for future scalability and maintainability while maintaining the proven reliability of existing lifecycle components.

---

← Back to [Documentation Hub](README.md)
