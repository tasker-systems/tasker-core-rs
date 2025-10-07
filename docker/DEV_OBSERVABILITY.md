# Development Environment with Observability (TAS-29 Phase 3)

This guide explains how to use the development Docker Compose setup with full observability stack for distributed tracing.

## Quick Start

### 1. Start the Stack

```bash
# From repository root
docker-compose -f docker/docker-compose.dev.yml up -d

# Check status
docker-compose -f docker/docker-compose.dev.yml ps

# View logs
docker-compose -f docker/docker-compose.dev.yml logs -f observability
```

### 2. Access Services

| Service | URL | Credentials |
|---------|-----|-------------|
| Grafana UI | http://localhost:3000 | admin/admin |
| PostgreSQL | localhost:5432 | tasker/tasker |
| OTLP gRPC | localhost:4317 | - |
| OTLP HTTP | localhost:4318 | - |
| Prometheus | http://localhost:9090 | - |
| Tempo | http://localhost:3200 | - |

### 3. Configure Tasker for OpenTelemetry

Create a `.env.dev` file in the repository root:

```bash
# Database
DATABASE_URL=postgresql://tasker:tasker@localhost:5432/tasker_rust_development

# OpenTelemetry (TAS-29 Phase 3)
TELEMETRY_ENABLED=true
OTEL_SERVICE_NAME=tasker-orchestration
OTEL_SERVICE_VERSION=0.1.0
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_TRACES_SAMPLER_ARG=1.0
DEPLOYMENT_ENVIRONMENT=development

# Logging
RUST_LOG=info
RUST_BACKTRACE=1
```

### 4. Run Tasker Services

```bash
# Load environment
source .env.dev

# Run database migrations
cargo sqlx migrate run

# Start orchestration service
cargo run --bin tasker-server --all-features

# Or start worker
cargo run --package tasker-worker --all-features
```

## Viewing Distributed Traces in Grafana

### 1. Access Grafana

1. Open http://localhost:3000
2. Login with `admin/admin` (you'll be prompted to change password)
3. Navigate to **Explore** in the left sidebar

### 2. Query Traces by correlation_id

In the Grafana Explore view:

1. Select **Tempo** as the data source
2. Switch to **TraceQL** query mode
3. Query by correlation_id:

```traceql
{correlation_id="550e8400-e29b-41d4-a716-446655440000"}
```

Or search all traces with correlation_id:

```traceql
{span.correlation_id != nil}
```

### 3. View Trace Timeline

- Click on any trace to see the full span timeline
- Spans are hierarchical showing the full request flow:
  - Task initialization
  - Step enqueueing
  - Worker execution
  - Result processing
  - Task finalization

### 4. Example Queries

**Find slow operations:**
```traceql
{duration > 1s}
```

**Find operations with errors:**
```traceql
{status = error}
```

**Find traces for specific task:**
```traceql
{task_uuid="550e8400-e29b-41d4-a716-446655440000"}
```

**Find traces for specific namespace:**
```traceql
{namespace="fulfillment"}
```

## Understanding the Trace Flow

A complete task execution will show these spans:

```
1. create_and_enqueue_task_from_request (task_initializer)
   ├── 2. enqueue_ready_steps (step_enqueuer)
   │   ├── 3. filter_and_prepare_enqueuable_steps
   │   └── 4. enqueue_steps_to_worker_queue
   ├── 5. execute_step (worker command_processor)
   │   ├── 6. try_claim_step (step_claim)
   │   └── 7. handler_execution (Ruby/Rust handler)
   ├── 8. handle_step_execution_result (result_processor)
   │   └── 9. process_orchestration_metadata
   └── 10. finalize_task (task_finalizer)
       └── 11. make_finalization_decision
```

Each span includes:
- **correlation_id**: End-to-end request tracking
- **task_uuid**: Task identifier
- **step_uuid**: Step identifier (where applicable)
- **namespace**: Task namespace
- **Duration**: Execution time
- **Status**: success/error

## Grafana Dashboards

### Creating a Custom Dashboard

1. Go to **Dashboards** → **New** → **New Dashboard**
2. Add panels for:
   - **Trace Rate**: Number of traces per second
   - **P95 Latency**: 95th percentile request duration
   - **Error Rate**: Percentage of failed spans
   - **correlation_id Timeline**: Traces grouped by correlation_id

### Pre-configured Tempo Dashboard

The LGTM stack includes a default Tempo dashboard showing:
- Total spans
- Span rate
- Error rate
- Duration histograms

## Logs Integration (Loki)

Logs are automatically collected by Loki and can be correlated with traces:

1. In Grafana **Explore**, select **Loki** data source
2. Query logs by correlation_id:

```logql
{container="tasker-orchestration"} |= "correlation_id"
```

3. Click "Show in Tempo" next to any log line to jump to the trace

## Stopping the Stack

```bash
# Stop services
docker-compose -f docker/docker-compose.dev.yml down

# Stop and remove volumes (clean slate)
docker-compose -f docker/docker-compose.dev.yml down -v
```

## Troubleshooting

### OTLP Connection Refused

If you see connection errors:

```
Error: Failed to connect to OTLP endpoint
```

**Solution**: Ensure the observability stack is running:
```bash
docker-compose -f docker/docker-compose.dev.yml ps observability
```

### No Traces Visible

1. Check `TELEMETRY_ENABLED=true` is set
2. Verify OTLP endpoint: `echo $OTEL_EXPORTER_OTLP_ENDPOINT`
3. Check service logs: `cargo run ... 2>&1 | grep -i "telemetry\|opentelemetry"`

### Grafana Password Reset

```bash
docker-compose -f docker/docker-compose.dev.yml exec observability \
  grafana-cli admin reset-admin-password newpassword
```

## Performance Considerations

- **Sampling**: Default is 100% (1.0) for development
- **Batch Size**: OTLP exporter batches spans for efficiency
- **Retention**: Tempo retains traces for 24 hours by default

To reduce overhead in development:

```bash
# Sample only 10% of traces
export OTEL_TRACES_SAMPLER_ARG=0.1
```

## Advanced Configuration

### Custom Tempo Configuration

Create `docker/config/tempo.yaml` and mount it:

```yaml
# In docker-compose.dev.yml
observability:
  volumes:
    - ./config/tempo.yaml:/etc/tempo/config.yaml:ro
```

### Backend-Specific Setup

See `config/tasker/base/telemetry.toml` for commented examples of:
- Honeycomb configuration
- Jaeger configuration
- Grafana Cloud Tempo configuration

## Next Steps

1. **Create custom dashboards** for your workflows
2. **Set up alerts** for error rates and latency
3. **Export traces** for analysis
4. **Integrate with CI/CD** for performance regression testing

## References

- [TAS-29 Phase 3 Implementation](../tasker-shared/src/logging.rs)
- [Grafana Tempo Documentation](https://grafana.com/docs/tempo/latest/)
- [OpenTelemetry Rust SDK](https://docs.rs/opentelemetry/)
- [TraceQL Query Language](https://grafana.com/docs/tempo/latest/traceql/)
