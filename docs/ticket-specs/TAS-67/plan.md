# TAS-67: Rust Worker Dual Event System - Implementation Plan

## Executive Summary

Refactor the Rust worker to use a non-blocking, event-driven pattern matching Ruby's FFI architecture. Extract common abstractions into `tasker-worker` to serve as a battle-tested foundation for TAS-72 (Python/PyO3 workers) and unify the architectural pattern across all language workers.

**Core Problem**: The Rust worker's `event_handler.rs` uses direct blocking `.call()` in the hot path, unlike Ruby's fully event-driven approach which is paradoxically faster due to true concurrency.

**Solution**: Introduce a dual-channel command pattern where handler invocation is fire-and-forget, and completions flow back through a separate channel—matching the Ruby pattern but native to Rust.

**Extended Scope**: Extract reusable patterns from `workers/ruby/ext/tasker_core/src/` into `tasker-worker/src/` so that Rust, Ruby, and Python workers share a unified architectural foundation while keeping language-specific FFI adaptations in their respective crates.

**Public Interface Stability**: Business-domain handler code (Ruby handlers, Rust handlers) is completely unaffected. The `StepHandler.call()` signature, handler registration, and YAML configuration remain unchanged. This is a significant but safe internal refactoring of the dispatch/completion infrastructure.

---

## Unified Architecture Vision

### The Power of Shared Patterns

While each language has its own FFI layer (Magnus for Ruby, PyO3 for Python, native for Rust) and its own event system (dry-events in Ruby, MPSC in Rust, TBD for Python), the **architectural pattern** should be identical:

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                           tasker-worker (shared)                                │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────────────────┐     ┌─────────────────────┐     ┌──────────────────┐  │
│  │ WorkerEventSystem   │────▶│ HandlerDispatchSvc  │────▶│ CompletionSvc    │  │
│  │ (receives from queue)│     │ (fire-and-forget)   │     │ (sends to orch)  │  │
│  └─────────────────────┘     └─────────────────────┘     └──────────────────┘  │
│            │                          │                          │             │
│            │         ┌────────────────┴────────────────┐         │             │
│            │         │  StepHandlerRegistry (trait)    │         │             │
│            │         │  StepHandler (trait)            │         │             │
│            │         └────────────────┬────────────────┘         │             │
│            │                          │                          │             │
├────────────┼──────────────────────────┼──────────────────────────┼─────────────┤
│            │                          │                          │             │
│  ┌─────────┴──────────────────────────┴──────────────────────────┴──────────┐  │
│  │                    Language-Specific Implementations                      │  │
│  ├───────────────────┬─────────────────────────┬────────────────────────────┤  │
│  │      Rust         │         Ruby            │         Python             │  │
│  ├───────────────────┼─────────────────────────┼────────────────────────────┤  │
│  │ RustHandlerRegistry│ RubyHandlerRegistry    │ PythonHandlerRegistry      │  │
│  │ (impl trait)       │ (impl trait via FFI)   │ (impl trait via PyO3)      │  │
│  │                    │ + polling adapter      │ + polling adapter          │  │
│  │ Direct async call  │ MPSC + try_recv poll   │ MPSC + try_recv poll       │  │
│  └───────────────────┴─────────────────────────┴────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────────┘
```

### Key Insight: Decoupled Non-Blocking Events in Both Directions

The unifying principle is **decoupled, non-blocking events flowing in both directions**:

1. **Dispatch Direction** (tasker → handler): Fire-and-forget via MPSC channel
2. **Completion Direction** (handler → tasker): Async completion via separate channel

This pattern is already implemented in Ruby FFI. We're now:
1. Formalizing it as shared abstractions in `tasker-worker`
2. Migrating Ruby FFI to use those shared abstractions
3. Implementing it for native Rust handlers
4. Preparing the foundation for Python

---

## Current Architecture Analysis

### Ruby's Non-Blocking Pattern (Reference Implementation)
```
Rust Worker → Bounded MPSC → Ruby Poll (10ms) → dry-events → Handler
                                                              ↓
