# TAS-69 Actor Implementations Analysis

## Overview

This document analyzes the 5 worker actors introduced in TAS-69, their traits, message handlers, and integration patterns.

## Actor Trait System

### Core Traits

Located in `tasker-worker/src/worker/actors/traits.rs`:

```rust
/// Base trait for all worker actors
pub trait WorkerActor: Send + Sync + 'static {
    /// Returns the actor's name for logging and debugging
    fn name(&self) -> &'static str;

    /// Returns the shared system context
    fn context(&self) -> &Arc<SystemContext>;

    /// Lifecycle hook called when actor starts
    fn started(&mut self) -> TaskerResult<()> { Ok(()) }

    /// Lifecycle hook called when actor stops
    fn stopped(&mut self) -> TaskerResult<()> { Ok(()) }
}

/// Message trait for typed communication
pub trait Message: Send + 'static {
    type Response: Send;
}

/// Handler trait for processing messages
#[async_trait]
pub trait Handler<M: Message>: WorkerActor {
    async fn handle(&self, msg: M) -> TaskerResult<M::Response>;
}
```

### Comparison with Orchestration Traits

| Aspect | Worker (TAS-69) | Orchestration (TAS-46) |
|--------|-----------------|------------------------|
| Base Trait | `WorkerActor` | `OrchestrationActor` |
| Handler Trait | `Handler<M>` | `Handler<M>` |
| Message Trait | `Message` | `Message` |
| Lifecycle Hooks | `started()`, `stopped()` | `started()`, `stopped()` |
| Context Type | `Arc<SystemContext>` | `Arc<SystemContext>` |

The traits are intentionally parallel for architectural consistency.

---

## Actor 1: StepExecutorActor

### Location
`tasker-worker/src/worker/actors/step_executor_actor.rs`

### Purpose
Coordinates step execution by delegating to `StepExecutorService`.

### Structure

```rust
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<RwLock<StepExecutorService>>,
}
```

### Message Handlers

| Message | Response | Purpose |
|---------|----------|---------|
| `ExecuteStepMessage` | `()` | Execute step from typed message |
| `ExecuteStepWithCorrelationMessage` | `()` | Execute with correlation tracking |
| `ExecuteStepFromPgmqMessage` | `()` | Execute from raw PGMQ message |
| `ExecuteStepFromEventMessage` | `()` | Execute from notification event |

### Handler Implementation

```rust
#[async_trait]
impl Handler<ExecuteStepMessage> for StepExecutorActor {
    async fn handle(&self, msg: ExecuteStepMessage) -> TaskerResult<()> {
        debug!(
            actor = self.name(),
            step_uuid = %msg.message.message.step_uuid,
            queue = %msg.queue_name,
            "Handling ExecuteStepMessage"
        );

        let mut service = self.service.write().await;
        service.execute_step(msg.message, &msg.queue_name).await
    }
}
```

### Additional Methods

```rust
impl StepExecutorActor {
    /// Set the event publisher for FFI handler invocation
    pub async fn set_event_publisher(&self, publisher: WorkerEventPublisher);

    /// Set the domain event handle for event publishing
    pub async fn set_domain_event_handle(&self, handle: DomainEventSystemHandle);

    /// Dispatch domain events after step completion
    pub async fn dispatch_domain_events(
        &self,
        step_uuid: Uuid,
        step_result: &StepExecutionResult,
        correlation_id: Option<Uuid>,
    );
}
```

---

## Actor 2: FFICompletionActor

### Location
`tasker-worker/src/worker/actors/ffi_completion_actor.rs`

### Purpose
Handles step completion events from FFI handlers and sends results to orchestration.

### Structure

```rust
pub struct FFICompletionActor {
    context: Arc<SystemContext>,
    service: Arc<FFICompletionService>,
}
```

### Message Handlers

| Message | Response | Purpose |
|---------|----------|---------|
| `ProcessStepCompletionMessage` | `()` | Process FFI handler completion |
| `SendStepResultMessage` | `()` | Send result to orchestration |

