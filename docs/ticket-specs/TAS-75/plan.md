# TAS-75: Backpressure Consistency Investigation

**Date**: 2025-12-08
**Status**: In Progress (Phase 1 Complete, Phase 2 Core Complete)
**Branch**: `jcoletaylor/tas-67-rust-worker-dual-event-system`
**Priority**: High
**Dependencies**: TAS-51 (MPSC Channels), TAS-67 (Worker Dual Event System)

---

## Executive Summary

This ticket investigates and addresses backpressure handling consistency across the tasker-core system. The goal is to ensure coherent, predictable behavior under load while preserving the system's core guarantee: **step idempotency**.

**Core Principle**: Any backpressure mechanism must ensure that step claiming, business logic execution, and result persistence remain stable and consistent. The system must gracefully degrade under load without compromising workflow correctness.

---

## Problem Statement

The system currently has multiple backpressure mechanisms implemented inconsistently:

| Component | Current Approach | Gap |
|-----------|------------------|-----|
| MPSC Channels | Bounded with monitoring (TAS-51) | Well-implemented |
| Handler Dispatch | Semaphore-bounded (TAS-67) | Well-implemented |
| API Endpoints | Circuit breaker (503) | **Metrics added** (TAS-75) |
| PGMQ Queues | No depth enforcement | **Planned** (soft limits) |
| Circuit Breakers | Partial coverage | **Documented** |
| Step Claiming | No refusal mechanism | **Missing** |

> **Note**: Rate limiting (429 responses) is explicitly **out of scope** for tasker-core. Rate limiting is the responsibility of upstream infrastructure (API Gateways, NLB/ALB, service mesh). Tasker focuses on system health-based backpressure via 503 responses.

---

## Research Findings

### 1. Orchestration System Backpressure

**Strengths**:
- MPSC channels bounded with configurable sizes (TAS-51)
- ChannelMonitor provides real-time observability
- Circuit breaker protects web API from database failures
- Backpressure strategies documented per channel

**Gaps Identified**:
- ~~No API rate limiting~~ (Out of scope - upstream responsibility)
- ~~No 503 backpressure signaling to clients~~ (**Implemented** - circuit breaker returns 503)
- Event system channels (`event_sender`, `stats_sender`) not monitored
- No queue depth checks before enqueueing steps (soft limits planned)
- Missing connection pool exhaustion handling

**Key Configuration** (`config/tasker/base/orchestration.toml`):
```toml
[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000

[orchestration.mpsc_channels.pgmq_events]
pgmq_event_buffer_size = 50000
```

### 2. Worker System Backpressure

**Strengths**:
- Semaphore-bounded handler dispatch (TAS-67)
- Permit release before send prevents backpressure cascade
- Handler timeout prevents stuck handlers
- Fire-and-forget domain events with explicit drop semantics

**Gaps Identified**:
- No explicit step claiming backpressure (workers always attempt claims)
- Domain events dropped silently when channel full (no metrics)
- Sequential completion processing (single-threaded)
- FfiDispatchChannel retries indefinitely with no backoff
- No worker-level load shedding

**Key Configuration** (`config/tasker/base/worker.toml`):
```toml
[worker.mpsc_channels.handler_dispatch]
max_concurrent_handlers = 10
handler_timeout_ms = 30000
dispatch_buffer_size = 1000
completion_buffer_size = 1000
```

### 3. PGMQ Messaging Backpressure

**Strengths**:
- Visibility timeout (30s) provides implicit backpressure
- Batch sizes configured and bounded
- Fallback polling mitigates notification loss
- Dead letter queue integration (TAS-49)

**Gaps Identified**:
- No maximum queue depth enforcement (soft limits planned, not hard rejection)
- `QueueCapacityExceeded` error defined but never raised
- pg_notify has 8KB payload limit (notifications silently fail)
- No queue depth monitoring/alerting (planned)
- Unbounded queue growth under sustained load

**Approach Decision**: Use soft limits with tiered monitoring (Normal → Warning → Critical → Overflow) rather than hard rejection. This keeps behavior portable for future RabbitMQ integration.