Orchestration ← PGMQ ← FFI Completion ← dry-events ← Handler Result
```

**Key Properties**:
1. Rust never blocks waiting for Ruby (bounded channel with backpressure)
2. Ruby controls execution timing (polling loop)
3. Handler dispatch via pub/sub (dry-events)
4. Completion flows back asynchronously

### Current Rust Worker Pattern (Blocking)
```rust
// workers/rust/src/event_handler.rs:119
let result = handler.call(&event.payload.task_sequence_step).await;  // BLOCKS HERE
```

**Issues**:
- Blocks event loop until handler completes
- Cannot process next step until current finishes
- Sequential execution despite async/await syntax

### Existing tasker-worker Infrastructure (TAS-69)
- `ActorCommandProcessor`: Uses `tokio::select!` for multiplexing
- `WorkerCommand` enum: Defines all worker operations
- `StepExecutorActor`, `FFICompletionActor`: Message-based routing
- Bounded channels with TAS-51 monitoring

### Ruby FFI Implementation Analysis (`workers/ruby/ext/tasker_core/src/`)

The Ruby FFI already implements the non-blocking dual-channel pattern. Key files:

| File | Current Responsibility | Migration Target |
|------|----------------------|------------------|
| `bridge.rs` | `RubyBridgeHandle`, `poll_step_events()`, global worker handle | Keep Magnus-specific, use shared services |
| `event_handler.rs` | `RubyEventHandler` - MPSC channel forwarding, completion handling | **Extract pattern to tasker-worker** |
| `bootstrap.rs` | `bootstrap_worker()` - uses `WorkerBootstrap` from tasker-worker | Keep, already uses shared bootstrap |
| `global_event_system.rs` | Singleton `WorkerEventSystem` | **Move to tasker-worker** (already there) |
| `in_process_event_ffi.rs` | Domain event polling for Ruby | Keep Magnus-specific |
| `event_publisher_ffi.rs` | Domain event publishing FFI | Keep Magnus-specific |
| `conversions.rs` | Ruby <-> Rust type conversions | Keep Magnus-specific |

#### Pattern Already Working in Ruby (lines 40-112 of `event_handler.rs`)

```rust
// workers/ruby/ext/tasker_core/src/event_handler.rs
pub struct RubyEventHandler {
    event_subscriber: Arc<WorkerEventSubscriber>,
    worker_id: String,
    event_sender: mpsc::Sender<StepExecutionEvent>,  // DISPATCH CHANNEL
    channel_monitor: ChannelMonitor,
}

