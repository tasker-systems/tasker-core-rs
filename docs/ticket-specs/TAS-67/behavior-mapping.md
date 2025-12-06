# TAS-67: Behavior Mapping - Main vs Feature Branch

## Overview

This document compares the step execution flow between the `main` branch (legacy approach) and the `jcoletaylor/tas-67-rust-worker-dual-event-system` feature branch (new dual-channel dispatch approach).

## Executive Summary

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| Step Dispatch | Direct handler.call() blocking | Dual-channel fire-and-forget |
| Concurrency | Sequential within worker | Bounded parallel (configurable) |
| FFI Integration | RubyEventHandler | FfiDispatchChannel |
| Completion Flow | Inline return | Separate completion channel |
| Timeout Handling | Handler-level | Channel-level with cleanup |

---

## Step Execution Flow Comparison

### Main Branch: Blocking Pattern

```
WorkerEventSystem receives StepExecutionEvent
         │
         ▼
StepExecutorActor claims step
         │
         ▼
Direct handler.call() ◄──── BLOCKS HERE until complete
         │
         ▼
Result returned inline
         │
         ▼
FFICompletionActor sends to orchestration
```

**Key Characteristics:**
- Single-threaded step processing
- Handler execution blocks the event loop
- Cannot process next step until current finishes
- Simple but limits throughput

### Feature Branch: Non-Blocking Dual-Channel Pattern

```
WorkerEventSystem receives StepExecutionEvent
         │
         ▼
StepExecutorActor claims step
         │
         ▼
Send to DISPATCH CHANNEL ◄──── Fire-and-forget, non-blocking
         │
         └─────────────────────────────────────┐
                                               ▼
                               HandlerDispatchService receives
                                               │
                                               ▼
                               Spawns handler.call() with semaphore
                                               │
                                               ▼
                               Handler completes
                                               │
                                               ▼
                               Send to COMPLETION CHANNEL
                                               │
                                               ▼
                               CompletionProcessorService receives
                                               │
                                               ▼
                               FFICompletionActor sends to orchestration
```

**Key Characteristics:**
- Non-blocking dispatch
- Bounded concurrent handlers (semaphore-controlled)
- Separate completion path
- Handler timeout protection

---

## FFI Integration Changes

### Main Branch: RubyEventHandler

```rust
// workers/ruby/ext/tasker_core/src/event_handler.rs
pub struct RubyEventHandler {
    event_sender: mpsc::Sender<StepExecutionEvent>,
    // ... internal channel management
}

impl RubyEventHandler {
    pub fn new() -> (Self, mpsc::Receiver<StepExecutionEvent>) {
        // Creates its own internal channel
    }
}
```

- Creates internal channel
- Ruby polls via `poll_step_events()`
- Completion sent via `send_step_completion_event()`
- Tight coupling between event reception and dispatch

### Feature Branch: FfiDispatchChannel

```rust
// tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs
pub struct FfiDispatchChannel {
    dispatch_receiver: Arc<Mutex<mpsc::Receiver<DispatchHandlerMessage>>>,
    completion_sender: mpsc::Sender<StepExecutionResult>,
    pending_events: Arc<RwLock<HashMap<Uuid, PendingEvent>>>,
    config: FfiDispatchChannelConfig,
}

impl FfiDispatchChannel {
    pub fn new(
        dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
        completion_sender: mpsc::Sender<StepExecutionResult>,
        config: FfiDispatchChannelConfig,
    ) -> Self { ... }

    pub fn poll(&self) -> Option<FfiStepEvent> { ... }
    pub fn complete(&self, event_id: Uuid, result: StepExecutionResult) -> bool { ... }
}
```

- Receives channels from DispatchHandles (WorkerCore owns creation)
- Event correlation via event_id
- Pending event tracking with timeout cleanup
- Clean separation between dispatch and completion

---

## Ruby Worker Bootstrap Changes

### Main Branch

```rust
// bootstrap.rs - Creates RubyEventHandler with internal channel
let (event_handler, event_receiver) = RubyEventHandler::new();
// Worker processes events via event_receiver
```

