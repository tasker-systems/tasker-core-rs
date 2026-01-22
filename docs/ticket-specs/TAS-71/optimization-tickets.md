# TAS-161 Follow-up: Optimization Tickets

**Date:** 2026-01-22
**Based on:** Profiling report from TAS-161, mutex analysis, connection pool review

---

## Summary of Findings

### Profiling Results (TAS-161)
- **System is I/O bound** (93%+ time waiting), not CPU bound
- **Logging overhead**: 5.15% combined (tracing 2.7% + stdout 2.45%)
- **DB operations**: 4.6% of worker time
- **JSON serialization**: 0.54% - minimal impact
- **Clone/allocation**: <0.3% - no hot spots

### Mutex-based Metrics Analysis
Found several locations using `std::sync::Mutex` or `std::sync::RwLock` in async contexts:

| Location | Type | Issue |
|----------|------|-------|
| `circuit_breaker.rs` | `Arc<Mutex<CircuitBreakerMetrics>>` | Called on every CB operation |
| `circuit_breaker.rs` | `Arc<Mutex<Option<Instant>>>` | opened_at timestamp |
| `ffi_dispatch_channel.rs` | `Arc<RwLock<HashMap<Uuid, PendingEvent>>>` | Blocking RwLock in async |
| `orchestration_event_system.rs` | `std::sync::Mutex<Vec<Duration>>` | Latency tracking |
| `state_manager.rs` | `Arc<Mutex<HashMap>>` | Task/step state machines |

### Connection Pool Configuration
- Main DB: max=25, min=5, acquire_timeout=10s
- PGMQ DB: max=15, min=3, acquire_timeout=5s
- No pool utilization metrics currently exposed

---

## Optimization Tickets

### Tier 1: Low-Hanging Fruit

#### TAS-162: Hot Path Logging Optimization

**Priority:** High
**Effort:** Low
**Impact:** ~2-3% CPU reduction

**Description:**
Reduce logging overhead in high-frequency code paths by implementing conditional logging guards and considering async/buffered logging for production.

**Targets:**
- `command_processor_actor` (4.31% total time)
- `result_processor` (4.03% total time)
- `step_enqueuer` (2.49% total time)
- `task_finalizer` (2.27% total time)

**Implementation:**
1. Add `tracing::enabled!` guards for DEBUG-level spans in hot paths
2. Evaluate moving to buffered stdout logging in production
3. Review necessity of all spans in actor message handlers
4. Consider `#[instrument(skip_all)]` for frequently-called helpers

**Acceptance Criteria:**
- [ ] Audit hot path logging in identified modules
- [ ] Implement `tracing::enabled!` guards where beneficial
- [ ] Benchmark before/after with profiling profile
- [ ] Document logging level guidelines for hot paths

---

#### TAS-134: Convert Hot-Path Stats to Atomic Counters (Existing)

**Priority:** Low → Medium (upgraded based on profiling)
**Effort:** Low (~4-5 hours)
**Impact:** Reduced lock contention for simple counter metrics

**Description:**
Convert `Arc<Mutex<Stats>>` patterns in hot paths to atomic counters, following the established `AtomicStepExecutionStats` pattern from TAS-69.

**Targets (simple counters):**
1. `InProcessEventBus` stats - `Arc<std::sync::Mutex<InProcessEventBusStats>>`
2. `WorkerEventSubscriber` stats - `Arc<std::sync::Mutex<WorkerEventSubscriberStats>>`
3. `CircuitBreakerMetrics` - `Arc<Mutex<CircuitBreakerMetrics>>` (**NEW** - added based on profiling)

**Implementation:** Use `AtomicU64` with `Ordering::Relaxed` for independent counters.

**Spec:** `docs/ticket-specs/TAS-134.md`

---

#### TAS-163: Concurrent Data Structures for HashMap-based State

**Priority:** Medium
**Effort:** Medium (~6 hours)
**Impact:** Reduced lock contention for non-counter state

**Description:**
Replace `std::sync::RwLock<HashMap<K, V>>` patterns with async-friendly concurrent data structures (`DashMap` or `tokio::sync::RwLock`) for state that requires map semantics.

**Targets (complex state - NOT simple counters):**

1. **FFI Dispatch Pending Events** (`ffi_dispatch_channel.rs:255`)
   - Current: `Arc<RwLock<HashMap<Uuid, PendingEvent>>>` (std::sync)
   - Recommendation: `DashMap`

2. **State Manager** (`state_manager.rs:97-98`)
   - Current: `Arc<Mutex<HashMap<Uuid, StateMachine>>>`
   - Recommendation: `DashMap`

