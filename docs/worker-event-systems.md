# Worker Event Systems Architecture

**Last Updated**: 2025-12-07
**Audience**: Architects, Developers
**Status**: Active (TAS-67 Complete)
**Related Docs**: [Worker Actors](worker-actors.md) | [Events and Commands](events-and-commands.md) | [TAS-67 Summary](ticket-specs/TAS-67/summary.md)

<- Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the worker event system architecture in tasker-worker, covering the dual-channel event pattern, domain event publishing, and FFI integration.

## Overview

The worker event system implements a **dual-channel architecture** for non-blocking step execution:

1. **WorkerEventSystem**: Receives step execution events from PGMQ queues
2. **HandlerDispatchService**: Fire-and-forget handler invocation with bounded concurrency
3. **CompletionProcessorService**: Routes results back to orchestration
4. **DomainEventSystem**: Fire-and-forget domain event publishing

This architecture enables true parallel handler execution while maintaining strict ordering guarantees for domain events.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           WORKER EVENT FLOW                                  │
└─────────────────────────────────────────────────────────────────────────────┘

                         PostgreSQL PGMQ Queues
                                  │
                                  │ pg_notify / polling
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         WorkerEventSystem                                    │
│  ┌──────────────────────┐    ┌──────────────────────┐                       │
│  │  WorkerQueueListener │    │  WorkerFallbackPoller │                      │
│  │  (event-driven)      │    │  (reliability)        │                      │
│  └──────────┬───────────┘    └──────────┬───────────┘                       │
│             │                           │                                    │
│             └───────────┬───────────────┘                                    │
│                         │                                                    │
│                         ▼                                                    │
│             WorkerCommand::ExecuteStepFromEvent                              │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      ActorCommandProcessor                                   │
│                              │                                               │
│                              ▼                                               │
│                      StepExecutorActor                                       │
│                              │                                               │
│                              │ claim step, send to dispatch channel          │
│                              ▼                                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
           Rust Workers               FFI Workers (Ruby/Python)
                    │                           │
                    ▼                           ▼
┌───────────────────────────────┐   ┌───────────────────────────────┐
│   HandlerDispatchService      │   │     FfiDispatchChannel        │
│                               │   │                               │
│   dispatch_receiver           │   │   pending_events HashMap      │
│         │                     │   │         │                     │
│         ▼                     │   │         ▼                     │
│   [Semaphore] N permits       │   │   poll_step_events()          │
│         │                     │   │         │                     │
│         ▼                     │   │         ▼                     │
│   handler.call()              │   │   Ruby/Python handler         │
│         │                     │   │         │                     │
│         ▼                     │   │         ▼                     │
│   PostHandlerCallback         │   │   complete_step_event()       │
│         │                     │   │         │                     │
│         ▼                     │   │         ▼                     │
│   completion_sender           │   │   PostHandlerCallback         │
│                               │   │         │                     │
└───────────────┬───────────────┘   │         ▼                     │
                │                   │   completion_sender           │
                │                   │                               │
                │                   └───────────────┬───────────────┘
                │                                   │
                └───────────────┬───────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CompletionProcessorService                                │
│                              │                                               │
│                              ▼                                               │
│                    FFICompletionService                                      │
│                              │                                               │
│                              ▼                                               │
│               orchestration_step_results queue                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
                         Orchestration
```

## Core Components

### 1. WorkerEventSystem

**Location**: `tasker-worker/src/worker/event_systems/worker_event_system.rs`

Implements the `EventDrivenSystem` trait for worker namespace queue processing. Supports three deployment modes:

| Mode | Description | Use Case |
|------|-------------|----------|
| `PollingOnly` | Traditional polling | Restricted environments |
| `EventDrivenOnly` | Pure pg_notify | Low latency requirements |
| `Hybrid` | Event-driven + polling fallback | Production (recommended) |

**Key Features**:
- Unified configuration via `WorkerEventSystemConfig`
- Atomic statistics with `AtomicU64` counters
- Converts `WorkerNotification` to `WorkerCommand` for processing

```rust
// Worker notification to command conversion
match notification {
    WorkerNotification::Event(WorkerQueueEvent::StepMessage(msg_event)) => {
        command_sender.send(WorkerCommand::ExecuteStepFromEvent {
            message_event: msg_event,
            resp: resp_tx,
        }).await;
    }
    // ...
}
```

### 2. HandlerDispatchService

**Location**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

Non-blocking handler dispatch with bounded parallelism. This is the core innovation of TAS-67.

**Architecture**:
```
dispatch_receiver → [Semaphore] → handler.call() → [callback] → completion_sender
                         │                              │
                         └─→ Bounded to N concurrent    └─→ Domain events
                              tasks