### Feature Branch

```rust
// bootstrap.rs - Uses FfiDispatchChannel from DispatchHandles
let dispatch_handles = system_handle.take_dispatch_handles().unwrap();
let ffi_dispatch_channel = FfiDispatchChannel::new(
    dispatch_handles.dispatch_receiver,
    dispatch_handles.completion_sender,
    config,
);
```

**Key Difference:** Channel ownership moves from Ruby FFI layer to WorkerCore, matching the Rust worker architecture.

---

## Message Format Changes

### Main Branch: StepExecutionEvent

```rust
pub struct StepExecutionEvent {
    pub event_id: Uuid,
    pub payload: StepEventPayload,
    // ... trace context
}
```

### Feature Branch: FfiStepEvent

```rust
pub struct FfiStepEvent {
    pub event_id: Uuid,          // Critical for completion correlation
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub correlation_id: Uuid,
    pub execution_event: StepExecutionEvent,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}
```

**Key Addition:** `event_id` is now exposed at the top level for completion correlation. Ruby uses this ID when calling `complete_step_event(event_id, result)`.

---

## Completion Flow Changes

### Main Branch

```ruby
# Ruby side
result = handler.execute(step_data)
TaskerCore::FFI.send_step_completion_event(step_uuid, result_hash)
```

### Feature Branch

```ruby
# Ruby side
event = TaskerCore::FFI.poll_step_events
result = handler.execute(event['task_sequence_step'])
TaskerCore::FFI.complete_step_event(event['event_id'], result_hash)
```

**Key Difference:** Completion is now correlated by `event_id` rather than `step_uuid`, enabling better tracking of in-flight events and timeout detection.

---

## Configuration Changes

### New Configuration Options (Feature Branch)

```toml
# config/tasker/base/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 1000       # Dispatch channel capacity
completion_buffer_size = 1000     # Completion channel capacity
max_concurrent_handlers = 10      # Semaphore permits for parallel execution
handler_timeout_ms = 30000        # Handler timeout in milliseconds
```

---

## Test Compatibility

| Test Category | Main | Feature | Notes |
|--------------|------|---------|-------|
| Ruby FFI Unit Tests | ✅ | ✅ | 226 tests pass |
| Rust Worker E2E | ✅ | ✅ | All workflow patterns work |
| Ruby Worker E2E (Docker) | ✅ | ⚠️ | Requires Docker image rebuild |
| Conversion Tests | ✅ | ✅ | New FfiStepEvent conversions |

---

## Backward Compatibility

### Breaking Changes

1. **RubyEventHandler removed** - Replaced by FfiDispatchChannel
2. **`send_step_completion_event` removed** - Replaced by `complete_step_event`
3. **Ruby event hash structure** - Now includes `event_id` at top level

### Migration Path

Ruby handlers need to update from:
```ruby
TaskerCore::FFI.send_step_completion_event(step_uuid, result)
```

To:
```ruby
TaskerCore::FFI.complete_step_event(event['event_id'], result)
```

---

## Performance Implications

### Expected Improvements

1. **Increased Throughput** - Non-blocking dispatch enables concurrent step processing
2. **Better Resource Utilization** - Configurable concurrency via semaphore
3. **Predictable Latency** - Timeout handling prevents handler stalls

### Potential Concerns

1. **Memory Usage** - Pending event tracking adds per-event overhead
2. **Complexity** - Dual-channel pattern has more moving parts
3. **Debugging** - Event correlation requires understanding event_id flow

---

## Summary

The TAS-67 changes transform the worker from a blocking, sequential executor to a non-blocking, concurrent system. The key architectural shift is moving from direct handler invocation to a dual-channel dispatch pattern that:

1. Decouples event reception from handler execution
2. Enables bounded parallelism
3. Provides timeout protection
4. Unifies Rust and FFI worker architectures

This aligns with the original design goal: making the Rust worker match Ruby's paradoxically-faster event-driven approach.