**Future Work**: Messaging circuit breaker is intentionally deferred. Currently PGMQ shares the same PostgreSQL instance as tasker data. When PGMQ is deployed to a separate database (or replaced with RabbitMQ), a dedicated messaging circuit breaker will be needed. The `MessagingError::CircuitBreakerOpen` error type already exists for this purpose.

**Key Configuration** (`config/tasker/base/common.toml`):
```toml
[pgmq]
visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100
# Planned: soft_depth_limit, warning_threshold_ratio, critical_threshold_ratio
```

### 4. Circuit Breaker Coverage

**Strengths**:
- Well-implemented 3-state machine (Closed, Open, HalfOpen)
- Configurable thresholds and timeouts
- Web circuit breaker separate from system operations
- Atomic state transitions

**Gaps Identified**:
- Web breaker only protects API, not internal operations
- `task_readiness` breaker configured but not actively used
- No circuit breaker for FFI operations
- No breaker for PGMQ send operations
- Limited orchestration operation protection

**Key Configuration** (`config/tasker/base/common.toml`):
```toml
[common.circuit_breakers.web]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2
```

### 5. Documentation Gaps

| Document | Status | Gap |
|----------|--------|-----|
| `docs/architecture-decisions/TAS-51-bounded-mpsc-channels.md` | Complete | - |
| `docs/operations/mpsc-channel-tuning.md` | Complete | - |
| `docs/development/mpsc-channel-guidelines.md` | Complete | - |
| `docs/worker-event-systems.md` | Complete | - |
| Unified backpressure strategy document | **Missing** | No single source of truth |
| PGMQ backpressure documentation | **Missing** | Queue depth, visibility timeout patterns |
| API-level backpressure documentation | **Missing** | 503 responses, rate limiting |
| Load testing strategy | **Missing** | No documented approach |
| Worker load shedding patterns | **Missing** | Step claim refusal strategies |

---

## Segmentation of Responsibility

### Orchestration System Responsibilities

The orchestration system must protect itself from:
1. **Client overload**: Too many `/v1/tasks` requests
2. **Internal saturation**: Command channel overflow
3. **Database exhaustion**: Connection pool depletion
4. **Queue explosion**: Unbounded PGMQ growth

**Recommended Backpressure Points**:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    ORCHESTRATION BACKPRESSURE POINTS                     │
└─────────────────────────────────────────────────────────────────────────┘

[1] API Layer (503 responses only - rate limiting is upstream)
    └── 503 Service Unavailable when circuit breaker open
    └── 503 Service Unavailable when queue depth critical
    └── 503 Service Unavailable when command channel saturated
    └── Retry-After header with calculated delay

[2] Command Processor Layer
    └── Bounded MPSC channel (existing - TAS-51)
    └── Backpressure: Block with timeout, then reject

[3] PGMQ Enqueue Layer
    └── Queue depth monitoring (soft limits, not hard rejection)
    └── Tiered response: Normal → Warning → Critical → Overflow
    └── Backpressure: API 503 at critical level

[4] Database Layer
    └── Connection pool limits (existing)
    └── Circuit breaker on failures (web database)
    └── Backpressure: Queue requests, then reject
```

### Worker System Responsibilities

The worker system must protect itself from:
1. **Handler saturation**: Too many concurrent handlers
2. **FFI backlog**: Ruby/Python handlers falling behind
3. **Completion overflow**: Results backing up
4. **Step starvation**: Claims outpacing processing

**Recommended Backpressure Points**:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      WORKER BACKPRESSURE POINTS                          │
└─────────────────────────────────────────────────────────────────────────┘

[1] Step Claiming Layer
    └── Check handler capacity before claiming
    └── Refusal: Leave message in queue (visibility timeout)
    └── Metric: claim_refusals counter

[2] Handler Dispatch Layer
    └── Semaphore-bounded (existing - TAS-67)
    └── Permit release before send (existing)
    └── Backpressure: Queue in dispatch channel

[3] FFI Dispatch Layer
    └── Pending event limit
    └── Starvation detection (existing)
    └── Backpressure: Block new dispatches

[4] Completion Processing Layer
    └── Bounded channel (existing)
    └── Consider parallel processing (TAS-67 future enhancement)
    └── Backpressure: Block handler completion send
```

