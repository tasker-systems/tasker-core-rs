# Observability Documentation

**Last Updated**: 2025-12-01
**Audience**: Operators, Developers
**Status**: Active
**Related Docs**: [Documentation Hub](../README.md) | [Benchmarks](../benchmarks/README.md) | [Deployment Patterns](../deployment-patterns.md) | [Domain Events](../domain-events.md)

← Back to [Documentation Hub](../README.md)

---

This directory contains documentation for monitoring, metrics, logging, and performance measurement in tasker-core.

---

## Quick Navigation

### 📊 **Performance & Benchmarking** → **[../benchmarks/](../benchmarks/)**
All benchmark documentation has been consolidated in the `docs/benchmarks/` directory.

**See**: [Benchmark README](../benchmarks/README.md) for:
- API performance benchmarks
- SQL function benchmarks
- Event propagation benchmarks
- End-to-end latency benchmarks
- Benchmark quick reference
- Performance targets and CI integration

**Migration Note**: The following files remain in this directory for historical context but are superseded by the consolidated benchmarks documentation:
- `benchmark-implementation-decision.md` - Decision rationale (archived)
- `benchmark-quick-reference.md` - Superseded by [../benchmarks/README.md](../benchmarks/README.md)
- `benchmark-strategy-summary.md` - Consolidated into benchmark-specific docs
- `benchmarking-guide.md` - SQL benchmarks moved to [../benchmarks/sql-benchmarks.md](../benchmarks/sql-benchmarks.md)
- `phase-5.4-distributed-benchmarks-plan.md` - Implementation complete, see [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)

---

## Observability Categories

### 1. **Metrics** (`metrics-*.md`)
**Purpose**: System health, performance counters, and operational metrics

**Documentation**:
- **[metrics-reference.md](./metrics-reference.md)** - Complete metrics catalog
- **[metrics-verification.md](./metrics-verification.md)** - Verification procedures
- **[VERIFICATION_RESULTS.md](./VERIFICATION_RESULTS.md)** - Test results and validation

**Key Metrics Tracked**:
- Task lifecycle events (created, started, completed, failed)
- Step execution metrics (claimed, executed, retried)
- Database operation performance (query times, cache hit rates)
- Worker health (active workers, queue depths, claim rates)
- System resource usage (memory, connections, threads)

**Export Targets**:
- OpenTelemetry (planned)
- Prometheus (supported)
- CloudWatch (planned)
- Datadog (planned)

**Quick Reference**:
```rust
// Example: Recording a metric
metrics::counter!("tasker.tasks.created").increment(1);
metrics::histogram!("tasker.step.execution_time_ms").record(elapsed_ms);
metrics::gauge!("tasker.workers.active").set(worker_count as f64);
```

---

### 2. **Logging** (`logging-standards.md`)
**Purpose**: Structured logging for debugging, audit trails, and operational visibility

**Documentation**:
- **[logging-standards.md](./logging-standards.md)** - Logging standards and best practices

**Log Levels**:
- **ERROR**: Critical failures requiring immediate attention
- **WARN**: Degraded operation or retry scenarios
- **INFO**: Significant lifecycle events and state transitions
- **DEBUG**: Detailed execution flow for troubleshooting
- **TRACE**: Exhaustive detail for deep debugging

**Structured Fields**:
```rust
info!(
    task_uuid = %task_uuid,
    correlation_id = %correlation_id,
    step_name = %step_name,
    elapsed_ms = elapsed.as_millis(),
    "Step execution completed successfully"
);
```

**Key Standards**:
- Use structured logging (not string interpolation)
- Include correlation IDs for distributed tracing
- Log state transitions at INFO level
- Include timing information for performance analysis
- Sanitize sensitive data (credentials, PII)

---

### 3. **Tracing and OpenTelemetry**
**Purpose**: Distributed request tracing across services

**Status**: ✅ **Active** (TAS-65 Complete)

**Documentation**:
- **[opentelemetry-improvements.md](./opentelemetry-improvements.md)** - TAS-65 telemetry enhancements

**Current Features**:
- Distributed trace propagation via correlation IDs (UUIDv7)
- Span creation for major operations:
  - API request handling
  - Step execution (claim → execute → submit)
  - Orchestration coordination
  - Domain event publishing
  - Message queue operations
- Two-phase FFI telemetry initialization (safe for Ruby/Python workers)
- Integration with Grafana LGTM stack (Prometheus, Tempo)
- Domain event metrics (`/metrics/events` endpoint)

**Two-Phase FFI Initialization** (TAS-65):
- **Phase 1**: Console-only logging (safe during FFI bridge setup)
- **Phase 2**: Full OpenTelemetry (after FFI established)