impl RubyEventHandler {
    pub async fn start(&self) -> TaskerResult<()> {
        // Subscribes to broadcast, forwards to MPSC (non-blocking)
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        // Fire-and-forget to dispatch channel
                        event_sender.send(event).await;
                    }
                    // ... error handling
                }
            }
        });
    }

    pub async fn handle_completion(&self, completion: StepExecutionCompletionEvent) {
        // COMPLETION flows back through separate path
        event_system.publish_step_completion(completion).await;
    }
}
```

This pattern should be generalized and moved to `tasker-worker`.

### Ruby Side Handler Resolution (`workers/ruby/lib/tasker_core/registry/`)

Ruby maintains its own handler registry that mirrors the pattern we need:

| File | Pattern |
|------|---------|
| `handler_registry.rb` | Singleton registry, template-driven discovery, `resolve_handler(name)` |
| `step_handler_resolver.rb` | Resolves handlers from task/step, creates instances with config |
| `event_bridge.rb` | dry-events pub/sub, wraps FFI events, validates completions |

Key methods we need to support in the shared `StepHandlerRegistry` trait:
- `get_handler(step)` → Returns callable handler
- `has_handler(name)` → Fast existence check
- `register(name, handler)` → Runtime registration

---

## What Moves to tasker-worker (Shared)

### 1. StepHandlerRegistry Trait (NEW)
Language-agnostic handler lookup interface.

### 2. StepHandler Trait (NEW)
Language-agnostic execution interface.

### 3. HandlerDispatchService (NEW)
Generalized from `RubyEventHandler` pattern:
- Receives dispatch messages from channel
- Resolves handler from registry
- Spawns bounded parallel execution
- Sends completions to completion channel

### 4. CompletionProcessorService (NEW)
Unified completion routing:
- Receives from completion channel
- Routes to orchestration via PGMQ
- Fires domain events

### 5. FFI Polling Adapter Pattern (NEW)
For FFI languages (Ruby, Python), handler invocation means:
1. Push dispatch message to bounded MPSC
2. Language runtime polls via `try_recv()`
3. Language runtime invokes handler
4. Language runtime calls completion FFI

This pattern exists in Ruby but isn't formalized. We'll extract:
- `FfiDispatchChannel` - typed channel for FFI polling
- `FfiCompletionReceiver` - completion submission interface

---

## What Stays Language-Specific

### Ruby (`workers/ruby/ext/tasker_core/src/`)
- Magnus FFI bindings (`init_bridge`, `define_singleton_method`)
- Ruby hash conversions (`conversions.rs`)
- `RubyBridgeHandle` structure (but slimmer, using shared services)
- dry-events integration (Ruby side)

### Rust (`workers/rust/`)
- `RustStepHandlerRegistry` (implements shared trait)
- 56 handler implementations
- Direct async call path (no polling needed)

### Python (TAS-72, future)
- PyO3 FFI bindings
- Python callable adaptation
- `PythonBridgeHandle` structure (modeled on Ruby)

---

## Proposed Architecture

### Dual-Channel Event Flow
```
┌────────────────────────────────────────────────────────────────────────┐
│                        STEP EXECUTION FLOW                              │
└────────────────────────────────────────────────────────────────────────┘

[1] WorkerEventSystem receives StepExecutionEvent
         ↓
[2] ActorCommandProcessor routes to StepExecutorActor
         ↓
[3] StepExecutorActor claims step, publishes to HANDLER DISPATCH CHANNEL
         ↓ (fire-and-forget, non-blocking)
[4] HandlerDispatchService receives from channel
         ↓
[5] Resolves handler from registry, invokes handler.call()
         ↓
[6] Handler completes, publishes to COMPLETION CHANNEL
         ↓
[7] CompletionProcessorService receives from channel
         ↓
[8] Routes to FFICompletionActor → Orchestration queue

TWO SEPARATE CHANNELS:
- Handler Dispatch Channel: StepExecutorActor → HandlerDispatchService
- Completion Channel: HandlerDispatchService → CompletionProcessorService
```

### New Abstractions for tasker-worker

#### 1. HandlerDispatchCommand (New Message Type)
```rust
// tasker-worker/src/worker/actors/messages.rs
pub struct DispatchHandlerMessage {
    pub event_id: Uuid,
    pub step_uuid: Uuid,
    pub task_sequence_step: TaskSequenceStep,
    pub correlation_id: Uuid,
    pub trace_context: Option<TraceContext>,
}

impl Message for DispatchHandlerMessage {
    type Response = ();  // Fire-and-forget
}
```

#### 2. HandlerDispatchService (New Service)
```rust
// tasker-worker/src/worker/services/handler_dispatch/service.rs
pub struct HandlerDispatchService {
    dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
    completion_sender: mpsc::Sender<StepExecutionResult>,
    handler_registry: Arc<dyn StepHandlerRegistry>,
}

