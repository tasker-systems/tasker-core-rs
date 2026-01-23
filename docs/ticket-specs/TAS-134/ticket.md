# TAS-134: Convert Hot-Path Stats to Atomic Counters

## Summary

Convert `Arc<Mutex<Stats>>` patterns in hot paths to atomic counters, following the established `AtomicStepExecutionStats` pattern from TAS-69. This eliminates lock contention in high-throughput event processing loops.

## Background

Inspired by [ownership-based concurrency thinking](https://medium.com/@rumen.rm/how-rusts-ownership-model-quietly-replaces-synchronization-63c6f0a7c40b): for simple counters, atomic operations *are* the ownership-based solution - each increment is an atomic transfer of ownership, eliminating the need for mutex coordination.

We already have a proven pattern in `WorkerStatusActor`:

```rust
pub struct AtomicStepExecutionStats {
    total_executed: AtomicU64,
    total_succeeded: AtomicU64,
    // ...
}
```

Several locations still use `Arc<Mutex<Stats>>` in hot paths and should be converted.

## Locations to Convert

### 1. `InProcessEventBus` (Higher Priority)

**File**: `tasker-worker/src/worker/in_process_event_bus.rs`

**Current**: `stats: Arc<std::sync::Mutex<InProcessEventBusStats>>`

**Hot path**: `publish()` - called for every domain event, locks mutex 1-3 times per event

**Fields to make atomic**:
- `total_events_dispatched: AtomicU64`
- `rust_handler_dispatches: AtomicU64`
- `ffi_channel_dispatches: AtomicU64`
- `rust_handler_errors: AtomicU64`
- `ffi_channel_drops: AtomicU64`

**Fields to query live** (not stored):
- `rust_subscriber_patterns` → `registry.pattern_count()`
- `rust_handler_count` → `registry.handler_count()`
- `ffi_subscriber_count` → `ffi_sender.receiver_count()`

### 2. `WorkerEventSubscriber` (Medium Priority)

**File**: `tasker-worker/src/worker/event_subscriber.rs`

**Current**: `stats: Arc<std::sync::Mutex<WorkerEventSubscriberStats>>`

**Hot path**: Completion listener loop - locks mutex for every completion event

**Fields to make atomic**:
- `completions_received: AtomicU64`
- `successful_completions: AtomicU64`
- `failed_completions: AtomicU64`
- `conversion_errors: AtomicU64`
- `unmatched_correlations: AtomicU64`

**Field to keep as-is**: `worker_id: String` (immutable after construction)

### 3. `CircuitBreakerMetrics` (Medium Priority)

**File**: `tasker-shared/src/resilience/circuit_breaker.rs`

**Current**: `metrics: Arc<Mutex<CircuitBreakerMetrics>>` (uses `tokio::sync::Mutex`)

**Hot path**: `record_success()` and `record_failure()` - called on every circuit breaker operation

**Fields to make atomic**:
- `total_calls: AtomicU64`
- `success_count: AtomicU64`
- `failure_count: AtomicU64`
- `consecutive_failures: AtomicU64`
- `half_open_calls: AtomicU64`
- `total_duration_nanos: AtomicU64` (store as nanoseconds)

**State transition handling**: The circuit breaker already uses `AtomicU8` for state. State transitions that need to check thresholds (e.g., `consecutive_failures >= failure_threshold`) can use `compare_exchange` patterns or keep a lightweight lock just for transitions.

**Note**: The `opened_at: Arc<Mutex<Option<Instant>>>` timestamp could be converted to `AtomicU64` storing nanoseconds since process start, but this is lower priority.

## Implementation Pattern

Follow the established `AtomicStepExecutionStats` pattern:

```rust
#[derive(Debug)]
pub struct AtomicInProcessEventBusStats {
    total_events_dispatched: AtomicU64,
    rust_handler_dispatches: AtomicU64,
    ffi_channel_dispatches: AtomicU64,
    rust_handler_errors: AtomicU64,
    ffi_channel_drops: AtomicU64,
}

impl AtomicInProcessEventBusStats {
    pub fn new() -> Self {
        Self {
            total_events_dispatched: AtomicU64::new(0),
            // ...
        }
    }

    #[inline]
    pub fn record_dispatch(&self) {
        self.total_events_dispatched.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn record_rust_dispatch(&self, error_count: usize) {
        self.rust_handler_dispatches.fetch_add(1, Ordering::Relaxed);
        if error_count > 0 {
            self.rust_handler_errors.fetch_add(error_count as u64, Ordering::Relaxed);
        }
    }

    /// Snapshot for API responses - queries live counts from registry/sender
    pub fn snapshot(
        &self,
        registry: &EventRegistry,
        ffi_sender: &broadcast::Sender<DomainEvent>,
    ) -> InProcessEventBusStats {
        InProcessEventBusStats {
            total_events_dispatched: self.total_events_dispatched.load(Ordering::Relaxed),
            rust_handler_dispatches: self.rust_handler_dispatches.load(Ordering::Relaxed),
            ffi_channel_dispatches: self.ffi_channel_dispatches.load(Ordering::Relaxed),
            rust_handler_errors: self.rust_handler_errors.load(Ordering::Relaxed),
            ffi_channel_drops: self.ffi_channel_drops.load(Ordering::Relaxed),
            rust_subscriber_patterns: registry.pattern_count(),
            rust_handler_count: registry.handler_count(),
            ffi_subscriber_count: ffi_sender.receiver_count(),
        }
    }
}
```

## Key Design Decisions

1. **Use `Ordering::Relaxed`** - These are independent counters with no ordering requirements relative to each other. Relaxed ordering is sufficient and fastest.

2. **Keep existing Stats structs for API responses** - `InProcessEventBusStats`, `WorkerEventSubscriberStats`, and `CircuitBreakerMetrics` remain as the serializable DTOs returned by `get_statistics()`. The atomic structs produce snapshots into these.

3. **Query live counts at snapshot time** - Pattern counts, handler counts, and subscriber counts are queried from the source of truth (registry, broadcast sender) rather than tracked separately.

4. **`#[inline]` on hot-path methods** - Encourage compiler to inline the atomic operations.

5. **Circuit breaker state transitions** - For operations that check thresholds (like opening the circuit when `consecutive_failures >= threshold`), use atomic load + compare, or keep a lightweight lock just for the transition decision.

## Testing

- Existing unit tests in all files should continue to pass
- Add concurrent stress test: spawn N tasks publishing events simultaneously, verify final counts match expected totals
- Benchmark before/after with `criterion` if we want to quantify improvement (optional)

## Effort Estimate

| Component | Estimate |
|-----------|----------|
| InProcessEventBus conversion | ~1-2 hours |
| WorkerEventSubscriber conversion | ~1 hour |
| CircuitBreakerMetrics conversion | ~1-2 hours |
| Testing | ~30 minutes |

**Total: ~4-5 hours**

## Out of Scope

The following use cases are NOT simple counters and are handled by TAS-163:
- `HashMap<Uuid, PendingEvent>` (FFI dispatch pending events)
- `HashMap<Uuid, StateMachine>` (state manager)
- `Vec<Duration>` (orchestration latency ring buffer)

For these, consider `DashMap`, `tokio::sync::RwLock`, or concurrent ring buffers.

## References

- Article that inspired this: [How Rust's Ownership Model Quietly Replaces Synchronization](https://medium.com/@rumen.rm/how-rusts-ownership-model-quietly-replaces-synchronization-63c6f0a7c40b)
- Existing pattern: `AtomicStepExecutionStats` in `tasker-worker/src/worker/actors/worker_status_actor.rs`
- TAS-69: Worker actor-service decomposition (established the atomic stats pattern)
- TAS-163: Concurrent data structures for non-counter cases

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-134/convert-hot-path-stats-to-atomic-counters](https://linear.app/tasker-systems/issue/TAS-134/convert-hot-path-stats-to-atomic-counters)
- Identifier: TAS-134
- Status: Backlog
- Priority: Low
- Assignee: Pete Taylor
- Labels: Improvement
- Project: [Tasker Core Workers](https://linear.app/tasker-systems/project/tasker-core-workers-3e6c7472b199). Workers in Tasker Core
- Created: 2026-01-10T03:11:07.707Z
- Updated: 2026-01-22T13:30:00.000Z