**Example**:
```rust
#[tracing::instrument(
    name = "publish_domain_event",
    skip(self, payload),
    fields(
        event_name = %event_name,
        namespace = %metadata.namespace,
        correlation_id = %metadata.correlation_id,
        delivery_mode = ?delivery_mode
    )
)]
async fn publish_event(&self, event_name: &str, ...) -> Result<()> {
    // Implementation
}
```

---

### 4. **Health Checks**
**Purpose**: Service health monitoring for orchestration, availability, and alerting

**Endpoints**:
- **`GET /health`** - Overall service health
- **`GET /health/ready`** - Readiness for traffic (K8s readiness probe)
- **`GET /health/live`** - Liveness check (K8s liveness probe)

**Health Indicators**:
- Database connection pool status
- Message queue connectivity
- Worker availability
- Circuit breaker states
- Resource utilization (memory, connections)

**Response Format**:
```json
{
  "status": "healthy",
  "checks": {
    "database": {
      "status": "healthy",
      "connections_active": 5,
      "connections_idle": 15,
      "connections_max": 20
    },
    "message_queue": {
      "status": "healthy",
      "queues_monitored": 3
    },
    "circuit_breakers": {
      "status": "healthy",
      "open_breakers": 0
    }
  },
  "uptime_seconds": 3600
}
```

---

## Observability Architecture

### Component-Level Instrumentation

```
┌─────────────────────────────────────────────────────────────┐
│                   Observability Stack                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  │
│  │  Metrics │  │   Logs   │  │  Traces  │  │  Health  │  │
│  │ (Counters│  │(Structured)│  │(Planned)│  │  Checks  │  │
│  │Histograms│  │   JSON   │  │  Spans   │  │   HTTP   │  │
│  │  Gauges) │  │   Fields │  │   Tags   │  │  Probes  │  │
│  └─────┬────┘  └─────┬────┘  └─────┬────┘  └─────┬────┘  │
│        │             │             │             │        │
└────────┼─────────────┼─────────────┼─────────────┼────────┘
         │             │             │             │
         ▼             ▼             ▼             ▼
  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐
  │Prometheus │ │  Loki /   │ │  Jaeger / │ │    K8s    │
  │   OTLP    │ │CloudWatch │ │   Tempo   │ │  Probes   │
  └───────────┘ └───────────┘ └───────────┘ └───────────┘
```

### Instrumentation Points

**Orchestration**:
- Task lifecycle transitions
- Step discovery and enqueueing
- Result processing
- Finalization operations
- Database query performance

**Worker**:
- Step claiming
- Handler execution
- Result submission
- FFI call overhead (Ruby workers)
- Event propagation latency

**Database**:
- Query execution times
- Connection pool metrics
- Transaction commit latency
- Buffer cache hit ratio

**Message Queue**:
- Message send/receive latency
- Queue depth
- Notification propagation time
- Message processing errors

---

## Performance Monitoring

### Key Performance Indicators (KPIs)

| Metric | Target | Alert Threshold | Notes |
|--------|--------|-----------------|-------|
| API Response Time (p99) | < 100ms | > 200ms | User-facing latency |
| SQL Function Time (mean) | < 3ms | > 5ms | Orchestration efficiency |
| Event Propagation (p95) | < 10ms | > 20ms | Real-time coordination |
| E2E Task Completion (p99) | < 500ms | > 1000ms | End-user experience |
| Worker Claim Success Rate | > 95% | < 90% | Resource contention |
| Database Connection Pool | < 80% | > 90% | Resource exhaustion |

### Monitoring Dashboards

**Recommended Dashboard Panels**:

1. **Task Throughput**
   - Tasks created/min
   - Tasks completed/min
   - Tasks failed/min
   - Active tasks count

2. **Step Execution**
   - Steps enqueued/min
   - Steps completed/min
   - Average step execution time
   - Step retry rate

3. **System Health**
   - Worker health status
   - Database connection pool utilization
   - Circuit breaker status
   - API response times (p50, p95, p99)

4. **Error Rates**
   - Task failures by namespace
   - Step failures by handler
   - Database errors
   - Message queue errors

---

## Correlation and Debugging

### Correlation ID Propagation

Every request generates a UUIDv7 correlation ID that flows through:
1. API request → Task creation
2. Task → Step enqueueing
3. Step → Worker execution
4. Worker → Result submission
5. Result → Orchestration processing

