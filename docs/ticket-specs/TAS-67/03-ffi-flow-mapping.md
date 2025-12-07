# TAS-67 FFI (Ruby) Flow Mapping

**Last Updated**: 2025-12-06
**Comparison**: Main branch (`tasker-core-main`) vs Feature branch (`tasker-core`)

---

## Executive Summary

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| **Dispatch Model** | Push (RubyEventHandler) | Pull (FfiDispatchChannel) |
| **Event Correlation** | Implicit | Explicit UUID |
| **Completion API** | Fire-and-forget | Tracked + callback |
| **Timeout Detection** | None | 30s configurable |
| **Domain Events** | Race condition risk | Ordered guarantee |

---

## Main Branch Behavior

### Dispatch Path

**`workers/ruby/ext/tasker_core/src/event_handler.rs`**

1. **RubyEventHandler Setup** (Line 21-59)
   - Creates bounded MPSC channel with configurable buffer size
   - Subscribes to worker event system via `WorkerEventSubscriber`
   - Returns `(handler, event_receiver)` tuple

2. **Event Subscription Loop** (Line 61-112)
   - Spawns async task that loops on `event_subscriber.subscribe_to_step_executions()`
   - Receives broadcast events from WorkerEventSystem
   - Sends `StepExecutionEvent` to MPSC channel for Ruby polling
   - No direct Ruby callback; entirely channel-based

**`workers/ruby/ext/tasker_core/src/bridge.rs`**

3. **Ruby Polling** (Line 76-81)
   - `RubyBridgeHandle::poll_step_event()` uses non-blocking `try_recv()`
   - Returns `Option<StepExecutionEvent>` (full event with complete payload)

4. **FFI Poll Function** (Line 86-130)
   - `poll_step_events()` calls `handle.poll_step_event()`
   - Converts `StepExecutionEvent` to Ruby hash via `convert_step_execution_event_to_ruby()`
   - Returns Ruby hash or nil

### Completion Path

**`workers/ruby/ext/tasker_core/src/event_handler.rs`** (Line 137-179)

1. Ruby calls `send_step_completion_event(completion_data)` after handler execution
2. Converts Ruby completion to `StepExecutionCompletionEvent`
3. Locks `WORKER_SYSTEM` to access event handler
4. Calls `handle.event_handler().handle_completion(rust_completion)`
5. Event handler publishes to global event system

**Bridge Handle Storage** (Line 24-82):
- Stores `RubyEventHandler` directly
- Stores MPSC `Receiver<StepExecutionEvent>` wrapped in `Arc<Mutex>`
- No mechanism for sending completions back through FFI layer
- Completions are fire-and-forget to global event system

---

## Feature Branch Behavior

### Dispatch Path

**`workers/ruby/ext/tasker_core/src/bootstrap.rs`** (Line 128-154)

1. **FfiDispatchChannel Setup**
   - Takes `DispatchHandles` from `WorkerSystemHandle`
   - Creates `DomainEventCallback` with `StepEventPublisherRegistry`
   - Constructs `FfiDispatchChannel` with:
     - `dispatch_handles.dispatch_receiver`
     - `dispatch_handles.completion_sender`
     - Runtime handle for async callback execution
     - Domain event callback

**`tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`** (Line 244-296)

2. **Event Subscription**
   - `poll()` uses non-blocking `try_lock()` + `try_recv()`
   - Receives `DispatchHandlerMessage` from Rust worker core
   - Converts to `FfiStepEvent` via `FfiStepEvent::from_dispatch_message()`
   - Returns `Option<FfiStepEvent>` with:
     - `event_id`: UUID for correlation
     - `task_uuid`, `step_uuid`, `correlation_id`: Identifiers
     - `execution_event`: Full `StepExecutionEvent` payload
     - `trace_id`, `span_id`: Optional trace context

**`workers/ruby/ext/tasker_core/src/bridge.rs`** (Line 70-74, 87-133)

3. **Ruby Polling**
   - `poll_step_event()` calls `ffi_dispatch_channel.poll()`
   - `poll_step_events()` converts `FfiStepEvent` to Ruby hash
   - Hash includes `event_id` string for completion correlation

### Completion Path

**`workers/ruby/ext/tasker_core/src/bridge.rs`** (Line 141-186)

1. Ruby calls `complete_step_event(event_id_str, completion_data)` with event correlation
2. Parses `event_id_str` as UUID
3. Converts Ruby completion to `StepExecutionResult`
4. Calls `handle.complete_step_event(event_id, result)`

**`tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`** (Line 360-438)