---

## Step Idempotency Constraints

**Critical Invariant**: Backpressure mechanisms must never compromise step idempotency.

### Safe Backpressure Points

These points preserve idempotency:

| Point | Safe Because |
|-------|--------------|
| API rate limiting | Task not yet created |
| Queue depth check | Step not yet enqueued |
| Step claim refusal | Message stays in queue, visibility timeout |
| Handler dispatch channel | Step claimed but not executed |
| Completion channel backpressure | Handler completed, result buffered |

### Unsafe Patterns (Must Avoid)

| Pattern | Risk |
|---------|------|
| Drop step after claiming | Lost work, inconsistent state |
| Timeout during handler execution | Duplicate execution on retry |
| Drop completion result | Step completes but orchestration unaware |
| Reset step state without visibility | Race condition with other workers |

### Idempotency Guarantees

```
Step Execution Idempotency Contract:

1. CLAIM: Atomic claim via pgmq_read_specific_message()
   - Only one worker can claim a message
   - Visibility timeout protects against worker crash

2. EXECUTE: Handler invocation
   - Handlers MUST be idempotent
   - Timeout generates failure result (not drop)
   - Panic generates failure result (not drop)

3. PERSIST: Result submission to orchestration queue
   - Completion channel bounded but blocking
   - Result MUST reach orchestration
   - If send fails, step remains in "in_progress" → recovered by orchestration

4. FINALIZE: Orchestration processes result
   - State transition is atomic
   - Duplicate results handled by state guards
```

---

## Implementation Plan

### Phase 1: Documentation Foundation ✅ COMPLETE

**Goal**: Establish comprehensive backpressure documentation

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Create `docs/backpressure-architecture.md` | Medium | High | ✅ Done |
| Update `docs/worker-event-systems.md` with backpressure section | Low | High | ✅ Done |
| Create `docs/operations/backpressure-monitoring.md` | Medium | High | ✅ Done |
| Update `docs/development/mpsc-channel-guidelines.md` with backpressure patterns | Low | Medium | Deferred |

### Phase 2: API Layer Backpressure (Core Complete)

**Goal**: Protect orchestration from client overload via 503 responses

> **Note**: Rate limiting (429) is out of scope - handled by upstream API Gateway/NLB/ALB.

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Implement 503 response when circuit breaker open | Low | High | ✅ Existing |
| Add `api_requests_rejected_total` metric | Low | High | ✅ Done |
| Add `api_circuit_breaker_state` gauge metric | Low | High | ✅ Done |
| ~~Add configurable rate limiting middleware~~ | ~~Medium~~ | ~~Medium~~ | Out of scope |
| Extend 503 with queue depth awareness | Medium | Medium | Planned |
| Extend 503 with channel saturation awareness | Medium | Medium | ✅ Done |
| Add Retry-After header calculation | Low | Medium | ✅ Done |
| Integrate backpressure into POST /v1/tasks handler | Low | High | ✅ Done |
| Document API backpressure behavior | Low | High | ✅ Done |

**Implementation Details (TAS-75)**:
- `ApiError::Backpressure` variant added with `Retry-After` HTTP header support
- `AppState::check_backpressure_status()` checks circuit breaker AND channel saturation
- `execute_with_backpressure_check()` helper for simple endpoint wrapping
- `record_backpressure_rejection()` metric recording with reason labels
- Tiered Retry-After calculation: 5s (degraded), 15s (high), 30s (critical)
- POST /v1/tasks integrated with comprehensive backpressure checking

### Phase 2b: Backpressure Unit Testing ✅ COMPLETE

**Goal**: Verify circuit breaker and backpressure logic through deterministic unit tests with mock components

