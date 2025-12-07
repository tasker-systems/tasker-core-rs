# TAS-67 Risk Mitigation Plan

**Last Updated**: 2025-12-07
**Status**: Implementation Complete (Phase 0)
**Risks Addressed**: 4 MEDIUM-level edge cases from `04-edge-cases-and-risks.md`

---

## 🔴 CRITICAL BUG DISCOVERED & FIXED (2025-12-07)

### Orphaned In-Process Event Bus in Ruby Bootstrap

**Severity**: P0 - Critical
**Discovery**: Found during investigation of flaky `test_mixed_workflow_scenario`
**Status**: ✅ FIXED

#### Problem Statement

**File**: `workers/ruby/ext/tasker_core/src/bootstrap.rs` (lines 155-159)

The Ruby bootstrap was creating a **local** `InProcessEventBus` that was immediately dropped when the function returned, orphaning the FFI receiver channel. This caused:

1. **Constant warning spam**: "In-process event channel closed" logged repeatedly
2. **Domain events not delivered**: FFI receiver had no sender (dropped)
3. **Flaky tests**: `test_mixed_workflow_scenario` failed under workspace load
4. **Usability concern**: Developers would see domain events as broken

**Buggy Code**:
```rust
// TAS-65 Phase 4.1: Create in-process event bus for fast domain events
info!("⚡ Creating in-process event bus for fast domain events...");
let in_process_bus = InProcessEventBus::new(InProcessEventBusConfig::default());
let in_process_event_receiver = in_process_bus.subscribe_ffi();
info!("✅ In-process event bus created with FFI subscriber");
// BUG: in_process_bus is DROPPED here when function returns!
// The receiver is orphaned - its sender channel is closed!
```

#### Root Cause Analysis

The `InProcessEventBus` was created as a local variable and stored only in `RubyBridgeHandle`. When the bootstrap function returned, the bus was dropped, but only the **receiver** was stored. This meant:

1. `InProcessEventBus::new()` creates internal MPSC channels
2. `subscribe_ffi()` returns a receiver for those channels
3. When `in_process_bus` goes out of scope, the **sender** is dropped
4. The receiver becomes orphaned - every `recv()` call fails immediately
5. Warning floods logs: "In-process event channel closed"

#### The Fix

**File**: `workers/ruby/ext/tasker_core/src/bootstrap.rs`

Changed to **subscribe to WorkerCore's in-process bus** instead of creating a local orphaned one:

```rust
// TAS-65 Phase 4.1: Get in-process event receiver from WorkerCore's event bus
// IMPORTANT: We must use WorkerCore's bus, not create a new one, otherwise the
// sender is dropped when the local bus goes out of scope.
info!("⚡ Subscribing to WorkerCore's in-process event bus for fast domain events...");
let in_process_event_receiver = runtime.block_on(async {
    let worker_core = system_handle.worker_core.lock().await;
    // Get the in-process bus from WorkerCore and subscribe for FFI
    let bus = worker_core.in_process_event_bus();
    let bus_guard = bus.write().await;
    bus_guard.subscribe_ffi()
});
info!("✅ Subscribed to WorkerCore's in-process event bus for FFI domain events");
```

#### Verification

After the fix:
- ✅ **1185 tests passed**, 7 skipped (all E2E tests pass)
- ✅ **Zero "channel closed" warnings** in Ruby worker logs
- ✅ **Flaky test `test_mixed_workflow_scenario`** now passes consistently
- ✅ **Domain events working** for both Ruby and Rust workers
- ✅ Ruby worker logs: "⚡ Subscribing to WorkerCore's in-process event bus"

#### Impact

| Before Fix | After Fix |
|------------|-----------|
| Constant warning spam in logs | Clean logs |
| Domain events silently dropped | Domain events delivered correctly |
| Flaky E2E tests under load | Consistent test results |
| Developers confused by "broken" events | Events work as documented |

#### Lessons Learned

1. **Channel ownership matters**: When creating MPSC channels, ensure the sender outlives the receiver
2. **Integration tests reveal real bugs**: The flaky test was caused by a real bug, not test instability
3. **Investigate warnings**: The "channel closed" warning was a symptom of a deeper architectural issue
4. **Use existing infrastructure**: WorkerCore already had the event bus - no need to create a new one

