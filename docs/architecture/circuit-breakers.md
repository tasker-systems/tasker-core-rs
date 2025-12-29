# Circuit Breakers

**Last Updated**: 2025-12-10
**Audience**: Architects, Operators, Developers
**Status**: Active (TAS-75)
**Related Docs**: [Backpressure Architecture](backpressure-architecture.md) | [Observability](observability/README.md) | [Operations: Backpressure Monitoring](operations/backpressure-monitoring.md)

<- Back to [Documentation Hub](README.md)

---

Circuit breakers provide fault isolation and cascade prevention across tasker-core. This document covers the circuit breaker architecture, implementations, configuration, and operational monitoring.

## Core Concept

Circuit breakers prevent cascading failures by failing fast when a component is unhealthy. Instead of waiting for slow or failing operations to timeout, circuit breakers detect failure patterns and immediately reject calls, giving the downstream system time to recover.

### State Machine

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     CIRCUIT BREAKER STATE MACHINE                            │
└─────────────────────────────────────────────────────────────────────────────┘

                    Success
                  ┌─────────┐
                  │         │
                  ▼         │
              ┌───────┐     │
      ───────>│CLOSED │─────┘
              └───┬───┘
                  │
                  │ failure_threshold
                  │ consecutive failures
                  │
                  ▼
              ┌───────┐
              │ OPEN  │◄─────────────────────┐
              └───┬───┘                      │
                  │                          │
                  │ timeout_seconds          │ Any failure
                  │ elapsed                  │ in half-open
                  │                          │
                  ▼                          │
            ┌──────────┐                     │
            │HALF-OPEN │─────────────────────┘
            └────┬─────┘
                 │
                 │ success_threshold
                 │ consecutive successes
                 │
                 ▼
            ┌───────┐
            │CLOSED │
            └───────┘