> **Rationale**: Integration testing of circuit breaker open/closed states in real scenarios is difficult and flaky. Unit tests with stand-in structs and channels provide reliable, fast verification of logic correctness without triggering actual system stress.

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Expand `WebDatabaseCircuitBreaker` unit tests | Low | High | ✅ Done (13 new tests) |
| Add `ApiError::Backpressure` helper method tests | Low | High | ✅ Done |
| Add `ApiError::Backpressure` response formatting tests | Low | Medium | ✅ Done |
| Add Retry-After calculation tests at saturation thresholds | Low | Medium | ✅ Done |
| Add `record_backpressure_rejection()` metric emission tests | Low | Medium | ✅ Done |

**Test Summary (28 total tests)**:
- **12 tests** in `tasker-shared/src/types/web.rs::backpressure_tests`
- **16 tests** in `tasker-orchestration/src/web/circuit_breaker.rs::tests`

**Testing Approach**:

1. **Mock AppState Pattern**:
   - Create test helper structs with controllable circuit breaker state
   - Inject channel saturation percentages (0%, 80%, 95%, 100%)
   - Configure failure counts and timestamps for deterministic behavior

2. **Circuit Breaker State Verification**:
   ```rust
   // Existing tests verify: starts closed, opens after threshold, closes on success
   // Add tests for:
   // - Half-open state transition after recovery timeout
   // - Concurrent failure recording (atomic operations)
   // - State persistence across success/failure sequences
   ```

3. **Backpressure Status Tests**:
   ```rust
   // Test check_backpressure_status() returns correct variants:
   // - None when healthy (circuit closed, saturation < 80%)
   // - ApiError::Backpressure with circuit_breaker reason when open
   // - ApiError::Backpressure with channel_saturated reason at 80%+
   // - Correct Retry-After values: 5s (80-90%), 15s (90-95%), 30s (95%+)
   ```

4. **Response Formatting Tests**:
   ```rust
   // Verify ApiError::Backpressure produces:
   // - HTTP 503 Service Unavailable status
   // - Retry-After header with correct value
   // - JSON body with BACKPRESSURE code and reason
   ```

**Benefits**:
- **Deterministic**: No timing dependencies or flaky conditions
- **Fast**: Milliseconds vs seconds for load-induced failures
- **Isolated**: Proves logic correctness without infrastructure complexity
- **Maintainable**: Clear test cases document expected behavior

### Phase 3: PGMQ Queue Management

**Goal**: Prevent unbounded queue growth via soft limits and monitoring

> **Approach**: Use soft limits with tiered monitoring (Normal → Warning → Critical → Overflow) rather than hard rejection. This keeps behavior portable for future RabbitMQ integration.

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Add queue depth monitoring | Medium | High | Planned |
| Implement tiered depth thresholds configuration | Medium | Medium | Planned |
| Add queue depth metrics to health endpoint | Low | Medium | Planned |
| Add API 503 response at critical queue depth | Medium | Medium | Planned |

### Phase 4: Worker Load Shedding

**Goal**: Enable workers to refuse work when overloaded

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Add capacity check before step claiming | Medium | High | Planned |
| Implement claim refusal metrics | Low | High | Planned |
| Add configurable claim threshold | Low | Medium | Planned |
| Document worker load shedding patterns | Low | Medium | Planned |

### Phase 5: Circuit Breaker Expansion

**Goal**: Consistent circuit breaker coverage

> **Note**: Messaging circuit breaker is intentionally deferred. Currently PGMQ shares the same PostgreSQL instance as tasker data. When PGMQ is deployed to a separate database (or replaced with RabbitMQ), a dedicated messaging circuit breaker will be needed. The `MessagingError::CircuitBreakerOpen` error type already exists for this purpose.

| Task | Effort | Priority | Status |
|------|--------|----------|--------|
| Add circuit breaker to FFI dispatch | Medium | Medium | Planned |
| Activate task_readiness circuit breaker | Low | Medium | Planned |
| Add circuit breaker to PGMQ send operations | Medium | Low | Deferred (future) |
| Document circuit breaker coverage matrix | Low | Medium | Planned |

### Phase 6: Load Testing

**Goal**: Validate backpressure behavior under load