---

## Executive Summary

This document provides concrete implementation plans for mitigating the four MEDIUM-level risks identified in the TAS-67 dual-channel refactor. Each mitigation includes specific code changes, file locations, configuration updates, and test strategies.

| Risk | Current State | Proposed Fix | Priority |
|------|---------------|--------------|----------|
| Semaphore Acquisition Failure | Silent exit, no failure result | Generate failure result | P1 - Immediate |
| FFI Polling Starvation | Timeout-only detection (reactive) | Proactive starvation metrics | P2 - Short-term |
| Completion Channel Backpressure | Permit held during send | Release permit before send | P1 - Immediate |
| FFI Thread Runtime Context | `block_on` with no timeout | Fire-and-forget + timeout | P1 - Immediate |

---

## Risk 1: Semaphore Acquisition Failure

### Problem Statement

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs` (lines 254-264)

When `semaphore.acquire_owned().await` fails (semaphore closed), the handler task exits silently without generating a failure result. The step remains in `InProgress` state with no orchestration awareness.

**Current Code**:
```rust
let _permit = match semaphore.acquire().await {
    Ok(permit) => permit,
    Err(_) => {
        error!(
            service_id = %service_id,
            step_uuid = %msg.step_uuid,
            "Semaphore closed - cannot execute handler"
        );
        return;  // <-- SILENT EXIT - NO FAILURE RESULT
    }
};
```

### Mitigation: Generate Failure Result

**Proposed Code** (replace lines 254-264):

```rust
let _permit = match semaphore.acquire().await {
    Ok(permit) => permit,
    Err(_) => {
        error!(
            service_id = %service_id,
            step_uuid = %msg.step_uuid,
            "Semaphore closed - cannot execute handler, generating failure result"
        );

        // Generate failure result so orchestration knows the step failed
        let failure_result = StepExecutionResult::failure(
            msg.step_uuid,
            "Handler execution failed - semaphore closed during acquisition".to_string(),
            None,                                             // error_code
            Some("semaphore_acquisition_failed".to_string()), // error_type
            true,                                             // retryable=true
            0,                                                // execution_time_ms
            Some(std::collections::HashMap::from([
                ("service_id".to_string(), serde_json::json!(service_id)),
                ("reason".to_string(), serde_json::json!("semaphore closed")),
                ("task_uuid".to_string(), serde_json::json!(msg.task_uuid.to_string())),
            ])),
        );

        // Send failure result to completion channel
        match sender.send(failure_result.clone()).await {
            Ok(()) => {
                debug!(
                    service_id = %service_id,
                    step_uuid = %msg.step_uuid,
                    "Semaphore failure result sent to completion channel"
                );

                // Invoke callback after successful send
                if let Some(ref cb) = callback {
                    cb.on_handler_complete(&msg.task_sequence_step, &failure_result, &service_id)
                        .await;
                }
            }
            Err(e) => {
                error!(
                    service_id = %service_id,
                    step_uuid = %msg.step_uuid,
                    error = %e,
                    "Failed to send semaphore failure result - channel closed"
                );
            }
        }
        return;
    }
};
```

### Test Strategy

```rust
#[tokio::test]
async fn test_semaphore_acquisition_failure_generates_failure_result() {
    // Setup: Create dispatch service, close semaphore
    // Verify: Failure result with error_type="semaphore_acquisition_failed" sent
    // Verify: Result has retryable=true
    // Verify: Orchestration receives and processes failure
}
```

### Configuration

No configuration changes required. Uses existing failure result patterns.

### Impact

- **Risk reduced**: MEDIUM → LOW
- **Recovery time**: 30+ seconds (polling) → immediate (failure result)
- **Backward compatible**: Yes (additive change)

---

## Risk 2: FFI Polling Starvation

### Problem Statement

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Ruby must actively call `poll_step_events()` to receive work. If polling falls behind, events buffer and timeout after 30 seconds with no proactive warning.

**Current Behavior**:
- `cleanup_timeouts()` exists but is never automatically called
- No metrics for pending event count or age
- No way for Ruby to monitor backlog
- 30-second timeout is reactive, not proactive

### Mitigation: Multi-Layer Starvation Detection

#### Layer 1: Add Metrics Method

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Add after line 200:

```rust
/// Metrics about pending events for monitoring
#[derive(Debug, Clone, Default)]
pub struct FfiDispatchMetrics {
    /// Total pending events in buffer
    pub pending_count: usize,
    /// Age of oldest pending event in milliseconds
    pub oldest_pending_age_ms: Option<u64>,
    /// Age of newest pending event in milliseconds
    pub newest_pending_age_ms: Option<u64>,
    /// UUID of oldest event (for debugging)
    pub oldest_event_id: Option<Uuid>,
}

