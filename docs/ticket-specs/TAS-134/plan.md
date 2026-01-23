# TAS-134: Convert Hot-Path Stats to Atomic Counters

## Summary

Convert 5 mutex-protected stats patterns to lock-free atomic counters, following the established `AtomicStepExecutionStats` pattern. Includes 3 ticket-specified locations + 2 additional discoveries.

## Phase 1: Simple Counter Conversions (EventRouter, InProcessEventBus, WorkerEventSubscriber)

All three have identical patterns: `Arc<std::sync::Mutex<Stats>>` with u64 fields incremented in hot paths.

### 1a. EventRouterStats → AtomicEventRouterStats

**File**: `tasker-worker/src/worker/event_router.rs`

- Create `AtomicEventRouterStats` with 6 AtomicU64 fields
- Add `#[inline]` record methods: `record_route()`, `record_durable()`, `record_fast()`, `record_broadcast()`, `record_fast_error()`, `record_routing_error()`
- Add `snapshot() -> EventRouterStats` method
- Replace `stats: Arc<std::sync::Mutex<EventRouterStats>>` with `stats: AtomicEventRouterStats`
- Update `route_event()`, `route_durable()`, `route_fast()`, `route_broadcast()` to use atomic methods
- Update `get_statistics()` to call `self.stats.snapshot()`

### 1b. InProcessEventBusStats → AtomicInProcessEventBusStats

**File**: `tasker-worker/src/worker/in_process_event_bus.rs`

- Create `AtomicInProcessEventBusStats` with 5 AtomicU64 fields (counter fields only)
- Add `#[inline]` record methods: `record_dispatch()`, `record_rust_dispatch(error_count)`, `record_ffi_dispatch()`, `record_ffi_drop()`
- Add `snapshot(&self, registry: &EventRegistry, ffi_sender: &broadcast::Sender<DomainEvent>) -> InProcessEventBusStats` - queries live counts from registry/sender at snapshot time
- Replace `stats: Arc<std::sync::Mutex<InProcessEventBusStats>>` with `stats: AtomicInProcessEventBusStats`
- Update `publish()`, `dispatch_to_rust_handlers()`, `dispatch_to_ffi_channel()` to use atomic methods
- Update `get_statistics()` to call `self.stats.snapshot(&self.registry, &self.ffi_sender)`

### 1c. WorkerEventSubscriberStats → AtomicWorkerEventSubscriberStats

**File**: `tasker-worker/src/worker/event_subscriber.rs`

- Create `AtomicWorkerEventSubscriberStats` with 5 AtomicU64 fields + `worker_id: String` (set once at construction)
- Add `#[inline]` record methods: `record_completion(success: bool)`, `record_conversion_error()`, `record_unmatched_correlation()`
- Add `snapshot() -> WorkerEventSubscriberStats` method
- Replace `stats: Arc<std::sync::Mutex<WorkerEventSubscriberStats>>` with `stats: Arc<AtomicWorkerEventSubscriberStats>` (Arc needed since cloned into spawned task)
- Update completion listener loop and error paths to use atomic methods
- Update `get_statistics()` to call `self.stats.snapshot()`

## Phase 2: CircuitBreakerMetrics → AtomicCircuitBreakerMetrics

**File**: `tasker-shared/src/resilience/circuit_breaker.rs`
**Metrics struct**: `tasker-shared/src/resilience/metrics.rs`

This is more complex due to:
- `total_duration: Duration` → store as `total_duration_nanos: AtomicU64`
- `consecutive_failures` needs atomic reset on success (Relaxed store is fine)
- Threshold checks (`consecutive_failures >= failure_threshold`) use `load()` then conditional transition
- `half_open_calls` counter with threshold check for recovery

**Approach**:
- Create `AtomicCircuitBreakerMetrics` with 6 AtomicU64 fields: `total_calls`, `success_count`, `failure_count`, `consecutive_failures`, `half_open_calls`, `total_duration_nanos`
- `record_success(duration)`: fetch_add counters + store(0) on consecutive_failures + add duration nanos
- `record_failure(duration)`: fetch_add counters + fetch_add consecutive_failures + add duration nanos
- `reset_half_open()`: store(0) on half_open_calls and consecutive_failures
- `snapshot() -> CircuitBreakerMetrics`: load all atomics, compute rates and average_duration, read state from existing AtomicU8

**State transition handling**: The current code does `if metrics.consecutive_failures >= threshold { transition_to_open() }`. With atomics, this becomes:
```rust
let failures = self.metrics.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
if failures >= self.config.failure_threshold as u64 {
    self.transition_to_open().await;
}
```
This is safe because state transitions are idempotent (store to AtomicU8) and duplicate transitions are harmless.

**opened_at handling**: Convert `Arc<Mutex<Option<Instant>>>` to `AtomicI64` storing nanos-since-epoch (using `Instant::now().elapsed()` relative to a base instant, or simply use `AtomicU64` with 0 = not set). Simpler: use `AtomicU64` storing `SystemTime` epoch nanos, with 0 meaning "not opened".

- Remove `metrics: Arc<Mutex<CircuitBreakerMetrics>>` field
- Add `metrics: AtomicCircuitBreakerMetrics` field directly
- Remove `opened_at: Arc<Mutex<Option<Instant>>>` field
- Add `opened_at_epoch_nanos: AtomicU64` (0 = circuit not open)
- Update `should_allow_call()`, `record_success()`, `record_failure()`, transition methods
- `get_metrics()` assembles snapshot with computed fields

## Phase 3: OrchestrationStatistics.last_processing_time

**File**: `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`

- Replace `last_processing_time: std::sync::Mutex<Option<Instant>>` with `last_processing_time_epoch_nanos: AtomicU64` (0 = never processed)
- Store `SystemTime::now().duration_since(UNIX_EPOCH).as_nanos() as u64` on each update
- Read: load atomic, if 0 return None, else reconstruct SystemTime
- Update the `Clone` impl to use `load(Ordering::Relaxed)`
- Update the notification handler (line 1353) to use atomic store
- Update `last_event_time()` or equivalent readers

## Struct Placement

All new `Atomic*` structs are defined in the same file as their usage (following the `AtomicStepExecutionStats` pattern in `worker_status_actor.rs`). The existing Stats DTOs in `tasker-shared/src/metrics/worker.rs` remain unchanged as the serializable response types.

## Ordering

All operations use `Ordering::Relaxed` - these are independent monitoring counters with no cross-field ordering requirements. The exception is `opened_at_epoch_nanos` which pairs with the state AtomicU8: use `Release` on store (when opening) and `Acquire` on load (when checking timeout).

## Verification

```bash
# After each phase:
cargo check --all-features
cargo clippy --all-targets --all-features
cargo test --features test-messaging --lib
cargo test --features test-services --package tasker-worker
cargo test --features test-services --package tasker-shared
cargo test --features test-services --package tasker-orchestration
```

## Files Modified

| Phase | File | Change |
|-------|------|--------|
| 1a | `tasker-worker/src/worker/event_router.rs` | Add AtomicEventRouterStats, update router |
| 1b | `tasker-worker/src/worker/in_process_event_bus.rs` | Add AtomicInProcessEventBusStats, update bus |
| 1c | `tasker-worker/src/worker/event_subscriber.rs` | Add AtomicWorkerEventSubscriberStats, update subscriber |
| 2 | `tasker-shared/src/resilience/circuit_breaker.rs` | Add AtomicCircuitBreakerMetrics, refactor CB |
| 2 | `tasker-shared/src/resilience/metrics.rs` | Keep as-is (DTO unchanged) |
| 3 | `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs` | Convert last_processing_time to AtomicU64 |