```

**States**:
- **Closed**: Normal operation. All calls allowed. Tracks consecutive failures.
- **Open**: Failing fast. All calls rejected immediately. Waiting for timeout.
- **Half-Open**: Testing recovery. Limited calls allowed. Single failure reopens.

## Circuit Breaker Implementations

Tasker-core has **four distinct circuit breaker implementations**, each protecting specific components:

| Circuit Breaker | Location | Purpose | Trigger Type |
|-----------------|----------|---------|--------------|
| Web Database | `tasker-orchestration` | API database operations | Error-based |
| Task Readiness | `tasker-orchestration` | Fallback poller database checks | Error-based |
| FFI Completion | `tasker-worker` | Ruby/Python handler completion channel | **Latency-based** |
| PGMQ Operations | `tasker-shared` | Message queue operations | Error-based |

### 1. Web Database Circuit Breaker

**Purpose**: Protects API endpoints from cascading database failures.

**Scope**: Independent from orchestration system's internal operations.

**Behavior**:
- Opens when database queries fail repeatedly
- Returns 503 with `Retry-After` header when open
- Fast-fail rejection with atomic state management

**Configuration** (`config/tasker/base/common.toml`):
```toml
[common.circuit_breakers.web]
failure_threshold = 5      # Consecutive failures before opening
timeout_seconds = 30       # Time in open state before testing recovery
success_threshold = 2      # Successes in half-open to fully close
```

**Health Check Integration**:
- Included in `/health/ready` endpoint
- State reported in `/health/detailed` response
- Metric: `api_circuit_breaker_state` (0=closed, 1=half-open, 2=open)

### 2. Task Readiness Circuit Breaker

**Purpose**: Protects fallback poller from database overload during polling cycles.

**Scope**: Independent from web circuit breaker, specific to task readiness queries.

**Behavior**:
- Opens when task readiness queries fail repeatedly
- Skips polling cycles when open (doesn't fail-fast, just skips)
- Allows orchestration to continue processing existing work

**Configuration** (`config/tasker/base/common.toml`):
```toml
[common.circuit_breakers.component_configs.task_readiness]
failure_threshold = 10     # Higher threshold for polling
timeout_seconds = 60       # Longer recovery window
success_threshold = 3      # More successes needed for confidence
```

**Why Separate from Web?**:
- Different failure patterns (polling vs request-driven)
- Different recovery semantics (skip vs reject)
- Isolation prevents web failures from stopping polling (and vice versa)

### 3. FFI Completion Circuit Breaker (TAS-75)

**Purpose**: Protects Ruby/Python worker completion channels from backpressure.

**Scope**: Worker-specific, protects FFI boundary.

**Behavior**:
- **Latency-based**: Treats slow sends (>100ms) as failures
- Opens when completion channel is consistently slow
- Prevents FFI threads from blocking on saturated channels
- Drops completions when open (with metrics), allowing handler threads to continue

**Configuration** (`config/tasker/base/worker.toml`):
```toml
[worker.circuit_breakers.ffi_completion_send]
failure_threshold = 5            # Slow sends before opening
recovery_timeout_seconds = 5     # Short recovery window
success_threshold = 2            # Successes to close
slow_send_threshold_ms = 100     # Latency threshold (100ms)
```

**Why Latency-Based?**:
- Slow channel sends indicate backpressure buildup
- Blocking FFI threads can cascade to Ruby/Python handler starvation
- Error-only detection misses slow-but-completing operations
- Latency detection catches degradation before total failure

**Metrics**:
- `ffi_completion_slow_sends_total` - Sends exceeding latency threshold
- `ffi_completion_circuit_open_rejections_total` - Rejections due to open circuit

### 4. PGMQ Operations Circuit Breaker

**Purpose**: Protects message queue operations from database failures.

**Scope**: Shared across orchestration and worker message operations.

**Behavior**:
- Opens when PGMQ operations fail repeatedly
- Both send and receive operations protected
- Coordinates with visibility timeout for message safety

**Configuration** (`config/tasker/base/common.toml`):
```toml
[common.circuit_breakers.component_configs.pgmq]
failure_threshold = 5      # Failures before opening
timeout_seconds = 30       # Recovery window
success_threshold = 2      # Successes to close
```

## Configuration Reference

### Global Settings

```toml
[common.circuit_breakers]
enabled = true                              # Master enable/disable

[common.circuit_breakers.global_settings]
max_circuit_breakers = 50                   # Maximum tracked breakers
metrics_collection_interval_seconds = 30    # Metrics aggregation interval
min_state_transition_interval_seconds = 5.0 # Debounce for rapid transitions
```

### Default Configuration

Applied to any circuit breaker without explicit configuration:

```toml
[common.circuit_breakers.default_config]
failure_threshold = 5      # 1-100 range
timeout_seconds = 30       # 1-300 range
success_threshold = 2      # 1-50 range
```

### Component-Specific Overrides

```toml
# Task readiness (polling-specific)
[common.circuit_breakers.component_configs.task_readiness]
failure_threshold = 10
timeout_seconds = 60
success_threshold = 3

