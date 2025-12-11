# AGENTS.md - Tasker Worker Crate

This file provides detailed context for AI agents working on the `tasker-worker` crate.

## Module Organization

```
tasker-worker/src/worker/
├── actors/                          # Actor pattern implementation
│   ├── messages.rs                  # WorkerCommand, DispatchHandlerMessage
│   ├── registry.rs                  # WorkerActorRegistry with lifecycle management
│   └── step_executor_actor.rs       # Step claiming and dispatch coordination
│
├── handlers/                        # Handler dispatch system (TAS-67)
│   ├── traits.rs                    # StepHandlerRegistry, StepHandler traits
│   ├── dispatch_service.rs          # HandlerDispatchService (semaphore-bounded)
│   ├── completion_processor.rs      # CompletionProcessorService
│   ├── ffi_dispatch_channel.rs      # FfiDispatchChannel for Ruby/Python polling
│   └── domain_event_callback.rs     # PostHandlerCallback implementation
│
├── event_systems/                   # Event-driven coordination
│   ├── worker_event_system.rs       # WorkerEventSystem (EventDrivenSystem trait)
│   └── domain_event_system.rs       # Fire-and-forget domain event publishing
│
├── actor_command_processor.rs       # Pure routing to actors
├── core.rs                          # Bootstrap with WorkerActorRegistry
└── step_claim.rs                    # Step claiming logic
```

## Dual-Channel Architecture (TAS-67)

```
StepExecutorActor → [Dispatch Channel] → HandlerDispatchService
                                              ↓
                                         handler.call()
                                              ↓
                    [Completion Channel] ← PostHandlerCallback
                                              ↓
                    CompletionProcessorService → FFICompletionService → Orchestration
```

**Key guarantees**:
- Fire-and-forget handler dispatch with semaphore-bounded concurrency
- Domain events fire only AFTER results committed to pipeline
- Comprehensive error handling: panics, timeouts, handler errors → proper failure results

## Key Services

### HandlerDispatchService (`handlers/dispatch_service.rs`)
- Semaphore-bounded parallel handler execution
- Configurable concurrency limits via TOML
- Spawns handler tasks, doesn't wait for completion

### CompletionProcessorService (`handlers/completion_processor.rs`)
- Routes step results to orchestration queue
- Ensures results committed before domain events fire
- Handles success/failure/timeout scenarios

### FfiDispatchChannel (`handlers/ffi_dispatch_channel.rs`)
- Pull-based polling interface for FFI workers (Ruby, Python)
- Unified abstraction for cross-language handler invocation
- Starvation warning monitoring

### StepExecutorActor (`actors/step_executor_actor.rs`)
- Handles step claiming from PGMQ queues
- Coordinates dispatch to handler services
- Manages step lifecycle within worker

## Configuration

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
```

## Worker Commands

| Command | Purpose |
|---------|---------|
| `ExecuteStep` | Execute a step with handler |
| `ExecuteStepWithCorrelation` | Execute with correlation ID |
| `SendStepResult` | Send result to orchestration |
| `ProcessStepCompletion` | Process completion callback |
| `ExecuteStepFromMessage` | Execute from PGMQ message |
| `GetWorkerStatus` | Health/status check |
| `SetEventIntegration` | Configure event system |

## Handler Traits

```rust
pub trait StepHandler: Send + Sync {
    fn handle(&self, context: StepContext)
        -> impl Future<Output = StepResult> + Send;
}

pub trait StepHandlerRegistry: Send + Sync {
    fn get_handler(&self, handler_class: &str) -> Option<Arc<dyn StepHandler>>;
}
```

## Related Documentation

- Worker event systems: `docs/worker-event-systems.md`
- Worker actors: `docs/worker-actors.md`
- FFI patterns: `docs/ffi-telemetry-pattern.md`
- TAS-67 spec: `docs/ticket-specs/TAS-67/`
- TAS-69 spec: `docs/ticket-specs/TAS-69/`
