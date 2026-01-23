# TAS-163: Research & Analysis - Concurrent Data Structures

## Codebase Inventory

A comprehensive search identified **10 HashMap/collection-based concurrent data structures** in production code (excluding simple counter stats handled by TAS-134). Below is the complete inventory, categorized by priority based on hot-path frequency.

---

## Hot-Path Locations (High Priority)

### 1. FFI Dispatch Pending Events

**File**: `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs:255`

**Current**:
```rust
pending_events: Arc<RwLock<HashMap<Uuid, PendingEvent>>>  // std::sync::RwLock
```

**Access Points**:
- `poll()` (sync) - `write()` to insert pending event (line 410-421)
- `poll_async()` - `write()` to insert pending event (line 455-467)
- `complete()` (sync) - `write()` to remove pending event (line 503-509)
- `complete_async()` - `write()` to remove pending event (line 678-683)
- `pending_count()` - `read()` for length (line 753-758)
- `metrics()` - `read()` to iterate all entries for age calculation (line 765-768)

**Access Pattern Analysis**:

| Aspect | Value | Notes |
|--------|-------|-------|
| Key type | `Uuid` | Random, not sequential |
| Value lifetime | Short | Request-scoped (poll → complete) |
| Access pattern | Write-heavy | Insert on poll, remove on complete |
| Iteration frequency | Low | Only `metrics()` and `pending_count()` |
| Size bounds | Bounded | Limited by upstream semaphore |
| Concurrency | Multi-writer | FFI threads call poll/complete concurrently |
| Ordering | None | No ordering requirements |
| Lock type | `std::sync::RwLock` | Blocking in sync context (appropriate here since FFI calls are sync) |

**Critical Observation**: The `poll()` and `complete()` methods are called from FFI (Ruby/Python) threads which are not tokio tasks. Using `std::sync::RwLock` is actually correct for the sync path. However, `poll_async()` and `complete_async()` also use the same lock - these DO run in async context.

**Recommendation**: **DashMap** - eliminates the RwLock entirely. Insert/remove/read all become lock-free at the shard level. Key advantages:
- No lock poisoning concern (current code handles it with `unwrap_or_else`)
- Sharded concurrent access for FFI threads calling poll/complete simultaneously
- `iter()` for metrics is infrequent and DashMap handles it safely
- Drop-in API compatibility

---

### 2. State Manager State Machines

**File**: `tasker-orchestration/src/orchestration/state_manager.rs:97-98`

**Current**:
```rust
task_state_machines: Arc<Mutex<HashMap<Uuid, TaskStateMachine>>>  // tokio::sync::Mutex
step_state_machines: Arc<Mutex<HashMap<Uuid, StepStateMachine>>>  // tokio::sync::Mutex
```

**Access Points**:
- `get_or_create_task_state_machine()` - lock → get or insert → clone → unlock (line 529)
- `get_or_create_step_state_machine()` - lock → get or insert → clone → unlock (line 564)
- Each acquisition holds the mutex across a potential database query for cache misses

**Access Pattern Analysis**:

| Aspect | Task SM | Step SM | Notes |
|--------|---------|---------|-------|
| Key type | `Uuid` | `Uuid` | Task/step UUIDs |
| Value lifetime | Medium | Short-Medium | Task/step processing duration |
| Access pattern | Read-heavy | Read-heavy | Cache hit = read, miss = read + write |
| Iteration frequency | Never | Never | Only point lookups |
| Size bounds | Grows with active tasks | Grows with active steps | No eviction |
| Concurrency | Multi-reader, rare writer | Multi-reader, rare writer | Actors accessing state |
| Ordering | None | None | No ordering requirements |
| Lock type | `tokio::sync::Mutex` | `tokio::sync::Mutex` | Async-aware (good) |

**Critical Observation**: The mutex is held across a potential `.await` (database fetch on cache miss at lines 536-549, 571-584). This means during a cache miss, ALL other state machine lookups are blocked waiting. This is a significant contention point under load.

**Recommendation**: **DashMap** - resolves the "lock held across await" problem:
- Cache hits become shard-level reads with no contention
- Cache misses only contend at the shard level during insert
- The database fetch happens OUTSIDE any lock
- Pattern: `if let Some(machine) = map.get(&uuid) { return clone } else { fetch_from_db(); map.insert(uuid, machine) }`
- Caveat: Two concurrent cache misses for the same UUID could both fetch from DB. This is harmless (idempotent insert, both get valid machines).

---

### 3. Orchestration Latency Buffer

**File**: `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs:97`

**Current**:
```rust
processing_latencies: std::sync::Mutex<Vec<Duration>>
```