impl FfiDispatchChannel {
    /// Get current metrics about pending events
    pub fn metrics(&self) -> FfiDispatchMetrics {
        let pending = self
            .pending_events
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if pending.is_empty() {
            return FfiDispatchMetrics::default();
        }

        let now = std::time::Instant::now();
        let mut ages: Vec<(Uuid, u64)> = pending
            .iter()
            .map(|(id, p)| {
                let age = now.duration_since(p.dispatched_at).as_millis() as u64;
                (*id, age)
            })
            .collect();

        ages.sort_by_key(|(_, age)| *age);

        FfiDispatchMetrics {
            pending_count: pending.len(),
            oldest_pending_age_ms: ages.last().map(|(_, age)| *age),
            newest_pending_age_ms: ages.first().map(|(_, age)| *age),
            oldest_event_id: ages.last().map(|(id, _)| *id),
        }
    }

    /// Get count of pending events
    pub fn pending_count(&self) -> usize {
        self.pending_events
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}
```

#### Layer 2: Add Starvation Warning Thresholds

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Add to `FfiDispatchChannelConfig`:

```rust
pub struct FfiDispatchChannelConfig {
    pub completion_timeout: Duration,
    pub service_id: String,
    pub runtime_handle: tokio::runtime::Handle,
    /// Warn if any pending event exceeds this age (milliseconds)
    pub starvation_warning_threshold_ms: u64,
}

impl FfiDispatchChannelConfig {
    pub fn new(runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            completion_timeout: Duration::from_secs(30),
            service_id: "ffi-dispatch".to_string(),
            runtime_handle,
            starvation_warning_threshold_ms: 10_000, // 10 seconds
        }
    }
}
```

Update `poll()` to check starvation (add after line 280):

```rust
// Check for aging events and warn
fn check_starvation_warnings(&self) {
    let pending = self
        .pending_events
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if pending.is_empty() {
        return;
    }

    let now = std::time::Instant::now();
    let threshold = Duration::from_millis(self.config.starvation_warning_threshold_ms);

    for (event_id, pending_event) in pending.iter() {
        let age = now.duration_since(pending_event.dispatched_at);
        if age > threshold {
            warn!(
                service_id = %self.config.service_id,
                event_id = %event_id,
                step_uuid = %pending_event.event.step_uuid,
                age_ms = age.as_millis(),
                threshold_ms = self.config.starvation_warning_threshold_ms,
                "FFI dispatch: pending event aging - slow polling detected"
            );
        }
    }
}
```

#### Layer 3: Expose Metrics to Ruby FFI

**File**: `workers/ruby/ext/tasker_core/src/bridge.rs`

Add function:

```rust
/// Get FFI dispatch channel metrics for monitoring
pub fn ffi_dispatch_metrics() -> Result<Value, Error> {
    let handle_guard = WORKER_SYSTEM
        .lock()
        .map_err(|e| Error::new(runtime_error(), format!("Lock failed: {}", e)))?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(runtime_error(), "Worker system not running")
    })?;

    let metrics = handle.ffi_dispatch_channel().metrics();
    let ruby = magnus::Ruby::get()?;

    let hash = ruby.hash_new();
    hash.aset(ruby.to_symbol("pending_count"), metrics.pending_count as i64)?;

    if let Some(age) = metrics.oldest_pending_age_ms {
        hash.aset(ruby.to_symbol("oldest_pending_age_ms"), age as i64)?;
    } else {
        hash.aset(ruby.to_symbol("oldest_pending_age_ms"), ruby.qnil())?;
    }

    Ok(hash.as_value())
}
```

### Configuration

**File**: `config/tasker/base/worker.toml` (UPDATE existing `[worker.mpsc_channels.ffi_dispatch]`)

```toml
# TAS-67: FFI Dispatch Channel Configuration
[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
# NEW: Warn if any pending event exceeds this age (milliseconds)
starvation_warning_threshold_ms = 10000
# NEW: Service identifier for logging
service_id = "ffi-dispatch"
```

### Test Strategy

```rust
#[tokio::test]
async fn test_ffi_dispatch_metrics_tracks_pending_events() {
    // Setup: Create channel, dispatch events
    // Verify: metrics() returns correct pending_count
    // Verify: oldest_pending_age_ms increases over time
}

#[tokio::test]
async fn test_starvation_warning_logged() {
    // Setup: Dispatch event, wait 11 seconds
    // Verify: Warning log emitted about aging event
}
```

### Impact

- **Risk reduced**: MEDIUM → LOW
- **Detection time**: 30s (timeout) → 10s (warning)
- **Observability**: None → Full metrics visibility

---

## Risk 3: Completion Channel Backpressure

### Problem Statement

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs` (lines 252-308)

Handler tasks hold semaphore permits while blocked on completion channel send. If the completion channel fills, this starves new handler dispatches.

**Current Flow**:
```
acquire_permit → execute_handler → send_result (BLOCKS) → release_permit
                                        ↑
                                  Permit held here!
```

### Mitigation 1: Release Permit Before Send (PRIMARY)

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

**Current Code** (lines 252-308):
```rust
tokio::spawn(async move {
    let _permit = match semaphore.acquire().await {
        Ok(permit) => permit,
        Err(_) => { return; }
    };

    let result = Self::execute_with_timeout(...).await;

    // PERMIT STILL HELD during blocking send!
    match sender.send(result.clone()).await {
        Ok(()) => { /* callback */ }
        Err(e) => { /* error */ }
    }
    // Permit released here
});
```

**Proposed Change**:
```rust
tokio::spawn(async move {
    let permit = match semaphore.acquire().await {
        Ok(p) => p,
        Err(_) => {
            // ... generate failure result (see Risk 1)
            return;
        }
    };

    let result = Self::execute_with_timeout(&registry, &msg, timeout, &service_id).await;

    // RELEASE PERMIT BEFORE sending
    drop(permit);

    // NOW send can block without starving new handlers
    match sender.send(result.clone()).await {
        Ok(()) => {
            if let Some(ref cb) = callback {
                cb.on_handler_complete(&msg.task_sequence_step, &result, &service_id)
                    .await;
            }
        }
        Err(e) => {
            error!(
                service_id = %service_id,
                step_uuid = %msg.step_uuid,
                error = %e,
                "Failed to send completion result"
            );
        }
    }
});
```

**Impact**:
- Solves core backpressure cascade
- Minimal code change (add `drop(permit);`)
- New handler dispatches can proceed even when completion channel is full

### Mitigation 2: Add Channel Saturation Monitoring

**File**: `tasker-worker/src/worker/handlers/dispatch_service.rs`

Add `ChannelMonitor` integration:

```rust
pub struct HandlerDispatchService<R, C> {
    dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
    completion_sender: mpsc::Sender<StepExecutionResult>,
    registry: Arc<R>,
    config: HandlerDispatchConfig,
    post_handler_callback: Option<Arc<C>>,
    completion_monitor: Arc<ChannelMonitor>, // NEW
}

impl<R, C> HandlerDispatchService<R, C> {
    pub fn new(
        // ... existing params ...
        completion_monitor: Arc<ChannelMonitor>,
    ) -> Self {
        // ... existing construction ...
    }
}
```

In the spawned task, add saturation check:

```rust
// Before send, check saturation
let capacity = sender.capacity();
let saturation = completion_monitor.calculate_saturation(capacity);
if saturation > 0.8 {
    warn!(
        service_id = %service_id,
        saturation_percent = saturation * 100.0,
        capacity = capacity,
        "Completion channel approaching capacity"
    );
}

match sender.send(result.clone()).await {
    Ok(()) => {
        completion_monitor.record_send_success();
        // ... callback
    }
    Err(e) => {
        completion_monitor.record_message_dropped();
        // ... error
    }
}
```

### Mitigation 3: FFI Send Timeout

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` (line 393)

Replace `blocking_send()` with timeout loop:

```rust
pub fn complete(&self, event_id: Uuid, result: StepExecutionResult) -> bool {
    // ... extract pending event ...

    let timeout = Duration::from_secs(10);
    let start = std::time::Instant::now();

    loop {
        match self.completion_sender.try_send(result.clone()) {
            Ok(()) => {
                let elapsed = start.elapsed();
                if elapsed > Duration::from_millis(100) {
                    warn!(
                        event_id = %event_id,
                        elapsed_ms = elapsed.as_millis(),
                        "FFI handler blocked waiting for completion channel"
                    );
                }
                // Success - invoke callback
                self.config.runtime_handle.block_on(
                    self.post_handler_callback
                        .on_handler_complete(&step, &result, &worker_id),
                );
                return true;
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                if start.elapsed() > timeout {
                    error!(
                        event_id = %event_id,
                        timeout_ms = timeout.as_millis(),
                        "FFI completion send timed out - result lost"
                    );
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!(event_id = %event_id, "Completion channel closed");
                return false;
            }
        }
    }
}
```

### Configuration

**File**: `config/tasker/base/worker.toml`

Add:
```toml
[worker.mpsc_channels.handler_dispatch]
# ... existing config ...
completion_send_timeout_ms = 10000  # Max 10s to send result
```

### Test Strategy

```rust
#[tokio::test]
async fn test_completion_backpressure_no_deadlock() {
    // Setup: Small completion channel (10), slow processor
    // Send 100 dispatch messages
    // Verify: System does NOT deadlock
    // Verify: New dispatches can still be enqueued
}

#[tokio::test]
async fn test_permit_release_before_send() {
    // Setup: Fill completion channel
    // Verify: Semaphore permits are released before blocking on send
    // Verify: New handlers can acquire permits
}
```

### Impact

- **Risk reduced**: MEDIUM → LOW
- **Deadlock risk**: Eliminated (permit release before send)
- **Visibility**: Full saturation monitoring

---

## Risk 4: FFI Thread Runtime Context

### Problem Statement

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` (lines 413-416)

Ruby FFI thread calls `runtime_handle.block_on()` to execute async callback. This blocks the Ruby thread and can deadlock if callback acquires locks.

**Current Code**:
```rust
self.config.runtime_handle.block_on(
    self.post_handler_callback
        .on_handler_complete(&step, &result, &worker_id),
);
```

**Deadlock Scenario**:
1. Ruby thread holds `WORKER_SYSTEM` lock
2. Calls `complete_step_event()` → `block_on(callback)`
3. Callback tries to acquire lock held by Ruby thread
4. Deadlock!

### Mitigation 1: Add Timeout to block_on (IMMEDIATE)

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Replace lines 413-416:

```rust
// Wrap callback with timeout to prevent indefinite blocking
let callback_timeout = self.config.callback_timeout;
let callback = self.post_handler_callback.clone();
let step_clone = step.clone();
let result_clone = result.clone();
let worker_id_clone = worker_id.clone();

match self.config.runtime_handle.block_on(async {
    tokio::time::timeout(
        callback_timeout,
        callback.on_handler_complete(&step_clone, &result_clone, &worker_id_clone),
    )
    .await
}) {
    Ok(()) => {
        debug!(
            service_id = %self.config.service_id,
            event_id = %event_id,
            "Post-handler callback completed"
        );
    }
    Err(_elapsed) => {
        error!(
            service_id = %self.config.service_id,
            event_id = %event_id,
            timeout_ms = callback_timeout.as_millis(),
            "Post-handler callback timed out - domain events NOT fired"
        );
        // Result still sent, but domain events skipped
    }
}
```

### Mitigation 2: Fire-and-Forget Strategy (RECOMMENDED)

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Replace the `block_on` call:

```rust
// Spawn callback as fire-and-forget - don't block Ruby thread
let callback = self.post_handler_callback.clone();
let step_clone = step.clone();
let result_clone = result.clone();
let worker_id_clone = worker_id.clone();
let service_id_clone = self.config.service_id.clone();

self.config.runtime_handle.spawn(async move {
    debug!(
        service_id = %service_id_clone,
        "Post-handler callback spawned (fire-and-forget)"
    );

    if let Err(e) = tokio::time::timeout(
        Duration::from_secs(5),
        callback.on_handler_complete(&step_clone, &result_clone, &worker_id_clone),
    )
    .await
    {
        error!(
            service_id = %service_id_clone,
            error = %e,
            "Post-handler callback failed or timed out"
        );
    }
});

// Return immediately - Ruby thread not blocked
true
```

### Configuration

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`

Add to `FfiDispatchChannelConfig`:

```rust
pub struct FfiDispatchChannelConfig {
    pub completion_timeout: Duration,
    pub service_id: String,
    pub runtime_handle: tokio::runtime::Handle,
    pub starvation_warning_threshold_ms: u64,
    /// Timeout for post-handler callbacks
    pub callback_timeout: Duration,
    /// Strategy for callback execution
    pub callback_strategy: CallbackStrategy,
}

#[derive(Debug, Clone, Default)]
pub enum CallbackStrategy {
    /// Block Ruby thread waiting for callback (original behavior)
    Blocking,
    /// Spawn callback with timeout, block waiting for completion
    BlockingWithTimeout,
    /// Spawn callback in background (recommended)
    #[default]
    FireAndForget,
}
```

**File**: `config/tasker/base/worker.toml` (UPDATE existing `[worker.mpsc_channels.ffi_dispatch]`)

```toml
# TAS-67: FFI Dispatch Channel Configuration
[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
starvation_warning_threshold_ms = 10000
callback_timeout_ms = 5000
callback_strategy = "FireAndForget"  # or "Blocking", "BlockingWithTimeout"
service_id = "ffi-dispatch"
```

### Documentation

**File**: `docs/development/ffi-callback-safety.md` (CREATE)

```markdown
# FFI Callback Safety Guidelines (TAS-67)

## Critical Constraint: Callbacks Execute from FFI Thread Context

When Ruby calls `complete_step_event()`, callbacks execute via:
- `block_on()` (blocking) or `spawn()` (fire-and-forget)

## Safe Operations in Callbacks

- Read-only operations on Arc<T>
- Sync channels (mpsc::blocking_send)
- Logging/tracing
- Atomic operations

## Unsafe Operations (May Deadlock)

- Acquiring `tokio::sync::RwLock`, `Mutex`, `Semaphore`
- Calling `.await` without timeout on external I/O
- Calling back to Ruby (re-acquires WORKER_SYSTEM lock)
- Long-running I/O

## Recommended: Fire-and-Forget Strategy

Set `callback_strategy = "FireAndForget"` to prevent:
- Ruby thread blocking
- Deadlock risks
- Slow callback cascade
```

### Test Strategy

```rust
#[tokio::test]
async fn test_callback_timeout_prevents_indefinite_blocking() {
    // Setup: Create slow callback (10s sleep)
    // Configure: 5s timeout
    // Verify: Returns within ~5s, not 10s
}

#[tokio::test]
async fn test_fire_and_forget_unblocks_ruby_immediately() {
    // Setup: Slow callback
    // Configure: FireAndForget strategy
    // Verify: complete() returns within milliseconds
    // Verify: Callback runs in background
}

#[tokio::test]
async fn test_no_deadlock_on_nested_locks() {
    // Setup: Callback that acquires registry lock
    // Verify: With FireAndForget, no deadlock occurs
}
```

### Impact

- **Risk reduced**: MEDIUM → LOW
- **Deadlock risk**: Eliminated (fire-and-forget)
- **Ruby thread blocking**: None (fire-and-forget)
- **Callback failures**: Isolated, don't affect result delivery

---

## Implementation Roadmap

### Phase 1: Immediate (This Sprint) - P1

| Task | File | Effort | Risk |
|------|------|--------|------|
| Semaphore failure result | dispatch_service.rs:254-264 | 1 hour | Very Low |
| Permit release before send | dispatch_service.rs:270 | 15 min | Very Low |
| Callback timeout | ffi_dispatch_channel.rs:413 | 1 hour | Low |
| Fire-and-forget strategy | ffi_dispatch_channel.rs:413 | 2 hours | Low |

**Estimated Total**: 4-5 hours

### Phase 2: Short-term (Next Sprint) - P2

| Task | File | Effort | Risk |
|------|------|--------|------|
| FFI metrics method | ffi_dispatch_channel.rs | 2 hours | Low |
| Starvation warnings | ffi_dispatch_channel.rs | 2 hours | Low |
| Ruby FFI metrics exposure | bridge.rs | 1 hour | Low |
| Channel saturation monitoring | dispatch_service.rs | 2 hours | Low |
| FFI send timeout | ffi_dispatch_channel.rs | 2 hours | Medium |

**Estimated Total**: 9-10 hours

### Phase 3: Medium-term (Backlog) - P3

| Task | File | Effort | Risk |
|------|------|--------|------|
| Configuration TOML additions | Various | 2 hours | Low |
| Documentation | docs/development/ | 2 hours | None |
| Comprehensive test suite | Various *_tests.rs | 4 hours | None |
| Parallel completion processor | completion_processor.rs | 4-6 hours | Medium |

**Estimated Total**: 12-14 hours

---

## Monitoring & Alerting

### New Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `tasker.ffi.pending_events` | Gauge | Count of pending FFI events |
| `tasker.ffi.oldest_event_age_ms` | Gauge | Age of oldest pending event |
| `tasker.channel.completion.saturation` | Gauge | Completion channel saturation (0-1) |
| `tasker.callback.timeout_count` | Counter | Post-handler callback timeouts |
| `tasker.semaphore.acquisition_failures` | Counter | Semaphore closed failures |

### Alert Rules

```yaml
# Starvation Detection
- alert: FFIPendingEventsHigh
  expr: tasker.ffi.pending_events > 100
  for: 30s
  severity: warning

- alert: FFIOldestEventAging
  expr: tasker.ffi.oldest_event_age_ms > 15000
  for: 10s
  severity: critical

# Backpressure Detection
- alert: CompletionChannelSaturated
  expr: tasker.channel.completion.saturation > 0.8
  for: 30s
  severity: warning

- alert: CompletionChannelCritical
  expr: tasker.channel.completion.saturation > 0.95
  for: 10s
  severity: critical

# Callback Health
- alert: CallbackTimeoutsHigh
  expr: rate(tasker.callback.timeout_count[5m]) > 1
  for: 1m
  severity: warning
```

---

## Testing Verification Checklist

Before merging Phase 1:

- [ ] Semaphore failure generates `StepExecutionResult::failure()`
- [ ] Failure result has `error_type="semaphore_acquisition_failed"`
- [ ] Failure result has `retryable=true`
- [ ] Permit released before completion channel send
- [ ] No deadlock under completion channel backpressure
- [ ] Callback timeout prevents indefinite blocking
- [ ] Fire-and-forget returns immediately
- [ ] All existing tests pass
- [ ] Clippy passes with no new warnings

---

## Summary

The four MEDIUM-level risks are addressed through:

1. **Semaphore Acquisition**: Generate explicit failure result (immediate orchestration awareness)
2. **FFI Polling Starvation**: Multi-layer detection with metrics + warnings (proactive visibility)
3. **Completion Backpressure**: Release permit before send (eliminate deadlock cascade)
4. **FFI Thread Context**: Fire-and-forget callbacks (eliminate deadlock, unblock Ruby)

All mitigations follow Tasker's design principles:
- **State machine guards** ensure idempotency
- **Retry semantics** handle transient failures
- **Fire-and-forget** domain events preserve step success
- **Explicit failure results** enable orchestration awareness

Total implementation effort: ~25-30 hours across 3 phases.