# PGMQ operations
[common.circuit_breakers.component_configs.pgmq]
failure_threshold = 5
timeout_seconds = 30
success_threshold = 2
```

### Worker-Specific Configuration

```toml
# FFI completion (latency-based)
[worker.circuit_breakers.ffi_completion_send]
failure_threshold = 5
recovery_timeout_seconds = 5
success_threshold = 2
slow_send_threshold_ms = 100
```

### Environment Overrides

Different environments may need different thresholds:

**Test** (`config/tasker/environments/test/common.toml`):
```toml
[common.circuit_breakers.default_config]
failure_threshold = 2      # Faster failure detection
timeout_seconds = 5        # Quick recovery for tests
success_threshold = 1
```

**Production** (`config/tasker/environments/production/common.toml`):
```toml
[common.circuit_breakers.default_config]
failure_threshold = 10     # More tolerance for transient failures
timeout_seconds = 60       # Longer recovery window
success_threshold = 5      # More confidence before closing
```

## Health Endpoint Integration

Circuit breaker states are exposed through health endpoints for monitoring and Kubernetes probes.

### Orchestration Health (`/health/detailed`)

```json
{
  "status": "healthy",
  "checks": {
    "circuit_breakers": {
      "status": "healthy",
      "message": "Circuit breaker state: Closed",
      "duration_ms": 1,
      "last_checked": "2025-12-10T10:00:00Z"
    }
  }
}
```

### Worker Health (`/health/detailed`)

```json
{
  "status": "healthy",
  "checks": {
    "circuit_breakers": {
      "status": "healthy",
      "message": "2 circuit breakers: 2 closed, 0 open, 0 half-open. Details: ffi_completion: closed (100 calls, 2 failures); task_readiness: closed (50 calls, 0 failures)",
      "duration_ms": 0,
      "last_checked": "2025-12-10T10:00:00Z"
    }
  }
}
```

### Health Status Mapping

| Circuit Breaker State | Health Status | Impact |
|-----------------------|---------------|--------|
| All Closed | `healthy` | Normal operation |
| Any Half-Open | `degraded` | Testing recovery |
| Any Open | `unhealthy` | Failing fast |

## Monitoring and Alerting

### Key Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `api_circuit_breaker_state` | Gauge | Web breaker state (0/1/2) |
| `tasker_circuit_breaker_state` | Gauge | Per-component state |
| `api_requests_rejected_total` | Counter | Rejections due to open breaker |
| `ffi_completion_slow_sends_total` | Counter | Slow send detections |
| `ffi_completion_circuit_open_rejections_total` | Counter | FFI breaker rejections |

### Prometheus Alerts

```yaml
groups:
  - name: circuit_breakers
    rules:
      - alert: TaskerCircuitBreakerOpen
        expr: api_circuit_breaker_state == 2
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Circuit breaker is OPEN"
          description: "Circuit breaker {{ $labels.component }} has been open for >1 minute"

      - alert: TaskerCircuitBreakerHalfOpen
        expr: api_circuit_breaker_state == 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Circuit breaker stuck in half-open"
          description: "Circuit breaker {{ $labels.component }} in half-open state >5 minutes"

      - alert: TaskerFFISlowSendsHigh
        expr: rate(ffi_completion_slow_sends_total[5m]) > 10
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "FFI completion channel experiencing backpressure"
          description: "Slow sends averaging >10/second, circuit breaker may open"
```

### Grafana Dashboard Panels

**Circuit Breaker State Timeline**:
```
Panel: Time series
Query: api_circuit_breaker_state
Value mappings: 0=Closed (green), 1=Half-Open (yellow), 2=Open (red)
```

**FFI Latency Percentiles**:
```
Panel: Time series
Queries:
  - histogram_quantile(0.50, ffi_completion_send_duration_seconds_bucket)
  - histogram_quantile(0.95, ffi_completion_send_duration_seconds_bucket)
  - histogram_quantile(0.99, ffi_completion_send_duration_seconds_bucket)
Thresholds: 100ms warning, 500ms critical
```

## Operational Procedures

### When Circuit Breaker Opens

**Immediate Actions**:
1. Check database connectivity: `pg_isready -h <host> -p 5432`
2. Check connection pool status: `/health/detailed` endpoint
3. Review recent error logs for root cause
4. Monitor queue depth for message backlog

**Recovery**:
- Circuit automatically tests recovery after `timeout_seconds`
- No manual intervention needed for transient failures
- For persistent failures, fix underlying issue first

**Escalation**:
- If breaker stays open >5 minutes, escalate to database team
- If breaker oscillates (open/half-open/open), increase `failure_threshold`

### Tuning Guidelines

**Symptom: Breaker opens too frequently**
- Increase `failure_threshold`
- Investigate root cause of failures
- Consider if failures are transient vs systemic

**Symptom: Breaker stays open too long**
- Decrease `timeout_seconds`
- Verify downstream system has recovered
- Check if `success_threshold` is too high

**Symptom: FFI breaker opens unnecessarily**
- Increase `slow_send_threshold_ms`
- Verify channel buffer sizes are adequate
- Check Ruby/Python handler throughput

## Architecture Integration

### Relationship to Backpressure

Circuit breakers are one layer of the broader backpressure strategy:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        RESILIENCE LAYER STACK                                │
└─────────────────────────────────────────────────────────────────────────────┘

Layer 1: Circuit Breakers     → Fast-fail on component failure
Layer 2: Bounded Channels     → Backpressure on internal queues
Layer 3: Visibility Timeouts  → Message-level retry safety
Layer 4: Semaphore Limits     → Handler execution rate limiting
Layer 5: Connection Pools     → Database resource management
```

