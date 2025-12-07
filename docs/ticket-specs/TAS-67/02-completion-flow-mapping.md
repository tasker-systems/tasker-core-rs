# TAS-67 Completion Flow Mapping

**Last Updated**: 2025-12-06
**Comparison**: Main branch (`tasker-core-main`) vs Feature branch (`tasker-core`)

---

## Executive Summary

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| **Completion Path** | Implicit through event publisher | Explicit completion channel |
| **Domain Event Timing** | During handler execution | AFTER completion channel succeeds |
| **Ordering Guarantee** | Race condition possible | Explicit: result → callback → events |
| **Backpressure** | Via event publisher | Via bounded completion channel |
| **Failure Semantics** | Events could fail step | Events do NOT fail step |

---

## Main Branch Behavior

### Flow Overview

```
Handler completes
    ↓
StepExecutorService.execute_ffi_handler()
    ↓
event_publisher.fire_step_execution_event_with_trace()
    ↓
shared_publisher.publish_step_execution()
    ↓
Event fired to FFI handlers (async)
    ↓
Result sent to orchestration queue
```

### Key Files

**`tasker-worker/src/worker/services/step_execution/service.rs`**
- Line 229-231: `fire_step_execution_event_with_trace()` called
- Line 335-338: Event published to shared publisher

**`tasker-worker/src/worker/event_publisher.rs`**
- Line 118-145: Event fired asynchronously to FFI handlers

### Issues

1. **No explicit separation** between handler dispatch and completion
2. Events fire **immediately** to FFI handlers
3. Domain events handled **implicitly** as part of event publishing
4. **Potential race condition**: Domain events fire before result is stored

---

## Feature Branch Behavior

### Flow Overview

```
Handler completes
    ↓
HandlerDispatchService creates StepExecutionResult
    ↓
sender.send(result.clone()).await  ← COMMIT TO PIPELINE
    ↓ (on success)
PostHandlerCallback.on_handler_complete()
    ↓
DomainEventCallback publishes events
    ↓
CompletionProcessorService receives from channel
    ↓
FFICompletionService.send_step_result()
    ↓
Result sent to orchestration queue
```

### Key Files

**`tasker-worker/src/worker/handlers/dispatch_service.rs`**
- Line 273-274: Handler execution via `execute_with_timeout()`
- Line 278-284: **CRITICAL ORDERING** - result send BEFORE callback

```rust
// 1. Send result to completion channel FIRST
match sender.send(result.clone()).await {
    Ok(()) => {
        // 2. Invoke post-handler callback AFTER successful send
        if let Some(ref cb) = callback {
            cb.on_handler_complete(&msg.task_sequence_step, &result, &service_id).await;
        }
    }
    Err(e) => {
        // Channel closed - domain events NOT fired
    }
}
```

**`tasker-worker/src/worker/handlers/domain_event_callback.rs`**
- Line 64-145: `DomainEventCallback.on_handler_complete()`
- Reads step config for `publishes_events`
- Gets publisher from registry
- Publishes events asynchronously
- Failures logged but don't fail step

**`tasker-worker/src/worker/handlers/completion_processor.rs`**
- Line 101-143: `CompletionProcessorService.run()` receives from channel
- Calls `FFICompletionService.send_step_result()`

**`tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`**
- Line 391-399: **FFI Ordering** - result send FIRST
- Line 401-416: Callback invoked AFTER successful send

```rust
// 1. Send to completion channel FIRST (blocking)
match self.completion_sender.blocking_send(result.clone()) {
    Ok(()) => {
        // 2. Invoke post-handler callback AFTER successful send
        self.config.runtime_handle.block_on(
            self.post_handler_callback.on_handler_complete(&step, &result, &worker_id)
        );
    }
    Err(e) => {
        // Channel closed - domain events NOT fired
    }
}
```

---

## Domain Event Ordering

### Main Branch

- Events fire **IMPLICITLY** through the event publishing system
- Events published when `fire_step_execution_event()` is called
- No explicit separation between handler completion and event publishing
- **Race condition**: If orchestration processes result before domain events fire

### Feature Branch (TAS-67 Design)

- Events fire **AFTER** result is successfully sent to completion channel
- Explicit ordering guarantee: `result.send() → callback.on_handler_complete() → events`
- If completion channel send fails, domain events are NOT published
- If domain event publishing fails, it does NOT fail the step completion
- Domain events are decoupled from step completion via fire-and-forget pattern

### Key Difference

- **Main**: Events fire TO handlers during step execution
- **Feature**: Results are committed to pipeline FIRST, then domain events published

---

## Behavioral Differences

| Aspect | Main Branch | Feature Branch |
|--------|-------------|----------------|
| **Handler Dispatch** | Direct FFI event publishing | Dual-channel pattern |
| **Result Delivery** | Implicit through event publisher | Explicit completion channel |
| **Domain Event Timing** | During handler execution | After completion channel succeeds |
| **Ordering Guarantee** | Implicit/Race condition possible | Explicit: result → callback → events |
| **Backpressure Handling** | Via event publisher | Via bounded completion channel |
| **Failure Semantics** | Domain event failures could fail step | Domain event failures do NOT fail step |
| **Decoupling** | Tightly coupled to event system | Decoupled via PostHandlerCallback trait |
| **Rust vs FFI** | Unified event publishing | Separate dispatch services + shared callback |

---

## Potential Concerns

### 1. Ordering Guarantee Enforcement

- Feature branch explicitly guarantees `result.send()` before domain events
- **Risk**: If callback invoked before result actually processed by CompletionProcessorService, orchestration might see result before events
- **Mitigation**: Result is buffered in bounded channel, callback runs after buffering succeeds

### 2. Backpressure on Completion Channel

If completion channel is FULL (bounded):
- `sender.send()` blocks (async) or `blocking_send()` blocks (FFI)
- During backpressure:
  - HandlerDispatchService tasks block
  - Domain event callbacks are delayed
  - FFI handlers get blocked

**Mitigation**: Configured buffer sizes via TOML, ChannelMonitor for observability

### 3. Domain Event Publishing Failures

- **Main Branch**: Would likely cause step failure
- **Feature Branch**: Logged but don't fail step (fire-and-forget)

**Trade-off**: Improved resilience vs potential lost domain events

### 4. FFI Thread Safety

- Feature branch FfiDispatchChannel runs from FFI thread (Ruby/Python)
- Uses `runtime_handle.block_on()` to call async callback from non-Tokio thread

**Mitigation**: Runtime handle stored in config, required at creation time

### 5. Completion Channel Closure

If completion channel closes (system shutdown):
- **Main**: Event publishing continues
- **Feature**: Domain events are NOT published (error logged)

**Mitigation**: Graceful shutdown waits for completion processor to drain

---

## Summary

The TAS-67 refactor introduces a **non-blocking dual-channel architecture** with explicit ordering guarantees:

**Main Advantage**: Result is committed to the completion pipeline **before** domain events fire, ensuring orchestration doesn't race domain event publishing.

**Main Trade-off**: Adds one more async hop (completion channel → CompletionProcessorService) but provides better decoupling and resilience through failure isolation.

**Key Risk**: Backpressure on bounded completion channel could block handler dispatch tasks if orchestration can't keep up with results.
