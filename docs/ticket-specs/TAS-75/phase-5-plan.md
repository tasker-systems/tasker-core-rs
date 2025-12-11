# TAS-75 Phase 5: Circuit Breaker Expansion - Detailed Implementation Plan

**Date**: 2025-12-10
**Status**: Planning
**Branch**: `jcoletaylor/tas-67-rust-worker-dual-event-system`
**Dependencies**: Phase 4 Complete, TAS-51 (MPSC Channels), TAS-67 (Dual Event System)

---

## Executive Summary

Phase 5 expands circuit breaker coverage to protect critical failure points that currently lack protection. The goal is consistent, predictable failure handling across all components with integration into the existing backpressure system.

**Key Objectives**:
1. Add circuit breaker protection to FFI dispatch operations
2. Activate the dormant `task_readiness` circuit breaker
3. Wire new circuit breakers into the `StatusEvaluator` backpressure aggregation
4. Document the complete circuit breaker coverage matrix

---

## Current Circuit Breaker Architecture

### Existing Infrastructure

**Core Circuit Breaker** (`tasker-shared/src/resilience/circuit_breaker.rs`):
```rust
pub struct CircuitBreaker {
    name: String,
    state: AtomicU8,           // Closed=0, Open=1, HalfOpen=2
    config: CircuitBreakerConfig,
    metrics: Arc<Mutex<CircuitBreakerMetrics>>,
    opened_at: Arc<Mutex<Option<Instant>>>,
}

impl CircuitBreaker {
    // Wrap operations with circuit breaker protection
    pub async fn call<F, T, E, Fut>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>

    // Manual control for emergency situations
    pub async fn force_open(&self)
    pub async fn force_closed(&self)

    // Health checks
    pub async fn is_healthy(&self) -> bool
}
```

**WebDatabaseCircuitBreaker** (`tasker-orchestration/src/web/circuit_breaker.rs`):
- Specialized wrapper for web API database operations
- Already integrated into `StatusEvaluator`
- Protects API endpoints from database failures

### Current Coverage Matrix

| Component | Circuit Breaker | Status | Notes |
|-----------|----------------|--------|-------|
| Web API → Database | `WebDatabaseCircuitBreaker` | ✅ Active | Integrated with StatusEvaluator |
| FFI Dispatch | None | ❌ Missing | Indefinite retry with backoff |
| Task Readiness | Configured | ⚠️ Dormant | Config exists, not used |
| PGMQ Send | None | ⏳ Deferred | Same DB instance currently |

---

## Phase 5a: FFI Completion Send Circuit Breaker

### Problem Statement

The FFI dispatch channel (`tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs`) currently has no protection against completion channel backpressure. Understanding the flow:

```
Ruby/Python handler completes (handler execution has its own timeout)
       │
       ▼
FfiDispatchChannel.complete()
       │
       ├── 1. Remove from pending_events map (instant)
       │
       ├── 2. completion_sender.try_send() with retry loop  ← THIS IS WHAT WE PROTECT
       │      └── Retries every 10ms until completion_send_timeout (10s)
       │      └── Should complete in MILLISECONDS when healthy
       │
       ├── 3. Spawn post-handler callback (fire-and-forget, async)
       │
       └── 4. Return true/false to FFI caller
```

**Key Insight**: The completion channel send (step 2) should complete in **milliseconds** when the channel is healthy. The 10-second timeout is a worst-case backstop. If sends consistently take > 100ms, this indicates channel backpressure.

**Current Behavior**:
1. `complete()` retries indefinitely with `try_send()` + 10ms sleep
2. Ruby/Python threads can block for up to `completion_send_timeout` (default 10s)
3. No early detection of channel health issues
4. No fail-fast when persistent backpressure detected

**Risk**: If the completion channel becomes persistently backed up, all FFI threads block waiting, causing handler starvation even when capacity exists.

### Proposed Solution

Create a latency-based circuit breaker that detects channel health issues early:

```rust
// tasker-worker/src/worker/handlers/ffi_completion_circuit_breaker.rs

/// Circuit breaker for FFI completion channel send operations
///
/// This breaker monitors completion channel send latency, NOT handler execution time.
/// Healthy sends complete in milliseconds; slow sends indicate backpressure.
pub struct FfiCompletionCircuitBreaker {
    breaker: CircuitBreaker,
    /// Threshold in ms above which a send is considered "slow" (failure)
    slow_send_threshold_ms: u64,
}

impl FfiCompletionCircuitBreaker {
    pub fn new(config: FfiCompletionCircuitBreakerConfig) -> Self {
        Self {
            breaker: CircuitBreaker::new(
                "ffi_completion_send".to_string(),
                CircuitBreakerConfig {
                    failure_threshold: config.failure_threshold,
                    timeout: Duration::from_secs(config.recovery_timeout_seconds),
                    success_threshold: config.success_threshold,
                },
            ),
            slow_send_threshold_ms: config.slow_send_threshold_ms,
        }
    }

    /// Check if circuit is healthy before attempting send
    pub fn is_healthy(&self) -> bool {
        self.breaker.state() == CircuitState::Closed
    }

    /// Record send result based on latency
    /// - Fast send (< threshold): success
    /// - Slow send (>= threshold): failure
    /// - Send error: failure
    pub async fn record_send_result(&self, elapsed: Duration, success: bool) {
        if !success || elapsed.as_millis() as u64 >= self.slow_send_threshold_ms {
            self.breaker.record_failure(elapsed).await;
        } else {
            self.breaker.record_success(elapsed).await;
        }
    }

    pub fn state(&self) -> CircuitState {
        self.breaker.state()
    }
}
```

### Integration Points

**1. FfiDispatchChannel Modification** (`ffi_dispatch_channel.rs:410-540`):

```rust
// In FfiDispatchChannel.complete()

// 1. Check circuit breaker BEFORE attempting send
if !self.circuit_breaker.is_healthy() {
    warn!(
        service_id = %self.config.service_id,
        event_id = %event_id,
        "FFI completion: circuit breaker open, failing fast"
    );
    // Return false - step will return to queue via visibility timeout
    // This is safe because result hasn't been sent yet
    return false;
}

// 2. Time the send operation
let send_start = Instant::now();
let send_result = /* existing try_send() retry loop */;
let send_elapsed = send_start.elapsed();

// 3. Record result based on latency
self.circuit_breaker.record_send_result(send_elapsed, send_result.is_ok()).await;

// Log slow sends for observability (even if successful)
if send_elapsed.as_millis() > 100 {
    warn!(
        service_id = %self.config.service_id,
        event_id = %event_id,
        elapsed_ms = send_elapsed.as_millis(),
        "FFI completion: slow send detected (channel backpressure)"
    );
}
```

**2. Configuration** (`config/tasker/base/worker.toml`):

```toml
[worker.circuit_breakers.ffi_completion_send]
# Number of slow/failed sends before circuit opens
failure_threshold = 5

# How long circuit stays open before testing recovery (seconds)
# Short because channel recovery should be fast once backpressure clears
recovery_timeout_seconds = 5

# Successful fast sends needed in half-open to close circuit
success_threshold = 2

# Send latency above this threshold (ms) counts as "slow" (failure)
# Healthy sends should complete in single-digit milliseconds
slow_send_threshold_ms = 100
```

**3. Fail-Fast Behavior**:

When circuit is open:
- `complete()` returns `false` immediately
- Step result is NOT sent to orchestration
- Step remains claimed but incomplete
- Visibility timeout expires, step returns to queue
- Another worker (or same worker after recovery) claims it

This is **idempotent** because:
- Step was claimed but handler result was never committed
- PGMQ visibility timeout handles retry
- State machine guards prevent duplicate completion

**4. Metrics Integration**:

```rust
// tasker-shared/src/metrics/worker.rs

/// FFI completion circuit breaker state gauge
pub fn ffi_completion_breaker_state() -> Gauge<u64>;

/// FFI completion send latency histogram
pub fn ffi_completion_send_duration() -> Histogram<f64>;

/// FFI completion slow sends counter
pub fn ffi_completion_slow_sends_total() -> Counter<u64>;

/// FFI completion circuit open counter (fail-fast events)
pub fn ffi_completion_circuit_open_rejections() -> Counter<u64>;
```

### Task Breakdown

