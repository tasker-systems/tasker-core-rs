# TAS-69 Service Decomposition Analysis

## Overview

This document analyzes the decomposition of the monolithic `WorkerProcessor` into three focused services, each with clear responsibilities and bounded contexts.

## Service Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ActorCommandProcessor                     │
│                           │                                  │
│    ┌──────────────────────┼──────────────────────┐          │
│    │                      │                      │          │
│    ▼                      ▼                      ▼          │
│ StepExecutorActor    FFICompletionActor   WorkerStatusActor │
│    │                      │                      │          │
│    ▼                      ▼                      ▼          │
│ StepExecutorService  FFICompletionService WorkerStatusSvc   │
└─────────────────────────────────────────────────────────────┘
```

## Service 1: StepExecutorService

### Location
`tasker-worker/src/worker/services/step_execution/service.rs`

### Responsibilities
- Step claiming from PGMQ queues
- Step state verification
- FFI handler invocation (Ruby/Python)
- Step context loading
- Domain event dispatch coordination

### Key Methods

```rust
impl StepExecutorService {
    /// Execute a step from a PGMQ message
    pub async fn execute_step(
        &mut self,
        message: pgmq::Message<SimpleStepMessage>,
        queue_name: &str,
    ) -> TaskerResult<StepExecutionResult>

    /// Set the event publisher for FFI handler invocation
    pub fn set_event_publisher(&mut self, publisher: WorkerEventPublisher)

    /// Set the domain event handle for event publishing
    pub fn set_domain_event_handle(&mut self, handle: DomainEventSystemHandle)

    /// Dispatch domain events after step completion
    pub fn dispatch_domain_events(
        &mut self,
        step_uuid: Uuid,
        step_result: &StepExecutionResult,
        correlation_id: Option<Uuid>,
    )
}
```

### Internal Flow

```
execute_step()
    │
    ├── 1. Extract step_uuid from message
    │
    ├── 2. Get task handler from TaskTemplateManager
    │       └── Validates namespace support
    │
    ├── 3. Claim step in database
    │       └── Atomic claim with visibility timeout
    │
    ├── 4. Verify step state (must be Enqueued)
    │
    ├── 5. Load step context from database
    │
    ├── 6. Invoke FFI handler
    │       ├── Ruby handler via magnus
    │       ├── Python handler via pyo3 (planned)
    │       └── WASM handler (planned)
    │
    ├── 7. Process handler result
    │       └── Transform to StepExecutionResult
    │
    ├── 8. Delete message from queue
    │
    └── 9. Return result (dispatch events handled by actor)
```

### Dependencies
- `SystemContext` - Database pool, message client
- `TaskTemplateManager` - Handler resolution
- `WorkerEventPublisher` - FFI handler invocation
- `DomainEventSystemHandle` - Event publishing

### Migration Source
Extracted from `WorkerProcessor::handle_execute_step()` (~400 lines)

---

## Service 2: FFICompletionService

### Location
`tasker-worker/src/worker/services/ffi_completion/service.rs`

### Responsibilities
- Process step completion events from FFI handlers
- Send step results to orchestration
- Coordinate with OrchestrationResultSender
- Handle correlation tracking for event-driven flow

### Key Methods

```rust
impl FFICompletionService {
    /// Process a step completion from FFI handler
    pub async fn process_completion(
        &self,
        step_result: StepExecutionResult,
        correlation_id: Option<Uuid>,
    ) -> TaskerResult<()>

    /// Send step result to orchestration
    pub async fn send_result(
        &self,
        result: StepExecutionResult,
    ) -> TaskerResult<()>
}
```

### Internal Flow

```
process_completion()
    │
    ├── 1. Validate step result
    │
    ├── 2. Log correlation ID if present
    │
    ├── 3. Send to orchestration via OrchestrationResultSender
    │       └── Uses PGMQ orchestration_step_results queue
    │
    └── 4. Return success/failure

send_result()
    │
    ├── 1. Create result message
    │
    ├── 2. Send via OrchestrationResultSender
    │       └── Atomic enqueue with notification
    │
    └── 3. Propagate errors (not silently swallowed)
```

### Dependencies
- `SystemContext` - Message client
- `OrchestrationResultSender` - Result delivery

### Migration Source
Extracted from `WorkerProcessor` completion handling (~200 lines)

---

## Service 3: WorkerStatusService

### Location
`tasker-worker/src/worker/services/worker_status/service.rs`

### Responsibilities
- Track step execution statistics
- Manage event integration state
- Provide health check responses
- Report registered handlers

### Key Methods

```rust
impl WorkerStatusService {
    /// Get current worker status
    pub fn get_status(&self) -> WorkerStatus

    /// Get event integration status
    pub fn get_event_status(&self) -> EventIntegrationStatus

    /// Set event integration enabled/disabled
    pub fn set_event_integration(&mut self, enabled: bool)

    /// Record step execution
    pub fn record_step_execution(&mut self, success: bool, duration_ms: f64)