| Task | Effort | Priority |
|------|--------|----------|
| Create load testing framework | High | Medium |
| Document load testing strategy | Medium | Medium |
| Benchmark backpressure thresholds | High | Low |
| Create chaos engineering scenarios | High | Low |

---

## Documentation Updates Required

### Existing Documents to Update

1. **`docs/architecture-decisions/TAS-51-bounded-mpsc-channels.md`**
   - Add section on backpressure cascade prevention
   - Cross-reference TAS-75 backpressure strategy

2. **`docs/worker-event-systems.md`**
   - Add "Backpressure Handling" section
   - Document claim refusal patterns
   - Document domain event drop semantics

3. **`docs/operations/mpsc-channel-tuning.md`**
   - Add backpressure symptom diagnosis
   - Add tuning recommendations for high load

4. **`docs/development/mpsc-channel-guidelines.md`**
   - Add backpressure strategy selection guide
   - Add examples for each strategy

### New Documents to Create

1. **`docs/backpressure-architecture.md`** (New)
   - Unified backpressure strategy overview
   - Component responsibility matrix
   - Decision tree for backpressure design

2. **`docs/operations/backpressure-monitoring.md`** (New)
   - Key metrics to monitor
   - Alerting thresholds
   - Runbook for backpressure events

3. **`docs/development/load-testing-guide.md`** (New)
   - Load testing framework documentation
   - Scenario definitions
   - Performance baseline expectations

---

## Success Criteria

### Functional Requirements

- [x] API returns 503 when circuit breaker is open (existing)
- [x] API circuit breaker rejection metrics emitted (TAS-75)
- [ ] API returns 503 with Retry-After when queue depth critical
- [x] API returns 503 with Retry-After when command channel saturated (TAS-75)
- [ ] Workers refuse step claims when at capacity
- [ ] PGMQ queue depth monitoring with tiered alerting
- [x] All backpressure events emit metrics (TAS-75)

### Non-Functional Requirements

- [ ] No step execution lost due to backpressure
- [ ] No duplicate step execution due to backpressure
- [ ] Graceful degradation under 2x expected load
- [ ] Recovery within 30s of load reduction
- [ ] Zero data corruption under any load condition

### Documentation Requirements

- [ ] Unified backpressure architecture document exists
- [ ] All backpressure points documented with behavior
- [ ] Operational runbook for backpressure scenarios
- [ ] Load testing strategy documented

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Backpressure causes step loss | Critical | Idempotency audit before implementation |
| Rate limiting breaks existing integrations | High | Feature flag, gradual rollout |
| Queue depth limits cause task failures | High | Conservative initial limits, monitoring |
| Load testing reveals fundamental issues | High | Phased approach, production shadowing |

---

## Related Documentation

- [TAS-51: Bounded MPSC Channels](../../architecture-decisions/TAS-51-bounded-mpsc-channels.md)
- [TAS-67: Worker Dual Event System](./TAS-67/summary.md)
- [Worker Event Systems Architecture](../worker-event-systems.md)
- [MPSC Channel Tuning](../operations/mpsc-channel-tuning.md)
- [Circuit Breaker Patterns](../architecture-decisions/) (to be created)

---

## Appendix: Current Backpressure Configuration

### Orchestration Channels

```toml
# config/tasker/base/orchestration.toml
[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000

[orchestration.mpsc_channels.pgmq_events]
pgmq_event_buffer_size = 50000

[orchestration.mpsc_channels.event_channel]
event_channel_buffer_size = 10000
```

### Worker Channels

```toml
# config/tasker/base/worker.toml
[worker.mpsc_channels.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000
max_concurrent_handlers = 10
handler_timeout_ms = 30000

[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000
completion_timeout_ms = 30000
starvation_warning_threshold_ms = 10000
```

### Circuit Breakers

```toml
# config/tasker/base/common.toml
[common.circuit_breakers.web]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2

[common.circuit_breakers.task_readiness]
failure_threshold = 3
timeout_seconds = 15
success_threshold = 1
```

### PGMQ

```toml
# config/tasker/base/pgmq.toml
[pgmq]
visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100
poll_interval_ms = 100
```