| Task | Effort | Priority | File(s) |
|------|--------|----------|---------|
| Create FfiCompletionCircuitBreaker struct | Low | High | `tasker-worker/src/worker/handlers/ffi_completion_circuit_breaker.rs` |
| Add circuit breaker config to WorkerConfig | Low | High | `tasker-shared/src/config/worker.rs` |
| Integrate latency tracking into FfiDispatchChannel | Medium | High | `tasker-worker/src/worker/handlers/ffi_dispatch_channel.rs` |
| Add OpenTelemetry metrics | Low | Medium | `tasker-shared/src/metrics/worker.rs` |
| Add unit tests for latency-based state transitions | Medium | High | Same files |
| Add integration test for fail-fast + recovery | Medium | Medium | `tests/` |

---

## Phase 5b: Task Readiness Circuit Breaker Activation

### Problem Statement

The `task_readiness` circuit breaker is configured but never instantiated:

**Configuration exists** (`config/tasker/base/common.toml`):
```toml
[common.circuit_breakers.task_readiness]
failure_threshold = 3
timeout_seconds = 15
success_threshold = 1
```

**But not used**: `TaskReadinessEventSystem` creates a `FallbackPoller` that queries the database directly without circuit breaker protection.

### Proposed Solution

Wire the `task_readiness` circuit breaker into the fallback poller:

**1. Create TaskReadinessCircuitBreaker** (similar to WebDatabaseCircuitBreaker):

```rust
// tasker-orchestration/src/orchestration/task_readiness/circuit_breaker.rs

pub struct TaskReadinessCircuitBreaker {
    breaker: CircuitBreaker,
}

impl TaskReadinessCircuitBreaker {
    pub fn from_config(config: &CircuitBreakerConfig) -> Self {
        Self {
            breaker: CircuitBreaker::new("task_readiness".to_string(), config.clone()),
        }
    }

    pub async fn execute_query<T, F, Fut>(&self, operation: F) -> TaskerResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = TaskerResult<T>>,
    {
        match self.breaker.call(operation).await {
            Ok(result) => result,
            Err(CircuitBreakerError::CircuitOpen { .. }) => {
                Err(TaskerError::service_unavailable("Task readiness circuit breaker open"))
            }
            Err(CircuitBreakerError::OperationFailed(e)) => Err(e),
            Err(CircuitBreakerError::ConfigurationError(msg)) => {
                Err(TaskerError::configuration_error(msg))
            }
        }
    }
}
```

**2. Integrate into FallbackPoller**:

The fallback poller queries for ready tasks periodically. Wrap database queries with circuit breaker:

```rust
// In FallbackPoller::poll_once()
let ready_tasks = self.circuit_breaker.execute_query(|| async {
    get_ready_tasks(&self.pool, batch_size).await
}).await?;
```

**3. Wire into StatusEvaluator**:

Add `task_readiness` circuit breaker state to the backpressure computation:

```rust
// In StatusEvaluator::compute_backpressure()
if db_status.task_readiness_breaker_open {
    return BackpressureStatus {
        active: true,
        reason: Some("Task readiness circuit breaker open".to_string()),
        retry_after_secs: Some(15),
        source: Some(BackpressureSource::CircuitBreaker),
    };
}
```

### Task Breakdown

| Task | Effort | Priority | File(s) |
|------|--------|----------|---------|
| Create TaskReadinessCircuitBreaker | Low | Medium | `tasker-orchestration/src/orchestration/task_readiness/circuit_breaker.rs` |
| Integrate into FallbackPoller | Medium | Medium | `tasker-orchestration/src/orchestration/task_readiness/fallback_poller.rs` |
| Add to DatabaseHealthStatus | Low | Medium | `tasker-orchestration/src/health/types.rs` |
| Wire into StatusEvaluator | Low | Medium | `tasker-orchestration/src/health/status_evaluator.rs` |
| Add unit tests | Medium | Medium | Same files |

---

## Phase 5c: Backpressure Integration

### Current Backpressure Architecture

The `StatusEvaluator` runs as a background task, evaluating:
1. **Database health** via `evaluate_db_status()`
2. **Channel saturation** via `evaluate_channel_status()`
3. **Queue depths** via `evaluate_queue_status()`