**Access Points**:
- `record_latency()` - lock → push → conditional drain (line 312-324)
- `processing_rate()` - lock → iterate last 100 (line 133-153)
- `average_latency_ms()` - lock → iterate all (line 156-167)

**Access Pattern Analysis**:

| Aspect | Value | Notes |
|--------|-------|-------|
| Key type | N/A | Sequential append (no key) |
| Value lifetime | Short | Dropped when buffer drains at 1000 |
| Access pattern | Append-heavy | Write on every event processed |
| Iteration frequency | Low | Only for stats endpoints |
| Size bounds | Bounded (1000) | Drains oldest 500 when full |
| Concurrency | Single writer likely | Event processing loop |
| Ordering | FIFO | Need oldest for drain |
| Lock type | `std::sync::Mutex` | Blocking in async context |

**Critical Observation**: This is NOT a HashMap pattern - it's a bounded ring buffer for metrics aggregation. The `Vec` with `drain(0..500)` is an inefficient ring buffer. Also uses `std::sync::Mutex` in what appears to be an async context (within `OrchestrationEventSystem` methods).

**Recommendation**: **`VecDeque<Duration>` with `tokio::sync::Mutex`** (simple improvement):
- `VecDeque` is O(1) for both push_back and drain from front
- `tokio::sync::Mutex` avoids blocking the runtime
- Keeps the same semantics, minimal code change

**Alternative**: If we want lock-free, `crossbeam::queue::ArrayQueue<Duration>` with capacity 1000:
- True lock-free push (overwrites oldest on full)
- For stats: snapshot into local vec, compute, discard
- More complex but zero contention

---

### 4. Correlation Tracker (Not in ticket but hot-path)

**File**: `tasker-worker/src/worker/event_subscriber.rs:383`

**Current**:
```rust
correlation_tracker: Arc<std::sync::Mutex<HashMap<Uuid, PendingExecution>>>
```

**Access Points**:
- `track_pending_execution()` - lock → insert (line 422-426)
- Async listener task - lock → remove (line 467-470)

**Access Pattern Analysis**:

| Aspect | Value | Notes |
|--------|-------|-------|
| Key type | `Uuid` | Correlation IDs |
| Value lifetime | Short | Insert on dispatch, remove on completion |
| Access pattern | Balanced | Insert + remove for each step |
| Iteration frequency | Never | Only point insert/remove |
| Size bounds | Bounded | Limited by concurrent step executions |
| Concurrency | Writer + async reader | Sync insert, async remove |
| Ordering | None | No ordering |
| Lock type | `std::sync::Mutex` | Used in both sync AND async contexts |

**Critical Observation**: `std::sync::Mutex` is locked inside an async task (line 467-470 inside `spawn_named!`). This can block the tokio runtime thread. The lock hold time is short (just remove), but it's still a concern under high throughput.

**Recommendation**: **DashMap** - same pattern as FFI dispatch pending events:
- Insert from sync context, remove from async context - DashMap handles both
- No lock poisoning concern
- Shard-level concurrency for parallel step executions

---

## Medium-Priority Locations

### 5. Task Template Manager Cache

**File**: `tasker-worker/src/worker/task_template_manager.rs:78`

**Current**:
```rust
cache: Arc<RwLock<HashMap<HandlerKey, CachedTemplate>>>
```

**Analysis**: Read-heavy cache with LRU eviction. Accessed on handler dispatch (moderate frequency). Could benefit from DashMap but is not a critical hot path since template lookups are amortized (cache hits are common after warmup).

**Recommendation**: **DashMap** - straightforward migration, eliminates contention on cache reads. Low risk, moderate benefit.

---

### 6. Worker Events Correlations

**File**: `tasker-shared/src/events/worker_events.rs:64`

**Current**:
```rust
event_correlations: Arc<RwLock<HashMap<Uuid, EventCorrelation>>>
```

**Analysis**: Debugging/observability data. Not in a hot path. Could be DashMap but low priority.

**Recommendation**: **Leave as-is** or migrate to DashMap opportunistically. Not a performance concern.

---

## Low-Priority Locations (Not Hot-Path)

### 7. Circuit Breaker Manager

**File**: `tasker-shared/src/resilience/manager.rs:17`

```rust
circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>
```

Setup-time access only. Breakers are created at initialization and then accessed by reference. The RwLock is rarely contended. **Leave as-is.**

### 8. Event Publisher Subscribers

**File**: `tasker-shared/src/events/publisher.rs:98`

```rust
subscribers: Arc<RwLock<HashMap<String, Vec<EventCallback>>>>
```