**Tracing a Request**:
```bash
# Find correlation ID from task creation
curl http://localhost:8080/v1/tasks/{task_uuid} | jq .correlation_id

# Search logs across all services
docker logs orchestration 2>&1 | grep {correlation_id}
docker logs worker 2>&1 | grep {correlation_id}

# Query database for full timeline
psql $DATABASE_URL -c "
  SELECT
    created_at,
    from_state,
    to_state,
    metadata->>'duration_ms' as duration
  FROM tasker.task_transitions
  WHERE metadata->>'correlation_id' = '{correlation_id}'
  ORDER BY created_at;
"
```

### Debug Logging

Enable debug logging for detailed execution flow:
```bash
# Docker Compose
RUST_LOG=debug docker-compose up

# Local development
RUST_LOG=tasker_worker=debug,tasker_orchestration=debug cargo run

# Specific modules
RUST_LOG=tasker_worker::worker::command_processor=trace cargo test
```

---

## Best Practices

### 1. **Structured Logging**
✅ **Do**:
```rust
info!(
    task_uuid = %task.uuid,
    namespace = %task.namespace,
    elapsed_ms = elapsed.as_millis(),
    "Task completed successfully"
);
```

❌ **Don't**:
```rust
info!("Task {} in namespace {} completed in {}ms",
    task.uuid, task.namespace, elapsed.as_millis());
```

### 2. **Metric Naming**
Use consistent, hierarchical naming:
```rust
metrics::counter!("tasker.tasks.created").increment(1);
metrics::counter!("tasker.tasks.completed").increment(1);
metrics::counter!("tasker.tasks.failed").increment(1);
metrics::histogram!("tasker.step.execution_time_ms").record(elapsed);
```

### 3. **Performance Measurement**
Measure at operation boundaries:
```rust
let start = Instant::now();
let result = operation().await?;
let elapsed = start.elapsed();

metrics::histogram!("tasker.operation.duration_ms")
    .record(elapsed.as_millis() as f64);

info!(
    operation = "operation_name",
    elapsed_ms = elapsed.as_millis(),
    success = result.is_ok(),
    "Operation completed"
);
```

### 4. **Error Context**
Include rich context in errors:
```rust
error!(
    task_uuid = %task_uuid,
    step_uuid = %step_uuid,
    error = %err,
    retry_count = retry_count,
    "Step execution failed, will retry"
);
```

---

## Tools and Integration

### Development Tools

**Metrics Visualization**:
```bash
# Prometheus (if configured)
open http://localhost:9090

# Grafana (if configured)
open http://localhost:3000
```

**Log Aggregation**:
```bash
# Docker Compose logs
docker-compose -f docker/docker-compose.test.yml logs -f

# Specific service
docker-compose -f docker/docker-compose.test.yml logs -f orchestration

# JSON parsing
docker-compose logs orchestration | jq 'select(.level == "ERROR")'
```

### Production Tools (Planned)

- **Metrics**: Prometheus + Grafana / DataDog / CloudWatch
- **Logs**: Loki / CloudWatch Logs / Splunk
- **Traces**: Jaeger / Tempo / Honeycomb
- **Alerts**: AlertManager / PagerDuty / Opsgenie

---

## Related Documentation

- **Benchmarks**: [../benchmarks/README.md](../benchmarks/README.md)
- **Architecture**: [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)
- **Race Conditions**: [TAS-29](https://linear.app/tasker-systems/issue/TAS-29)
- **SQL Functions**: [../task-and-step-readiness-and-execution.md](../task-and-step-readiness-and-execution.md)

---

## File Organization

### Current Files

**Active**:
- `metrics-reference.md` - Complete metrics catalog
- `metrics-verification.md` - Verification procedures
- `logging-standards.md` - Logging best practices
- `opentelemetry-improvements.md` - TAS-65 telemetry enhancements
- `VERIFICATION_RESULTS.md` - Test results

**Archived** (superseded by `docs/benchmarks/`):
- `benchmark-implementation-decision.md`
- `benchmark-quick-reference.md`
- `benchmark-strategy-summary.md`
- `benchmarking-guide.md`
- `phase-5.4-distributed-benchmarks-plan.md`

### Recommended Cleanup

Move benchmark files to `docs/archive/` or delete:
```bash
# Option 1: Archive
mkdir -p docs/archive/benchmarks
mv docs/observability/benchmark-*.md docs/archive/benchmarks/
mv docs/observability/phase-5.4-*.md docs/archive/benchmarks/

# Option 2: Delete (information consolidated)
rm docs/observability/benchmark-*.md
rm docs/observability/phase-5.4-*.md
```

---

## Contributing

When adding observability instrumentation:

1. **Follow standards**: Use structured logging and consistent metric naming
2. **Include context**: Add correlation IDs and relevant metadata
3. **Document metrics**: Update metrics-reference.md with new metrics
4. **Test instrumentation**: Verify metrics and logs in development
5. **Consider performance**: Avoid expensive operations in hot paths
