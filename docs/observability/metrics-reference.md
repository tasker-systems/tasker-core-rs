# TAS-29 Phase 3.3: OpenTelemetry Metrics Reference

**Status**: ✅ Complete
**Export Interval**: 60 seconds
**OTLP Endpoint**: http://localhost:4317
**Grafana UI**: http://localhost:3000

This document provides a complete reference for all OpenTelemetry metrics instrumented in the Tasker orchestration system.

## Table of Contents

- [Overview](#overview)
- [Configuration](#configuration)
- [Orchestration Metrics](#orchestration-metrics)
- [Worker Metrics](#worker-metrics)
- [Resilience Metrics](#resilience-metrics)
- [Database Metrics](#database-metrics)
- [Messaging Metrics](#messaging-metrics)
- [Example Queries](#example-queries)
- [Dashboard Recommendations](#dashboard-recommendations)

---

## Overview

The Tasker system exports 47+ OpenTelemetry metrics across 5 domains:

| Domain | Metrics | Description |
|--------|---------|-------------|
| **Orchestration** | 11 | Task lifecycle, step coordination, finalization |
| **Worker** | 10 | Step execution, claiming, result submission |
| **Resilience** | 8+ | Circuit breakers (TAS-75), MPSC channels (TAS-51) |
| **Database** | 7 | SQL query performance, connection pools |
| **Messaging** | 11 | PGMQ queue operations, message processing |

All metrics include `correlation_id` labels for distributed tracing correlation with Tempo traces.

### Histogram Metric Naming

OpenTelemetry automatically appends `_milliseconds` to histogram metric names when the unit is specified as `ms`. This provides clarity in Prometheus queries.

**Pattern**: `metric_name` → `metric_name_milliseconds_{bucket,sum,count}`

**Example**:
- Code: `tasker.step.execution.duration` with unit "ms"
- Prometheus: `tasker_step_execution_duration_milliseconds_*`

### Query Patterns: Instant vs Rate-Based

**Instant/Recent Data Queries** - Use these when:
- Testing with burst/batch task execution
- Viewing data from recent runs (last few minutes)
- Data points are sparse or clustered together
- You want simple averages without time windows

**Rate-Based Queries** - Use these when:
- Continuous production monitoring
- Data flows steadily over time
- Calculating per-second rates
- Building alerting rules

**Why the difference matters**: The `rate()` function calculates per-second change rates over a time window. It requires data points spread across that window. If you run 26 tasks in quick succession, all data points cluster at one timestamp, and `rate()` returns no data because there's no rate change to calculate.

---

## Configuration

### Enable OpenTelemetry

**File**: `config/tasker/environments/development/telemetry.toml`

```toml
[telemetry]
enabled = true
service_name = "tasker-core-dev"
sample_rate = 1.0

[telemetry.opentelemetry]
enabled = true  # Must be true to export metrics
```

### Verify in Logs

```bash
# Should see:
# opentelemetry_enabled=true
# NOT: Metrics collection disabled (TELEMETRY_ENABLED=false)
```

---

## Orchestration Metrics

**Module**: `tasker-shared/src/metrics/orchestration.rs`
**Instrumentation**: `tasker-orchestration/src/orchestration/lifecycle/*.rs`

### Counters

#### `tasker.tasks.requests.total`
**Description**: Total number of task creation requests received
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `task_type`: Task name (e.g., "mathematical_sequence")
- `namespace`: Task namespace (e.g., "rust_e2e_linear")

**Instrumented In**: `task_initializer.rs:start_task_initialization()`

**Example Query**:
```promql
# Total task requests
tasker_tasks_requests_total

# By namespace
sum by (namespace) (tasker_tasks_requests_total)

# Specific correlation_id
tasker_tasks_requests_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected Output**: (To be verified)

---

#### `tasker.tasks.completions.total`
**Description**: Total number of tasks that completed successfully
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID

**Instrumented In**: `task_finalizer.rs:finalize_task()` (FinalizationAction::Completed)

**Example Query**:
```promql
# Total completions
tasker_tasks_completions_total

# Completion rate over 5 minutes
rate(tasker_tasks_completions_total[5m])
```

**Expected Output**: (To be verified)

---

#### `tasker.tasks.failures.total`
**Description**: Total number of tasks that failed
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID

**Instrumented In**: `task_finalizer.rs:finalize_task()` (FinalizationAction::Failed)

**Example Query**:
```promql
# Total failures
tasker_tasks_failures_total

# Error rate over 5 minutes
rate(tasker_tasks_failures_total[5m])
```

**Expected Output**: (To be verified)

---

#### `tasker.steps.enqueued.total`
**Description**: Total number of steps enqueued to worker queues
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `namespace`: Task namespace
- `step_name`: Name of the enqueued step

**Instrumented In**: `step_enqueuer.rs:enqueue_steps()`

**Example Query**:
```promql
# Total steps enqueued
tasker_steps_enqueued_total

# By step name
sum by (step_name) (tasker_steps_enqueued_total)

# For specific task
tasker_steps_enqueued_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected Output**: (To be verified)

---

#### `tasker.step_results.processed.total`
**Description**: Total number of step results processed from workers
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `result_type`: "success", "error", "timeout", "cancelled", "skipped"

**Instrumented In**: `result_processor.rs:process_step_result()`

**Example Query**:
```promql
# Total results processed
tasker_step_results_processed_total

# By result type
sum by (result_type) (tasker_step_results_processed_total)

# Success rate
rate(tasker_step_results_processed_total{result_type="success"}[5m])
```

**Expected Output**: (To be verified)

---

### Histograms

#### `tasker.task.initialization.duration`
**Description**: Task initialization duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_task_initialization_duration_milliseconds_bucket`
- `tasker_task_initialization_duration_milliseconds_sum`
- `tasker_task_initialization_duration_milliseconds_count`

**Labels**:
- `correlation_id`: Request correlation ID
- `task_type`: Task name

**Instrumented In**: `task_initializer.rs:start_task_initialization()`

**Example Queries**:

**Instant/Recent Data** (works immediately after task execution):
```promql
# Simple average initialization time
tasker_task_initialization_duration_milliseconds_sum /
tasker_task_initialization_duration_milliseconds_count

# P95 latency
histogram_quantile(0.95, sum by (le) (tasker_task_initialization_duration_milliseconds_bucket))

# P99 latency
histogram_quantile(0.99, sum by (le) (tasker_task_initialization_duration_milliseconds_bucket))
```

**Rate-Based** (for continuous monitoring, requires data spread over time):
```promql
# Average initialization time over 5 minutes
rate(tasker_task_initialization_duration_milliseconds_sum[5m]) /
rate(tasker_task_initialization_duration_milliseconds_count[5m])

# P95 latency over 5 minutes
histogram_quantile(0.95, sum by (le) (rate(tasker_task_initialization_duration_milliseconds_bucket[5m])))
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

#### `tasker.task.finalization.duration`
**Description**: Task finalization duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_task_finalization_duration_milliseconds_bucket`
- `tasker_task_finalization_duration_milliseconds_sum`
- `tasker_task_finalization_duration_milliseconds_count`

**Labels**:
- `correlation_id`: Request correlation ID
- `final_state`: "complete", "error", "cancelled"

**Instrumented In**: `task_finalizer.rs:finalize_task()`

**Example Queries**:

**Instant/Recent Data**:
```promql
# Simple average finalization time
tasker_task_finalization_duration_milliseconds_sum /
tasker_task_finalization_duration_milliseconds_count

# P95 by final state
histogram_quantile(0.95,
  sum by (final_state, le) (
    tasker_task_finalization_duration_milliseconds_bucket
  )
)
```

**Rate-Based**:
```promql
# Average finalization time over 5 minutes
rate(tasker_task_finalization_duration_milliseconds_sum[5m]) /
rate(tasker_task_finalization_duration_milliseconds_count[5m])

# P95 by final state over 5 minutes
histogram_quantile(0.95,
  sum by (final_state, le) (
    rate(tasker_task_finalization_duration_milliseconds_bucket[5m])
  )
)
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

#### `tasker.step_result.processing.duration`
**Description**: Step result processing duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_step_result_processing_duration_milliseconds_bucket`
- `tasker_step_result_processing_duration_milliseconds_sum`
- `tasker_step_result_processing_duration_milliseconds_count`

**Labels**:
- `correlation_id`: Request correlation ID
- `result_type`: "success", "error", "timeout", "cancelled", "skipped"

**Instrumented In**: `result_processor.rs:process_step_result()`

**Example Queries**:

**Instant/Recent Data**:
```promql
# Simple average result processing time
tasker_step_result_processing_duration_milliseconds_sum /
tasker_step_result_processing_duration_milliseconds_count

# P50, P95, P99 latencies
histogram_quantile(0.50, sum by (le) (tasker_step_result_processing_duration_milliseconds_bucket))
histogram_quantile(0.95, sum by (le) (tasker_step_result_processing_duration_milliseconds_bucket))
histogram_quantile(0.99, sum by (le) (tasker_step_result_processing_duration_milliseconds_bucket))
```

**Rate-Based**:
```promql
# Average result processing time over 5 minutes
rate(tasker_step_result_processing_duration_milliseconds_sum[5m]) /
rate(tasker_step_result_processing_duration_milliseconds_count[5m])

# P95 latency over 5 minutes
histogram_quantile(0.95, sum by (le) (rate(tasker_step_result_processing_duration_milliseconds_bucket[5m])))
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

### Gauges

#### `tasker.tasks.active`
**Description**: Number of tasks currently being processed
**Type**: Gauge (u64)
**Labels**:
- `state`: Current task state

**Status**: Planned (not yet instrumented)

---

#### `tasker.steps.ready`
**Description**: Number of steps ready for execution
**Type**: Gauge (u64)
**Labels**:
- `namespace`: Worker namespace

**Status**: Planned (not yet instrumented)

---

## Worker Metrics

**Module**: `tasker-shared/src/metrics/worker.rs`
**Instrumentation**: `tasker-worker/src/worker/*.rs`

### Counters

#### `tasker.steps.executions.total`
**Description**: Total number of step executions attempted
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID

**Instrumented In**: `command_processor.rs:handle_execute_step()`

**Example Query**:
```promql
# Total step executions
tasker_steps_executions_total

# Execution rate
rate(tasker_steps_executions_total[5m])

# For specific task
tasker_steps_executions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected Output**: (To be verified)

---

#### `tasker.steps.successes.total`
**Description**: Total number of step executions that completed successfully
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `namespace`: Worker namespace

**Instrumented In**: `command_processor.rs:handle_execute_step()` (success path)

**Example Query**:
```promql
# Total successes
tasker_steps_successes_total

# Success rate
rate(tasker_steps_successes_total[5m]) / rate(tasker_steps_executions_total[5m])

# By namespace
sum by (namespace) (tasker_steps_successes_total)
```

**Expected Output**: (To be verified)

---

#### `tasker.steps.failures.total`
**Description**: Total number of step executions that failed
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `namespace`: Worker namespace (or "unknown" for early failures)
- `error_type`: "claim_failed", "database_error", "step_not_found", "message_deletion_failed"

**Instrumented In**: `command_processor.rs:handle_execute_step()` (error paths)

**Example Query**:
```promql
# Total failures
tasker_steps_failures_total

# Failure rate
rate(tasker_steps_failures_total[5m]) / rate(tasker_steps_executions_total[5m])

# By error type
sum by (error_type) (tasker_steps_failures_total)

# Error distribution
topk(5, sum by (error_type) (tasker_steps_failures_total))
```

**Expected Output**: (To be verified)

---

#### `tasker.steps.claimed.total`
**Description**: Total number of steps claimed from queues
**Type**: Counter (u64)
**Labels**:
- `namespace`: Worker namespace
- `claim_method`: "event", "poll"

**Instrumented In**: `step_claim.rs:try_claim_step()`

**Example Query**:
```promql
# Total claims
tasker_steps_claimed_total

# By claim method
sum by (claim_method) (tasker_steps_claimed_total)

# Claim rate
rate(tasker_steps_claimed_total[5m])
```

**Expected Output**: (To be verified)

---

#### `tasker.steps.results_submitted.total`
**Description**: Total number of step results submitted to orchestration
**Type**: Counter (u64)
**Labels**:
- `correlation_id`: Request correlation ID
- `result_type`: "completion"

**Instrumented In**: `orchestration_result_sender.rs:send_completion()`

**Example Query**:
```promql
# Total submissions
tasker_steps_results_submitted_total

# Submission rate
rate(tasker_steps_results_submitted_total[5m])

# For specific task
tasker_steps_results_submitted_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected Output**: (To be verified)

---

### Histograms

#### `tasker.step.execution.duration`
**Description**: Step execution duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_step_execution_duration_milliseconds_bucket`
- `tasker_step_execution_duration_milliseconds_sum`
- `tasker_step_execution_duration_milliseconds_count`

**Labels**:
- `correlation_id`: Request correlation ID
- `namespace`: Worker namespace
- `result`: "success", "error"

**Instrumented In**: `command_processor.rs:handle_execute_step()`

**Example Queries**:

**Instant/Recent Data**:
```promql
# Simple average execution time
tasker_step_execution_duration_milliseconds_sum /
tasker_step_execution_duration_milliseconds_count

# P95 latency by namespace
histogram_quantile(0.95,
  sum by (namespace, le) (
    tasker_step_execution_duration_milliseconds_bucket
  )
)

# P99 latency
histogram_quantile(0.99, sum by (le) (tasker_step_execution_duration_milliseconds_bucket))
```

**Rate-Based**:
```promql
# Average execution time over 5 minutes
rate(tasker_step_execution_duration_milliseconds_sum[5m]) /
rate(tasker_step_execution_duration_milliseconds_count[5m])

# P95 latency by namespace over 5 minutes
histogram_quantile(0.95,
  sum by (namespace, le) (
    rate(tasker_step_execution_duration_milliseconds_bucket[5m])
  )
)
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

#### `tasker.step.claim.duration`
**Description**: Step claiming duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_step_claim_duration_milliseconds_bucket`
- `tasker_step_claim_duration_milliseconds_sum`
- `tasker_step_claim_duration_milliseconds_count`

**Labels**:
- `namespace`: Worker namespace
- `claim_method`: "event", "poll"

**Instrumented In**: `step_claim.rs:try_claim_step()`

**Example Queries**:

**Instant/Recent Data**:
```promql
# Simple average claim time
tasker_step_claim_duration_milliseconds_sum /
tasker_step_claim_duration_milliseconds_count

# Compare event vs poll claiming (P95)
histogram_quantile(0.95,
  sum by (claim_method, le) (
    tasker_step_claim_duration_milliseconds_bucket
  )
)
```

**Rate-Based**:
```promql
# Average claim time over 5 minutes
rate(tasker_step_claim_duration_milliseconds_sum[5m]) /
rate(tasker_step_claim_duration_milliseconds_count[5m])

# P95 by claim method over 5 minutes
histogram_quantile(0.95,
  sum by (claim_method, le) (
    rate(tasker_step_claim_duration_milliseconds_bucket[5m])
  )
)
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

#### `tasker.step_result.submission.duration`
**Description**: Step result submission duration in milliseconds
**Type**: Histogram (f64)
**Unit**: ms
**Prometheus Metric Names**:
- `tasker_step_result_submission_duration_milliseconds_bucket`
- `tasker_step_result_submission_duration_milliseconds_sum`
- `tasker_step_result_submission_duration_milliseconds_count`

**Labels**:
- `correlation_id`: Request correlation ID
- `result_type`: "completion"

**Instrumented In**: `orchestration_result_sender.rs:send_completion()`

**Example Queries**:

**Instant/Recent Data**:
```promql
# Simple average submission time
tasker_step_result_submission_duration_milliseconds_sum /
tasker_step_result_submission_duration_milliseconds_count

# P95 submission latency
histogram_quantile(0.95, sum by (le) (tasker_step_result_submission_duration_milliseconds_bucket))
```

**Rate-Based**:
```promql
# Average submission time over 5 minutes
rate(tasker_step_result_submission_duration_milliseconds_sum[5m]) /
rate(tasker_step_result_submission_duration_milliseconds_count[5m])

# P95 submission latency over 5 minutes
histogram_quantile(0.95, sum by (le) (rate(tasker_step_result_submission_duration_milliseconds_bucket[5m])))
```

**Expected Output**: ✅ Verified - Returns millisecond values

---

### Gauges

#### `tasker.steps.active_executions`
**Description**: Number of steps currently being executed
**Type**: Gauge (u64)
**Labels**:
- `namespace`: Worker namespace
- `handler_type`: "rust", "ruby"

**Status**: Defined but not actively instrumented (gauge tracking removed during implementation)

---

#### `tasker.queue.depth`
**Description**: Current queue depth per namespace
**Type**: Gauge (u64)
**Labels**:
- `namespace`: Worker namespace

**Status**: Planned (not yet instrumented)

---

## Resilience Metrics

**Module**: `tasker-shared/src/metrics/worker.rs`, `tasker-orchestration/src/web/circuit_breaker.rs`
**Instrumentation**: Circuit breakers (TAS-75), MPSC channels (TAS-51)
**Related Docs**: [Circuit Breakers](../circuit-breakers.md) | [Backpressure Architecture](../backpressure-architecture.md)

### Circuit Breaker Metrics

Circuit breakers provide fault isolation and cascade prevention. These metrics track breaker state transitions and related operations.

#### `api_circuit_breaker_state`
**Description**: Current state of the web API database circuit breaker
**Type**: Gauge (i64)
**Values**: 0=Closed, 1=Half-Open, 2=Open
**Labels**: None

**Instrumented In**: `tasker-orchestration/src/web/circuit_breaker.rs`

**Example Queries**:
```promql
# Current state
api_circuit_breaker_state

# Alert when open
api_circuit_breaker_state == 2
```

---

#### `tasker_circuit_breaker_state`
**Description**: Per-component circuit breaker state
**Type**: Gauge (i64)
**Values**: 0=Closed, 1=Half-Open, 2=Open
**Labels**:
- `component`: Circuit breaker name (e.g., "ffi_completion", "task_readiness", "pgmq")

**Instrumented In**: Various circuit breaker implementations

**Example Queries**:
```promql
# All circuit breaker states
tasker_circuit_breaker_state

# Check specific component
tasker_circuit_breaker_state{component="ffi_completion"}

# Count open breakers
count(tasker_circuit_breaker_state == 2)
```

---

#### `api_requests_rejected_total`
**Description**: Total API requests rejected due to open circuit breaker
**Type**: Counter (u64)
**Labels**:
- `endpoint`: The rejected endpoint path

**Instrumented In**: `tasker-orchestration/src/web/circuit_breaker.rs`

**Example Queries**:
```promql
# Total rejections
api_requests_rejected_total

# Rejection rate
rate(api_requests_rejected_total[5m])

# By endpoint
sum by (endpoint) (api_requests_rejected_total)
```

---

#### `ffi_completion_slow_sends_total`
**Description**: FFI completion channel sends exceeding latency threshold (100ms default)
**Type**: Counter (u64)
**Labels**: None

**Instrumented In**: `tasker-worker/src/worker/handlers/ffi_completion_circuit_breaker.rs`

**Example Queries**:
```promql
# Total slow sends
ffi_completion_slow_sends_total

# Slow send rate (alerts at >10/sec)
rate(ffi_completion_slow_sends_total[5m]) > 10
```

**Alert Threshold**: Warning when rate exceeds 10/second for 2 minutes

---

#### `ffi_completion_circuit_open_rejections_total`
**Description**: FFI completion operations rejected due to open circuit breaker
**Type**: Counter (u64)
**Labels**: None

**Instrumented In**: `tasker-worker/src/worker/handlers/ffi_completion_circuit_breaker.rs`

**Example Queries**:
```promql
# Total rejections
ffi_completion_circuit_open_rejections_total

# Rejection rate
rate(ffi_completion_circuit_open_rejections_total[5m])
```

---

### MPSC Channel Metrics

Bounded MPSC channels provide backpressure control. These metrics track channel utilization and overflow events.

#### `mpsc_channel_usage_percent`
**Description**: Current fill percentage of a bounded MPSC channel
**Type**: Gauge (f64)
**Labels**:
- `channel`: Channel name (e.g., "orchestration_command", "pgmq_notifications")
- `component`: Owning component

**Instrumented In**: Channel monitor integration points

**Example Queries**:
```promql
# All channel usage
mpsc_channel_usage_percent

# High usage channels
mpsc_channel_usage_percent > 80

# By component
max by (component) (mpsc_channel_usage_percent)
```

**Alert Thresholds**:
- Warning: > 80% for 15 minutes
- Critical: > 90% for 5 minutes

---

#### `mpsc_channel_capacity`
**Description**: Configured buffer capacity for an MPSC channel
**Type**: Gauge (u64)
**Labels**:
- `channel`: Channel name
- `component`: Owning component

**Instrumented In**: Channel monitor initialization

**Example Queries**:
```promql
# All channel capacities
mpsc_channel_capacity

# Compare usage to capacity
mpsc_channel_usage_percent / 100 * mpsc_channel_capacity
```

---

#### `mpsc_channel_full_events_total`
**Description**: Count of channel overflow events (backpressure applied)
**Type**: Counter (u64)
**Labels**:
- `channel`: Channel name
- `component`: Owning component

**Instrumented In**: Channel send operations with backpressure handling

**Example Queries**:
```promql
# Total overflow events
mpsc_channel_full_events_total

# Overflow rate
rate(mpsc_channel_full_events_total[5m])

# By channel
sum by (channel) (mpsc_channel_full_events_total)
```

**Alert Threshold**: Any overflow events indicate backpressure is occurring

---

### Resilience Dashboard Panels

**Circuit Breaker State Timeline**:
```promql
# Panel: Time series with state mapping
api_circuit_breaker_state
# Value mappings: 0=Closed (green), 1=Half-Open (yellow), 2=Open (red)
```

**FFI Completion Health**:
```promql
# Panel: Multi-stat showing slow sends and rejections
rate(ffi_completion_slow_sends_total[5m])
rate(ffi_completion_circuit_open_rejections_total[5m])
```

**Channel Saturation Overview**:
```promql
# Panel: Gauge showing max channel usage
max(mpsc_channel_usage_percent)
# Thresholds: Green < 70%, Yellow < 90%, Red >= 90%
```

**Backpressure Events**:
```promql
# Panel: Time series of overflow rate
rate(mpsc_channel_full_events_total[5m])
```

---

## Database Metrics

**Module**: `tasker-shared/src/metrics/database.rs`
**Status**: ⚠️ Defined but not yet instrumented

### Planned Metrics

- `tasker.sql.queries.total` - Counter
- `tasker.sql.query.duration` - Histogram
- `tasker.db.pool.connections_active` - Gauge
- `tasker.db.pool.connections_idle` - Gauge
- `tasker.db.pool.wait_duration` - Histogram
- `tasker.db.transactions.total` - Counter
- `tasker.db.transaction.duration` - Histogram

---

## Messaging Metrics

**Module**: `tasker-shared/src/metrics/messaging.rs`
**Status**: ⚠️ Defined but not yet instrumented

### Planned Metrics

- `tasker.queue.messages_sent.total` - Counter
- `tasker.queue.messages_received.total` - Counter
- `tasker.queue.messages_deleted.total` - Counter
- `tasker.queue.message_send.duration` - Histogram
- `tasker.queue.message_receive.duration` - Histogram
- `tasker.queue.depth` - Gauge
- `tasker.queue.age_seconds` - Gauge
- `tasker.queue.visibility_timeouts.total` - Counter
- `tasker.queue.errors.total` - Counter
- `tasker.queue.retry_attempts.total` - Counter

> **Note**: Circuit breaker metrics (including queue-related circuit breakers) are documented in the [Resilience Metrics](#resilience-metrics) section.

---

## Example Queries

### Task Execution Flow

**Complete task execution for a specific correlation_id:**

```promql
# 1. Task creation
tasker_tasks_requests_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 2. Steps enqueued
tasker_steps_enqueued_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 3. Steps executed
tasker_steps_executions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 4. Steps succeeded
tasker_steps_successes_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 5. Results submitted
tasker_steps_results_submitted_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 6. Results processed
tasker_step_results_processed_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}

# 7. Task completed
tasker_tasks_completions_total{correlation_id="0199c3e0-ccdb-7581-87ab-3f67daeaa4a5"}
```

**Expected Flow**: 1 → N → N → N → N → N → 1 (where N = number of steps)

---

### Performance Analysis

**Task initialization latency percentiles:**

**Instant/Recent Data**:
```promql
# P50 (median)
histogram_quantile(0.50, sum by (le) (tasker_task_initialization_duration_milliseconds_bucket))

# P95
histogram_quantile(0.95, sum by (le) (tasker_task_initialization_duration_milliseconds_bucket))

# P99
histogram_quantile(0.99, sum by (le) (tasker_task_initialization_duration_milliseconds_bucket))
```

**Rate-Based** (continuous monitoring):
```promql
# P50 (median)
histogram_quantile(0.50, sum by (le) (rate(tasker_task_initialization_duration_milliseconds_bucket[5m])))

# P95
histogram_quantile(0.95, sum by (le) (rate(tasker_task_initialization_duration_milliseconds_bucket[5m])))

# P99
histogram_quantile(0.99, sum by (le) (rate(tasker_task_initialization_duration_milliseconds_bucket[5m])))
```

**Step execution latency by namespace:**

**Instant/Recent Data**:
```promql
histogram_quantile(0.95,
  sum by (namespace, le) (
    tasker_step_execution_duration_milliseconds_bucket
  )
)
```

**Rate-Based**:
```promql
histogram_quantile(0.95,
  sum by (namespace, le) (
    rate(tasker_step_execution_duration_milliseconds_bucket[5m])
  )
)
```

**End-to-end task duration (from request to completion):**

This requires combining initialization + step execution + finalization durations. Use the simple average approach for instant data:

```promql
# Average task initialization
tasker_task_initialization_duration_milliseconds_sum /
tasker_task_initialization_duration_milliseconds_count

# Average step execution
tasker_step_execution_duration_milliseconds_sum /
tasker_step_execution_duration_milliseconds_count

# Average finalization
tasker_task_finalization_duration_milliseconds_sum /
tasker_task_finalization_duration_milliseconds_count
```

---

### Error Rate Monitoring

**Overall step failure rate:**

```promql
rate(tasker_steps_failures_total[5m]) /
rate(tasker_steps_executions_total[5m])
```

**Error distribution by type:**

```promql
topk(5, sum by (error_type) (tasker_steps_failures_total))
```

**Task failure rate:**

```promql
rate(tasker_tasks_failures_total[5m]) /
(rate(tasker_tasks_completions_total[5m]) + rate(tasker_tasks_failures_total[5m]))
```

---

### Throughput Monitoring

**Task request rate:**

```promql
rate(tasker_tasks_requests_total[1m])
rate(tasker_tasks_requests_total[5m])
rate(tasker_tasks_requests_total[15m])
```

**Step execution throughput:**

```promql
sum(rate(tasker_steps_executions_total[5m]))
```

**Step completion rate (successes + failures):**

```promql
sum(rate(tasker_steps_successes_total[5m])) +
sum(rate(tasker_steps_failures_total[5m]))
```

---

## Dashboard Recommendations

### Task Execution Overview Dashboard

**Panels:**

1. **Task Request Rate**
   - Query: `rate(tasker_tasks_requests_total[5m])`
   - Visualization: Time series graph

2. **Task Completion Rate**
   - Query: `rate(tasker_tasks_completions_total[5m])`
   - Visualization: Time series graph

3. **Task Success/Failure Ratio**
   - Query: Two series
     - Completions: `rate(tasker_tasks_completions_total[5m])`
     - Failures: `rate(tasker_tasks_failures_total[5m])`
   - Visualization: Stacked area chart

4. **Task Initialization Latency (P95)**
   - Query: `histogram_quantile(0.95, rate(tasker_task_initialization_duration_bucket[5m]))`
   - Visualization: Time series graph

5. **Steps Enqueued vs Executed**
   - Query: Two series
     - Enqueued: `rate(tasker_steps_enqueued_total[5m])`
     - Executed: `rate(tasker_steps_executions_total[5m])`
   - Visualization: Time series graph

---

### Worker Performance Dashboard

**Panels:**

1. **Step Execution Throughput by Namespace**
   - Query: `sum by (namespace) (rate(tasker_steps_executions_total[5m]))`
   - Visualization: Time series graph (multi-series)

2. **Step Success Rate**
   - Query: `rate(tasker_steps_successes_total[5m]) / rate(tasker_steps_executions_total[5m])`
   - Visualization: Gauge (0-1 scale)

3. **Step Execution Latency Percentiles**
   - Query: Three series
     - P50: `histogram_quantile(0.50, rate(tasker_step_execution_duration_bucket[5m]))`
     - P95: `histogram_quantile(0.95, rate(tasker_step_execution_duration_bucket[5m]))`
     - P99: `histogram_quantile(0.99, rate(tasker_step_execution_duration_bucket[5m]))`
   - Visualization: Time series graph

4. **Step Claiming Performance (Event vs Poll)**
   - Query: `histogram_quantile(0.95, sum by (claim_method, le) (rate(tasker_step_claim_duration_bucket[5m])))`
   - Visualization: Time series graph

5. **Error Distribution by Type**
   - Query: `sum by (error_type) (rate(tasker_steps_failures_total[5m]))`
   - Visualization: Pie chart or bar chart

---

### System Health Dashboard

**Panels:**

1. **Overall Task Success Rate**
   - Query: `rate(tasker_tasks_completions_total[5m]) / (rate(tasker_tasks_completions_total[5m]) + rate(tasker_tasks_failures_total[5m]))`
   - Visualization: Stat panel with thresholds (green > 0.95, yellow > 0.90, red < 0.90)

2. **Step Failure Rate**
   - Query: `rate(tasker_steps_failures_total[5m]) / rate(tasker_steps_executions_total[5m])`
   - Visualization: Stat panel with thresholds

3. **Average Task End-to-End Duration**
   - Query: Combination of initialization, execution, and finalization durations
   - Visualization: Time series graph

4. **Result Processing Latency**
   - Query: `rate(tasker_step_result_processing_duration_sum[5m]) / rate(tasker_step_result_processing_duration_count[5m])`
   - Visualization: Time series graph

5. **Active Operations**
   - Query: Currently not instrumented (gauges removed)
   - Status: Planned future enhancement

---

## Verification Checklist

Use this checklist to verify metrics are working correctly:

### Prerequisites
- [ ] `telemetry.opentelemetry.enabled = true` in development config
- [ ] Services restarted after config change
- [ ] Logs show `opentelemetry_enabled=true`
- [ ] Grafana LGTM container running on ports 3000, 4317

### Basic Verification
- [ ] At least one task created via CLI
- [ ] Correlation ID captured from task creation
- [ ] Trace visible in Grafana Tempo for correlation ID

### Orchestration Metrics
- [ ] `tasker_tasks_requests_total` returns non-zero
- [ ] `tasker_steps_enqueued_total` returns expected step count
- [ ] `tasker_step_results_processed_total` returns expected result count
- [ ] `tasker_tasks_completions_total` increments on success
- [ ] `tasker_task_initialization_duration_bucket` has histogram data

### Worker Metrics
- [ ] `tasker_steps_executions_total` returns non-zero
- [ ] `tasker_steps_successes_total` matches successful steps
- [ ] `tasker_steps_claimed_total` returns expected claims
- [ ] `tasker_steps_results_submitted_total` matches result submissions
- [ ] `tasker_step_execution_duration_bucket` has histogram data

### Resilience Metrics
- [ ] `api_circuit_breaker_state` returns 0 (Closed) during normal operation
- [ ] `/health/detailed` endpoint shows circuit breaker states
- [ ] `mpsc_channel_usage_percent` returns values < 80% (no saturation)
- [ ] `mpsc_channel_full_events_total` is 0 or very low (no backpressure)
- [ ] FFI workers: `ffi_completion_slow_sends_total` is near zero

### Correlation
- [ ] All metrics filterable by `correlation_id`
- [ ] Correlation ID in metrics matches trace ID in Tempo
- [ ] Complete execution flow visible from request to completion

---

## Troubleshooting

### No Metrics Appearing

**Check 1**: OpenTelemetry enabled
```bash
grep "opentelemetry_enabled" tmp/*.log
# Should show: opentelemetry_enabled=true
```

**Check 2**: OTLP endpoint accessible
```bash
curl -v http://localhost:4317 2>&1 | grep Connected
# Should show: Connected to localhost (127.0.0.1) port 4317
```

**Check 3**: Grafana LGTM running
```bash
curl -s http://localhost:3000/api/health | jq
# Should return healthy status
```

**Check 4**: Wait for export interval (60 seconds)
Metrics are batched and exported every 60 seconds. Wait at least 1 minute after task execution.

### Metrics Missing Labels

If correlation_id or other labels are missing, check:
- Logs for `correlation_id` field presence
- Metric instrumentation includes KeyValue::new() calls
- Labels match between metric definition and usage

### Histogram Buckets Empty

If histogram queries return no data:
- Verify histogram is initialized: check logs for metric initialization
- Ensure duration values are non-zero and reasonable
- Check that `record()` is called, not `add()` for histograms

---

## Next Steps

### Phase 3.4 (Future)
- Instrument database metrics (7 metrics)
- Instrument messaging metrics (11 metrics)
- Add gauge tracking for active operations
- Implement queue depth monitoring

### Production Readiness
- Create alert rules for error rates
- Set up automated dashboards
- Configure metric retention policies
- Add metric aggregation for long-term storage

---

**Last Updated**: 2025-12-10
**Test Task**: `mathematical_sequence` (correlation_id: 0199c3e0-ccdb-7581-87ab-3f67daeaa4a5)
**Status**: All orchestration and worker metrics verified and producing data ✅

**Recent Updates**:
- 2025-12-10: Added Resilience Metrics section (TAS-75 circuit breakers, TAS-51 MPSC channels)
- 2025-10-08: Initial metrics verification completed