See [Backpressure Architecture](backpressure-architecture.md) for the complete strategy.

### Independence Principle

Each circuit breaker operates **independently**:

- Web breaker can be open while task readiness breaker is closed
- FFI breaker state doesn't affect PGMQ breaker
- Prevents single failure mode from cascading across components
- Allows targeted recovery per component

### Integration Points

| Component | Circuit Breaker | Integration Point |
|-----------|-----------------|-------------------|
| `tasker-orchestration/src/web` | Web Database | API request handlers |
| `tasker-orchestration/src/orchestration/task_readiness` | Task Readiness | Fallback poller loop |
| `tasker-worker/src/worker/handlers` | FFI Completion | Completion channel sends |
| `tasker-shared/src/messaging` | PGMQ | Unified message client |

## Troubleshooting

### Common Issues

**Issue**: Web circuit breaker flapping (open → half-open → open rapidly)

**Diagnosis**:
1. Check database query latency (slow queries can cause timeout failures)
2. Review connection pool saturation
3. Check if PostgreSQL is under memory pressure

**Resolution**:
- Increase `failure_threshold` if failures are transient
- Increase `timeout_seconds` to give more recovery time
- Fix underlying database performance issues

---

**Issue**: FFI completion circuit breaker opens during normal load

**Diagnosis**:
1. Check Ruby/Python handler execution time
2. Review completion channel buffer utilization
3. Verify worker concurrency settings

**Resolution**:
- Increase `slow_send_threshold_ms` if handlers are legitimately slow
- Increase channel buffer size in worker config
- Reduce handler concurrency if system is overloaded

---

**Issue**: Task readiness breaker open but web API working fine

**Diagnosis**:
- Task readiness queries may be slower/different than API queries
- Polling may hit database at different times (e.g., during maintenance)

**Resolution**:
- Independent breakers are working as designed
- Check specific task readiness query performance
- Consider database index optimization for readiness queries

## Source Code Reference

| Component | File |
|-----------|------|
| Generic Circuit Breaker | `tasker-shared/src/resilience/circuit_breaker.rs` |
| Circuit Breaker Config | `tasker-shared/src/config/circuit_breaker.rs` |
| Web Circuit Breaker | `tasker-orchestration/src/web/circuit_breaker.rs` |
| Task Readiness Breaker | `tasker-orchestration/src/orchestration/task_readiness/circuit_breaker.rs` |
| FFI Completion Breaker | `tasker-worker/src/worker/handlers/ffi_completion_circuit_breaker.rs` |
| Worker Health Integration | `tasker-worker/src/web/handlers/health.rs` |
| Circuit Breaker Types | `tasker-shared/src/types/api/worker.rs` |

---

## Related Documentation

- **[Backpressure Architecture](backpressure-architecture.md)** - Complete resilience strategy
- **[Operations: Backpressure Monitoring](operations/backpressure-monitoring.md)** - Operational runbooks
- **[Operations: MPSC Channel Tuning](operations/mpsc-channel-tuning.md)** - Channel capacity management
- **[Observability](observability/README.md)** - Metrics and logging standards
- **[Configuration Management](configuration-management.md)** - TOML configuration reference

<- Back to [Documentation Hub](README.md)