```

**Key Design Decisions**:

1. **Semaphore-Bounded Concurrency**: Limits concurrent handlers to prevent resource exhaustion
2. **Permit Release Before Send**: Prevents backpressure cascade
3. **Post-Handler Callback**: Domain events fire only after result is committed

```rust
tokio::spawn(async move {
    let permit = semaphore.acquire().await?;

    let result = execute_with_timeout(&registry, &msg, timeout).await;

    // Release permit BEFORE sending to completion channel
    drop(permit);

    // Send result FIRST
    sender.send(result.clone()).await?;

    // Callback fires AFTER result is committed
    if let Some(cb) = callback {
        cb.on_handler_complete(&step, &result, &worker_id).await;
    }
});
```

**Error Handling**:

| Scenario | Behavior |
|----------|----------|
| Handler timeout | `StepExecutionResult::failure()` with `error_type=handler_timeout` |
| Handler panic | Caught via `catch_unwind()`, failure result generated |
| Handler error | Failure result with `error_type=handler_error` |
| Semaphore closed | Failure result with `error_type=semaphore_acquisition_failed` |

### 3. FfiDispatchChannel

**Location**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Pull-based polling interface for FFI workers (Ruby, Python). Enables language-specific handlers without complex FFI memory management.

**Flow**:
```
Rust                           Ruby/Python
  │                                 │
  │  dispatch(step)                 │
  │ ──────────────────────────────► │
  │                                 │ pending_events.insert()
  │                                 │
  │  poll_step_events()             │
  │ ◄────────────────────────────── │
  │                                 │
  │                                 │ handler.call()
  │                                 │
  │  complete_step_event(result)    │
  │ ◄────────────────────────────── │
  │                                 │
  │  PostHandlerCallback            │
  │  completion_sender.send()       │
  │                                 │
```

**Key Features**:
- Thread-safe pending events map with lock poisoning recovery
- Configurable completion timeout (default 30s)
- Starvation detection and warnings
- Fire-and-forget callbacks via `runtime_handle.spawn()`

### 4. CompletionProcessorService

**Location**: `tasker-worker/src/worker/handlers/completion_processor.rs`

Receives completed step results and routes to orchestration queue via `FFICompletionService`.

```
completion_receiver → CompletionProcessorService → FFICompletionService → orchestration_step_results
```

**Note**: Currently processes completions sequentially. Parallel processing is documented as a future enhancement in `docs/ticket-specs/TAS-67/future-enhancements.md`.

### 5. DomainEventSystem

**Location**: `tasker-worker/src/worker/event_systems/domain_event_system.rs`

Async system for fire-and-forget domain event publishing.

**Architecture**:
```
command_processor.rs                  DomainEventSystem
      │                                     │
      │ try_send(command)                   │ spawn process_loop()
      ▼                                     ▼
mpsc::Sender<DomainEventCommand>  →  mpsc::Receiver
                                            │
                                            ▼
                                    EventRouter → PGMQ / InProcess
```

**Key Design**:
- `try_send()` never blocks - if channel is full, events are dropped with metrics
- Background task processes commands asynchronously
- Graceful shutdown drains fast events up to configurable timeout
- Three delivery modes: Durable (PGMQ), Fast (in-process), Broadcast

## Shared Event Abstractions

### EventDrivenSystem Trait

**Location**: `tasker-shared/src/event_system/event_driven.rs`

Unified trait for all event-driven systems:

```rust
#[async_trait]
pub trait EventDrivenSystem: Send + Sync {
    type SystemId: Send + Sync + Clone;
    type Event: Send + Sync + Clone;
    type Config: Send + Sync + Clone;
    type Statistics: EventSystemStatistics;

    fn system_id(&self) -> Self::SystemId;
    fn deployment_mode(&self) -> DeploymentMode;
    fn is_running(&self) -> bool;

    async fn start(&mut self) -> Result<(), DeploymentModeError>;
    async fn stop(&mut self) -> Result<(), DeploymentModeError>;
    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError>;
    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError>;

    fn statistics(&self) -> Self::Statistics;
    fn config(&self) -> &Self::Config;
}
```

### Deployment Modes

**Location**: `tasker-shared/src/event_system/deployment.rs`

```rust
pub enum DeploymentMode {
    PollingOnly,      // Traditional polling, no events
    EventDrivenOnly,  // Pure event-driven, no polling
    Hybrid,           // Event-driven with polling fallback
}
```

## PostHandlerCallback Trait

**Location**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

Extensibility point for post-handler actions:

```rust
#[async_trait]
pub trait PostHandlerCallback: Send + Sync + 'static {
    /// Called after a handler completes
    async fn on_handler_complete(
        &self,
        step: &TaskSequenceStep,
        result: &StepExecutionResult,
        worker_id: &str,
    );

    /// Name of this callback for logging purposes
    fn name(&self) -> &str;
}
```

**Implementations**:
- `NoOpCallback`: Default no-operation callback
- `DomainEventCallback`: Publishes domain events to `DomainEventSystem`

## Configuration

### Worker Event System

```toml
# config/tasker/base/event_systems.toml
[event_systems.worker]
system_id = "worker-event-system"
deployment_mode = "Hybrid"