Subscribe is rare (startup), publish reads the map. Read-heavy with very rare writes. **Leave as-is** - RwLock is appropriate for this pattern.

### 9. RabbitMQ Queue Stats

**File**: `tasker-shared/src/messaging/service/providers/rabbitmq.rs:99`

```rust
queue_stats: Arc<RwLock<HashMap<String, Arc<QueueStatistics>>>>
```

Low-frequency access. **Leave as-is.**

### 10. PGMQ Demux Senders

**File**: `tasker-shared/src/messaging/service/providers/pgmq.rs:686`

```rust
senders: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<MessageNotification>>>>
```

Setup-time access for routing. **Leave as-is.**

---

## Summary: Implementation Scope

| # | Location | Current | Recommended | Priority |
|---|----------|---------|-------------|----------|
| 1 | FFI Pending Events | `Arc<RwLock<HashMap>>` | **DashMap** | High |
| 2 | State Manager (task) | `Arc<Mutex<HashMap>>` | **DashMap** | High |
| 3 | State Manager (step) | `Arc<Mutex<HashMap>>` | **DashMap** | High |
| 4 | Latency Buffer | `Mutex<Vec<Duration>>` | **tokio::sync::Mutex<VecDeque>** | High |
| 5 | Correlation Tracker | `Arc<Mutex<HashMap>>` | **DashMap** | High |
| 6 | Template Cache | `Arc<RwLock<HashMap>>` | **DashMap** | Medium |
| 7-10 | Others | Various | Leave as-is | Low |

---

## DashMap Migration Considerations

### API Differences

```rust
// RwLock<HashMap> → DashMap
// Before:
let map = self.pending_events.read().unwrap();
let value = map.get(&key);

// After:
let value = self.pending_events.get(&key);  // Returns Ref<K, V>

// Before (mutable):
let mut map = self.pending_events.write().unwrap();
map.insert(key, value);

// After:
self.pending_events.insert(key, value);  // No lock needed

// Iteration (metrics):
// Before:
let map = self.pending_events.read().unwrap();
for (k, v) in map.iter() { ... }

// After:
for entry in self.pending_events.iter() {
    let (k, v) = (entry.key(), entry.value());
}
```

### Thread Safety

DashMap is `Send + Sync` and safe to use from both sync and async contexts. This resolves the mixed sync/async access pattern in `FfiDispatchChannel` and `CorrelatedCompletionListener`.

### Potential Issues

1. **Deadlock with nested access**: DashMap uses internal sharded locks. Accessing multiple entries simultaneously (e.g., iterating while inserting) is safe but can deadlock if you hold a `Ref` and try to `insert` on the same shard. Our patterns are all single-operation (get, insert, remove, iterate) so this is not a concern.

2. **Memory overhead**: DashMap has ~256 bytes overhead per shard (default 16 shards). Negligible for our use case.

3. **Clone requirement on values**: `get()` returns a `Ref<K, V>` that holds a shard read-lock. If callers need owned values, they must clone. Our current code already clones (e.g., state machines are cloned after get).

---

## Latency Buffer: VecDeque vs Alternatives

The latency buffer is a special case - it's not a key-value store. Options:

| Option | Pros | Cons |
|--------|------|------|
| `tokio::sync::Mutex<VecDeque>` | Simple, O(1) drain | Still a single lock |
| `crossbeam::ArrayQueue` | Lock-free | Fixed capacity, no partial drain |
| `AtomicU64` ring buffer | Zero allocation | Manual index management |
| `hdrhistogram` | Full percentile support | Loses individual values |

**Chosen approach**: `tokio::sync::Mutex<VecDeque<Duration>>` because:
- Minimal code change
- Eliminates `std::sync::Mutex` in async context (the real problem)
- `VecDeque::drain(..500)` is O(1) amortized (vs Vec's O(n) shift)
- Lock hold time is very short (push is O(1), drain is O(n) but only every 1000 events)
- The single-writer pattern means contention is already low

---

## `last_processing_time` Field

**File**: `orchestration_event_system.rs:95`

```rust
last_processing_time: std::sync::Mutex<Option<Instant>>
```

This is a single value, not a collection. It should become an `AtomicU64` storing nanos-since-epoch, or simply be replaced by recording `Instant::now()` inline. However, since this is a single-value mutex (not a collection), it's more naturally TAS-134 scope. We can address it here since we're already modifying `OrchestrationStatistics`.

**Recommendation**: Convert to `AtomicU64` storing `elapsed().as_nanos()` from process start, or use `AtomicCell<Option<Instant>>` from crossbeam. Simplest: just inline the assignment since it's single-writer.