    /// Perform health check
    pub async fn health_check(&self) -> TaskerResult<WorkerHealthStatus>
}
```

### Internal State

```rust
struct WorkerStatusService {
    worker_id: String,
    start_time: Instant,
    stats: StepExecutionStats,
    event_integration_enabled: bool,
    task_template_manager: Arc<TaskTemplateManager>,
}

struct StepExecutionStats {
    total_executed: u64,
    total_succeeded: u64,
    total_failed: u64,
    average_execution_time_ms: f64,
}
```

### Dependencies
- `TaskTemplateManager` - Handler enumeration
- `SystemContext` - Health check queries

### Migration Source
Extracted from `WorkerProcessor` status/health logic (~150 lines)

---

## Comparison with Orchestration Services

### Pattern Consistency

The worker service decomposition follows the same pattern established in TAS-46 for orchestration:

| Orchestration (TAS-46) | Worker (TAS-69) |
|------------------------|-----------------|
| `TaskInitializationService` | `StepExecutorService` |
| `ResultProcessingService` | `FFICompletionService` |
| `TaskFinalizationService` | `WorkerStatusService` |

### Shared Characteristics

1. **Single Responsibility**: Each service handles one concern
2. **Dependency Injection**: Services receive dependencies via constructor
3. **Async Interface**: All public methods are async
4. **Error Propagation**: Uses `TaskerResult<T>` consistently
5. **Stateless Design**: Services hold references, not mutable state (mostly)

---

## Service Integration Patterns

### Actor-Service Binding

Each actor wraps exactly one service:

```rust
// StepExecutorActor
pub struct StepExecutorActor {
    context: Arc<SystemContext>,
    service: Arc<RwLock<StepExecutorService>>,
}

#[async_trait]
impl Handler<ExecuteStepMessage> for StepExecutorActor {
    async fn handle(&self, msg: ExecuteStepMessage) -> TaskerResult<()> {
        let mut service = self.service.write().await;
        service.execute_step(msg.message, &msg.queue_name).await
    }
}
```

### Service Initialization

Services are created during registry build:

```rust
impl WorkerActorRegistry {
    async fn build_with_components(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> TaskerResult<Self> {
        // Create services
        let step_executor_service = StepExecutorService::new(
            worker_id.clone(),
            context.clone(),
            task_template_manager.clone(),
        );

        let ffi_completion_service = FFICompletionService::new(
            context.clone(),
            worker_id.clone(),
        );

        let worker_status_service = WorkerStatusService::new(
            context.clone(),
            worker_id.clone(),
            task_template_manager.clone(),
        );

        // ... wrap in actors
    }
}
```

---

## LOC Analysis

### Before Decomposition

| Component | Lines |
|-----------|-------|
| `command_processor.rs` (all logic) | 1575 |

### After Decomposition

| Service | Lines | % of Original |
|---------|-------|---------------|
| `StepExecutorService` | ~400 | 25% |
| `FFICompletionService` | ~200 | 13% |
| `WorkerStatusService` | ~200 | 13% |
| **Total Services** | ~800 | 51% |

Remaining ~750 lines are distributed across:
- Actor implementations (~500 lines)
- Type definitions (~120 lines)
- Command processor routing (~130 lines)

---

## Testing Strategy

### Unit Testing

Each service can be tested in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "...")]
    async fn test_step_executor_service_execute(pool: PgPool) {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let ttm = Arc::new(TaskTemplateManager::new(...));

        let mut service = StepExecutorService::new(
            "test_worker".to_string(),
            context,
            ttm,
        );

        // Test step execution
        let result = service.execute_step(message, "test_queue").await;
        assert!(result.is_ok());
    }
}
```

### Integration Testing

Services tested through actor handlers in E2E tests:
- 42 Rust E2E tests exercise full flow
- 24 Ruby E2E tests validate FFI integration

---

## Edge Cases and Considerations

### 1. Mutable State in Services

`StepExecutorService` uses `RwLock` for interior mutability:

```rust
service: Arc<RwLock<StepExecutorService>>
```

This allows:
- Multiple readers for status queries
- Exclusive writer for execution state updates
- Thread-safe access across async boundaries

### 2. Event Publisher Injection

The event publisher is set after actor creation:

```rust
// In WorkerCore initialization
step_executor_actor.set_event_publisher(event_publisher).await;
```

This two-phase initialization is necessary because:
- Event publisher depends on command sender
- Command sender created with processor
- Processor creates registry with actors

### 3. Domain Event Dispatch

Events are dispatched by the actor, not the service:

```rust
// In StepExecutorActor
let result = self.service.write().await.execute_step(...).await;
if result.is_ok() {
    self.dispatch_domain_events(step_uuid, &result, correlation_id).await;
}
```

This ensures events fire only after successful execution.

---

## Conclusion

The service decomposition successfully separates concerns:

| Service | Concern | LOC |
|---------|---------|-----|
| `StepExecutorService` | Step execution | ~400 |
| `FFICompletionService` | Result delivery | ~200 |
| `WorkerStatusService` | Health/status | ~200 |

Each service:
- Has a single responsibility
- Is testable in isolation
- Follows established patterns from TAS-46
- Integrates cleanly with the actor layer
