# TAS-163: Concurrent Data Structures for HashMap-based State

## Summary

Analyze and optimize the concurrent data structure patterns currently using `std::sync::RwLock<HashMap<K, V>>` or similar blocking constructs in async contexts.

This ticket complements TAS-134 (atomic counters for simple metrics). While TAS-134 handles simple counter conversions, this ticket addresses more complex concurrent state management with an emphasis on **choosing the right data structure for each use case**, not just defaulting to DashMap.

## Background

From TAS-161 profiling analysis, we identified several locations using blocking `std::sync::RwLock` in async contexts. Before jumping to solutions, we should analyze whether HashMap is even the right abstraction for each case.

### Why Not Just Use DashMap Everywhere?

HashMap (and DashMap) may be the common choice, but not always the best fit:

| Pattern | HashMap Good For | Consider Alternatives When |
|---------|------------------|---------------------------|
| Key-value lookup | Random access by key | Keys are sequential integers |
| Dynamic sizing | Unknown entry count | Bounded, known max size |
| Long-lived entries | Entries persist | Short-lived, high churn |
| Sparse keys | Keys spread across space | Dense, sequential keys |

### Alternative Data Structures to Evaluate

| Structure | Best For | Rust Crates |
|-----------|----------|-------------|
| **Slab/SlotMap** | Integer-indexed, generation-tracked | `slab`, `slotmap`, `thunderdome` |
| **ArrayQueue** | Bounded FIFO/ring buffer | `crossbeam::queue::ArrayQueue` |
| **SegQueue** | Unbounded concurrent queue | `crossbeam::queue::SegQueue` |
| **Sharded HashMap** | High write contention | `dashmap` (built-in sharding) |
| **Lock-free list** | Append-mostly patterns | `crossbeam::queue` |
| **Arena allocator** | Batch allocation/deallocation | `bumpalo`, `typed-arena` |

---

## Phase 1: Access Pattern Analysis

Before selecting data structures, analyze each location:

### Analysis Checklist

For each data structure, document:
1. **Key type**: UUID, integer, string?
2. **Value lifetime**: Short (request-scoped) or long (process lifetime)?
3. **Access pattern**: Read-heavy, write-heavy, or balanced?
4. **Iteration frequency**: How often do we need to iterate all entries?
5. **Size bounds**: Known max? Typical size? Growth pattern?
6. **Concurrency pattern**: Single writer? Multiple writers? Read-mostly?
7. **Ordering requirements**: Need sorted access? FIFO? LIFO?

---

## Locations to Analyze