[event_systems.worker.metadata.listener]
retry_interval_seconds = 5
max_retry_attempts = 3
event_timeout_seconds = 60
batch_processing = true
connection_timeout_seconds = 30

[event_systems.worker.metadata.fallback_poller]
enabled = true
polling_interval_ms = 100
batch_size = 10
age_threshold_seconds = 30
max_age_hours = 24
visibility_timeout_seconds = 60
```

### Handler Dispatch

```toml
# config/tasker/base/worker.toml
[worker.mpsc_channels.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000
max_concurrent_handlers = 10
handler_timeout_ms = 30000

[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
starvation_warning_threshold_ms = 10000
callback_timeout_ms = 5000
completion_send_timeout_ms = 10000
```

## Integration with Worker Actors

The event systems integrate with the worker actor architecture (TAS-69):

```
WorkerEventSystem
       │
       ▼
ActorCommandProcessor
       │
       ├──► StepExecutorActor ──► dispatch_sender
       │
       ├──► FFICompletionActor ◄── completion_receiver
       │
       └──► DomainEventActor ◄── PostHandlerCallback
```

See [Worker Actors Documentation](worker-actors.md) for actor details.

## Event Flow Guarantees

### Ordering Guarantee

Domain events fire AFTER result is committed to completion channel:

```
handler.call()
    → result committed to completion_sender
    → PostHandlerCallback.on_handler_complete()
    → domain events dispatched
```

This eliminates race conditions where downstream systems see events before orchestration processes results.

### Idempotency Guarantee

State machine guards prevent duplicate execution:

1. Step claimed atomically via `transition_step_state_atomic()`
2. State guards reject duplicate claims
3. Results are deduplicated by completion channel

### Fire-and-Forget Guarantee

Domain event failures never fail step completion:

```rust
// DomainEventCallback
pub async fn on_handler_complete(&self, step, result, worker_id) {
    // dispatch_events uses try_send() - never blocks
    // If channel full, events dropped with metrics
    // Step completion is NOT affected
    self.handle.dispatch_events(events, publisher_name, correlation_id);
}
```

## Monitoring

### Key Metrics

| Metric | Description |
|--------|-------------|
| `tasker.worker.events_processed` | Total events processed |
| `tasker.worker.events_failed` | Events that failed processing |
| `tasker.ffi.pending_events` | Pending FFI events (starvation indicator) |
| `tasker.ffi.oldest_event_age_ms` | Age of oldest pending event |
| `tasker.channel.completion.saturation` | Completion channel utilization |
| `tasker.domain_events.dispatched` | Domain events dispatched |
| `tasker.domain_events.dropped` | Domain events dropped (backpressure) |

### Health Checks

```rust
async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
    if self.is_running.load(Ordering::Acquire) {
        Ok(DeploymentModeHealthStatus::Healthy)
    } else {
        Ok(DeploymentModeHealthStatus::Critical)
    }
}
```

## Best Practices

### 1. Choose Deployment Mode

- **Production**: Use `Hybrid` for reliability with event-driven performance
- **Development**: Use `EventDrivenOnly` for fastest iteration
- **Restricted environments**: Use `PollingOnly` when pg_notify unavailable

### 2. Tune Concurrency

```toml
[worker.mpsc_channels.handler_dispatch]
max_concurrent_handlers = 10  # Start here, increase based on monitoring
```

Monitor:
- Semaphore wait times
- Handler execution latency
- Completion channel saturation

### 3. Configure Timeouts

```toml
handler_timeout_ms = 30000        # Match your slowest handler
completion_timeout_ms = 30000     # FFI completion timeout
callback_timeout_ms = 5000        # Domain event callback timeout
```

### 4. Monitor Starvation

For FFI workers, monitor pending event age:

```ruby
# Ruby
metrics = Tasker.ffi_dispatch_metrics
if metrics[:oldest_pending_age_ms] > 10000
  warn "FFI polling falling behind"
end
```

## Related Documentation

- [Worker Actor-Based Architecture](worker-actors.md) - Actor pattern implementation
- [Events and Commands](events-and-commands.md) - Command pattern details
- [TAS-67 Summary](ticket-specs/TAS-67/summary.md) - Implementation summary
- [TAS-67 Edge Cases](ticket-specs/TAS-67/04-edge-cases-and-risks.md) - Risk analysis
- [FFI Callback Safety](development/ffi-callback-safety.md) - FFI guidelines
- [RCA: Parallel Execution Timing Bugs](architecture-decisions/rca-parallel-execution-timing-bugs.md) - Lessons learned

---

<- Back to [Documentation Hub](README.md)
