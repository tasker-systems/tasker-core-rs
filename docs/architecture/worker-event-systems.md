# Worker Event Systems Architecture

**Last Updated**: 2026-01-15
**Audience**: Architects, Developers
**Status**: Active (TAS-67 Complete, TAS-133 Messaging Abstraction)
**Related Docs**: [Worker Actors](worker-actors.md) | [Events and Commands](events-and-commands.md) | [Messaging Abstraction](messaging-abstraction.md)

<- Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the worker event system architecture in tasker-worker, covering the dual-channel event pattern, domain event publishing, and FFI integration.

## Overview

The worker event system implements a **dual-channel architecture** for non-blocking step execution:

1. **WorkerEventSystem**: Receives step execution events via provider-agnostic subscriptions (TAS-133)
2. **HandlerDispatchService**: Fire-and-forget handler invocation with bounded concurrency
3. **CompletionProcessorService**: Routes results back to orchestration
4. **DomainEventSystem**: Fire-and-forget domain event publishing

**Messaging Backend Support** (TAS-133): The worker event system supports multiple messaging backends (PGMQ, RabbitMQ) through a provider-agnostic abstraction. See [Messaging Abstraction](messaging-abstraction.md) for details.

This architecture enables true parallel handler execution while maintaining strict ordering guarantees for domain events.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           WORKER EVENT FLOW                                  │
└─────────────────────────────────────────────────────────────────────────────┘

                    MessagingProvider (PGMQ or RabbitMQ)
                                  │
                                  │ provider.subscribe_many() (TAS-133)
                                  ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         WorkerEventSystem                                    │
│  ┌──────────────────────┐    ┌──────────────────────┐                       │
│  │  WorkerQueueListener │    │  WorkerFallbackPoller │                      │
│  │  (provider-agnostic) │    │  (PGMQ only)          │                      │
│  └──────────┬───────────┘    └──────────┬───────────┘                       │
│             │                           │                                    │
│             └───────────┬───────────────┘                                    │
│                         │                                                    │
│                         ▼                                                    │
│   MessageNotification::Message → ExecuteStepFromMessage (RabbitMQ)          │
│   MessageNotification::Available → ExecuteStepFromEvent (PGMQ)              │
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

Implements the `EventDrivenSystem` trait for worker namespace queue processing. Supports three deployment modes with provider-agnostic message handling (TAS-133):

| Mode | Description | PGMQ Behavior | RabbitMQ Behavior |
|------|-------------|---------------|-------------------|
| `PollingOnly` | Traditional polling | Poll PGMQ tables | Poll via basic_get |
| `EventDrivenOnly` | Pure push delivery | pg_notify signals | basic_consume push |
| `Hybrid` | Event-driven + polling | pg_notify + fallback | Push only (no fallback) |

**Provider-Specific Behavior**:
- **PGMQ**: Uses `MessageNotification::Available` (signal-only), requires fallback polling
- **RabbitMQ**: Uses `MessageNotification::Message` (full payload), no fallback needed

**Key Features**:
- Unified configuration via `WorkerEventSystemConfig`
- Atomic statistics with `AtomicU64` counters
- Converts `WorkerNotification` to `WorkerCommand` for processing

```rust
// Worker notification to command conversion (TAS-133 provider-agnostic)
match notification {
    // RabbitMQ style - full message delivered
    WorkerNotification::Message(msg) => {
        command_sender.send(WorkerCommand::ExecuteStepFromMessage {
            queue_name: msg.queue_name.clone(),
            message: msg,
            resp: resp_tx,
        }).await;
    }
    // PGMQ style - signal-only, requires fetch
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

#### Handler Resolution (TAS-93)

Before handler execution, the dispatch service resolves the handler using a **resolver chain pattern**:

```
HandlerDefinition                    ResolverChain                    Handler
     │                                    │                              │
     │  callable: "process_payment"       │                              │
     │  method: "refund"                  │                              │
     │  resolver: null                    │                              │
     │                                    │                              │
     ├───────────────────────────────────►│                              │
     │                                    │                              │
     │                    ┌───────────────┴───────────────┐              │
     │                    │ ExplicitMappingResolver (10)  │              │
     │                    │ can_resolve? ─► YES           │              │
     │                    │ resolve() ─────────────────────────────────►│
     │                    └───────────────────────────────┘              │
     │                                                                   │
     │                    ┌───────────────────────────────┐              │
     │                    │ MethodDispatchWrapper         │              │
     │                    │ (if method != "call")         │◄─────────────┤
     │                    └───────────────────────────────┘              │