### Handler Implementation

```rust
#[async_trait]
impl Handler<ProcessStepCompletionMessage> for FFICompletionActor {
    async fn handle(&self, msg: ProcessStepCompletionMessage) -> TaskerResult<()> {
        debug!(
            actor = self.name(),
            correlation_id = ?msg.correlation_id,
            "Handling ProcessStepCompletionMessage"
        );

        self.service.process_completion(msg.step_result, msg.correlation_id).await
    }
}
```

---

## Actor 3: TemplateCacheActor

### Location
`tasker-worker/src/worker/actors/template_cache_actor.rs`

### Purpose
Manages task template cache through `TaskTemplateManager`.

### Structure

```rust
pub struct TemplateCacheActor {
    context: Arc<SystemContext>,
    task_template_manager: Arc<TaskTemplateManager>,
}
```

### Message Handlers

| Message | Response | Purpose |
|---------|----------|---------|
| `RefreshTemplateCacheMessage` | `()` | Refresh cache for namespace |
| `GetTemplateMessage` | `Option<TaskHandler>` | Retrieve handler for task |

### Handler Implementation

```rust
#[async_trait]
impl Handler<RefreshTemplateCacheMessage> for TemplateCacheActor {
    async fn handle(&self, msg: RefreshTemplateCacheMessage) -> TaskerResult<()> {
        debug!(
            actor = self.name(),
            namespace = ?msg.namespace,
            "Handling RefreshTemplateCacheMessage"
        );

        self.task_template_manager.refresh_cache(msg.namespace).await
    }
}
```

### Note on TaskTemplateManager Sharing

The `TaskTemplateManager` is shared from `WorkerCore` to preserve discovered namespaces:

```rust
// WorkerCore creates the manager with discovered namespaces
let task_template_manager = Arc::new(TaskTemplateManager::new(
    context.task_handler_registry.clone(),
));
task_template_manager.discover_namespaces().await?;

// Shared with registry via constructor
let registry = WorkerActorRegistry::build_with_task_template_manager(
    context.clone(),
    worker_id,
    task_template_manager.clone(),
).await?;
```

---

## Actor 4: DomainEventActor

### Location
`tasker-worker/src/worker/actors/domain_event_actor.rs`

### Purpose
Wraps existing `DomainEventSystem` for event dispatching.

### Structure

```rust
pub struct DomainEventActor {
    context: Arc<SystemContext>,
    domain_event_system: Option<DomainEventSystemHandle>,
}
```

### Message Handlers

| Message | Response | Purpose |
|---------|----------|---------|
| `DispatchEventsMessage` | `()` | Dispatch domain events |

### Handler Implementation

```rust
#[async_trait]
impl Handler<DispatchEventsMessage> for DomainEventActor {
    async fn handle(&self, msg: DispatchEventsMessage) -> TaskerResult<()> {
        if let Some(ref handle) = self.domain_event_system {
            handle.dispatch_events(msg.events).await?;
        }
        Ok(())
    }
}
```

### Integration Note

This actor wraps the existing event system rather than replacing it, ensuring backward compatibility with the TAS-43 event-driven architecture.

---

## Actor 5: WorkerStatusActor

### Location
`tasker-worker/src/worker/actors/worker_status_actor.rs`

### Purpose
Provides worker status, health checks, and event integration management.

### Structure

```rust
pub struct WorkerStatusActor {
    context: Arc<SystemContext>,
    service: Arc<RwLock<WorkerStatusService>>,
}
```

### Message Handlers

| Message | Response | Purpose |
|---------|----------|---------|
| `GetWorkerStatusMessage` | `WorkerStatus` | Get worker status |
| `GetEventStatusMessage` | `EventIntegrationStatus` | Get event integration status |
| `SetEventIntegrationMessage` | `()` | Enable/disable event integration |
| `HealthCheckMessage` | `WorkerHealthStatus` | Perform health check |

