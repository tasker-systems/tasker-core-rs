# TAS-65 Phase 1.4: OpenTelemetry Span Hierarchy

**Status**: Implementation In Progress
**Last Updated**: 2025-01-22

## Overview

Phase 1.4 implements OpenTelemetry span instrumentation to create distributed traces for complete task orchestration visibility. This enables Jaeger UI visualization of task and step execution flows with TAS-29 correlation_id propagation.

## Span Hierarchy Design

```
task.orchestration (correlation_id = task.correlation_id)
├── task.state_transition (from_state, to_state)
│   ├── step.discovery (if Initializing → EnqueuingSteps)
│   ├── step.enqueuing (if EnqueuingSteps → StepsInProcess)
│   └── result.evaluation (if StepsInProcess → EvaluatingResults)
└── step.execution (step_uuid, task_uuid)
    ├── step.state_transition (from_state, to_state)
    ├── handler.execution (handler_name, namespace)
    └── result.submission (to orchestration queue)
```

## Implementation Strategy

### 1. State Machine Action Spans

**File**: `tasker-shared/src/state_machine/actions.rs`

Add span creation to `PublishTransitionEventAction`:

```rust
use opentelemetry::trace::{Span, Tracer};
use opentelemetry::{global, KeyValue};

impl StateAction<Task> for PublishTransitionEventAction {
    async fn execute(&self, task: &Task, from_state: Option<String>, to_state: String, ...) {
        let tracer = global::tracer("tasker-state-machine");
        let mut span = tracer
            .span_builder("task.state_transition")
            .with_attributes(vec![
                KeyValue::new("correlation_id", task.correlation_id.to_string()),
                KeyValue::new("task_uuid", task.task_uuid.to_string()),
                KeyValue::new("from_state", from_state.as_deref().unwrap_or("unknown")),
                KeyValue::new("to_state", to_state.clone()),
            ])
            .start(&tracer);
        
        // Existing event publishing logic...
        
        span.end();
    }
}
```

### 2. Orchestration Service Spans

**Files to Instrument**:
- `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs`
- `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs`
- `tasker-orchestration/src/orchestration/lifecycle/result_processing/message_handler.rs`
- `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer_services/service.rs`

**Pattern**:
```rust
#[tracing::instrument(
    skip(self),
    fields(
        correlation_id = %task.correlation_id,
        task_uuid = %task.task_uuid
    )
)]
pub async fn create_task_from_request(&self, task_request: TaskRequest) -> Result<...> {
    // Use tracing macros - OpenTelemetry layer converts to spans
    tracing::info!("Task initialization started");
    // ... logic ...
}
```

### 3. Worker Service Spans

**Files to Instrument**:
- `tasker-worker/src/worker/command_processor.rs`
- `tasker-worker/src/worker/handlers/*`

**Pattern**:
```rust
#[tracing::instrument(
    skip(self, step),
    fields(
        task_uuid = %step.task_uuid,
        step_uuid = %step.workflow_step_uuid,
        handler_name = %step.name
    )
)]
pub async fn execute_step(&self, step: WorkflowStep) -> Result<...> {
    tracing::info!("Step execution started");
    // ... handler invocation ...
}
```

## Span Attributes

### Task Orchestration Spans

**Required Attributes**:
- `correlation_id` (UUID) - TAS-29 correlation ID
- `task_uuid` (UUID) - Task identifier
- `namespace` (string) - Task namespace

**Optional Attributes**:
- `parent_correlation_id` (UUID) - For nested workflows
- `task_name` (string) - Handler name
- `priority` (int) - Task priority

### Step Execution Spans

**Required Attributes**:
- `task_uuid` (UUID) - Parent task identifier
- `step_uuid` (UUID) - Step identifier
- `handler_name` (string) - Step handler name
- `namespace` (string) - Worker namespace

**Optional Attributes**:
- `attempt_number` (int) - Retry attempt
- `max_attempts` (int) - Retry limit