3. **Orchestration Latency Buffer** (`orchestration_event_system.rs:97`)
   - Current: `std::sync::Mutex<Vec<Duration>>` (ring buffer)
   - Recommendation: `tokio::sync::Mutex` (simple migration)

**Spec:** `docs/ticket-specs/TAS-163.md`

**Note:** TAS-134 and TAS-163 are complementary:
- **TAS-134**: Simple counters → atomic operations
- **TAS-163**: HashMap/complex state → DashMap/tokio::sync

---

### Tier 2: Medium Effort

#### TAS-164: Connection Pool Observability and Tuning

**Priority:** Medium
**Effort:** Medium
**Impact:** Better capacity planning, potential latency reduction

**Description:**
Add connection pool utilization metrics and evaluate tuning based on observed patterns. Current profiling shows 4.6% of worker time in database operations, with 0.85% in pool acquire and 1.32% in connection waiting.

**Current Configuration:**
```toml
[common.database.pool]
max_connections = 25
min_connections = 5
acquire_timeout_seconds = 10
idle_timeout_seconds = 300
max_lifetime_seconds = 1800

[common.pgmq_database.pool]
max_connections = 15
min_connections = 3
acquire_timeout_seconds = 5
```

**Implementation:**
1. **Add Pool Metrics Endpoint**
   - Expose pool utilization via `/health` or `/metrics`
   - Track: active connections, idle connections, acquire latency P50/P95/P99

2. **SQLx Event Handlers**
   - Implement `sqlx::pool::ConnectOptions::after_connect` callbacks
   - Log/metric slow connection acquisitions (>100ms)

3. **Tuning Recommendations** (after metrics collection):
   - If acquire waits common: increase `max_connections`
   - If connections frequently expire: adjust `max_lifetime`
   - If many idle connections: reduce `min_connections`

4. **Config-Driven Pool Sizing**
   - Add environment-specific overrides in `config/tasker/environments/`
   - Consider separate pool sizes for orchestration vs worker

**Acceptance Criteria:**
- [ ] Pool utilization metrics exposed in health/metrics endpoint
- [ ] Slow acquisition logging (>100ms threshold)
- [ ] Document tuning guidelines based on observed metrics
- [ ] Add pool configuration to environment overrides

---

### Tier 3: Not Recommended (Based on Profiling)

The following optimizations were evaluated but are **not recommended** based on profiling data:

| Optimization | Profiling Result | Recommendation |
|--------------|------------------|----------------|
| MessagePack serialization | JSON only 0.54% | Keep JSON for debugging ergonomics |
| Clone optimization | <0.02% | No hot spots identified |
| Memory layout optimization | No allocation hot spots | Not needed |
| SmallVec/CompactString | Allocation <0.3% | Premature optimization |

---

## Appendix: Files to Modify

### TAS-162 (Logging)
- `tasker-orchestration/src/orchestration/actors/command_processor_actor.rs`
- `tasker-orchestration/src/orchestration/lifecycle/result_processor.rs`
- `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs`
- `tasker-orchestration/src/orchestration/lifecycle/task_finalizer.rs`

### TAS-134 (Atomic Counters)
- `tasker-worker/src/worker/in_process_event_bus.rs`
- `tasker-worker/src/worker/event_subscriber.rs`
- `tasker-shared/src/resilience/circuit_breaker.rs`

### TAS-163 (Concurrent Data Structures)
- `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`
- `tasker-orchestration/src/orchestration/state_manager.rs`
- `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`

### TAS-164 (Connection Pools)
- `tasker-shared/src/database/pools.rs`
- `config/tasker/base/common.toml`
- `config/tasker/environments/*/common.toml`
- Health endpoint handlers

---

## Priority Matrix

| Ticket | Priority | Effort | Impact | Dependencies |
|--------|----------|--------|--------|--------------|
| TAS-162 | High | Low | ~2-3% CPU | None |
| TAS-134 | Medium | Low | Lock contention (counters) | None |
| TAS-163 | Medium | Medium | Lock contention (HashMaps) | None |
| TAS-164 | Medium | Medium | Observability | None |

**Recommended Order:** TAS-162 → TAS-134 → TAS-163 → TAS-164

**Note:** TAS-134 and TAS-163 can be done in parallel since they target different data structures.

---

## References

- Profiling Report: `docs/ticket-specs/TAS-71/profiling-report.md`
- Profiling Spec: `docs/ticket-specs/TAS-71/TAS-161-profiling.md`
- Profile Data: `docs/ticket-specs/TAS-71/profile-data/`
