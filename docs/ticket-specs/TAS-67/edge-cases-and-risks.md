# TAS-67: Edge Cases and Risks

## Overview

This document identifies edge cases, potential risks, and mitigation strategies for the dual-channel dispatch system introduced in TAS-67.

---

## Edge Cases

### 1. Event ID Mismatch on Completion

**Scenario:** Ruby handler calls `complete_step_event` with an event_id that doesn't match any pending event.

**Current Behavior:**
```rust
pub fn complete(&self, event_id: Uuid, result: StepExecutionResult) -> bool {
    let pending_event = self.pending_events.write().unwrap().remove(&event_id);
    if pending_event.is_none() {
        warn!("FFI dispatch: completion for unknown event");
        return false;
    }
    // ...
}
```

**Mitigation:**
- Returns `false` to caller indicating failure
- Warning logged for debugging
- No crash or panic

**Risk Level:** Low - Handler receives feedback that completion failed

---

### 2. Handler Timeout Before Completion

**Scenario:** Ruby handler takes longer than `completion_timeout` (default 30s) to complete.

**Current Behavior:**
```rust
pub fn cleanup_timeouts(&self) -> usize {
    // Removes timed-out events from pending
    // Sends failure result to completion channel
    let result = StepExecutionResult::failure(
        step_uuid,
        format!("FFI handler timed out after {}ms", timeout.as_millis()),
        None,
        Some("ffi_timeout".to_string()),
        true,  // Retryable
        timeout.as_millis() as i64,
        None,
    );
    // ...
}
```

**Mitigation:**
- Automatic timeout detection via `cleanup_timeouts()`
- Sends failure result with `retryable: true`
- Step will be retried by orchestration (TAS-49)

**Risk Level:** Medium - Requires periodic `cleanup_timeouts()` calls

**Recommended Action:** Add background task that periodically calls `cleanup_timeouts()`

---

### 3. Dispatch Channel Full (Backpressure)

**Scenario:** Dispatch channel reaches capacity while steps are still being dispatched.

**Current Behavior:**
- Dispatch uses `blocking_send()` which will block until space is available
- For FFI `poll()`, uses `try_recv()` which is non-blocking

**Mitigation:**
- Configured buffer sizes via TOML
- Backpressure naturally propagates to step enqueueing

**Risk Level:** Low - Standard channel backpressure semantics

---

### 4. Completion Channel Full

**Scenario:** Completion channel reaches capacity while results are being submitted.

**Current Behavior:**
```rust
match self.completion_sender.blocking_send(result) {
    Ok(()) => true,
    Err(e) => {
        error!("FFI dispatch: failed to send completion");
        false
    }
}
```

**Mitigation:**
- Returns `false` on send failure
- Error logged
- Handler can implement retry logic

**Risk Level:** Medium - Could lead to stuck steps without retry

**Recommended Action:** Consider adding retry with backoff for completion sends

---

### 5. Worker Shutdown During Processing

**Scenario:** Worker system shutdown while handlers are processing steps.

**Current Behavior:**
- `stop_worker()` drops the RubyBridgeHandle
- Channels are closed
- In-flight handlers may not complete

**Mitigation:**
- Graceful shutdown via `transition_to_graceful_shutdown()`
- Steps marked as in-flight are retryable (TAS-49)

**Risk Level:** Medium - In-flight work may be lost

**Recommended Action:** Wait for pending events to drain before shutdown

---

### 6. Concurrent Bootstrap Calls

**Scenario:** Multiple threads call `bootstrap_worker()` simultaneously.

**Current Behavior:**
```rust
let mut handle_guard = WORKER_SYSTEM.lock()?;
if handle_guard.is_some() {
    return Ok(hash with "already_running" status);
}
```

**Mitigation:**
- Mutex protects global state
- Returns graceful "already_running" status
- No double-initialization

**Risk Level:** Low - Properly synchronized

---

### 7. Poll Without Bootstrap

**Scenario:** `poll_step_events()` called before `bootstrap_worker()`.