Results are aggregated in `compute_backpressure()` and cached for API handlers.

### New Integration Points

**1. Extend DatabaseHealthStatus**:

```rust
// tasker-orchestration/src/health/types.rs
pub struct DatabaseHealthStatus {
    // Existing fields
    pub evaluated: bool,
    pub is_connected: bool,
    pub circuit_breaker_open: bool,
    pub circuit_breaker_failures: u32,

    // New: task_readiness circuit breaker
    pub task_readiness_breaker_open: bool,
    pub task_readiness_failures: u32,
}
```

**2. Worker Health via `/health` Endpoint**:

Workers expose circuit breaker state via the existing `/health` endpoint:

```rust
// Worker /health response includes circuit breaker state
{
    "status": "healthy",  // or "degraded" when breaker open
    "circuit_breakers": {
        "ffi_completion_send": {
            "state": "closed",  // closed | open | half_open
            "failure_count": 0,
            "slow_send_count": 0,
            "last_transition": "2025-12-10T10:00:00Z"
        }
    },
    "metrics": {
        "completion_send_p99_ms": 5.2,
        "completion_send_slow_rate": 0.0
    }
}
```

> **Note**: gRPC streaming for real-time health is deferred to a separate ticket covering gRPC options across the board.

**3. Alerting via OpenTelemetry**:

Circuit breaker metrics are already wired into OpenTelemetry. Alerts can be configured in the observability backend (Grafana, Honeycomb) based on:
- `circuit_breaker_state{name="ffi_completion_send"} == 1` (open state)
- `ffi_completion_slow_sends_total` rate > threshold
- `ffi_completion_send_duration` p99 > 100ms

### Task Breakdown

| Task | Effort | Priority | File(s) |
|------|--------|----------|---------|
| Extend DatabaseHealthStatus | Low | High | `tasker-orchestration/src/health/types.rs` |
| Update evaluate_db_status() | Low | High | `tasker-orchestration/src/health/db_status.rs` |
| Update compute_backpressure() | Low | High | `tasker-orchestration/src/health/status_evaluator.rs` |
| Add circuit breaker state to worker /health | Low | High | `workers/rust/src/api/` |
| Document worker health endpoint format | Low | Medium | `docs/operations/` |

---

## Phase 5d: Documentation

### Circuit Breaker Coverage Matrix Document

Create `docs/circuit-breaker-coverage.md`:

```markdown
# Circuit Breaker Coverage Matrix

## Production Circuit Breakers

| Name | Component | Protects | Trigger | Failure Mode | Recovery |
|------|-----------|----------|---------|--------------|----------|
| `web_database` | API Layer | Database queries | Query error | 503 Backpressure | Auto after 30s |
| `task_readiness` | Orchestration | Ready task queries | Query error | Skip polling cycle | Auto after 15s |
| `ffi_completion_send` | Worker FFI | Completion channel | Send latency > 100ms | Fail fast, PGMQ retry | Auto after 5s |

## Configuration Reference

### Standard Circuit Breakers (error-triggered)

```toml
failure_threshold = 5       # Consecutive errors before opening
timeout_seconds = 30        # Time in open state before half-open
success_threshold = 2       # Successes in half-open to close
```

### Latency-Based Circuit Breakers (FFI completion)

```toml
failure_threshold = 5       # Consecutive slow sends before opening
recovery_timeout_seconds = 5  # Time in open state before half-open
success_threshold = 2       # Fast sends in half-open to close
slow_send_threshold_ms = 100  # Latency threshold for "slow" classification
```

## Monitoring

### Metrics
- `circuit_breaker_state{name="..."}` - Current state (0=closed, 1=open, 2=half-open)
- `circuit_breaker_failures_total{name="..."}` - Total failure count
- `circuit_breaker_transitions_total{name="...", from="...", to="..."}` - State transitions
- `ffi_completion_send_duration` - Histogram of send latencies
- `ffi_completion_slow_sends_total` - Counter of sends exceeding threshold

### Alerts
- Circuit open > 5 minutes: Page on-call
- Circuit flapping (>3 open/close in 10 minutes): Investigate root cause
- FFI completion slow sends > 10/min: Channel backpressure building
```