### Handler Implementation

```rust
#[async_trait]
impl Handler<GetWorkerStatusMessage> for WorkerStatusActor {
    async fn handle(&self, _msg: GetWorkerStatusMessage) -> TaskerResult<WorkerStatus> {
        let service = self.service.read().await;
        Ok(service.get_status())
    }
}

#[async_trait]
impl Handler<HealthCheckMessage> for WorkerStatusActor {
    async fn handle(&self, _msg: HealthCheckMessage) -> TaskerResult<WorkerHealthStatus> {
        let service = self.service.read().await;
        service.health_check().await
    }
}
```

---

## Message Definitions

### Location
`tasker-worker/src/worker/actors/messages.rs`

### Step Execution Messages

```rust
pub struct ExecuteStepMessage {
    pub message: pgmq::Message<SimpleStepMessage>,
    pub queue_name: String,
}
impl Message for ExecuteStepMessage {
    type Response = ();
}

pub struct ExecuteStepWithCorrelationMessage {
    pub message: pgmq::Message<SimpleStepMessage>,
    pub queue_name: String,
    pub correlation_id: Uuid,
}
impl Message for ExecuteStepWithCorrelationMessage {
    type Response = ();
}

pub struct ExecuteStepFromPgmqMessage {
    pub message: pgmq::Message,
    pub queue_name: String,
}
impl Message for ExecuteStepFromPgmqMessage {
    type Response = ();
}

pub struct ExecuteStepFromEventMessage {
    pub message_event: MessageReadyEvent,
}
impl Message for ExecuteStepFromEventMessage {
    type Response = ();
}
```

### Completion Messages

```rust
pub struct ProcessStepCompletionMessage {
    pub step_result: StepExecutionResult,
    pub correlation_id: Option<Uuid>,
}
impl Message for ProcessStepCompletionMessage {
    type Response = ();
}

pub struct SendStepResultMessage {
    pub result: StepExecutionResult,
}
impl Message for SendStepResultMessage {
    type Response = ();
}
```

### Status Messages

```rust
pub struct GetWorkerStatusMessage;
impl Message for GetWorkerStatusMessage {
    type Response = WorkerStatus;
}

pub struct GetEventStatusMessage;
impl Message for GetEventStatusMessage {
    type Response = EventIntegrationStatus;
}

pub struct SetEventIntegrationMessage {
    pub enabled: bool,
}
impl Message for SetEventIntegrationMessage {
    type Response = ();
}

pub struct HealthCheckMessage;
impl Message for HealthCheckMessage {
    type Response = WorkerHealthStatus;
}
```

### Template Messages

```rust
pub struct RefreshTemplateCacheMessage {
    pub namespace: Option<String>,
}
impl Message for RefreshTemplateCacheMessage {
    type Response = ();
}
```

---

## WorkerActorRegistry

### Location
`tasker-worker/src/worker/actors/registry.rs`

### Purpose
Central registry managing all worker actors with lifecycle coordination.

### Structure

```rust
pub struct WorkerActorRegistry {
    context: Arc<SystemContext>,
    worker_id: String,
    pub step_executor_actor: Arc<StepExecutorActor>,
    pub ffi_completion_actor: Arc<FFICompletionActor>,
    pub template_cache_actor: Arc<TemplateCacheActor>,
    pub domain_event_actor: Arc<DomainEventActor>,
    pub worker_status_actor: Arc<WorkerStatusActor>,
}
```

### Build Methods

```rust
impl WorkerActorRegistry {
    /// Build with auto-generated worker ID
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self>;

    /// Build with specific worker ID
    pub async fn build_with_worker_id(
        context: Arc<SystemContext>,
        worker_id: String,
    ) -> TaskerResult<Self>;

    /// Build with pre-initialized TaskTemplateManager (for namespace sharing)
    pub async fn build_with_task_template_manager(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> TaskerResult<Self>;
}
```