**Current Behavior:**
```rust
let handle = handle_guard.as_ref().ok_or_else(|| {
    Error::new(runtime_error(), "Worker system not running")
})?;
```

**Mitigation:**
- Returns Ruby RuntimeError with clear message
- No crash or undefined behavior

**Risk Level:** Low - Clear error handling

---

### 8. Ruby Handler Panic

**Scenario:** Ruby handler raises an exception during execution.

**Current Behavior:**
- Exception propagates to Ruby caller
- `complete_step_event()` never called
- Event remains in pending list until timeout

**Mitigation:**
- `cleanup_timeouts()` eventually marks as failed
- Step is retried by orchestration

**Risk Level:** Medium - Relies on timeout for cleanup

**Recommended Action:** Ruby wrapper should catch exceptions and call `complete_step_event` with failure result

---

## Risks and Mitigations

### High Risk

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Memory leak from pending events | System degradation | Low | `cleanup_timeouts()` removes stale entries |
| Channel deadlock | System hang | Very Low | All channels use non-blocking operations where appropriate |

### Medium Risk

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Orphaned steps after shutdown | Data loss | Medium | Graceful shutdown + retry system |
| Handler timeouts not detected | Stuck steps | Medium | Periodic `cleanup_timeouts()` |
| Ruby exception handling | Delayed cleanup | Medium | Timeout-based recovery |

### Low Risk

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Event ID collisions | Incorrect correlation | Very Low | UUIDs have negligible collision probability |
| Channel capacity issues | Temporary slowdown | Low | Configurable buffer sizes |

---

## Testing Coverage

### Unit Tests

| Scenario | Coverage |
|----------|----------|
| Basic poll/complete cycle | ✅ In FfiDispatchChannel tests |
| Timeout cleanup | ✅ In FfiDispatchChannel tests |
| Channel configuration | ✅ Config tests |
| Ruby conversion functions | ✅ In conversion tests |

### Integration Tests

| Scenario | Coverage |
|----------|----------|
| Ruby FFI integration | ✅ 226 Ruby unit tests |
| Rust worker E2E | ✅ Linear, diamond, order workflows |
| Ruby Docker E2E | ⚠️ Requires Docker rebuild |

### Missing Coverage

1. **Stress tests** - High-volume concurrent dispatch
2. **Chaos tests** - Network failures, OOM conditions
3. **Long-running tests** - Memory stability over time

---

## Deployment Considerations

### Pre-Deployment Checklist

- [ ] Rebuild Docker images for Ruby worker
- [ ] Verify TOML configuration for buffer sizes
- [ ] Set appropriate timeout values
- [ ] Enable monitoring for channel metrics
- [ ] Review handler timeout settings

### Rollback Plan

If issues occur after deployment:

1. Revert to main branch code
2. Rebuild Docker images
3. No database migrations required (no schema changes)

### Feature Flag

Consider wrapping in feature flag:
```toml
[worker.dispatch]
use_ffi_dispatch_channel = true  # false reverts to legacy
```

---

## Monitoring Recommendations

### Key Metrics

1. **Pending Event Count** - `ffi_dispatch_channel.pending_count()`
2. **Timeout Events** - Count of `cleanup_timeouts()` removals
3. **Completion Success Rate** - `complete()` return values
4. **Channel Saturation** - Via TAS-51 ChannelMonitor

### Alerts

| Metric | Threshold | Action |
|--------|-----------|--------|
| Pending events | > 1000 | Investigate handler slowdown |
| Timeout rate | > 5% | Increase timeout or fix handlers |
| Completion failures | > 1% | Check channel capacity |

---

## Summary

The dual-channel dispatch system introduces several edge cases that are handled through:

1. **Timeout-based cleanup** - Prevents memory leaks and stuck events
2. **Graceful degradation** - Returns clear errors rather than crashing
3. **Retry integration** - Failed steps are retried by orchestration (TAS-49)
4. **Monitoring hooks** - Metrics available for all key operations

The main operational concern is ensuring `cleanup_timeouts()` is called periodically, which should be addressed by adding a background cleanup task.