### 1. FFI Dispatch Pending Events (Higher Priority)

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs:255`

**Current**:
```rust
pending_events: Arc<RwLock<HashMap<Uuid, PendingEvent>>>  // std::sync::RwLock
```

**Hot path**:
- `poll()` - inserts pending event on every dispatch
- `complete()` - removes pending event on every completion
- `metrics()` - reads all pending events for age calculations

#### Access Pattern Analysis

| Aspect | Value | Notes |
|--------|-------|-------|
| Key type | `Uuid` | Random, not sequential |
| Value lifetime | Short | Request-scoped (poll → complete) |
| Access pattern | Balanced | Insert on poll, remove on complete |
| Iteration frequency | Low | Only for `metrics()` endpoint |
| Size bounds | Bounded | Limited by semaphore (max concurrent handlers) |
| Concurrency | Multi-writer | FFI threads + async context |
| Ordering | None | No ordering requirements |

#### Candidate Data Structures

| Option | Pros | Cons |
|--------|------|------|
| **DashMap** | Drop-in replacement, sharded | Overkill if bounded small |
| **Slab + secondary index** | Integer keys, O(1) lookup | Need UUID→index mapping |
| **SlotMap** | Generation tracking for safety | Still need UUID→key mapping |
| **tokio::sync::RwLock<HashMap>** | Simple migration | Still contended lock |

#### Preliminary Recommendation

**DashMap** is likely appropriate here because:
- Keys are random UUIDs (not sequential integers)
- Need fast lookup by UUID for `complete()`
- Bounded size (semaphore-limited), so sharding overhead acceptable

**Alternative to investigate**: If we control the dispatch ID, we could use a `Slab<PendingEvent>` with integer keys and return the slab key instead of UUID. This would be faster but requires API changes.

### 2. State Manager State Machines (Medium Priority)

**File**: `tasker-orchestration/src/orchestration/state_manager.rs:97-98`

**Current**:
```rust
task_state_machines: Arc<Mutex<HashMap<Uuid, TaskStateMachine>>>
step_state_machines: Arc<Mutex<HashMap<Uuid, StepStateMachine>>>
```

**Hot path**: State machine lookups and updates during task/step processing

#### Access Pattern Analysis

| Aspect | Task SM | Step SM | Notes |
|--------|---------|---------|-------|
| Key type | `Uuid` | `Uuid` | Task/step UUIDs |
| Value lifetime | Medium | Short-Medium | Task duration |
| Access pattern | Read-heavy | Read-heavy | Lookups >> mutations |
| Iteration frequency | Rare | Rare | Mostly point lookups |
| Size bounds | Unbounded | Unbounded | Grows with active tasks |
| Concurrency | Multi-reader | Multi-reader | Orchestration actors |
| Ordering | None | None | No ordering requirements |

#### Candidate Data Structures

| Option | Pros | Cons |
|--------|------|------|
| **DashMap** | Read-optimized sharding | Memory overhead per shard |
| **flurry** | Java ConcurrentHashMap port | Less mature than DashMap |
| **evmap** | Eventually consistent, very fast reads | Complex for mutations |
| **papaya** | Modern, lock-free | Newer, less battle-tested |
| **tokio::sync::RwLock<HashMap>** | Simple | Single lock contention |

#### Preliminary Recommendation

**DashMap** is a good fit because:
- Read-heavy workload benefits from sharded reads
- UUID keys (random distribution) spread across shards naturally
- No ordering requirements

**Alternative to investigate**: `evmap` (eventually-consistent map) if we can tolerate slightly stale reads. State machines are typically accessed by their "owner" actor, so staleness may not matter.

### 3. Orchestration Latency Buffer (Lower Priority)

**File**: `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:97`

**Current**:
```rust
processing_latencies: std::sync::Mutex<Vec<Duration>>
```

**Hot path**: `record_latency()` - called after every event processing

**Issue**: Not a HashMap - it's a bounded ring buffer pattern (keeps last 1000 entries, drains oldest 500 when full).

#### Access Pattern Analysis

| Aspect | Value | Notes |
|--------|-------|-------|
| Key type | N/A | Sequential append (no key) |
| Value lifetime | Short | Dropped when buffer drains |
| Access pattern | Append-heavy | Write on every event |
| Iteration frequency | Low | Only for stats endpoints |
| Size bounds | Bounded (1000) | Drains to 500 when full |
| Concurrency | Single writer | Event processing loop |
| Ordering | FIFO | Need oldest entries for drain |

#### Candidate Data Structures

| Option | Pros | Cons |
|--------|------|------|
| **tokio::sync::Mutex<Vec>** | Simple migration | Still lock contention |
| **crossbeam::ArrayQueue** | Lock-free, bounded | Fixed size, no resize |
| **ringbuf** | Zero-copy ring buffer | Single producer/consumer |
| **Fixed array + AtomicUsize** | Minimal overhead | Manual wrap-around logic |
| **Histogram (hdrhistogram)** | Statistical summary | Loses individual values |

#### Key Insight

This is a **metrics aggregation** pattern, not a data lookup pattern. We're collecting latencies to compute:
- `processing_rate()` - events per second
- `average_latency_ms()` - mean latency

**Question**: Do we need individual latency values, or would a streaming statistical summary suffice?

#### Preliminary Recommendations

**Option A (Simple)**: `tokio::sync::Mutex<VecDeque<Duration>>`
- `VecDeque` is better than `Vec` for FIFO with `pop_front()`
- Async-friendly mutex avoids blocking runtime

**Option B (Lock-free)**: `crossbeam::ArrayQueue<Duration>` with size 1000
- True lock-free append
- For stats: drain into local Vec, compute, discard

**Option C (Best for metrics)**: `hdrhistogram::Histogram`
- Directly compute percentiles without storing all values
- Much lower memory footprint
- Trade-off: can't recompute over "last N" values

**Recommendation**: Start with Option A for simplicity. Consider Option C if we want better statistical analysis (P50, P95, P99) without storing 1000 values.

## Implementation Approach

### Phase 1: Analysis (Required First)

Before implementing any changes:

1. **Instrument current usage** - Add temporary metrics to measure:
   - Operations per second (insert, lookup, remove, iterate)
   - Typical collection sizes
   - Lock hold times

2. **Validate assumptions** - Confirm the access patterns documented above

3. **Benchmark candidates** - For each location, benchmark 2-3 candidates with realistic workloads

### Phase 2: Implementation

Based on analysis, implement the selected data structures.

#### DashMap Migration Pattern

```rust
// Before
pending_events: Arc<RwLock<HashMap<Uuid, PendingEvent>>>

