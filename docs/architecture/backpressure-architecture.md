# Backpressure Architecture

**Last Updated**: 2025-12-08
**Audience**: Architects, Developers, Operations
**Status**: Active (TAS-75)
**Related Docs**: [Worker Event Systems](worker-event-systems.md) | [MPSC Channel Guidelines](development/mpsc-channel-guidelines.md) | [TAS-75](https://linear.app/tasker-systems/issue/TAS-75)

<- Back to [Documentation Hub](README.md)

---

This document provides the unified backpressure strategy for tasker-core, covering all system components from API ingestion through worker execution.

## Core Principle

> **Step idempotency is the primary constraint.** Any backpressure mechanism must ensure that step claiming, business logic execution, and result persistence remain stable and consistent. The system must gracefully degrade under load without compromising workflow correctness.

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         BACKPRESSURE FLOW OVERVIEW                           │
└─────────────────────────────────────────────────────────────────────────────┘

                            ┌─────────────────┐
                            │  External Client │
                            └────────┬────────┘
                                     │
                    ┌────────────────▼────────────────┐
                    │  [1] API LAYER BACKPRESSURE      │
                    │  • Circuit breaker (503)         │
                    │  • System overload (503)         │
                    │  • Request validation            │
                    └────────────────┬────────────────┘
                                     │
                    ┌────────────────▼────────────────┐
                    │  [2] ORCHESTRATION BACKPRESSURE  │
                    │  • Command channel (bounded)     │
                    │  • Connection pool limits        │
                    │  • PGMQ depth checks             │
                    └────────────────┬────────────────┘
                                     │
                         ┌───────────┴───────────┐
                         │     PGMQ Queues       │
                         │  • Namespace queues   │
                         │  • Result queues      │
                         └───────────┬───────────┘
                                     │
                    ┌────────────────▼────────────────┐
                    │  [3] WORKER BACKPRESSURE         │
                    │  • Claim capacity check          │
                    │  • Semaphore-bounded handlers    │
                    │  • Completion channel bounds     │
                    └────────────────┬────────────────┘
                                     │
                    ┌────────────────▼────────────────┐
                    │  [4] RESULT FLOW BACKPRESSURE    │
                    │  • Completion channel bounds     │
                    │  • Domain event drop semantics   │
                    └─────────────────────────────────┘
```

## Backpressure Points by Component

### 1. API Layer

The API layer provides backpressure through 503 responses with intelligent `Retry-After` headers.

> **Rate Limiting (429)**: Rate limiting is intentionally out of scope for tasker-core. This responsibility belongs to upstream infrastructure (API Gateways, NLB/ALB, service mesh). Tasker focuses on system health-based backpressure via 503 responses.

| Mechanism | Status | Behavior |
|-----------|--------|----------|
| Circuit Breaker | **Implemented** | Return 503 when database breaker open |
| System Overload | **Planned** | Return 503 when queue/channel saturation detected |
| Request Validation | **Implemented** | Return 400 for invalid requests |

**Response Codes**:
- `200 OK` - Request accepted
- `400 Bad Request` - Invalid request format
- `503 Service Unavailable` - System overloaded (includes `Retry-After` header)

**503 Response Triggers**:
1. **Circuit Breaker Open**: Database operations failing repeatedly
2. **Queue Depth High** (Planned): PGMQ namespace queues approaching capacity
3. **Channel Saturation** (Planned): Command channel buffer > 80% full

**Retry-After Header Strategy**:
```
503 Service Unavailable
Retry-After: {calculated_delay_seconds}

Calculation:
- Circuit breaker open: Use breaker timeout (default 30s)
- Queue depth high: Estimate based on processing rate
- Channel saturation: Short delay (5-10s) for buffer drain
```

**Configuration**:
```toml
# config/tasker/base/common.toml
[circuit_breakers.web]
failure_threshold = 5      # Failures before opening
timeout_seconds = 30       # Time in open state
success_threshold = 2      # Successes to close
```

### 2. Orchestration Layer

The orchestration layer protects internal processing from command flooding.

| Mechanism | Status | Behavior |
|-----------|--------|----------|
| Command Channel | **Implemented** | Bounded MPSC with monitoring |
| Connection Pool | **Implemented** | Database connection limits |
| PGMQ Depth Check | **Planned** | Reject enqueue when queue too deep |

**Command Channel Backpressure**:
```
Command Sender → [Bounded Channel] → Command Processor
                      │
                      └── If full: Block with timeout → Reject
```

**Configuration**:
```toml
# config/tasker/base/orchestration.toml
[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000

[orchestration.mpsc_channels.pgmq_events]
pgmq_event_buffer_size = 50000
```

### 3. Messaging Layer

The messaging layer provides the backbone between orchestration and workers. Currently implemented with PGMQ, designed to be portable for future RabbitMQ/other backends.

| Mechanism | Status | Behavior |
|-----------|--------|----------|
| Visibility Timeout | **Implemented** | Messages return to queue after timeout |
| Batch Size Limits | **Implemented** | Bounded message reads |
| Queue Depth Check | **Planned** | Reject enqueue when depth exceeded |
| Messaging Circuit Breaker | **Not Implemented** | See note below |

> **Messaging Circuit Breaker Gap**: The `UnifiedMessageClient` abstraction currently has no circuit breaker integration. Error types exist (`MessagingError::CircuitBreakerOpen`) but the client doesn't wrap operations with circuit breaker protection. This is intentional—the web database circuit breaker handles database failures, and adding a separate messaging circuit breaker would require careful coordination to avoid conflicting recovery behaviors.

**Queue Depth Monitoring** (Planned):

The system will work with PGMQ's native capabilities rather than enforcing arbitrary limits. Queue depth monitoring provides visibility without hard rejection:

```
┌──────────────────────────────────────────────────────────────────────┐
│ QUEUE DEPTH STRATEGY                                                  │
├──────────────────────────────────────────────────────────────────────┤
│ Level    │ Depth Ratio │ Action                                       │
├──────────────────────────────────────────────────────────────────────┤
│ Normal   │ < 70%       │ Normal operation                             │
│ Warning  │ 70-85%      │ Log warning, emit metric                     │
│ Critical │ 85-95%      │ API returns 503 for new tasks                │
│ Overflow │ > 95%       │ API rejects all writes, alert operators      │
└──────────────────────────────────────────────────────────────────────┘

Note: Depth ratio = current_depth / configured_soft_limit
Soft limit is advisory, not a hard PGMQ constraint.
```

**Portability Considerations**:
- Queue depth semantics vary by backend (PGMQ vs RabbitMQ vs SQS)
- Configuration is backend-agnostic where possible
- Backend-specific tuning goes in backend-specific config sections

**Configuration**:
```toml
# config/tasker/base/common.toml
[pgmq]
visibility_timeout_seconds = 30
default_batch_size = 10
max_batch_size = 100

# Planned backpressure settings
[pgmq.backpressure]
soft_depth_limit = 100000      # Advisory limit, not enforced by PGMQ
warning_threshold_ratio = 0.70
critical_threshold_ratio = 0.85
```

### 4. Worker Layer

The worker layer protects handler execution from saturation.

| Mechanism | Status | Behavior |
|-----------|--------|----------|
| Semaphore-Bounded Dispatch | **Implemented** | Max concurrent handlers |
| Claim Capacity Check | **Planned** | Refuse claims when at capacity |
| Handler Timeout | **Implemented** | Kill stuck handlers |
| Completion Channel | **Implemented** | Bounded result buffer |

**Handler Dispatch Flow**:
```
Step Message
     │
     ▼
┌─────────────────┐
│ Capacity Check  │──── At capacity? ──── Leave in queue
│ (Planned)       │                       (visibility timeout)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Acquire Permit  │
│ (Semaphore)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Execute Handler │
│ (with timeout)  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Release Permit  │──── BEFORE sending to completion channel
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Send Completion │
└─────────────────┘
```

**Configuration**:
```toml
# config/tasker/base/worker.toml
[worker.mpsc_channels.handler_dispatch]
dispatch_buffer_size = 1000
completion_buffer_size = 1000
max_concurrent_handlers = 10
handler_timeout_ms = 30000
```

### 5. Domain Events

Domain events use fire-and-forget semantics to avoid blocking the critical path.

| Mechanism | Status | Behavior |
|-----------|--------|----------|
| Try-Send | **Implemented** | Non-blocking send |
| Drop on Full | **Implemented** | Events dropped if channel full |
| Metrics | **Planned** | Track dropped events |

**Domain Event Flow**:
```
Handler Complete
     │
     ├── Result → Completion Channel (blocking, must succeed)
     │
     └── Domain Events → try_send() → If full: DROP with metric
                                       │
                                       └── Step execution NOT affected
```

---

## Segmentation of Responsibility

### Orchestration System

The orchestration system must protect itself from:

1. **Client overload**: Too many `/v1/tasks` requests
2. **Internal saturation**: Command channel overflow
3. **Database exhaustion**: Connection pool depletion
4. **Queue explosion**: Unbounded PGMQ growth

**Backpressure Response Hierarchy**:
1. Return 503 to client with Retry-After (fastest, cheapest)
2. Block at command channel (internal buffering)
3. Soft-reject at queue depth threshold (503 to new tasks)
4. Circuit breaker opens (stop accepting work)

### Worker System

The worker system must protect itself from:

1. **Handler saturation**: Too many concurrent handlers
2. **FFI backlog**: Ruby/Python handlers falling behind
3. **Completion overflow**: Results backing up
4. **Step starvation**: Claims outpacing processing

**Backpressure Response Hierarchy**:
1. Refuse step claim (leave in queue, visibility timeout)
2. Block at dispatch channel (internal buffering)
3. Block at completion channel (handler waits)
4. Circuit breaker opens (stop claiming work)

---

## Step Idempotency Guarantees

### Safe Backpressure Points

These backpressure points preserve step idempotency:

| Point | Why Safe |
|-------|----------|
| API 503 rejection | Task not yet created |
| Queue depth soft-limit | Step not yet enqueued |
| Step claim refusal | Message stays in queue, visibility timeout protects |
| Handler dispatch channel full | Step claimed but execution queued |
| Completion channel backpressure | Handler completed, result buffered |

### Unsafe Patterns (NEVER DO)

| Pattern | Risk | Mitigation |
|---------|------|------------|
| Drop step after claiming | Lost work | Always send result (success or failure) |
| Timeout during handler execution | Duplicate execution on retry | Handlers MUST be idempotent |
| Drop completion result | Orchestration unaware of completion | Completion channel blocks, never drops |
| Reset step state without visibility timeout | Race with other workers | Use PGMQ visibility timeout |

### Idempotency Contract

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    STEP EXECUTION IDEMPOTENCY CONTRACT                       │
└─────────────────────────────────────────────────────────────────────────────┘

1. CLAIM: Atomic via pgmq_read_specific_message()
   ├── Only one worker can claim a message
   ├── Visibility timeout protects against worker crash
   └── If claim fails: Message stays in queue → another worker claims

2. EXECUTE: Handler invocation (FFI boundary critical - see below)
   ├── Handlers SHOULD be idempotent (business logic recommendation)
   ├── Timeout generates FAILURE result (not drop)
   ├── Panic generates FAILURE result (not drop)
   └── Error generates FAILURE result (not drop)

3. PERSIST: Result submission
   ├── Completion channel is bounded but BLOCKING
   ├── Result MUST reach orchestration (never dropped)
   └── If send fails: Step remains "in_progress" → recovered by orchestration

4. FINALIZE: Orchestration processes result
   ├── State transition is atomic
   ├── Duplicate results handled by state guards
   └── Idempotent: Same result processed twice = same outcome
```

### FFI Boundary Idempotency Semantics

The FFI boundary (Rust → Ruby/Python handler) creates a critical demarcation for error classification:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    FFI BOUNDARY ERROR CLASSIFICATION                         │
└─────────────────────────────────────────────────────────────────────────────┘

                           FFI BOUNDARY
                                │
    BEFORE FFI CROSSING         │         AFTER FFI CROSSING
    (System Layer)              │         (Business Logic Layer)
                                │
    ┌─────────────────────┐     │     ┌─────────────────────┐
    │ System errors are   │     │     │ System failures     │
    │ RETRYABLE:          │     │     │ are PERMANENT:      │
    │                     │     │     │                     │
    │ • Channel timeout   │     │     │ • Worker crash      │
    │ • Queue unavailable │     │     │ • FFI panic         │
    │ • Claim race lost   │     │     │ • Process killed    │
    │ • Network partition │     │     │                     │
    │ • Message malformed │     │     │ We cannot know if   │
    │                     │     │     │ business logic      │
    │ Step has NOT been   │     │     │ executed or not.    │
    │ handed to handler.  │     │     │                     │
    └─────────────────────┘     │     └─────────────────────┘
                                │
                                │     ┌─────────────────────┐
                                │     │ Developer errors    │
                                │     │ are TRUSTED:        │
                                │     │                     │
                                │     │ • RetryableError →  │
                                │     │   System retries    │
                                │     │                     │
                                │     │ • PermanentError →  │
                                │     │   Step fails        │
                                │     │                     │
                                │     │ Developer knows     │
                                │     │ their domain logic. │
                                │     └─────────────────────┘
```

**Key Principles**:

1. **Before FFI**: Any system error is safe to retry because no business logic has executed.

2. **After FFI, system failure**: If the worker crashes or FFI call fails after dispatch, we MUST treat it as permanent failure. We cannot know if the handler:
   - Never started (safe to retry)
   - Started but didn't complete (unknown side effects)
   - Completed but didn't return (work is done)

3. **After FFI, developer error**: Trust the developer's classification:
   - `RetryableError`: Developer explicitly signals safe to retry (e.g., temporary API unavailable)
   - `PermanentError`: Developer explicitly signals not retriable (e.g., invalid data, business rule violation)

**Implementation Guidance**:

```rust
// BEFORE FFI - system error, retryable
match dispatch_to_handler(step).await {
    Err(DispatchError::ChannelFull) => StepResult::retryable("dispatch_channel_full"),
    Err(DispatchError::Timeout) => StepResult::retryable("dispatch_timeout"),
    Ok(ffi_handle) => {
        // AFTER FFI - different rules apply
        match ffi_handle.await {
            // System crash after FFI = permanent (unknown state)
            Err(FfiError::ProcessCrash) => StepResult::permanent("handler_crash"),
            Err(FfiError::Panic) => StepResult::permanent("handler_panic"),

            // Developer-returned errors = trust their classification
            Ok(HandlerResult::RetryableError(msg)) => StepResult::retryable(msg),
            Ok(HandlerResult::PermanentError(msg)) => StepResult::permanent(msg),
            Ok(HandlerResult::Success(data)) => StepResult::success(data),
        }
    }
}
```

> **Note**: We RECOMMEND handlers be idempotent but cannot REQUIRE it—business logic is developer-controlled. The system provides visibility timeout protection and duplicate result handling, but ultimate idempotency responsibility lies with handler implementations.

---

## Backpressure Decision Tree

Use this decision tree when designing new backpressure mechanisms:

```
                    ┌─────────────────────────┐
                    │ New Backpressure Point  │
                    └────────────┬────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │ Does this affect step   │
                    │ execution correctness?  │
                    └────────────┬────────────┘
                                 │
                   ┌─────────────┴─────────────┐
                   │                           │
                  Yes                          No
                   │                           │
                   ▼                           ▼
         ┌─────────────────┐         ┌─────────────────┐
         │ Can the work be │         │ Safe to drop    │
         │ retried safely? │         │ or timeout      │
         └────────┬────────┘         └─────────────────┘
                  │
        ┌─────────┴─────────┐
        │                   │
       Yes                  No
        │                   │
        ▼                   ▼
  ┌───────────┐      ┌───────────────┐
  │ Use block │      │ MUST NOT DROP │
  │ or reject │      │ Block until   │
  │ (retriable│      │ success       │
  │ error)    │      └───────────────┘
  └───────────┘
```

---

## Configuration Reference

> **TOML Structure**: Configuration files are organized as `config/tasker/base/{common,worker,orchestration}.toml` with environment overrides in `config/tasker/environments/{test,development,production}/`.

### Complete Backpressure Configuration

```toml
# ════════════════════════════════════════════════════════════════════════════
# config/tasker/base/common.toml - Shared settings
# ════════════════════════════════════════════════════════════════════════════

[circuit_breakers.web]
failure_threshold = 5      # Failures before opening
timeout_seconds = 30       # Time in open state before half-open
success_threshold = 2      # Successes in half-open to close

[pgmq]
visibility_timeout_seconds = 30    # Time before unclaimed message returns
default_batch_size = 10            # Default messages per read
max_batch_size = 100               # Maximum messages per read
poll_interval_ms = 100             # Polling interval for fallback

# Planned: Queue depth backpressure
# [pgmq.backpressure]
# soft_depth_limit = 100000
# warning_threshold_ratio = 0.70
# critical_threshold_ratio = 0.85

# ════════════════════════════════════════════════════════════════════════════
# config/tasker/base/orchestration.toml - Orchestration layer
# ════════════════════════════════════════════════════════════════════════════

[orchestration.mpsc_channels.command_processor]
command_buffer_size = 5000

[orchestration.mpsc_channels.pgmq_events]
pgmq_event_buffer_size = 50000

[orchestration.mpsc_channels.event_channel]
event_channel_buffer_size = 10000

# ════════════════════════════════════════════════════════════════════════════
# config/tasker/base/worker.toml - Worker layer
# ════════════════════════════════════════════════════════════════════════════

[worker.mpsc_channels.handler_dispatch]
dispatch_buffer_size = 1000        # Steps waiting for handler
completion_buffer_size = 1000      # Results waiting for orchestration
max_concurrent_handlers = 10       # Semaphore permits
handler_timeout_ms = 30000         # Max handler execution time

[worker.mpsc_channels.ffi_dispatch]
dispatch_buffer_size = 1000        # FFI events waiting for Ruby/Python
completion_timeout_ms = 30000      # Time to wait for FFI completion
starvation_warning_threshold_ms = 10000  # Warn if event waits this long

# Planned:
# claim_capacity_threshold = 0.8   # Refuse claims at 80% capacity
```

---

## Monitoring and Alerting

See [Backpressure Monitoring Runbook](operations/backpressure-monitoring.md) for:
- Key metrics to monitor
- Alerting thresholds
- Incident response procedures

### Key Metrics Summary

| Metric | Type | Alert Threshold |
|--------|------|-----------------|
| `api_requests_rejected_total` | Counter | > 10/min |
| `circuit_breaker_state` | Gauge | state = open |
| `mpsc_channel_saturation` | Gauge | > 80% |
| `pgmq_queue_depth` | Gauge | > 80% of max |
| `worker_claim_refusals_total` | Counter | > 10/min |
| `handler_semaphore_wait_time_ms` | Histogram | p99 > 1000ms |

---

## Related Documentation

- [Worker Event Systems](worker-event-systems.md) - Dual-channel architecture
- [MPSC Channel Guidelines](development/mpsc-channel-guidelines.md) - Channel creation guide
- [MPSC Channel Tuning](operations/mpsc-channel-tuning.md) - Operational tuning
- [TAS-51: Bounded MPSC Channels](../decisions/TAS-51-bounded-mpsc-channels.md) - ADR
- [TAS-75](https://linear.app/tasker-systems/issue/TAS-75) - Backpressure architecture ticket

---

<- Back to [Documentation Hub](README.md)