### Lifecycle Management

```rust
impl WorkerActorRegistry {
    /// Shutdown all actors gracefully
    pub async fn shutdown(&mut self) {
        // Reverse initialization order
        // 1. WorkerStatusActor
        // 2. DomainEventActor
        // 3. TemplateCacheActor
        // 4. FFICompletionActor
        // 5. StepExecutorActor
    }
}
```

### Initialization Order

During `build()`:
1. Create `StepExecutorActor`
2. Create `FFICompletionActor`
3. Create `TemplateCacheActor`
4. Create `DomainEventActor`
5. Create `WorkerStatusActor`
6. Call `started()` on each actor

---

## Actor Interaction Patterns

### Command Processing Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                     WorkerCommand                                │
│                          │                                       │
│                          ▼                                       │
│              ActorCommandProcessor                               │
│                          │                                       │
│         ┌────────────────┼────────────────┐                     │
│         │                │                │                     │
│         ▼                ▼                ▼                     │
│   ExecuteStep      ProcessCompletion  GetStatus                 │
│         │                │                │                     │
│         ▼                ▼                ▼                     │
│  StepExecutorActor  FFICompletionActor  WorkerStatusActor       │
│         │                │                │                     │
│         ▼                ▼                ▼                     │
│  StepExecutorSvc    FFICompletionSvc   WorkerStatusSvc          │
└─────────────────────────────────────────────────────────────────┘
```

### Event Dispatch Flow

```
StepExecutorActor
       │
       ├── execute_step() returns
       │
       └── dispatch_domain_events()
                    │
                    ▼
           DomainEventActor
                    │
                    ▼
           DomainEventSystem (existing)
```

---

## Thread Safety

### RwLock Usage

Services with mutable state use `RwLock`:

```rust
service: Arc<RwLock<StepExecutorService>>
```

Benefits:
- Multiple concurrent readers for status queries
- Exclusive writer for state mutations
- Async-compatible via tokio's RwLock

### Arc Sharing

All actors are `Arc`-wrapped in the registry:

```rust
pub step_executor_actor: Arc<StepExecutorActor>
```

This enables:
- Cheap cloning for command processor
- Shared access across async tasks
- Deterministic cleanup via Arc reference counting

---

## Testing

### Actor Unit Tests

Each actor has unit tests verifying:
- Creation
- Send + Sync bounds
- Name accessor
- Message handling (where testable)

Example:
```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_executor_actor_creation(pool: sqlx::PgPool) {
    let context = Arc::new(
        SystemContext::with_pool(pool).await.expect("Failed to create context"),
    );
    let ttm = Arc::new(TaskTemplateManager::new(
        context.task_handler_registry.clone(),
    ));

    let actor = StepExecutorActor::new(
        context.clone(),
        "test_worker".to_string(),
        ttm,
    );

    assert_eq!(actor.name(), "StepExecutorActor");
}
```

### Registry Tests

```rust
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_worker_actor_registry_builds_successfully(pool: sqlx::PgPool) {
    let context = Arc::new(
        SystemContext::with_pool(pool).await.expect("Failed to create context"),
    );

    let registry = WorkerActorRegistry::build(context).await;
    assert!(registry.is_ok());

    let registry = registry.unwrap();
    assert!(registry.worker_id().starts_with("worker_"));
}
```

---

## Summary

The 5 worker actors provide:

| Actor | Messages Handled | Service Wrapped |
|-------|------------------|-----------------|
| `StepExecutorActor` | 4 | `StepExecutorService` |
| `FFICompletionActor` | 2 | `FFICompletionService` |
| `TemplateCacheActor` | 2 | `TaskTemplateManager` |
| `DomainEventActor` | 1 | `DomainEventSystem` |
| `WorkerStatusActor` | 4 | `WorkerStatusService` |

The actor layer provides:
- Type-safe message passing
- Lifecycle management
- Clear separation of concerns
- Consistency with orchestration architecture (TAS-46)
