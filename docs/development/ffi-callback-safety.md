# FFI Callback Safety Guidelines (TAS-67)

**Last Updated**: 2025-12-07
**Status**: Production Implementation
**Applies To**: Ruby FFI workers, future Python/PyO3 workers

---

## Overview

When FFI workers (Ruby, Python) execute step handlers, they call back into Rust to submit completion results. This document describes the safety considerations and implementation patterns used to prevent deadlocks and ensure reliable operation.

## Architecture

### Completion Flow

```
Ruby Handler Execution
         │
         ▼
complete_step_event() ─────────────────────────────────┐
         │                                              │
         ▼                                              │
FfiDispatchChannel.complete()                          │
         │                                              │
         ├──► Remove from pending_events               │
         │                                              │
         ├──► Send result to completion channel        │
         │    (try_send with retry loop)               │
         │                                              │
         └──► Spawn post-handler callback ◄────────────┘
              (fire-and-forget)
         │
         ▼
Return to Ruby immediately
```

### Critical Constraint: Callbacks Execute from FFI Thread Context

When Ruby calls `complete_step_event()`, the Rust code runs in Ruby's thread context. This creates potential for deadlocks if callbacks try to acquire locks that Ruby's thread might hold.

## Fire-and-Forget Callback Strategy

**Implementation**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

The post-handler callback (which fires domain events) uses a fire-and-forget pattern:

```rust
// Spawn callback as fire-and-forget - don't block Ruby thread
self.config.runtime_handle.spawn(async move {
    if let Err(e) = tokio::time::timeout(
        callback_timeout,
        callback.on_handler_complete(&step, &result, &worker_id),
    ).await {
        error!(
            event_id = %event_id,
            error = %e,
            "Post-handler callback failed or timed out"
        );
    }
});

// Return immediately - Ruby thread not blocked
```

### Why Fire-and-Forget?

1. **Ruby thread unblocked**: Handler completion returns immediately
2. **No deadlock risk**: Callback runs in Tokio runtime, not Ruby's thread
3. **Isolated failures**: Callback errors don't affect step result delivery
4. **Timeout protection**: 5-second timeout prevents runaway callbacks

## Safe Operations in Callbacks

The `DomainEventCallback.on_handler_complete()` can safely perform:

| Operation | Safe? | Notes |
|-----------|-------|-------|
| Read `Arc<T>` data | Yes | Read-only, no lock contention |
| `mpsc::Sender.send()` | Yes | Channel operations are safe |
| Logging/tracing | Yes | Independent subsystem |
| Atomic operations | Yes | Lock-free by design |
| HTTP requests | Yes* | With timeout |
| Database queries | Yes* | With timeout |

*Always use timeouts for I/O operations.

## Unsafe Operations (May Cause Deadlock)

| Operation | Risk | Alternative |
|-----------|------|-------------|
| Acquire `tokio::sync::Mutex` | HIGH | Use channels instead |
| Acquire `tokio::sync::RwLock` | HIGH | Use channels instead |
| Acquire `Semaphore` | HIGH | Don't acquire in callbacks |
| `.await` without timeout | MEDIUM | Always use `tokio::time::timeout` |
| Call back to Ruby | CRITICAL | Never do this |
| Long-running sync I/O | MEDIUM | Use async with timeout |

### Example: Deadlock Scenario (Prevented)

```rust
// WOULD DEADLOCK (if not fire-and-forget):
// 1. Ruby thread holds WORKER_SYSTEM lock
// 2. Calls complete_step_event() → callback
// 3. Callback tries to call Ruby → needs WORKER_SYSTEM lock
// 4. Deadlock!

// SAFE (fire-and-forget):
// 1. Ruby thread holds WORKER_SYSTEM lock
// 2. Calls complete_step_event() → spawns callback in background
// 3. Returns immediately, Ruby releases lock
// 4. Callback runs independently in Tokio runtime
```

## Configuration

**File**: `config/tasker/base/mpsc_channels.toml`

```toml
[mpsc_channels.worker.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
callback_timeout_ms = 5000              # Post-handler callback timeout
starvation_warning_threshold_ms = 10000 # Warn if events wait > 10s
completion_send_timeout_ms = 10000      # Max time to send result
```

## Completion Channel Backpressure

### Problem

If the completion channel fills up, handler completions could block indefinitely.

### Solution: Try-Send with Retry Loop