impl HandlerDispatchService {
    pub async fn run(&mut self) {
        while let Some(msg) = self.dispatch_receiver.recv().await {
            let handler = self.handler_registry.get(&msg.task_sequence_step);
            let result = handler.call(&msg.task_sequence_step).await;
            self.completion_sender.send(result).await;
        }
    }
}
```

#### 3. StepHandlerRegistry Trait (New Trait)
```rust
// tasker-worker/src/worker/handlers/registry.rs
#[async_trait]
pub trait StepHandlerRegistry: Send + Sync {
    async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>>;
    fn register(&self, name: &str, handler: Arc<dyn StepHandler>);
}

#[async_trait]
pub trait StepHandler: Send + Sync {
    async fn call(&self, step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult>;
    fn name(&self) -> &str;
}
```

#### 4. CompletionProcessorService (Enhanced)
```rust
// tasker-worker/src/worker/services/completion_processor/service.rs
pub struct CompletionProcessorService {
    completion_receiver: mpsc::Receiver<StepExecutionResult>,
    ffi_completion_actor: Arc<FFICompletionActor>,
    domain_event_actor: Arc<DomainEventActor>,
}

impl CompletionProcessorService {
    pub async fn run(&mut self) {
        while let Some(result) = self.completion_receiver.recv().await {
            // Send to orchestration
            self.ffi_completion_actor.handle(SendStepResultMessage { result: result.clone() }).await;

            // Fire domain events (fire-and-forget)
            self.domain_event_actor.handle(DispatchDomainEventsMessage { result }).await;
        }
    }
}
```

---

## File Changes

### New Files to Create

```
tasker-worker/src/worker/
├── handlers/
│   ├── mod.rs                     # Handler abstractions
│   ├── registry.rs                # StepHandlerRegistry trait
│   └── dispatch_service.rs        # HandlerDispatchService
├── services/
│   └── completion_processor/
│       ├── mod.rs
│       └── service.rs             # CompletionProcessorService
```

### Files to Modify

| File | Changes |
|------|---------|
| `tasker-worker/src/worker/actors/messages.rs` | Add `DispatchHandlerMessage` |
| `tasker-worker/src/worker/actors/registry.rs` | Add dispatch/completion channels |
| `tasker-worker/src/worker/actors/step_executor_actor.rs` | Use dispatch channel instead of direct call |
| `tasker-worker/src/worker/actor_command_processor.rs` | Add third channel to `tokio::select!` |
| `tasker-worker/src/worker/mod.rs` | Export new modules |
| `workers/rust/src/event_handler.rs` | Implement `StepHandlerRegistry` trait |
| `workers/rust/src/step_handlers/registry.rs` | Implement trait, remove direct invocation |

### Configuration Changes

```toml
# config/tasker/base/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000

# config/tasker/environments/test/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 100
completion_buffer_size = 100
```

---

## Implementation Phases

### Phase 1: Core Abstractions (tasker-worker)
1. Create `StepHandlerRegistry` and `StepHandler` traits in `tasker-worker/src/worker/handlers/`
2. Create `DispatchHandlerMessage` type in `tasker-worker/src/worker/actors/messages.rs`
3. Create `HandlerDispatchService` with channel-based processing
4. Create `CompletionProcessorService`
5. Add configuration for new channels in `mpsc_channels.toml`
6. Create `FfiDispatchChannel` wrapper for polling-based languages

### Phase 2: Actor Integration
1. Modify `WorkerActorRegistry` to include dispatch/completion channels
2. Update `StepExecutorActor` to send to dispatch channel (fire-and-forget)
3. Update `ActorCommandProcessor` to spawn dispatch/completion services
4. Add third channel to `tokio::select!` loop

### Phase 3: Rust Worker Refactor
1. Implement `StepHandlerRegistry` trait in `workers/rust`
2. Adapt existing `RustStepHandler` trait to extend shared `StepHandler`
3. Remove blocking `.call()` from `workers/rust/src/event_handler.rs`
4. Wire up to new dispatch service

### Phase 4: Ruby FFI Migration
1. Refactor `workers/ruby/ext/tasker_core/src/event_handler.rs`:
   - Replace custom `RubyEventHandler` with shared `FfiDispatchChannel`
   - Use `HandlerDispatchService` from tasker-worker
2. Update `workers/ruby/ext/tasker_core/src/bridge.rs`:
   - Slim down `RubyBridgeHandle` to use shared services
   - Keep Magnus-specific FFI bindings
3. Update `workers/ruby/ext/tasker_core/src/bootstrap.rs`:
   - Wire up to new shared services
   - Maintain backward compatibility with Ruby side

### Phase 5: Validation & Testing
1. Add integration tests for non-blocking behavior (both Rust and Ruby)
2. Benchmark comparison with previous implementation
3. Verify domain event publishing still works
4. Test backpressure handling
5. Validate Ruby integration tests still pass
6. End-to-end tests with mixed Rust/Ruby handlers

### Phase 6: Behavior Mapping & Edge Case Analysis
Following the TAS-69 validation pattern, compare feature branch against main branch at `/Users/petetaylor/projects/tasker-systems/tasker-core-main`:

1. **Behavior Mapping** (create `docs/ticket-specs/TAS-67/behavior-mapping.md`)
   - Function-by-function tracing of dispatch flow (before/after)
   - Function-by-function tracing of completion flow (before/after)
   - Ruby FFI event handling flow comparison
   - Identify any behavioral differences requiring explicit handling

2. **Edge Cases & Risks** (create `docs/ticket-specs/TAS-67/edge-cases-and-risks.md`)
   - Concurrent handler execution edge cases
   - Backpressure scenarios (channel full)
   - Handler timeout behavior
   - FFI polling edge cases (Ruby, future Python)
   - Error propagation through dispatch/completion chain
   - Graceful degradation scenarios

3. **Gap Analysis**
   - Use parallel agents to:
     - Trace Rust worker dispatch path (main vs feature)
     - Trace Ruby FFI dispatch path (main vs feature)
     - Trace completion path (main vs feature)
   - Document any gaps found and fixes applied

4. **Verification Matrix**
   - All edge cases covered by unit or E2E tests
   - No silent behavioral changes
   - Performance characteristics preserved or improved

---

## Critical Files Reference

### tasker-worker (New Files)
```
tasker-worker/src/worker/
├── handlers/
│   ├── mod.rs                        # Handler abstractions module
│   ├── traits.rs                     # StepHandlerRegistry, StepHandler traits
│   ├── dispatch_service.rs           # HandlerDispatchService
│   └── ffi_dispatch_channel.rs       # FfiDispatchChannel for polling languages
├── services/
│   └── completion_processor/
│       ├── mod.rs
│       └── service.rs                # CompletionProcessorService
```

### tasker-worker (Modify)
- `tasker-worker/src/worker/actors/messages.rs` - Add `DispatchHandlerMessage`
- `tasker-worker/src/worker/actors/registry.rs` - Add dispatch/completion channels
- `tasker-worker/src/worker/actors/step_executor_actor.rs` - Use dispatch channel
- `tasker-worker/src/worker/actor_command_processor.rs` - Add third channel to `tokio::select!`
- `tasker-worker/src/worker/mod.rs` - Export new modules

### workers/rust (Refactor)
- `workers/rust/src/event_handler.rs` - Remove blocking loop, use dispatch service
- `workers/rust/src/step_handlers/mod.rs` - Adapt `RustStepHandler` to extend shared trait
- `workers/rust/src/step_handlers/registry.rs` - Implement `StepHandlerRegistry` trait
- `workers/rust/src/main.rs` - Wire up new architecture

### workers/ruby/ext (Refactor to use shared services)
- `workers/ruby/ext/tasker_core/src/event_handler.rs` - Replace with `FfiDispatchChannel`
- `workers/ruby/ext/tasker_core/src/bridge.rs` - Slim down, use shared services
- `workers/ruby/ext/tasker_core/src/bootstrap.rs` - Wire up shared services

### Configuration
- `config/tasker/base/mpsc_channels.toml`
- `config/tasker/environments/*/mpsc_channels.toml`

---

## FFI Dispatch Channel Abstraction (NEW)

For FFI languages (Ruby, Python), the handler dispatch pattern requires special handling because:
1. The foreign language runtime controls when handlers execute
2. We can't directly call into Ruby/Python from async Rust
3. Polling is the safe pattern for cross-language communication

### FfiDispatchChannel

```rust
// tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs

/// Generic dispatch channel for FFI-based workers
///
/// Provides a thread-safe, pollable interface for language runtimes
/// to receive step execution events and submit completions.
pub struct FfiDispatchChannel {
    /// Bounded channel for dispatch messages (Rust → FFI)
    dispatch_receiver: Mutex<mpsc::Receiver<DispatchHandlerMessage>>,
    /// Sender for completions (FFI → Rust)
    completion_sender: mpsc::Sender<StepCompletionMessage>,
    /// Channel monitor for observability
    channel_monitor: ChannelMonitor,
}

impl FfiDispatchChannel {
    /// Poll for next step execution event (non-blocking)
    /// Called by FFI layer (Ruby's poll_step_events, Python's poll_step_events)
    pub fn try_recv(&self) -> Option<DispatchHandlerMessage> {
        let mut receiver = self.dispatch_receiver.lock().ok()?;
        receiver.try_recv().ok()
    }

    /// Submit step completion (called after FFI handler completes)
    pub async fn send_completion(&self, completion: StepCompletionMessage) -> TaskerResult<()> {
        self.completion_sender.send(completion).await
            .map_err(|e| TaskerError::ChannelError(e.to_string()))
    }
}
```

### How Ruby Uses FfiDispatchChannel

```rust
// workers/ruby/ext/tasker_core/src/bridge.rs (simplified)

pub struct RubyBridgeHandle {
    pub system_handle: WorkerSystemHandle,
    // BEFORE: Custom RubyEventHandler with its own channel logic
    // AFTER: Use shared FfiDispatchChannel from tasker-worker
    pub ffi_dispatch: Arc<FfiDispatchChannel>,
    pub runtime: tokio::runtime::Runtime,
}

/// FFI function for Ruby to poll for step execution events
pub fn poll_step_events() -> Result<Value, Error> {
    let handle = get_worker_handle()?;

    // Use shared abstraction
    if let Some(msg) = handle.ffi_dispatch.try_recv() {
        convert_dispatch_message_to_ruby(msg)
    } else {
        Ok(ruby.qnil().as_value())
    }
}
```

### How Python Will Use FfiDispatchChannel (TAS-72)

```rust
// workers/python/src/bridge.rs (future)

pub struct PythonBridgeHandle {
    pub system_handle: WorkerSystemHandle,
    pub ffi_dispatch: Arc<FfiDispatchChannel>,  // Same abstraction!
    pub runtime: tokio::runtime::Runtime,
}

#[pyfunction]
fn poll_step_events(py: Python<'_>) -> PyResult<Option<PyObject>> {
    let handle = get_worker_handle()?;

    // Identical pattern to Ruby
    if let Some(msg) = handle.ffi_dispatch.try_recv() {
        convert_dispatch_message_to_python(py, msg)
    } else {
        Ok(None)
    }
}
```

---

## Benefits for TAS-72 (Python Workers)

This refactoring creates the foundation for Python workers:

1. **StepHandlerRegistry trait**: Python implements via PyO3
2. **FfiDispatchChannel**: Identical polling pattern to Ruby
3. **Dispatch channel pattern**: Same as Ruby's polling approach
4. **Completion flow**: Identical to Ruby FFI completion
5. **Configuration-driven**: Same TOML patterns

Python workers will only need to:
1. Create PyO3 FFI bindings (mirroring Ruby's Magnus bindings)
2. Implement `StepHandlerRegistry` via PyO3
3. Use `FfiDispatchChannel` from tasker-worker (no custom channel logic)
4. Wire up to existing dispatch/completion infrastructure

---

## Testing Strategy

### Unit Tests (tasker-worker)
- `StepHandlerRegistry` trait implementations
- `HandlerDispatchService` processes messages correctly
- `CompletionProcessorService` routes to actors
- `FfiDispatchChannel` polling and completion submission
- Backpressure handling when channels full

### Unit Tests (workers/rust)
- `RustStepHandlerRegistry` implements trait correctly
- Non-blocking dispatch verified
- Existing handlers work with new architecture

### Integration Tests (workers/rust)
- End-to-end step execution with new pattern
- Concurrent step handling (verify non-blocking)
- Domain event publishing after completion
- Benchmark: blocking vs non-blocking

### Integration Tests (workers/ruby)
- Ruby handlers still execute correctly
- Polling pattern works with `FfiDispatchChannel`
- Completion flow unchanged
- `spec/integration/` tests pass with new architecture

### Cross-Language Tests
- Mixed Rust/Ruby handler workflows
- Verify both languages use same completion path
- Performance comparison: Rust direct vs Ruby polling

### Performance Tests
- Benchmark vs blocking implementation
- Throughput under concurrent load
- Latency measurements
- Ruby polling overhead measurement

---

## Design Decisions (Confirmed)

### 1. Concurrency Model: Bounded Parallel Execution
Spawn up to N concurrent handlers using a semaphore, with N configurable via TOML.

```rust
// HandlerDispatchService with bounded parallelism
pub struct HandlerDispatchService {
    dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
    completion_sender: mpsc::Sender<StepExecutionResult>,
    handler_registry: Arc<dyn StepHandlerRegistry>,
    concurrency_semaphore: Arc<Semaphore>,  // Bounds parallel handlers
    handler_timeout: Duration,
}

impl HandlerDispatchService {
    pub async fn run(&mut self) {
        while let Some(msg) = self.dispatch_receiver.recv().await {
            let permit = self.concurrency_semaphore.clone().acquire_owned().await;
            let registry = self.handler_registry.clone();
            let sender = self.completion_sender.clone();
            let timeout = self.handler_timeout;

            tokio::spawn(async move {
                let _permit = permit;  // Held until task completes
                let result = Self::execute_with_timeout(&registry, &msg, timeout).await;
                let _ = sender.send(result).await;
            });
        }
    }
}
```

**Configuration**:
```toml
[mpsc_channels.worker.handler_dispatch]
max_concurrent_handlers = 10  # Semaphore permits
handler_timeout_ms = 30000    # 30 second default
```

### 2. Error Handling: Log AND Send Failure Result
On handler panic or error, both log the error and send a failure result to the completion channel.

```rust
async fn execute_with_timeout(
    registry: &Arc<dyn StepHandlerRegistry>,
    msg: &DispatchHandlerMessage,
    timeout: Duration,
) -> StepExecutionResult {
    let result = tokio::time::timeout(timeout, async {
        std::panic::catch_unwind(AssertUnwindSafe(|| async {
            let handler = registry.get(&msg.task_sequence_step).await?;
            handler.call(&msg.task_sequence_step).await
        })).await
    }).await;

    match result {
        Ok(Ok(Ok(step_result))) => step_result,
        Ok(Ok(Err(handler_error))) => {
            error!(step_uuid = %msg.step_uuid, error = %handler_error, "Handler returned error");
            StepExecutionResult::failure(msg.step_uuid, handler_error.to_string(), ...)
        }
        Ok(Err(panic_error)) => {
            error!(step_uuid = %msg.step_uuid, "Handler panicked: {:?}", panic_error);
            StepExecutionResult::failure(msg.step_uuid, "Handler panicked", ...)
        }
        Err(_timeout) => {
            error!(step_uuid = %msg.step_uuid, timeout_ms = timeout.as_millis(), "Handler timed out");
            StepExecutionResult::failure(msg.step_uuid, "Handler timed out", ...)
        }
    }
}
```

### 3. Timeout Handling: Configurable via TOML
Handlers are wrapped in `tokio::timeout` with configurable duration.

```toml
# config/tasker/base/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
handler_timeout_ms = 30000  # 30 seconds default

# config/tasker/environments/test/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
handler_timeout_ms = 5000   # 5 seconds for tests
```

---

## Updated Configuration Schema

```toml
# config/tasker/base/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 1000       # Channel from StepExecutorActor
completion_buffer_size = 1000     # Channel to CompletionProcessorService
max_concurrent_handlers = 10      # Semaphore permits for parallel execution
handler_timeout_ms = 30000        # Handler timeout in milliseconds

# config/tasker/environments/production/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 5000
completion_buffer_size = 5000
max_concurrent_handlers = 50      # Higher parallelism in production
handler_timeout_ms = 60000        # 60 seconds for production
```

---

## Summary: What Changes Where

### New Shared Abstractions (tasker-worker)

| Component | Purpose | Benefits |
|-----------|---------|----------|
| `StepHandlerRegistry` trait | Language-agnostic handler lookup | Rust, Ruby, Python use same interface |
| `StepHandler` trait | Language-agnostic execution | Consistent call signature across languages |
| `HandlerDispatchService` | Non-blocking dispatch with bounded parallelism | Eliminates blocking in hot path |
| `CompletionProcessorService` | Unified completion routing | Single path to orchestration |
| `FfiDispatchChannel` | Polling-based dispatch for FFI languages | Ruby/Python share identical pattern |
| `DispatchHandlerMessage` | Fire-and-forget dispatch message | Type-safe dispatch contract |

### Rust Worker Changes (workers/rust)

| Before | After |
|--------|-------|
| `handler.call(&step).await` blocks event loop | Dispatch message sent, handler spawned |
| `RustStepHandler` is local trait | Implements shared `StepHandler` trait |
| `RustStepHandlerRegistry` is local | Implements shared `StepHandlerRegistry` trait |

### Ruby FFI Changes (workers/ruby/ext)

| Before | After |
|--------|-------|
| Custom `RubyEventHandler` with channel logic | Uses shared `FfiDispatchChannel` |
| Channel setup in Ruby-specific code | Channel setup in tasker-worker |
| `poll_step_events()` uses custom receiver | Uses `FfiDispatchChannel::try_recv()` |

### Benefits Realized

1. **Non-blocking Rust handlers**: 10-100x throughput improvement potential
2. **Unified architecture**: Rust, Ruby, Python share same patterns
3. **Reduced code duplication**: Ruby FFI code becomes slimmer
4. **TAS-72 foundation**: Python workers need minimal new code
5. **Configuration-driven**: All channel sizes, timeouts via TOML
6. **Observable**: ChannelMonitor on all channels
7. **Backpressure**: Bounded channels prevent memory exhaustion

---

## Design Decisions (Resolved)

1. **FfiDispatchChannel naming**: ✅ **Fully replace `RubyEventHandler`** with `FfiDispatchChannel`. No thin wrapper needed - Ruby-specific logging can happen at the FFI boundary in `bridge.rs`.

2. **Phase 4 scope**: ✅ **Ruby FFI migration happens in TAS-67**. Migrating Ruby FFI alongside Rust ensures the abstractions in `tasker-worker` are properly language-agnostic and don't over-index on Rust-specific patterns.

3. **Validation approach**: ✅ **Phase 6 added** for behavior mapping and edge case analysis against main branch at `/Users/petetaylor/projects/tasker-systems/tasker-core-main`, following the TAS-69 pattern.

---

## Related Tickets

- **TAS-69**: Worker system refactored to command-actor-service pattern (completed)
- **TAS-51**: Bounded MPSC channels with configuration (completed)
- **TAS-72**: Python/PyO3 workers (future, depends on TAS-67)