```

**Built-in Resolvers**:

| Resolver | Priority | Function |
|----------|----------|----------|
| `ExplicitMappingResolver` | 10 | Hash lookup of registered handlers |
| `ClassConstantResolver` | 100 | Runtime class lookup (Ruby only) |
| `ClassLookupResolver` | 100 | Runtime class lookup (Python/TypeScript only) |

**Method Dispatch**: When `handler.method` is specified and not `"call"`, a `MethodDispatchWrapper` is applied to invoke the specified method instead of the default `call()` method.

See [Handler Resolution Guide](../guides/handler-resolution.md) for complete documentation.

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

**Note**: Currently processes completions sequentially. Parallel processing is planned as a future enhancement.

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

## Backpressure Handling

The worker event system implements multiple backpressure mechanisms to ensure graceful degradation under load while preserving step idempotency.

### Backpressure Points

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      WORKER BACKPRESSURE FLOW                                │
└─────────────────────────────────────────────────────────────────────────────┘

[1] Step Claiming
    │
    ├── Planned: Capacity check before claiming
    │   └── If at capacity: Leave message in queue (visibility timeout)
    │
    ▼
[2] Handler Dispatch Channel (Bounded)
    │
    ├── dispatch_buffer_size = 1000
    │   └── If full: Sender blocks until space available
    │
    ▼
[3] Semaphore-Bounded Execution
    │
    ├── max_concurrent_handlers = 10
    │   └── If permits exhausted: Task waits for permit
    │
    ├── CRITICAL: Permit released BEFORE sending to completion channel
    │   └── Prevents backpressure cascade
    │
    ▼
[4] Completion Channel (Bounded)
    │
    ├── completion_buffer_size = 1000
    │   └── If full: Handler task blocks until space available
    │
    ▼
[5] Domain Events (Fire-and-Forget)
    │
    └── try_send() semantics
        └── If channel full: Events DROPPED (step execution unaffected)
```

### Handler Dispatch Backpressure

The `HandlerDispatchService` uses semaphore-bounded parallelism:

```rust
// Permit acquisition blocks if all permits in use
let permit = semaphore.acquire().await?;

let result = execute_with_timeout(&registry, &msg, timeout).await;

// CRITICAL: Release permit BEFORE sending to completion channel
// This prevents backpressure cascade where full completion channel
// holds permits, starving new handler execution
drop(permit);

// Now send to completion channel (may block if full)
sender.send(result).await?;
```

**Why permit release before send matters**:
- If completion channel is full, handler task blocks on send
- If permit is held during block, no new handlers can start
- By releasing permit first, new handlers can start even if completions are backing up

### FFI Dispatch Backpressure

The `FfiDispatchChannel` handles backpressure for Ruby/Python workers:

| Scenario | Behavior |
|----------|----------|
| Dispatch channel full | Sender blocks |
| FFI polling too slow | Starvation warning logged |
| Completion timeout | Failure result generated |
| Callback timeout | Callback fire-and-forget, logged |

**Starvation Detection**:
```toml
[worker.mpsc_channels.ffi_dispatch]
starvation_warning_threshold_ms = 10000  # Warn if event waits > 10s
```

### Domain Event Drop Semantics

Domain events use `try_send()` and are explicitly designed to be droppable:

```rust
// Domain events fire AFTER result is committed
// They are non-critical and use fire-and-forget semantics
match event_sender.try_send(event) {
    Ok(()) => { /* Event dispatched */ }
    Err(TrySendError::Full(_)) => {
        // Event dropped - step execution NOT affected
        warn!("Domain event dropped: channel full");
        metrics.increment("domain_events_dropped");
    }
}
```

**Why this is safe**: Domain events are informational. Dropping them does not affect step execution correctness. The step result is already committed to the completion channel before domain events fire.

### Step Claiming Backpressure (Planned)

Future enhancement: Workers will check capacity before claiming steps:

```rust
// Planned implementation
fn should_claim_step(&self) -> bool {
    let available = self.semaphore.available_permits();
    let threshold = self.config.claim_capacity_threshold;  // e.g., 0.8
    let max = self.config.max_concurrent_handlers;

    available as f64 / max as f64 > (1.0 - threshold)
}
```

If at capacity:
- Worker does NOT acknowledge the PGMQ message
- Message returns to queue after visibility timeout
- Another worker (or same worker later) claims it

### Idempotency Under Backpressure

All backpressure mechanisms preserve step idempotency:

| Backpressure Point | Idempotency Guarantee |
|--------------------|----------------------|
| Claim refusal | Message stays in queue, visibility timeout protects |
| Dispatch channel full | Step claimed but queued for execution |
| Semaphore wait | Step claimed, waiting for permit |
| Completion channel full | Handler completed, result buffered |
| Domain event drop | Non-critical, step result already persisted |

**Critical Rule**: A claimed step MUST produce a result (success or failure). Backpressure may delay but never drop step execution.

For comprehensive backpressure strategy, see [Backpressure Architecture](backpressure-architecture.md).

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

- [Messaging Abstraction](messaging-abstraction.md) - Provider-agnostic messaging (TAS-133)
- [Backpressure Architecture](backpressure-architecture.md) - Unified backpressure strategy
- [Worker Actor-Based Architecture](worker-actors.md) - Actor pattern implementation
- [Events and Commands](events-and-commands.md) - Command pattern details
- [TAS-67 ADR](../decisions/TAS-67-dual-event-system.md) - Dual-channel event system decision
- [FFI Callback Safety](development/ffi-callback-safety.md) - FFI guidelines
- [RCA: Parallel Execution Timing Bugs](../decisions/rca-parallel-execution-timing-bugs.md) - Lessons learned
- [Backpressure Monitoring Runbook](operations/backpressure-monitoring.md) - Metrics and alerting

---

<- Back to [Documentation Hub](README.md)