```rust
pub fn complete(&self, event_id: Uuid, result: StepExecutionResult) -> bool {
    let timeout = self.config.completion_send_timeout;
    let start = Instant::now();

    loop {
        match self.completion_sender.try_send(result.clone()) {
            Ok(()) => {
                // Success - log if we had to wait
                if start.elapsed() > Duration::from_millis(100) {
                    warn!(
                        event_id = %event_id,
                        elapsed_ms = start.elapsed().as_millis(),
                        "FFI completion delayed by channel backpressure"
                    );
                }
                return true;
            }
            Err(TrySendError::Full(_)) => {
                if start.elapsed() > timeout {
                    error!(
                        event_id = %event_id,
                        "FFI completion send timed out - result lost"
                    );
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TrySendError::Closed(_)) => {
                error!(event_id = %event_id, "Completion channel closed");
                return false;
            }
        }
    }
}
```

## Starvation Detection

### Problem

If Ruby's polling falls behind, events queue up and may timeout before being processed.

### Solution: Proactive Starvation Warnings

The `FfiDispatchChannel` provides metrics and warnings:

```rust
// Ruby can query metrics for health monitoring
pub fn ffi_dispatch_metrics() -> FfiDispatchMetrics {
    FfiDispatchMetrics {
        pending_count: 5,
        oldest_pending_age_ms: Some(8500),
        starvation_detected: false,  // True if any event > 10s old
        starving_event_count: 0,
    }
}

// Automatic warnings logged for aging events
pub fn check_starvation_warnings() {
    // Logs warning for any event exceeding starvation_warning_threshold_ms
}
```

### Ruby Integration

```ruby
# Ruby EventPoller checks starvation every ~1 second
STARVATION_CHECK_INTERVAL = 100  # poll iterations

def poll_events_loop
  while @active
    @poll_count += 1
    check_starvation_periodically  # Calls FFI every 100 polls

    event_data = TaskerCore::FFI.poll_step_events
    # ... process event
  end
end

# Public method for external health checks
def ffi_dispatch_metrics
  TaskerCore::FFI.get_ffi_dispatch_metrics
end
```

## Monitoring

### Key Metrics

| Metric | Type | Alert Threshold |
|--------|------|-----------------|
| `tasker.ffi.pending_events` | Gauge | > 100 for 30s |
| `tasker.ffi.oldest_event_age_ms` | Gauge | > 15000ms for 10s |
| `tasker.callback.timeout_count` | Counter | rate > 1/5m |
| `tasker.channel.completion.saturation` | Gauge | > 0.95 for 10s |

### Alert Rules Example

```yaml
- alert: FFIOldestEventAging
  expr: tasker.ffi.oldest_event_age_ms > 15000
  for: 10s
  labels:
    severity: critical
  annotations:
    summary: "FFI events aging - Ruby polling may be starved"

- alert: CallbackTimeoutsHigh
  expr: rate(tasker.callback.timeout_count[5m]) > 1
  for: 1m
  labels:
    severity: warning
  annotations:
    summary: "Post-handler callbacks timing out frequently"
```

## Troubleshooting

### Symptom: Ruby handler completions slow

**Check**:
1. Completion channel saturation via metrics
2. `FfiDispatchMetrics.pending_count` growing
3. Warnings about "FFI completion delayed"

**Solutions**:
1. Increase `completion_buffer_size`
2. Optimize completion processor throughput
3. Check for completion processor backlog

### Symptom: Domain events not firing

**Check**:
1. Callback timeout warnings in logs
2. Domain event channel metrics
3. Domain event system health

**Solutions**:
1. Increase `callback_timeout_ms`
2. Check domain event processor health
3. Verify no deadlocks in event handlers

### Symptom: "channel closed" warnings

**Check**:
1. Worker shutdown sequence
2. Completion processor health
3. System shutdown ordering

**Solutions**:
1. Ensure graceful shutdown via `transition_to_graceful_shutdown()`
2. Wait for in-flight handlers before stopping
3. Check for panic in completion processor

## Implementation Checklist

For new FFI workers (e.g., Python/PyO3):

- [ ] Use fire-and-forget callback pattern
- [ ] Implement completion send with retry loop
- [ ] Add starvation warning integration
- [ ] Expose metrics FFI function
- [ ] Document thread safety constraints
- [ ] Add completion timeout configuration
- [ ] Test under backpressure conditions

## Related Documentation

- [MPSC Channel Guidelines](./mpsc-channel-guidelines.md) - Channel sizing and monitoring
- [Domain Events](../domain-events.md) - Event publishing patterns
- [FFI Telemetry Pattern](../ffi-telemetry-pattern.md) - Logging across FFI boundary