## Integration with Existing Infrastructure

### Tracing Layer Configuration

The system already uses `tracing` macros throughout. We leverage the OpenTelemetry tracing layer to convert `tracing` spans to OpenTelemetry spans:

```rust
// Already configured in SystemContext initialization
use opentelemetry_sdk::trace::TracerProvider;
use tracing_subscriber::{layer::SubscriberExt, Registry};

let tracer_provider = TracerProvider::builder()
    .with_batch_exporter(...)
    .build();

let telemetry = tracing_opentelemetry::layer()
    .with_tracer(tracer_provider.tracer("tasker"));

let subscriber = Registry::default()
    .with(telemetry)
    .with(tracing_subscriber::fmt::layer());

tracing::subscriber::set_global_default(subscriber)?;
```

### Correlation ID Propagation

TAS-29 correlation_id is already tracked:
- `Task.correlation_id` (UUID) - Set at task creation
- `Task.parent_correlation_id` (Option<UUID>) - For nested workflows

These are automatically included in span attributes via `#[tracing::instrument]` field mappings.

## Observability Cost Analysis

### Span Creation Overhead

**Hot Paths** (< 1ms overhead acceptable):
- Task state transitions: ~10-50 per task
- Step state transitions: ~100-1000 per task (depends on step count)

**Strategy**: Use sampling for high-volume spans
- Task spans: 100% sampling (low volume, high value)
- Step spans: Namespace-aware sampling (from observability config)

### Span Storage

**Jaeger Backend**:
- Spans stored in-memory or persistent storage (Cassandra/Elasticsearch)
- Retention policy configured at backend level
- Typical: 7-30 days for debugging workflows

**Cost Mitigation**:
- Sample high-frequency spans
- Short span lifetime (seconds to minutes)
- Minimal attribute cardinality

## Success Criteria

✅ **Phase 1.4 Complete When**:
1. All state machine actions emit spans with correlation_id
2. Orchestration lifecycle services instrumented
3. Worker step execution instrumented
4. Jaeger UI shows complete task → step trace hierarchy
5. Spans include all required attributes
6. Zero breaking changes (all tests pass)

## Testing Strategy

### Unit Tests
```rust
#[tokio::test]
async fn test_task_state_transition_span_attributes() {
    // Verify span includes correlation_id, from_state, to_state
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_end_to_end_trace_hierarchy() {
    // Create task, execute steps, verify Jaeger span hierarchy
}
```

### Manual Verification
1. Start Jaeger: `docker run -p 16686:16686 -p 4318:4318 jaegertracing/all-in-one`
2. Run integration test with OpenTelemetry exporter enabled
3. Open Jaeger UI: `http://localhost:16686`
4. Search for trace by correlation_id
5. Verify span hierarchy matches design

## Implementation Notes

### Existing Tracing Infrastructure

The codebase already uses:
- `#[tracing::instrument]` throughout orchestration services
- `tracing::info!`, `tracing::error!` for logging
- Structured field extraction via `fields()` parameter

**Action**: Minimal changes needed - just ensure correlation_id is in field mappings.

### State Machine Integration

State machines pass string-based state parameters. For Phase 1.4:
- Continue using existing event publishing
- Add span creation alongside event publishing
- Use `correlation_id` from Task/WorkflowStep entities

### Future Enhancements (Phase 2)

Phase 2 will add domain events to PGMQ:
- Spans for PGMQ message publishing
- Trace context propagation in PGMQ messages
- Cross-service trace correlation

## References

- TAS-29: correlation_id specification
- TAS-65 Plan: `docs/ticket-specs/TAS-65/plan.md`
- TAS-65 Observability Guidelines: `docs/ticket-specs/TAS-65/observability-guidelines.md`
- OpenTelemetry Tracing Spec: https://opentelemetry.io/docs/specs/otel/trace/api/
