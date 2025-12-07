# TAS-67 Handler Dispatch Flow Mapping

**Last Updated**: 2025-12-06
**Comparison**: Main branch (`tasker-core-main`) vs Feature branch (`tasker-core`)

---

## Executive Summary

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| **Dispatch Pattern** | Broadcast channel + event subscription | Dual-channel with bounded MPSC |
| **Blocking Behavior** | Event listener blocks on handler | Fire-and-forget to dispatch channel |
| **Concurrency Control** | Unbounded tokio tasks | Bounded semaphore (configurable) |
| **Timeout Handling** | None | 30s configurable with panic catching |
| **FFI Support** | Broadcast subscription | FfiDispatchChannel with polling |

---

## Main Branch Behavior

### Flow Overview

```
WorkerEventSystem
    ↓ (broadcast)
StepExecutorService.fire_step_execution_event()
    ↓ (broadcast::send)
RustEventHandler listener task (spawned)
    ↓ (receiver.recv())
handler.call() ← BLOCKING POINT
    ↓
Result published to event system
```

### Key Files

**`tasker-worker/src/worker/services/step_execution/service.rs`**
- Line 135-247: Main `execute_step()` method
- Line 313-353: `execute_ffi_handler()` - fires event to FFI handlers

**`workers/rust/src/event_handler.rs`**
- Event subscription loop
- Spawns async task that loops on broadcast receiver
- Blocks on `receiver.recv().await` until handler completes

### Blocking Analysis

The main branch uses a **push-based event model**:
1. `StepExecutorService` publishes event to broadcast channel
2. Listener task receives event via `receiver.recv().await`
3. Handler invoked inline: `handler.call(&step).await`
4. Listener task blocked until handler completes
5. Next event not processed until current handler finishes

**Concurrency**: Unbounded - all handlers spawned as concurrent tokio tasks with no semaphore limit.

---

## Feature Branch Behavior

### Flow Overview

```
DispatchHandlerMessage
    ↓ (mpsc channel)
HandlerDispatchService.run()
    ↓ (dispatch_receiver.recv())
Semaphore.acquire() ← CONCURRENCY GATE
    ↓
tokio::spawn(handler execution)
    ↓ (fire-and-forget)
HandlerDispatchService continues immediately
    ↓
Handler completes in background
    ↓
Result sent to completion_sender
    ↓
PostHandlerCallback invoked
```

### Key Files

**`tasker-worker/src/worker/handlers/dispatch_service.rs`**
- Line 229-315: `HandlerDispatchService::run()` main loop
- Line 252-264: Semaphore acquisition for bounded parallelism
- Line 276-307: Result send and callback ordering
- Line 318-476: `execute_with_timeout()` with panic catching

**`tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`**
- Line 244-296: `poll()` for FFI languages
- Line 360-438: `complete()` with ordering guarantees

**`tasker-worker/src/worker/core.rs`**
- Line 308-312: Dispatch channel creation
- Line 319-322: `max_concurrent_handlers` from configuration

### Non-Blocking Design

The feature branch uses a **pull-based dispatch model**:
1. `HandlerDispatchService` receives from bounded MPSC channel
2. Semaphore controls concurrent handler execution (default 10)
3. Handler spawned via `tokio::spawn()` - fire-and-forget
4. Service immediately returns to receive next message
5. Handler completes in background, sends to completion channel
6. Callback invoked AFTER completion send succeeds

**Concurrency**: Bounded by semaphore (configurable via TOML).

---

## Behavioral Differences

### 1. Dispatch Latency

| Main | Feature |
|------|---------|
| Very low (broadcast) | Low (bounded mpsc) |
| No backpressure | Backpressure via channel bounds |

### 2. Handler Concurrency

| Main | Feature |
|------|---------|
| Unbounded tokio tasks | Bounded semaphore (default 10) |
| Memory risk under load | Predictable resource usage |

### 3. Timeout Handling

| Main | Feature |
|------|---------|
| None | 30s configurable |
| Handlers can hang forever | Timeout generates failure result |

### 4. Panic Handling

| Main | Feature |
|------|---------|
| Tokio catches (silent) | Explicit `catch_unwind()` |
| No failure result generated | Failure result with panic message |

### 5. Domain Event Ordering

| Main | Feature |
|------|---------|
| Events fire during execution | Events fire AFTER completion send |
| Race condition possible | Ordering guaranteed |

### 6. FFI Support

| Main | Feature |
|------|---------|
| Broadcast listener | Polling + async bridge |
| No timeout detection | Configurable cleanup |

---

## Potential Concerns

### 1. Dispatch Sender Location

The analysis did not find where `dispatch_sender.send()` is called. Need to verify:
- How messages enter the dispatch channel
- Backpressure handling when channel is full
- Error handling if send fails

### 2. Semaphore Poisoning

If semaphore is closed or drops:
- Line 260: `let Ok(permit) = semaphore.acquire_owned().await else { return };`
- Handler task silently skips with no failure result
- Orchestration won't know step was never executed

### 3. FFI Cleanup Responsibility

`FfiDispatchChannel` requires periodic `cleanup_timeouts()` calls:
- Not automatic - application must call
- Without cleanup, pending_events map grows unbounded
- Memory leak if cleanup never called

### 4. Async from Sync (FFI)

FFI completion uses `block_on()` from non-tokio thread:
- Line 413: `self.config.runtime_handle.block_on(...)`
- Could deadlock if callback holds locks
- Blocks FFI thread until callback completes

---

## Configuration

```toml
# config/tasker/base/mpsc_channels.toml
[mpsc_channels.worker.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000
max_concurrent_handlers = 10
handler_timeout_ms = 30000
```

---

## Summary

The feature branch introduces a **significant architectural improvement**:

1. **Non-blocking dispatch**: Fire-and-forget pattern eliminates blocking
2. **Bounded concurrency**: Semaphore prevents resource exhaustion
3. **Timeout safety**: Configurable timeout with failure result generation
4. **Ordering guarantees**: Domain events fire only after completion succeeds

**Trade-off**: Adds complexity with dual-channel architecture, but provides predictable resource usage and proper ordering guarantees.