// After
pending_events: Arc<DashMap<Uuid, PendingEvent>>

// Insert
self.pending_events.insert(event_id, pending_event);

// Remove
let pending = self.pending_events.remove(&event_id);

// Iteration for metrics
let ages: Vec<_> = self.pending_events
    .iter()
    .map(|entry| {
        let age = now.duration_since(entry.value().dispatched_at);
        (*entry.key(), age.as_millis() as u64)
    })
    .collect();
```

#### tokio::sync::RwLock Migration Pattern

```rust
// Before (std::sync)
let mut pending = self.pending_events.write().unwrap();

// After (tokio::sync)
let mut pending = self.pending_events.write().await;
```

**Note**: This requires the calling function to be async, which may require signature changes.

#### Slab/SlotMap Pattern (if applicable)

```rust
// If we can control the key type
use slab::Slab;

pending_events: Arc<Mutex<Slab<PendingEvent>>>

// Insert returns integer key
let key = pending_events.lock().await.insert(event);

// Lookup by integer key (O(1), no hashing)
let event = pending_events.lock().await.get(key);
```

## Key Design Decisions

1. **Analysis before implementation** - Don't assume HashMap/DashMap is the answer. Understand the access pattern first.

2. **Right tool for the job** - Different patterns warrant different structures:
   - Random key lookup → HashMap/DashMap
   - Sequential/integer keys → Slab/SlotMap
   - Append-only metrics → Ring buffer or histogram

3. **Consider the API boundary** - Some optimizations (like Slab) require changing the key type exposed to callers. Evaluate if API changes are acceptable.

4. **Benchmark with realistic workloads** - Micro-benchmarks can be misleading. Test with actual concurrency patterns.

5. **Maintain API compatibility where possible** - The external `metrics()` and `get_statistics()` APIs should return the same types.

## Questions to Answer During Analysis

1. **FFI Dispatch**: Can we change the event ID from UUID to an integer key? Would enable Slab.

2. **State Manager**: Are state machines ever accessed from multiple actors simultaneously, or is there implicit single-owner semantics?

3. **Latency Buffer**: Do we actually need individual latency values, or would P50/P95/P99 percentiles from a histogram suffice?

4. **General**: What's the typical size of each collection under production load? Small collections may not benefit from sharding overhead.

## Dependencies

- `dashmap` crate (already in workspace dependencies)
- No new dependencies needed for tokio::sync migrations

## Testing

- Existing unit tests should continue to pass
- Add concurrent stress tests for FFI dispatch channel
- Verify no deadlocks under high contention

## Effort Estimate

| Phase | Component | Estimate |
|-------|-----------|----------|
| **Phase 1: Analysis** | | |
| | Document access patterns | ~1 hour |
| | Add instrumentation (temporary) | ~1 hour |
| | Benchmark candidates | ~2 hours |
| | Write recommendation doc | ~1 hour |
| **Phase 2: Implementation** | | |
| | FFI dispatch pending events | ~2 hours |
| | State manager | ~2 hours |
| | Latency buffer | ~1 hour |
| | Testing | ~1 hour |

**Phase 1 Total: ~5 hours**
**Phase 2 Total: ~6 hours**
**Grand Total: ~11 hours** (can be split across sessions)

## Out of Scope

Simple counter metrics are handled by TAS-134:
- `InProcessEventBus` stats
- `WorkerEventSubscriber` stats
- `CircuitBreakerMetrics`

## Files to Modify

- `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`
- `tasker-orchestration/src/orchestration/state_manager.rs`
- `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`

## References

- TAS-134: Atomic counters for simple metrics (complementary ticket)
- TAS-161: Profiling report that identified these locations
- DashMap docs: https://docs.rs/dashmap
- Optimization Tickets: `docs/ticket-specs/TAS-71/optimization-tickets.md`

## Metadata
- URL: [https://linear.app/tasker-systems/issue/TAS-163/migrate-metrics-to-swmr-pattern-single-writer-many-reader](https://linear.app/tasker-systems/issue/TAS-163/migrate-metrics-to-swmr-pattern-single-writer-many-reader)
- Identifier: TAS-163
- Status: Backlog
- Priority: Medium
- Created: 2026-01-22