5. **Two-Phase Completion**:

   **Phase 1: Completion Send** (Line 391-399)
   - Looks up `event_id` in `pending_events` map
   - Sends `StepExecutionResult` to completion channel via `blocking_send()`
   - Ensures result is committed to pipeline BEFORE domain events fire

   **Phase 2: Domain Event Callback** (Line 401-416)
   - After successful send, invokes `post_handler_callback.on_handler_complete()`
   - Uses stored runtime handle: `self.config.runtime_handle.block_on()`
   - Domain events only fire AFTER result is in pipeline

6. **Timeout Management** (Line 528-579)
   - Tracks dispatch time for each pending event
   - `cleanup_timeouts()` detects handlers that never completed
   - Sends timeout failure result to completion channel

---

## API Compatibility

### FFI Function Signatures

| Function | Main Branch | Feature Branch | Breaking? |
|----------|-------------|----------------|-----------|
| **Polling** | `poll_step_events() -> Value` | `poll_step_events() -> Value` | No |
| **Completion** | `send_step_completion_event(data: Value)` | `complete_step_event(event_id: String, data: Value)` | **YES** |
| **Return Type** | Nil (fire-and-forget) | `bool` (success indicator) | **YES** |
| **Event ID** | Implicit in completion data | Explicit parameter | **YES** |

### Polled Event Structure

| Field | Main Branch | Feature Branch |
|-------|-------------|----------------|
| `event_id` | In payload | In root hash (UUID string) |
| `task_sequence_step` | Root level | Still at root (via conversion) |
| `trace_id`, `span_id` | In root | In root + in payload |
| Correlation | Implicit | Explicit `correlation_id`, `event_id` |

---

## Behavioral Differences

### 1. Dispatch Architecture

| Main | Feature |
|------|---------|
| Push-based event forwarding | Pull-based dispatch |
| WorkerEventSystem publishes events | FfiDispatchChannel pulls on demand |
| Ruby polls MPSC for converted events | Only converts when Ruby polls |

### 2. Completion Flow

| Main | Feature |
|------|---------|
| No correlation between submission and acknowledgment | Explicit event_id correlation |
| Fire-and-forget: Ruby sends, Rust publishes | Tracked completion with pending event map |
| No timeout detection | Timeout detection (30s default) |
| No success/failure feedback to Ruby | Returns bool indicating success |

### 3. Domain Event Publishing

| Main | Feature |
|------|---------|
| Events published immediately | Events published AFTER completion send |
| No coordination with pipeline | Two-phase: commit result first, then events |
| Race condition: events fire before result stored | Ordering guaranteed |

### 4. Thread Safety

| Main | Feature |
|------|---------|
| Async event handler loop runs independently | Polling and completion coordinated |
| Ruby polls from separate thread | Pending event tracking prevents double-complete |
| Completion uses block_on for single async call | Runtime handle stored for callbacks |

---

## Potential Concerns

### 1. Breaking Change: Completion API

- **Risk**: Existing Ruby code calling `send_step_completion_event` will break
- Function signature changed (1 → 2 parameters)
- Return type changed (nil → bool)
- Function renamed to `complete_step_event`

### 2. Event Structure in Ruby

- New function `convert_ffi_step_event_to_ruby()` still exposes `task_sequence_step` at root
- Backwards-compatible structure despite different internal organization
- Includes correlation fields for easy access

### 3. Double-Complete Prevention

- Ruby calls complete twice for same event_id
- First call: removes from pending, sends result, invokes callback
- Second call: lookup fails, warns and returns false
- Explicit check prevents duplicate processing

### 4. Callback Execution from FFI Thread

- Ruby FFI threads don't have Tokio runtime
- `block_on()` on stored runtime handle allows callback execution
- Blocks Ruby thread until callback completes

### 5. Event Loss on Channel Disconnect

- Pending events map not cleared if channel disconnects
- `cleanup_timeouts()` removes after timeout (30s)
- FFI thread continues (parent language runtime owns lifecycle)

---

## Summary of Key Changes

| Aspect | Main | Feature | Impact |
|--------|------|---------|--------|
| **Dispatch Model** | Push (RubyEventHandler) | Pull (FfiDispatchChannel) | More efficient |
| **Event Correlation** | Implicit | Explicit UUID | Better error handling |
| **Completion API** | Fire-and-forget | Tracked + callback | Breaking change |
| **Timeout Detection** | None | 30s configurable | Prevents stuck handlers |
| **Domain Events** | Race condition risk | Ordered guarantee | Correctness improvement |

The feature branch represents a significant architectural improvement focused on **explicit correlation, proper ordering, and timeout safety** for FFI dispatch, at the cost of **API incompatibility** that requires Ruby code updates.
