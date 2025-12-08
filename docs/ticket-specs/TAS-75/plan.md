# TAS-75: Backpressure Consistency Investigation

**Date**: 2025-12-08
**Status**: Planning
**Branch**: `jcoletaylor/tas-75-backpressure-consistency`
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
| API Endpoints | No rate limiting | **Missing** |
| PGMQ Queues | No depth enforcement | **Missing** |
| Circuit Breakers | Partial coverage | **Inconsistent** |
| Step Claiming | No refusal mechanism | **Missing** |

This inconsistency creates unpredictable behavior under load and potential cascading failures.

---

## Research Findings

### 1. Orchestration System Backpressure

**Strengths**:
- MPSC channels bounded with configurable sizes (TAS-51)
- ChannelMonitor provides real-time observability
- Circuit breaker protects web API from database failures
- Backpressure strategies documented per channel

**Gaps Identified**:
- No API rate limiting (explicitly removed in TAS-61)
- No 503 backpressure signaling to clients
- Event system channels (`event_sender`, `stats_sender`) not monitored
- No queue depth checks before enqueueing steps
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
- No maximum queue depth enforcement
- `QueueCapacityExceeded` error defined but never raised
- pg_notify has 8KB payload limit (notifications silently fail)
- No queue depth monitoring/alerting
- Unbounded queue growth under sustained load

**Key Configuration** (`config/tasker/base/pgmq.toml`):
```toml
[pgmq]
visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100
# Note: No max_queue_depth configuration
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

[1] API Gateway Layer
    └── Rate limiting (requests/sec per client)
    └── 503 Service Unavailable when circuit open
    └── 429 Too Many Requests when rate exceeded

[2] Command Processor Layer
    └── Bounded MPSC channel (existing - TAS-51)
    └── Backpressure: Block with timeout, then reject

[3] PGMQ Enqueue Layer
    └── Queue depth check before enqueue
    └── Circuit breaker on PGMQ failures
    └── Backpressure: Reject with retriable error

[4] Database Layer
    └── Connection pool limits (existing)
    └── Circuit breaker on failures (partial)
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

### Phase 1: Documentation Foundation

**Goal**: Establish comprehensive backpressure documentation

| Task | Effort | Priority |
|------|--------|----------|
| Create `docs/backpressure-architecture.md` | Medium | High |
| Update `docs/worker-event-systems.md` with backpressure section | Low | High |
| Create `docs/operations/backpressure-monitoring.md` | Medium | High |
| Update `docs/development/mpsc-channel-guidelines.md` with backpressure patterns | Low | Medium |

### Phase 2: API Layer Backpressure

**Goal**: Protect orchestration from client overload

| Task | Effort | Priority |
|------|--------|----------|
| Implement 503 response when circuit breaker open | Low | High |
| Add configurable rate limiting middleware | Medium | Medium |
| Add `/v1/health` endpoint with capacity metrics | Low | Medium |
| Document API backpressure behavior | Low | High |

### Phase 3: PGMQ Queue Management

**Goal**: Prevent unbounded queue growth

| Task | Effort | Priority |
|------|--------|----------|
| Add queue depth monitoring | Medium | High |
| Implement max queue depth configuration | Medium | Medium |
| Add circuit breaker to PGMQ send operations | Medium | Medium |
| Add queue depth metrics to health endpoint | Low | Medium |

### Phase 4: Worker Load Shedding

**Goal**: Enable workers to refuse work when overloaded

| Task | Effort | Priority |
|------|--------|----------|
| Add capacity check before step claiming | Medium | High |
| Implement claim refusal metrics | Low | High |
| Add configurable claim threshold | Low | Medium |
| Document worker load shedding patterns | Low | Medium |

### Phase 5: Circuit Breaker Expansion

**Goal**: Consistent circuit breaker coverage

| Task | Effort | Priority |
|------|--------|----------|
| Add circuit breaker to FFI dispatch | Medium | Medium |
| Activate task_readiness circuit breaker | Low | Medium |
| Add circuit breaker to PGMQ send operations | Medium | Medium |
| Document circuit breaker coverage matrix | Low | Medium |

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

- [ ] API returns 503 when circuit breaker is open
- [ ] API returns 429 when rate limit exceeded (if implemented)
- [ ] Workers refuse step claims when at capacity
- [ ] PGMQ enqueue fails gracefully when queue at depth limit
- [ ] All backpressure events emit metrics

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