### Task Breakdown

| Task | Effort | Priority | File(s) |
|------|--------|----------|---------|
| Create circuit-breaker-coverage.md | Medium | Medium | `docs/circuit-breaker-coverage.md` |
| Update backpressure-architecture.md | Low | Medium | `docs/backpressure-architecture.md` |
| Add to operations runbook | Low | Medium | `docs/operations/backpressure-monitoring.md` |

---

## Implementation Order

### Recommended Sequence

1. **Phase 5a.1**: FFI Dispatch Circuit Breaker infrastructure (struct, config)
2. **Phase 5a.2**: FFI Dispatch integration + metrics
3. **Phase 5b.1**: Task Readiness Circuit Breaker infrastructure
4. **Phase 5b.2**: Task Readiness FallbackPoller integration
5. **Phase 5c**: StatusEvaluator integration for new breakers
6. **Phase 5d**: Documentation

### Rationale

- FFI dispatch is higher priority because it's in the critical path for all Ruby/Python workers
- Task readiness is medium priority because fallback polling is already resilient
- StatusEvaluator integration depends on both being implemented
- Documentation can be done incrementally

---

## Testing Strategy

### Unit Tests

**FFI Dispatch Circuit Breaker**:
- Circuit opens after failure threshold
- Circuit stays open until timeout
- Half-open allows test calls
- Circuit closes after success threshold
- Metrics are emitted correctly

**Task Readiness Circuit Breaker**:
- Same state machine tests
- Integration with FallbackPoller skip behavior
- Recovery after timeout

### Integration Tests

**Backpressure Flow**:
- Open FFI dispatch breaker → verify worker degrades gracefully
- Open task readiness breaker → verify orchestration continues with cached state
- Multiple breakers open → verify priority order in backpressure reporting

### Load Tests (Phase 6)

- Sustained load causing channel backup → verify breaker opens
- Recovery after load reduction → verify breaker closes
- Breaker flapping detection under variable load

---

## Configuration Summary

### New Configuration (worker.toml)

```toml
[worker.circuit_breakers.ffi_completion_send]
# Latency-based circuit breaker for completion channel sends
# Protects FFI threads from blocking on backed-up completion channel

# Number of slow/failed sends before circuit opens
failure_threshold = 5

# How long circuit stays open before testing recovery (seconds)
# Short because channel recovery should be fast once backpressure clears
recovery_timeout_seconds = 5

# Successful fast sends needed in half-open to close circuit
success_threshold = 2

# Send latency above this threshold (ms) counts as "slow" (failure)
# Healthy sends should complete in single-digit milliseconds
slow_send_threshold_ms = 100
```

### Existing Configuration (common.toml)

```toml
[common.circuit_breakers.web]
# Database query protection for web API
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2

[common.circuit_breakers.task_readiness]
# Database query protection for task readiness polling
failure_threshold = 3
timeout_seconds = 15
success_threshold = 1
```

### Configuration Rationale

| Breaker | Timeout | Rationale |
|---------|---------|-----------|
| `ffi_completion_send` | 5s | Channel recovery is fast; want to test quickly |
| `task_readiness` | 15s | Database recovery may take longer |
| `web` | 30s | Conservative for production API stability |

| Breaker | Slow Threshold | Rationale |
|---------|----------------|-----------|
| `ffi_completion_send` | 100ms | Healthy channel sends complete in < 10ms |
| `task_readiness` | N/A | Uses standard failure (query error) |
| `web` | N/A | Uses standard failure (query error) |

---

## Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| False positive breaker opens | Medium | Low | Conservative thresholds, monitoring |
| Breaker doesn't open fast enough | High | Low | Appropriate failure threshold |
| Metrics overhead | Low | Low | Lazy initialization, batching |
| Breaking changes to worker config | Medium | Low | Defaults match current behavior |

---

## Success Criteria

- [ ] FFI dispatch circuit breaker operational with metrics
- [ ] Task readiness circuit breaker operational
- [ ] StatusEvaluator reports all circuit breaker states
- [ ] Health endpoint shows circuit breaker status
- [ ] Documentation complete
- [ ] All existing tests pass
- [ ] New unit tests for circuit breaker behavior
