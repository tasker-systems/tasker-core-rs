# Backpressure Monitoring Runbook

**Last Updated**: 2025-12-08
**Audience**: Operations, SRE, On-Call Engineers
**Status**: Active (TAS-75)
**Related Docs**: [Backpressure Architecture](../backpressure-architecture.md) | [MPSC Channel Tuning](mpsc-channel-tuning.md)

---

This runbook provides guidance for monitoring, alerting, and responding to backpressure events in tasker-core.

## Quick Reference

### Critical Metrics Dashboard

| Metric | Normal | Warning | Critical | Action |
|--------|--------|---------|----------|--------|
| `api_circuit_breaker_state` | closed | - | open | See [Circuit Breaker Open](#circuit-breaker-open) |
| `api_requests_rejected_total` | < 1/min | > 5/min | > 20/min | See [API Rejections](#api-rejections-high) |
| `mpsc_channel_saturation` | < 50% | > 70% | > 90% | See [Channel Saturation](#channel-saturation) |
| `pgmq_queue_depth` | < 50% max | > 70% max | > 90% max | See [Queue Depth High](#pgmq-queue-depth-high) |
| `worker_claim_refusals_total` | < 5/min | > 20/min | > 50/min | See [Claim Refusals](#worker-claim-refusals-high) |
| `handler_semaphore_wait_ms_p99` | < 100ms | > 500ms | > 2000ms | See [Handler Wait](#handler-wait-time-high) |
| `domain_events_dropped_total` | < 10/min | > 50/min | > 200/min | See [Domain Events](#domain-events-dropped) |

---

## Key Metrics

### API Layer Metrics

#### `api_requests_total`
- **Type**: Counter
- **Labels**: `endpoint`, `status_code`, `method`
- **Description**: Total API requests received
- **Usage**: Calculate request rate, error rate

#### `api_requests_rejected_total`
- **Type**: Counter
- **Labels**: `endpoint`, `reason` (rate_limit, circuit_breaker, validation)
- **Description**: Requests rejected due to backpressure
- **Alert**: > 10/min sustained

#### `api_circuit_breaker_state`
- **Type**: Gauge
- **Values**: 0 = closed, 1 = half-open, 2 = open
- **Description**: Current circuit breaker state
- **Alert**: state = 2 (open)

#### `api_request_latency_ms`
- **Type**: Histogram
- **Labels**: `endpoint`
- **Description**: Request processing time
- **Alert**: p99 > 5000ms

### Orchestration Metrics

#### `orchestration_command_channel_size`
- **Type**: Gauge
- **Description**: Current command channel buffer usage
- **Alert**: > 80% of `command_buffer_size`

#### `orchestration_command_channel_saturation`
- **Type**: Gauge (0.0 - 1.0)
- **Description**: Channel saturation ratio
- **Alert**: > 0.8 sustained for > 1min

#### `pgmq_queue_depth`
- **Type**: Gauge
- **Labels**: `queue_name`
- **Description**: Messages in queue
- **Alert**: > configured max_queue_depth * 0.8

#### `pgmq_enqueue_failures_total`
- **Type**: Counter
- **Labels**: `queue_name`, `reason`
- **Description**: Failed enqueue operations
- **Alert**: > 0 (any failure)

### Worker Metrics

#### `worker_claim_refusals_total`
- **Type**: Counter
- **Labels**: `worker_id`, `namespace`
- **Description**: Step claims refused due to capacity
- **Alert**: > 10/min sustained

#### `worker_handler_semaphore_permits_available`
- **Type**: Gauge
- **Labels**: `worker_id`
- **Description**: Available handler permits
- **Alert**: = 0 sustained for > 30s

#### `worker_handler_semaphore_wait_ms`
- **Type**: Histogram
- **Labels**: `worker_id`
- **Description**: Time waiting for semaphore permit
- **Alert**: p99 > 1000ms

#### `worker_dispatch_channel_saturation`
- **Type**: Gauge
- **Labels**: `worker_id`
- **Description**: Dispatch channel saturation
- **Alert**: > 0.8 sustained

#### `worker_completion_channel_saturation`
- **Type**: Gauge
- **Labels**: `worker_id`
- **Description**: Completion channel saturation
- **Alert**: > 0.8 sustained

#### `domain_events_dropped_total`
- **Type**: Counter
- **Labels**: `worker_id`, `event_type`
- **Description**: Domain events dropped due to channel full
- **Alert**: > 50/min (informational)

---

## Alert Configurations

### Prometheus Alert Rules

```yaml
groups:
  - name: tasker_backpressure
    rules:
      # API Layer
      - alert: TaskerCircuitBreakerOpen
        expr: api_circuit_breaker_state == 2
        for: 30s
        labels:
          severity: critical
        annotations:
          summary: "Tasker API circuit breaker is open"
          description: "Circuit breaker {{ $labels.instance }} has been open for > 30s"
          runbook: "https://docs/operations/backpressure-monitoring.md#circuit-breaker-open"

      - alert: TaskerAPIRejectionsHigh
        expr: rate(api_requests_rejected_total[5m]) > 0.1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High rate of API request rejections"
          description: "{{ $value | printf \"%.2f\" }} requests/sec being rejected"
          runbook: "https://docs/operations/backpressure-monitoring.md#api-rejections-high"

      # Orchestration Layer
      - alert: TaskerCommandChannelSaturated
        expr: orchestration_command_channel_saturation > 0.8
        for: 1m
        labels:
          severity: warning
        annotations:
          summary: "Orchestration command channel is saturated"
          description: "Channel saturation at {{ $value | printf \"%.0f\" }}%"
          runbook: "https://docs/operations/backpressure-monitoring.md#channel-saturation"

      - alert: TaskerPGMQQueueDepthHigh
        expr: pgmq_queue_depth / pgmq_queue_max_depth > 0.8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "PGMQ queue depth is high"
          description: "Queue {{ $labels.queue_name }} at {{ $value | printf \"%.0f\" }}% capacity"
          runbook: "https://docs/operations/backpressure-monitoring.md#pgmq-queue-depth-high"

      # Worker Layer
      - alert: TaskerWorkerClaimRefusalsHigh
        expr: rate(worker_claim_refusals_total[5m]) > 0.2
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High rate of worker claim refusals"
          description: "Worker {{ $labels.worker_id }} refusing {{ $value | printf \"%.1f\" }} claims/sec"
          runbook: "https://docs/operations/backpressure-monitoring.md#worker-claim-refusals-high"

      - alert: TaskerHandlerWaitTimeHigh
        expr: histogram_quantile(0.99, worker_handler_semaphore_wait_ms_bucket) > 2000
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Handler wait time is high"
          description: "p99 handler wait time is {{ $value | printf \"%.0f\" }}ms"
          runbook: "https://docs/operations/backpressure-monitoring.md#handler-wait-time-high"

      - alert: TaskerDomainEventsDropped
        expr: rate(domain_events_dropped_total[5m]) > 1
        for: 5m
        labels:
          severity: info
        annotations:
          summary: "Domain events being dropped"
          description: "{{ $value | printf \"%.1f\" }} events/sec dropped"
          runbook: "https://docs/operations/backpressure-monitoring.md#domain-events-dropped"
```

---

## Incident Response Procedures

### Circuit Breaker Open

**Severity**: Critical

**Symptoms**:
- API returning 503 responses
- `api_circuit_breaker_state = 2`
- Downstream operations failing

**Immediate Actions**:
1. Check database connectivity
   ```bash
   psql $DATABASE_URL -c "SELECT 1"
   ```
2. Check PGMQ extension health
   ```bash
   psql $DATABASE_URL -c "SELECT * FROM pgmq.meta LIMIT 5"
   ```
3. Check recent error logs
   ```bash
   kubectl logs -l app=tasker-orchestration --tail=100 | grep ERROR
   ```

**Recovery**:
- Circuit breaker will automatically attempt recovery after `timeout_seconds` (default: 30s)
- If database is healthy, breaker should close after `success_threshold` (default: 2) successful requests
- If database is unhealthy, fix database first

**Escalation**:
- If breaker remains open > 5 min after database recovery: Escalate to engineering

---

### API Rejections High

**Severity**: Warning

**Symptoms**:
- Clients receiving 429 or 503 responses
- `api_requests_rejected_total` increasing

**Diagnosis**:
1. Check rejection reason distribution
   ```promql
   sum by (reason) (rate(api_requests_rejected_total[5m]))
   ```
2. If `reason=rate_limit`: Legitimate load spike or client misbehavior
3. If `reason=circuit_breaker`: See [Circuit Breaker Open](#circuit-breaker-open)

**Actions**:
- **Rate limit rejections**:
  - Identify high-volume client
  - Consider increasing rate limit or contacting client
- **Circuit breaker rejections**:
  - Follow circuit breaker procedure

---

### Channel Saturation

**Severity**: Warning → Critical if sustained

**Symptoms**:
- `mpsc_channel_saturation > 0.8`
- Increased latency
- Potential backpressure cascade

**Diagnosis**:
1. Identify saturated channel
   ```promql
   orchestration_command_channel_saturation > 0.8
   ```
2. Check upstream rate
   ```promql
   rate(orchestration_commands_received_total[5m])
   ```
3. Check downstream processing rate
   ```promql
   rate(orchestration_commands_processed_total[5m])
   ```

**Actions**:
1. **Temporary**: Scale up orchestration replicas
2. **Short-term**: Increase channel buffer size
   ```toml
   [orchestration.mpsc_channels.command_processor]
   command_buffer_size = 10000  # Increase from 5000
   ```
3. **Long-term**: Investigate why processing is slow

---

### PGMQ Queue Depth High

**Severity**: Warning → Critical if approaching max

**Symptoms**:
- `pgmq_queue_depth` growing
- Step execution delays
- Potential OOM if queue grows unbounded

**Diagnosis**:
1. Identify growing queue
   ```promql
   pgmq_queue_depth{queue_name=~".*"}
   ```
2. Check worker health
   ```promql
   sum(worker_handler_semaphore_permits_available)
   ```
3. Check for stuck workers
   ```promql
   count(worker_claim_refusals_total) by (worker_id)
   ```

**Actions**:
1. **Scale workers**: Add more worker replicas
2. **Increase handler concurrency** (short-term):
   ```toml
   [worker.mpsc_channels.handler_dispatch]
   max_concurrent_handlers = 20  # Increase from 10
   ```
3. **Investigate slow handlers**: Check handler execution latency

---

### Worker Claim Refusals High

**Severity**: Warning

**Symptoms**:
- `worker_claim_refusals_total` increasing
- Workers at capacity
- Step execution delayed

**Diagnosis**:
1. Check handler permit usage
   ```promql
   worker_handler_semaphore_permits_available
   ```
2. Check handler execution time
   ```promql
   histogram_quantile(0.99, worker_handler_execution_ms_bucket)
   ```

**Actions**:
1. **Scale workers**: Add replicas
2. **Optimize handlers**: If execution time is high
3. **Adjust threshold**: If refusals are premature
   ```toml
   [worker]
   claim_capacity_threshold = 0.9  # More aggressive claiming
   ```

---

### Handler Wait Time High

**Severity**: Warning

**Symptoms**:
- `handler_semaphore_wait_ms_p99 > 1000ms`
- Steps waiting for execution
- Increased end-to-end latency

**Diagnosis**:
1. Check permit utilization
   ```promql
   1 - (worker_handler_semaphore_permits_available / worker_handler_semaphore_permits_total)
   ```
2. Check completion channel saturation
   ```promql
   worker_completion_channel_saturation
   ```

**Actions**:
1. **Increase permits** (if CPU/memory allow):
   ```toml
   [worker.mpsc_channels.handler_dispatch]
   max_concurrent_handlers = 15
   ```
2. **Optimize handlers**: Reduce execution time
3. **Scale workers**: If resources constrained

---

### Domain Events Dropped

**Severity**: Informational

**Symptoms**:
- `domain_events_dropped_total` increasing
- Downstream event consumers missing events

**Diagnosis**:
1. This is expected behavior under load
2. Check if drop rate is excessive
   ```promql
   rate(domain_events_dropped_total[5m]) / rate(domain_events_dispatched_total[5m])
   ```

**Actions**:
- **If < 1% dropped**: Normal, no action needed
- **If > 5% dropped**: Consider increasing event channel buffer
  ```toml
  [shared.domain_events]
  buffer_size = 20000  # Increase from 10000
  ```
- **Note**: Domain events are non-critical. Dropping does not affect step execution.

---

## Capacity Planning

### Determining Appropriate Limits

#### Command Channel Size
```
Required buffer = (peak_requests_per_second) * (avg_processing_time_ms / 1000) * safety_factor

Example:
  peak_requests_per_second = 100
  avg_processing_time_ms = 50
  safety_factor = 2

  Required buffer = 100 * 0.05 * 2 = 10 messages
  Recommended: 5000 (50x headroom for bursts)
```

#### Handler Concurrency
```
Optimal concurrency = (worker_cpu_cores) * (1 + (io_wait_ratio))

Example:
  worker_cpu_cores = 4
  io_wait_ratio = 0.8 (handlers are mostly I/O bound)

  Optimal concurrency = 4 * 1.8 = 7.2
  Recommended: 8-10 permits
```

#### PGMQ Queue Depth
```
Max depth = (expected_processing_rate) * (max_acceptable_delay_seconds)

Example:
  expected_processing_rate = 100 steps/sec
  max_acceptable_delay = 60 seconds

  Max depth = 100 * 60 = 6000 messages
  Recommended: 10000 (headroom for bursts)
```

---

## Grafana Dashboard

Import this dashboard for backpressure monitoring:

```json
{
  "dashboard": {
    "title": "Tasker Backpressure",
    "panels": [
      {
        "title": "Circuit Breaker State",
        "type": "stat",
        "targets": [{"expr": "api_circuit_breaker_state"}]
      },
      {
        "title": "API Rejections Rate",
        "type": "graph",
        "targets": [{"expr": "rate(api_requests_rejected_total[5m])"}]
      },
      {
        "title": "Channel Saturation",
        "type": "graph",
        "targets": [
          {"expr": "orchestration_command_channel_saturation", "legendFormat": "orchestration"},
          {"expr": "worker_dispatch_channel_saturation", "legendFormat": "worker-dispatch"},
          {"expr": "worker_completion_channel_saturation", "legendFormat": "worker-completion"}
        ]
      },
      {
        "title": "PGMQ Queue Depth",
        "type": "graph",
        "targets": [{"expr": "pgmq_queue_depth", "legendFormat": "{{queue_name}}"}]
      },
      {
        "title": "Handler Wait Time (p99)",
        "type": "graph",
        "targets": [{"expr": "histogram_quantile(0.99, worker_handler_semaphore_wait_ms_bucket)"}]
      },
      {
        "title": "Worker Claim Refusals",
        "type": "graph",
        "targets": [{"expr": "rate(worker_claim_refusals_total[5m])"}]
      }
    ]
  }
}
```

---

## Related Documentation

- [Backpressure Architecture](../backpressure-architecture.md) - Strategy overview
- [MPSC Channel Tuning](mpsc-channel-tuning.md) - Channel configuration
- [Worker Event Systems](../worker-event-systems.md) - Worker architecture
- [TAS-75](https://linear.app/tasker-systems/issue/TAS-75) - Implementation plan
