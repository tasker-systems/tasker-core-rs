# TAS-89-3: Implement Real Worker Metrics

**Parent**: TAS-89 (Codebase Evaluation)
**Type**: Enhancement
**Priority**: Medium
**Effort**: 3-4 hours

---

## Summary

Worker metrics currently return hardcoded values (0.0, 1.0), making observability dashboards useless. Implement real metric calculations to enable proper monitoring and capacity planning.

---

## Problem Statement

### Current State (Broken)

| Metric | Current Value | Should Be |
|--------|---------------|-----------|
| `processing_rate` | 0.0 | Events processed per second |
| `average_latency_ms` | 0.0 | Moving average of processing time |
| `deployment_mode_score` | 1.0 | Effectiveness calculation |
| `handler_count` | 0 | Actual registered handler count |

### Impact

- **Cannot measure throughput** - dashboards show 0
- **Cannot detect slow processing** - latency always 0
- **Cannot evaluate deployment modes** - score always 1.0
- **Cannot count handlers** - registry size unknown

---

## Scope

### 1. Implement Processing Rate Calculation

**Location**: `tasker-worker/src/worker/event_systems/worker_event_system.rs:366`

**Approach**: Track events processed in a sliding window.

```rust
struct MetricsTracker {
    events_processed: AtomicU64,
    window_start: Instant,
    window_duration: Duration,
}

impl MetricsTracker {
    fn record_event(&self) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);
    }

    fn processing_rate(&self) -> f64 {
        let elapsed = self.window_start.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.events_processed.load(Ordering::Relaxed) as f64 / elapsed
        } else {
            0.0
        }
    }
}
```

---

### 2. Implement Average Latency Calculation

**Location**: `tasker-worker/src/worker/event_systems/worker_event_system.rs:367`

**Approach**: Use exponential moving average.

```rust
struct LatencyTracker {
    ema_latency_ms: AtomicU64, // stored as microseconds for precision
    alpha: f64, // smoothing factor (e.g., 0.1)
}

impl LatencyTracker {
    fn record_latency(&self, latency: Duration) {
        let latency_us = latency.as_micros() as f64;
        let current = self.ema_latency_ms.load(Ordering::Relaxed) as f64;
        let new_value = self.alpha * latency_us + (1.0 - self.alpha) * current;
        self.ema_latency_ms.store(new_value as u64, Ordering::Relaxed);
    }

    fn average_latency_ms(&self) -> f64 {
        self.ema_latency_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }
}
```

---

### 3. Implement Deployment Mode Score

**Location**: `tasker-worker/src/worker/event_systems/worker_event_system.rs:368`

**Approach**: Calculate based on event-driven vs polling usage.

```rust
fn deployment_mode_score(&self) -> f64 {
    let event_driven_count = self.event_driven_processed.load(Ordering::Relaxed);
    let polling_count = self.polling_processed.load(Ordering::Relaxed);
    let total = event_driven_count + polling_count;

    if total == 0 {
        1.0 // Default when no processing yet
    } else {
        // Higher score = more event-driven (better)
        event_driven_count as f64 / total as f64
    }
}
```

---

### 4. Implement Handler Count

**Location**: `workers/python/src/observability.rs:177`

**Approach**: Query actual handler registry.

```rust
fn handler_count(&self) -> usize {
    self.handler_registry.len()
}
```

---

## Files to Modify

| File | Change |
|------|--------|
| `tasker-worker/src/worker/event_systems/worker_event_system.rs` | Add metrics tracking, implement calculations |
| `workers/python/src/observability.rs` | Query real handler count |
| May need new `metrics_tracker.rs` module | If metrics logic is substantial |

---

## Acceptance Criteria

- [ ] `processing_rate` reflects actual events/second
- [ ] `average_latency_ms` reflects actual processing time
- [ ] `deployment_mode_score` reflects event-driven vs polling ratio
- [ ] `handler_count` reflects actual registered handlers
- [ ] Metrics update in real-time as events are processed
- [ ] Tests verify metrics change under load

---

## Testing

```rust
#[tokio::test]
async fn processing_rate_increases_under_load() {
    let worker = start_worker().await;

    // Check initial rate
    let initial = worker.metrics().processing_rate;

    // Process some events
    for _ in 0..100 {
        worker.process_event(test_event()).await;
    }

    // Rate should have increased
    let after = worker.metrics().processing_rate;
    assert!(after > initial);
}

#[tokio::test]
async fn latency_reflects_actual_processing_time() {
    let worker = start_worker_with_slow_handler(Duration::from_millis(50)).await;

    // Process events
    for _ in 0..10 {
        worker.process_event(test_event()).await;
    }

    // Latency should be around 50ms
    let latency = worker.metrics().average_latency_ms;
    assert!(latency > 40.0 && latency < 100.0);
}
```

---

## Risk Assessment

**Risk**: Low
- Adding metrics tracking has minimal performance overhead
- No change to existing behavior, only adds measurement
- Metrics are informational, not control-flow affecting
