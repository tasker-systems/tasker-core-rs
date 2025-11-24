# TAS-65: Distributed Event System Architecture - Implementation Plan

**Status**: PHASE 1 INFRASTRUCTURE COMPLETE - Worker Instrumentation In Progress
**Branch**: `jcoletaylor/tas-65-distributed-event-system-architecture`
**Last Updated**: 2025-11-24

## Document Structure

This directory contains:
- `plan.md` (this file): Detailed implementation plan with phase breakdowns
- `original-spec.md`: Original specification for reference
- `observability-guidelines.md`: Observability cost analysis and instrumentation guidelines (TAS-29 correlation_id, namespace-aware sampling)
- `observability-config-integration.md`: Configuration system integration notes (component-based TOML, dynamic namespace handling, Task::for_orchestration())
- `phase-*.md`: Phase-specific detailed guides (to be created)

---

## Quick Reference

### Key Design Decisions

1. **PGMQ-Only for Distributed Events**: All durable events use PGMQ queues, eliminating complex FFI bridge patterns
2. **Two-Tier Architecture**: System events → OpenTelemetry (internal), Domain events → PGMQ (developer-facing)
3. **TAS-35 Alignment**: Designed with MessagingService trait for future multi-broker support
4. **Event Source Correction**: Events published from Rust state machine actions, not Ruby sequence objects
5. **Observability Cost Awareness**: Instrumentation only where I/O already exists; atomic counters in hot paths (see `observability-guidelines.md`)

### Phase Overview

| Phase | Duration | Parallel Work | Key Deliverable |
|-------|----------|---------------|-----------------|
| Phase 1 | Week 1 (Days 1-5) | Analysis + Implementation can run in parallel | OpenTelemetry migration complete |
| Phase 1.5 | Days 6-8 | Worker span instrumentation bridges to Phase 2 | Worker step execution spans instrumented |
| Phase 2 | Week 2-3 | Multiple sub-phases can run in parallel | PGMQ-backed domain events working |
| Phase 3 | Week 4 | Design work, can start during Phase 2 | Message broker abstraction layer |
| Phase 4 | Week 5 | Testing can start as phases complete | Production-ready system |

---

## Phase 1: System Events → OpenTelemetry Migration

**Duration**: Week 1 (5 days)
**Dependencies**: None - can start immediately
**Parallel Work**: Analysis and implementation can overlap

### Phase 1.1: Event Usage Audit (Days 1-2)

**Goal**: Complete inventory of current event publication points and usage patterns.

**Sub-Phase 1.1a: Code Analysis (Parallel Work 1)**

Implementation Notes:
```bash
# Find all event publication points
rg "event_publisher\.publish" --type rust
rg "PublishTransitionEventAction" --type rust

# Find state machine transition points
rg "determine_task_event_name" tasker-shared/src/state_machine/
rg "determine_step_event_name" tasker-shared/src/state_machine/

# Find orchestration service event calls
rg "workflow\." tasker-orchestration/src/ --type rust
```

**Deliverable**: Create `docs/ticket-specs/TAS-65/event-audit.md` with:
- Complete list of event publication points
- State transition → event mapping table
- Unused event constants (candidates for removal)

**Testing Checkpoint**: Audit document reviewed and validated against actual codebase

**Sub-Phase 1.1b: Workflow Event Analysis (Parallel Work 2)**

Implementation Notes:
```bash
# Audit workflow orchestration events
rg "WORKFLOW_" tasker-shared/src/constants.rs
rg "workflow\.(task_started|task_completed|viable_steps)" tasker-orchestration/src/
```

**Deliverable**: Add workflow event usage to audit document

**Testing Checkpoint**: All 8 workflow events accounted for (used or marked for removal)

### Phase 1.2: State Machine Action Updates (Days 2-4)

**Goal**: Migrate all system event publication from EventPublisher to OpenTelemetry.

**Sub-Phase 1.2a: Task State Machine Events (Parallel Work 1)**

Implementation Location: `tasker-shared/src/state_machine/actions.rs`

**Step 1**: Update `PublishTransitionEventAction` for tasks

```rust
// BEFORE (current implementation)
impl StateAction<Task> for PublishTransitionEventAction {
    async fn execute(&self, task: &Task, from_state: Option<String>, to_state: String, event: &str, _pool: &PgPool) -> ActionResult<()> {
        let event_name = determine_task_event_name(&from_state, &to_state);
        if let Some(event_name) = event_name {
            self.event_publisher.publish(event_name, context).await?;
        }
        Ok(())
    }
}

// AFTER (OpenTelemetry integration)
impl StateAction<Task> for PublishTransitionEventAction {
    async fn execute(&self, task: &Task, from_state: Option<String>, to_state: String, event: &str, _pool: &PgPool) -> ActionResult<()> {
        use tracing::{event, Level};
        
        // Emit span event for state transition
        event!(
            Level::INFO,
            event_name = determine_task_event_name(&from_state, &to_state).unwrap_or("unknown"),
            task_uuid = %task.task_uuid,
            from_state = from_state.as_deref().unwrap_or("none"),
            to_state = %to_state,
            "Task state transition"
        );
        
        // Update metrics
        self.update_task_metrics(&to_state, task);
        
        Ok(())
    }
}
```

**Step 2**: Expand `determine_task_event_name_from_states()` for all TAS-41 transitions

```rust
fn determine_task_event_name_from_states(from: TaskState, to: TaskState) -> Option<&'static str> {
    use TaskState::*;
    
    match (from, to) {
        // Initial transitions
        (Pending, Initializing) => Some("task.start_requested"),
        
        // TAS-41 state machine paths
        (Initializing, EnqueuingSteps) => Some("task.steps_discovery_completed"),
        (EnqueuingSteps, StepsInProcess) => Some("task.steps_enqueued"),
        (StepsInProcess, EvaluatingResults) => Some("task.step_results_received"),
        
        // Waiting state transitions
        (EvaluatingResults, WaitingForDependencies) => Some("task.awaiting_dependencies"),
        (EvaluatingResults, WaitingForRetry) => Some("task.retry_backoff_started"),
        (EvaluatingResults, BlockedByFailures) => Some("task.blocked_by_failures"),
        
        // Return from waiting states
        (WaitingForDependencies, EnqueuingSteps) => Some("task.dependencies_satisfied"),
        (WaitingForRetry, EnqueuingSteps) => Some("task.retry_requested"),
        
        // Terminal transitions
        (_, Complete) => Some("task.completed"),
        (_, Error) => Some("task.failed"),
        (_, Cancelled) => Some("task.cancelled"),
        (_, ResolvedManually) => Some("task.resolved_manually"),
        
        _ => None,
    }
}
```

**Deliverable**: All 14 task lifecycle events mapped to OpenTelemetry

**Testing Checkpoint**:
```bash
# Verify no task events remain in EventPublisher
cargo test --package tasker-shared state_machine::actions -- --nocapture
# Check that spans are emitted
RUST_LOG=info cargo test task_state_machine | grep "task\."
```

**Sub-Phase 1.2b: Step State Machine Events (Parallel Work 2)**

Implementation Location: `tasker-shared/src/state_machine/actions.rs`

Similar to 1.2a, but for step transitions. Update `determine_step_event_name()`:

```rust
fn determine_step_event_name_from_states(from: WorkflowStepState, to: WorkflowStepState) -> Option<&'static str> {
    use WorkflowStepState::*;
    
    match (from, to) {
        // Pipeline flow
        (Pending, Enqueued) => Some("step.enqueue_requested"),
        (Enqueued, InProgress) => Some("step.execution_requested"),
        (InProgress, EnqueuedForOrchestration) => Some("step.result_submitted"),
        (EnqueuedForOrchestration, Complete) => Some("step.completed"),
        
        // Error handling
        (InProgress, EnqueuedAsErrorForOrchestration) => Some("step.failed"),
        (EnqueuedAsErrorForOrchestration, Error) => Some("step.failed"),
        (Error, Pending) => Some("step.retry_requested"),
        
        // Terminal states
        (_, Complete) => Some("step.completed"),
        (_, Error) => Some("step.failed"),
        (_, Cancelled) => Some("step.cancelled"),
        (_, ResolvedManually) => Some("step.resolved_manually"),
        
        _ => None,
    }
}
```

**Deliverable**: All 12 step lifecycle events mapped to OpenTelemetry

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared step_state_machine -- --nocapture
RUST_LOG=info cargo test step_state_machine | grep "step\."
```

### Phase 1.3: Metrics Implementation (Days 3-5)

**Goal**: Add comprehensive metrics for task and step lifecycles.

**Sub-Phase 1.3a: Task Metrics (Parallel Work 1)**

Implementation Location: `tasker-shared/src/metrics/orchestration.rs`

```rust
// Add to existing metrics module
use metrics::{counter, gauge, histogram};

pub struct TaskMetrics;

impl TaskMetrics {
    pub fn record_state_transition(from_state: &str, to_state: &str, namespace: &str) {
        counter!(
            "tasker.task.transitions.total",
            1,
            "from_state" => from_state,
            "to_state" => to_state,
            "namespace" => namespace
        );
    }
    
    pub fn record_completion(namespace: &str) {
        counter!("tasker.tasks.completions.total", 1, "namespace" => namespace);
    }
    
    pub fn record_failure(namespace: &str, error_code: &str) {
        counter!(
            "tasker.tasks.failures.total",
            1,
            "namespace" => namespace,
            "error_code" => error_code
        );
    }
    
    pub fn update_waiting_gauge(count: i64, reason: &str) {
        gauge!(
            "tasker.tasks.waiting",
            count as f64,
            "reason" => reason
        );
    }
    
    pub fn record_duration(duration_secs: f64, namespace: &str) {
        histogram!(
            "tasker.task.duration.seconds",
            duration_secs,
            "namespace" => namespace
        );
    }
}
```

**Deliverable**: Task metrics module complete

**Testing Checkpoint**:
```bash
# Run with metrics exporter enabled
TELEMETRY_ENABLED=true cargo test orchestration::
# Verify Prometheus endpoint exposes metrics
curl http://localhost:9090/metrics | grep tasker.task
```

**Sub-Phase 1.3b: Step Metrics (Parallel Work 2)**

Implementation Location: `tasker-shared/src/metrics/worker.rs`

Similar structure to task metrics, but for steps:

```rust
pub struct StepMetrics;

impl StepMetrics {
    pub fn record_execution_start(namespace: &str, step_name: &str) {
        counter!(
            "tasker.steps.executions.started.total",
            1,
            "namespace" => namespace,
            "step_name" => step_name
        );
    }
    
    pub fn record_completion(namespace: &str, step_name: &str) {
        counter!(
            "tasker.steps.completions.total",
            1,
            "namespace" => namespace,
            "step_name" => step_name
        );
    }
    
    pub fn record_failure(namespace: &str, step_name: &str, error_type: &str) {
        counter!(
            "tasker.steps.failures.total",
            1,
            "namespace" => namespace,
            "step_name" => step_name,
            "error_type" => error_type
        );
    }
    
    pub fn record_retry(namespace: &str, step_name: &str, attempt: i32) {
        counter!(
            "tasker.steps.retries.total",
            1,
            "namespace" => namespace,
            "step_name" => step_name,
            "attempt" => attempt.to_string()
        );
    }
    
    pub fn record_duration(duration_secs: f64, namespace: &str, step_name: &str) {
        histogram!(
            "tasker.step.duration.seconds",
            duration_secs,
            "namespace" => namespace,
            "step_name" => step_name
        );
    }
}
```

**Deliverable**: Step metrics module complete

**Testing Checkpoint**:
```bash
TELEMETRY_ENABLED=true cargo test worker::
curl http://localhost:9090/metrics | grep tasker.step
```

### Phase 1.4: Span Hierarchy Implementation (Days 4-5)

**Goal**: Create proper parent-child span relationships for distributed tracing.

Implementation Location: Multiple orchestration lifecycle files

**Key Pattern**:
```rust
use tracing::{span, event, Level};

// In TaskInitializer
pub async fn initialize_task(&self, request: TaskRequestMessage) -> TaskerResult<TaskUuid> {
    let task_span = span!(
        Level::INFO,
        "task.orchestration",
        task_uuid = %request.task_uuid,
        namespace = %request.namespace
    );
    
    async move {
        event!(Level::INFO, "task.initialize_requested");
        
        // Initialization logic...
        
        event!(Level::INFO, "task.steps_discovery_completed", count = ready_steps.len());
        
        Ok(task_uuid)
    }
    .instrument(task_span)
    .await
}

// In step execution
pub async fn execute_step(&self, step_uuid: Uuid) -> TaskerResult<StepExecutionResult> {
    let step_span = span!(
        Level::INFO,
        "step.execution",
        step_uuid = %step_uuid,
        parent = &current_task_span  // Link to task span
    );
    
    async move {
        event!(Level::INFO, "step.execution_requested");
        event!(Level::INFO, "step.before_handle");
        
        // Execution logic...
        
        event!(Level::INFO, "step.completed");
        Ok(result)
    }
    .instrument(step_span)
    .await
}
```

**Deliverable**: Complete span hierarchy for task → step relationships

**Testing Checkpoint**:
```bash
# Run with Jaeger collector
docker-compose up -d jaeger
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 cargo test --test integration_tests

# Check Jaeger UI at http://localhost:16686
# Verify span hierarchy shows task.orchestration → step.execution
```

### Phase 1.5: Worker Step Execution Span Instrumentation (Days 6-8)

**Goal**: Add correlation_id, task_uuid, and step_uuid attributes to worker step execution spans for complete distributed tracing.

**Status**: ✅ **INFRASTRUCTURE VALIDATED** - Phase 1 complete, Phase 1.5 identified during testing

#### Background

Phase 1 successfully implemented OpenTelemetry infrastructure across all services:
- ✅ Orchestration spans include correlation_id, task_uuid, step_uuid attributes
- ✅ Grafana LGTM stack capturing 50,000+ traces with indexed correlation IDs
- ✅ Two-phase FFI telemetry pattern working for Ruby workers
- ✅ All 3 services healthy with telemetry enabled

**Discovered Gap** (2025-11-24 validation testing):

Testing with explicit task creation revealed that while infrastructure spans are being created, **worker step execution spans lack domain attributes**:

```bash
# Created task directly with tasker-cli for validation:
Task UUID: 019ab624-8257-700b-a471-9a0b0374f144
Correlation ID: 5ddfd67c-4273-4c07-99fd-11d3d8f84aae

# Tempo query results:
✅ Orchestration spans: Have correlation_id, task_uuid, step_uuid
✅ Worker infrastructure spans: Present (read_messages, reserve_capacity)
❌ Worker step execution spans: Missing correlation_id, task_uuid, step_uuid attributes

# Impact: Cannot link worker execution to orchestration spans in distributed tracing
```

**Why This Matters**:

Phase 1.5 serves two critical purposes:
1. **Immediate**: Complete distributed tracing visibility (orchestration → worker execution flow)
2. **Bridge to Phase 2**: Worker instrumentation patterns will be reused when publishing domain events from step handlers

When we implement Phase 2 domain events, we'll publish events from the same worker step execution contexts. Getting span instrumentation right now ensures domain events will have correct correlation_id and task context.

#### Sub-Phase 1.5a: Rust Worker Span Instrumentation (Days 6-7)

**Implementation Location**: `tasker-worker/src/worker/command_processor.rs`

**Step 1**: Extract execution context attributes

```rust
// In handle_execute_step() method
pub async fn handle_execute_step(
    &self,
    step_uuid: Uuid,
    correlation_id: Option<Uuid>,
) -> TaskerResult<StepExecutionResult> {
    // Fetch step details to get task_uuid, step_name, namespace
    let step = self.fetch_step_details(step_uuid).await?;

    // Create properly attributed span
    let step_span = span!(
        Level::INFO,
        "worker.step_execution",
        correlation_id = %correlation_id.unwrap_or_else(Uuid::nil),
        task_uuid = %step.task_uuid,
        step_uuid = %step_uuid,
        step_name = %step.name,
        namespace = %step.namespace
    );

    async move {
        event!(Level::INFO, "step.execution_started");

        // Handler execution...
        let result = self.execute_handler(&step).await?;

        event!(Level::INFO, "step.execution_completed",
            success = result.is_success(),
            duration_ms = result.duration_ms
        );

        Ok(result)
    }
    .instrument(step_span)
    .await
}
```

**Step 2**: Update handler execution to inherit span context

```rust
// In handler execution (e.g., tasker-worker/src/worker/handler_executor.rs)
async fn execute_rust_handler(
    &self,
    handler: &RustStepHandler,
    context: &StepExecutionContext,
) -> TaskerResult<StepExecutionResult> {
    // Parent span already contains correlation_id, task_uuid, step_uuid
    // All events and child spans will inherit these attributes

    event!(Level::DEBUG, "handler.execution_started",
        handler_type = "rust"
    );

    let result = handler.execute(context).await?;

    event!(Level::DEBUG, "handler.execution_completed");

    Ok(result)
}
```

**Deliverable**: Rust worker step execution spans include all required attributes

**Testing Checkpoint**:
```bash
# Create test task with explicit correlation ID
export CORRELATION_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
./target/release/tasker-cli task create \
  --namespace linear_workflow \
  --name mathematical_sequence \
  --version 1.0.0 \
  --input '{"even_number": 8}' \
  --correlation-id $CORRELATION_ID \
  --format json | tee /tmp/task_result.json

# Extract task UUID
export TASK_UUID=$(jq -r '.task_uuid' /tmp/task_result.json)

# Wait for execution
sleep 5

# Query Tempo for worker spans
curl -s "http://localhost:3200/api/search?tags=task_uuid=${TASK_UUID}" | jq .

# Verify worker.step_execution spans have all attributes:
# ✅ correlation_id
# ✅ task_uuid
# ✅ step_uuid
# ✅ step_name
# ✅ namespace
```

#### Sub-Phase 1.5b: Ruby Worker Span Instrumentation (Days 7-8)

**Implementation Location**: `workers/ruby/lib/tasker_core/step_handler/base.rb`

**Challenge**: Ruby step handlers execute through FFI bridge. Need to:
1. Pass span context across FFI boundary
2. Instrument Ruby execution while preserving parent span relationship

**Step 1**: Pass execution context from Rust to Ruby (FFI boundary)

```rust
// In workers/ruby/ext/tasker_core/src/handler_bridge.rs

pub fn execute_ruby_step_handler(
    step_uuid: Uuid,
    task_uuid: Uuid,
    correlation_id: Uuid,
    step_name: String,
    namespace: String,
    handler_class: String,
    context_json: String,
) -> Result<String, Error> {
    // Current span context is already set up by Rust worker
    // Just need to ensure Ruby handler inherits it

    let span = span!(
        Level::INFO,
        "ruby.handler_execution",
        correlation_id = %correlation_id,
        task_uuid = %task_uuid,
        step_uuid = %step_uuid,
        step_name = %step_name,
        namespace = %namespace,
        handler_class = %handler_class
    );

    let _guard = span.enter();

    // Call Ruby handler
    let result = Ruby::eval(&format!(
        "TaskerCore::HandlerExecutor.execute('{}', '{}')",
        handler_class, context_json
    ))?;

    Ok(result.to_string())
}
```

**Step 2**: Add tracing helpers to Ruby base handler

```ruby
# workers/ruby/lib/tasker_core/step_handler/base.rb

module TaskerCore
  module StepHandler
    class Base
      attr_reader :execution_context

      def initialize(context)
        @execution_context = context
      end

      # Access to execution metadata for tracing
      def correlation_id
        execution_context[:correlation_id]
      end

      def task_uuid
        execution_context[:task_uuid]
      end

      def step_uuid
        execution_context[:step_uuid]
      end

      def step_name
        execution_context[:step_name]
      end

      def namespace
        execution_context[:namespace]
      end

      # Log with span context (delegates to Rust tracing via FFI)
      def log_info(message, **attributes)
        TaskerCore::Internal.emit_event('INFO', message, {
          correlation_id: correlation_id,
          task_uuid: task_uuid,
          step_uuid: step_uuid,
          **attributes
        })
      end

      def log_debug(message, **attributes)
        TaskerCore::Internal.emit_event('DEBUG', message, {
          correlation_id: correlation_id,
          task_uuid: task_uuid,
          step_uuid: step_uuid,
          **attributes
        })
      end

      # Template method - subclasses implement
      def handle
        raise NotImplementedError, "Subclasses must implement #handle"
      end
    end
  end
end
```

**Step 3**: Add FFI helper for Ruby event emission

```rust
// In workers/ruby/ext/tasker_core/src/tracing_ffi.rs

#[magnus::wrap(class = "TaskerCore::Internal")]
struct RubyTracingBridge;

impl RubyTracingBridge {
    fn emit_event(
        level: String,
        message: String,
        attributes: RHash,
    ) -> Result<(), Error> {
        use tracing::{event, Level};

        // Convert Ruby hash to structured attributes
        let correlation_id: String = attributes.get("correlation_id")
            .and_then(|v: Value| v.try_convert::<String>().ok())
            .unwrap_or_default();
        let task_uuid: String = attributes.get("task_uuid")
            .and_then(|v: Value| v.try_convert::<String>().ok())
            .unwrap_or_default();
        let step_uuid: String = attributes.get("step_uuid")
            .and_then(|v: Value| v.try_convert::<String>().ok())
            .unwrap_or_default();

        // Emit event with attributes
        match level.as_str() {
            "INFO" => event!(
                Level::INFO,
                message = %message,
                correlation_id = %correlation_id,
                task_uuid = %task_uuid,
                step_uuid = %step_uuid
            ),
            "DEBUG" => event!(
                Level::DEBUG,
                message = %message,
                correlation_id = %correlation_id,
                task_uuid = %task_uuid,
                step_uuid = %step_uuid
            ),
            _ => {}
        }

        Ok(())
    }
}
```

**Deliverable**: Ruby step handlers can emit events with full span context

**Testing Checkpoint**:
```bash
# Test Ruby worker with linear_workflow template
export CORRELATION_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
./target/release/tasker-cli task create \
  --namespace linear_workflow \
  --name mathematical_sequence \
  --version 1.0.0 \
  --input '{"even_number": 8}' \
  --correlation-id $CORRELATION_ID \
  --format json | tee /tmp/task_result.json

# Query Tempo for Ruby handler spans
export TASK_UUID=$(jq -r '.task_uuid' /tmp/task_result.json)
curl -s "http://localhost:3200/api/search?tags=task_uuid=${TASK_UUID}&tags=handler_class=LinearWorkflow::*" | jq .

# Verify ruby.handler_execution spans exist with attributes
```

#### Shared Patterns for Phase 2 Domain Events

The worker instrumentation patterns established in Phase 1.5 directly enable Phase 2 domain event publication:

**Pattern 1: Execution Context Extraction**
```rust
// Used in Phase 1.5 for spans, will be reused in Phase 2 for events
struct ExecutionContext {
    correlation_id: Uuid,
    task_uuid: Uuid,
    step_uuid: Uuid,
    step_name: String,
    namespace: String,
}

// Phase 1.5: Create span with context
let span = span!(Level::INFO, "worker.step_execution",
    correlation_id = %context.correlation_id,
    task_uuid = %context.task_uuid,
    // ...
);

// Phase 2: Publish domain event with same context
domain_event_publisher.publish_event(
    "step.completed",
    payload,
    EventMetadata {
        correlation_id: context.correlation_id,
        task_uuid: context.task_uuid,
        step_uuid: context.step_uuid,
        namespace: context.namespace,
        // ...
    }
).await?;
```

**Pattern 2: FFI Context Threading**
```rust
// Phase 1.5: Pass context to Ruby for tracing
execute_ruby_step_handler(
    step_uuid, task_uuid, correlation_id,
    step_name, namespace, handler_class, context_json
)?;

// Phase 2: Ruby handlers will use same context for events
# Ruby handler in Phase 2:
class MyHandler < TaskerCore::StepHandler::Base
  def handle
    # Context inherited from FFI boundary
    publish_event('my.custom.event', {
      # correlation_id, task_uuid automatically attached
      # from execution_context
      data: process_data
    })
  end
end
```

**Pattern 3: Observability Integration**
```rust
// Phase 1.5: Span + metrics for step execution
let step_span = span!(Level::INFO, "worker.step_execution", /* ... */);
async move {
    event!(Level::INFO, "step.execution_started");
    // ... handler execution
    metrics::histogram!("step.duration", duration);
}.instrument(step_span).await

// Phase 2: Domain events will be published within same span
async move {
    event!(Level::INFO, "step.execution_started");

    // Handler publishes domain event
    domain_event_publisher.publish(event).await?;

    // Span context automatically propagates to event metadata
    metrics::counter!("domain_events.published");
}.instrument(step_span).await
```

#### Phase 1.5 Success Criteria

- [ ] Rust worker step execution spans include correlation_id, task_uuid, step_uuid, step_name, namespace
- [ ] Ruby worker spans inherit context across FFI boundary
- [ ] Ruby step handlers can emit events with span context via `log_info`/`log_debug`
- [ ] Tempo queries show complete trace hierarchy: orchestration → worker infrastructure → step execution
- [ ] tasker-cli created tasks with explicit correlation IDs are traceable end-to-end
- [ ] Shared patterns documented for Phase 2 domain event publication
- [ ] Zero regressions: existing worker functionality unchanged
- [ ] Documentation: Update `docs/ffi-telemetry-pattern.md` with worker span instrumentation examples

### Phase 1 Success Criteria

- [ ] All 34 system events accounted for (14 task + 12 step + 8 workflow)
- [ ] Zero system events remain in EventPublisher broadcast channels
- [ ] All state transitions emit OpenTelemetry span events
- [ ] Task and step metrics modules complete and tested
- [ ] Jaeger UI shows complete trace hierarchy
- [ ] Prometheus endpoint exposes all defined metrics
- [ ] **Phase 1.5**: Worker step execution spans fully instrumented with correlation context
- [ ] Documentation: `docs/ticket-specs/TAS-65/phase1-telemetry.md` complete

---

## Phase 2: PGMQ-Backed Domain Events

**Duration**: Weeks 2-3 (10 days)
**Dependencies**: Phase 1 + Phase 1.5 complete (worker span instrumentation patterns reused for event publication)
**Parallel Work**: Multiple sub-phases can run independently

**Bridge from Phase 1.5**: Worker execution context extraction and FFI threading patterns established in Phase 1.5 are directly reused for domain event publication. All domain events will be published from the same instrumented worker contexts, ensuring correlation_id and task metadata are automatically propagated.

### Phase 2.1: Domain Event Infrastructure (Days 1-3)

**Goal**: Set up PGMQ queues and message structures for domain events.

**Sub-Phase 2.1a: Queue Management (Parallel Work 1)**

Implementation Location: `tasker-shared/src/events/domain_events.rs`

```rust
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    pub event_id: Uuid,
    pub event_name: String,
    pub event_version: String,
    pub payload: serde_json::Value,
    pub metadata: EventMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub task_uuid: Uuid,
    pub step_uuid: Option<Uuid>,
    pub step_name: Option<String>,
    pub namespace: String,
    pub correlation_id: Uuid,
    pub fired_at: chrono::DateTime<chrono::Utc>,
    pub fired_by: String,
}

pub struct DomainEventQueueManager {
    pool: PgPool,
}

impl DomainEventQueueManager {
    pub async fn create_namespace_queues(&self, namespace: &str) -> Result<()> {
        let queue_name = format!("{}_domain_events", namespace);
        let dlq_name = format!("{}_domain_events_dlq", namespace);
        
        // Create main event queue
        sqlx::query!("SELECT pgmq.create_queue($1)", queue_name)
            .execute(&self.pool)
            .await?;
        
        // Create DLQ
        sqlx::query!("SELECT pgmq.create_queue($1)", dlq_name)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    pub async fn purge_namespace_queues(&self, namespace: &str) -> Result<()> {
        let queue_name = format!("{}_domain_events", namespace);
        sqlx::query!("SELECT pgmq.purge_queue($1)", queue_name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

**Deliverable**: Queue lifecycle management complete

**Testing Checkpoint**:
```bash
# Test queue creation
cargo test --package tasker-shared domain_events::queue_manager -- --nocapture

# Verify queues in database
psql $DATABASE_URL -c "SELECT * FROM pgmq.meta WHERE queue_name LIKE '%_domain_events%'"
```

**Sub-Phase 2.1b: Event Publication API (Parallel Work 2)**

Implementation Location: `tasker-shared/src/events/domain_events.rs`

```rust
pub struct DomainEventPublisher {
    pool: PgPool,
    queue_manager: Arc<DomainEventQueueManager>,
}

impl DomainEventPublisher {
    pub async fn publish_event(
        &self,
        event_name: &str,
        payload: serde_json::Value,
        metadata: EventMetadata,
    ) -> Result<Uuid> {
        let event = DomainEvent {
            event_id: Uuid::now_v7(),
            event_name: event_name.to_string(),
            event_version: "1.0".to_string(),
            payload,
            metadata: metadata.clone(),
        };
        
        let queue_name = format!("{}_domain_events", metadata.namespace);
        let message = serde_json::to_value(&event)?;
        
        // Use PGMQ send with notification
        let msg_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2)",
            queue_name,
            message
        )
        .fetch_one(&self.pool)
        .await?;
        
        // Emit metric
        metrics::counter!(
            "tasker.domain_events.published.total",
            1,
            "event_name" => event_name,
            "namespace" => metadata.namespace.as_str()
        );
        
        Ok(event.event_id)
    }
}
```

**Deliverable**: Event publication API working

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared domain_events::publisher -- --nocapture

# Verify events in PGMQ
psql $DATABASE_URL -c "SELECT * FROM pgmq.{namespace}_domain_events"
```

### Phase 2.2: Event Schema and Validation (Days 2-4)

**Goal**: Extend task template schema with event declarations and add validation.

**Sub-Phase 2.2a: StepDefinition Extension (Parallel Work 1)**

Implementation Location: `tasker-shared/src/models/domain/step_definition.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDeclaration {
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,  // JSON Schema
    pub delivery_mode: EventDeliveryMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDeliveryMode {
    Durable,  // PGMQ with at-least-once delivery
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub handler: StepHandler,
    
    // NEW: Event declarations
    #[serde(default)]
    pub publishes_events: Vec<EventDeclaration>,
    
    // ... existing fields
}
```

**Deliverable**: StepDefinition supports event declarations

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared models::domain::step_definition

# Test YAML parsing
cat > test_template.yaml <<EOF
name: test_task
namespace_name: test
version: 1.0.0
steps:
  - name: test_step
    handler:
      callable: TestHandler
    publishes_events:
      - name: test.event
        description: "Test event"
        schema:
          type: object
          properties:
            value: { type: string }
        delivery_mode: durable
EOF

cargo test template_loading_with_events
```

**Sub-Phase 2.2b: JSON Schema Validation (Parallel Work 2)**

Implementation Location: `tasker-shared/src/events/validation.rs`

```rust
use jsonschema::{JSONSchema, ValidationError};

pub struct EventSchemaValidator {
    schemas: HashMap<String, JSONSchema>,
}

impl EventSchemaValidator {
    pub fn register_event_schema(&mut self, event_name: &str, schema: serde_json::Value) -> Result<()> {
        let compiled = JSONSchema::compile(&schema)
            .map_err(|e| EventError::InvalidSchema(e.to_string()))?;
        self.schemas.insert(event_name.to_string(), compiled);
        Ok(())
    }
    
    pub fn validate_payload(&self, event_name: &str, payload: &serde_json::Value) -> Result<()> {
        let schema = self.schemas.get(event_name)
            .ok_or_else(|| EventError::UnknownEvent(event_name.to_string()))?;
        
        schema.validate(payload)
            .map_err(|errors| EventError::ValidationFailed {
                event_name: event_name.to_string(),
                errors: errors.collect(),
            })?;
        
        Ok(())
    }
}
```

**Deliverable**: Event payload validation working

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared events::validation -- --nocapture
```

### Phase 2.3: Event Consumer Service (Days 4-7)

**Goal**: Implement worker-side event consumer with handler dispatch.

**Sub-Phase 2.3a: Event Registry (Parallel Work 1)**

Implementation Location: `tasker-shared/src/events/registry.rs`

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type EventHandler = Arc<dyn Fn(DomainEvent) -> BoxFuture<'static, Result<()>> + Send + Sync>;

pub struct EventRegistry {
    handlers: Arc<RwLock<HashMap<String, Vec<EventHandler>>>>,
    pattern_handlers: Arc<RwLock<Vec<(String, EventHandler)>>>,
}

impl EventRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            pattern_handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub async fn subscribe<F, Fut>(&self, event_name: &str, handler: F)
    where
        F: Fn(DomainEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let boxed: EventHandler = Arc::new(move |event| Box::pin(handler(event)));
        
        if event_name.contains('*') {
            // Pattern subscription
            let mut patterns = self.pattern_handlers.write().await;
            patterns.push((event_name.to_string(), boxed));
        } else {
            // Exact match subscription
            let mut handlers = self.handlers.write().await;
            handlers.entry(event_name.to_string())
                .or_insert_with(Vec::new)
                .push(boxed);
        }
    }
    
    pub async fn dispatch(&self, event: DomainEvent) -> Result<()> {
        let mut all_handlers = Vec::new();
        
        // Exact match handlers
        if let Some(handlers) = self.handlers.read().await.get(&event.event_name) {
            all_handlers.extend(handlers.iter().cloned());
        }
        
        // Pattern match handlers
        for (pattern, handler) in self.pattern_handlers.read().await.iter() {
            if Self::matches_pattern(&event.event_name, pattern) {
                all_handlers.push(handler.clone());
            }
        }
        
        // Execute all handlers concurrently
        let results = futures::future::join_all(
            all_handlers.into_iter().map(|h| h(event.clone()))
        ).await;
        
        // Collect errors
        let errors: Vec<_> = results.into_iter().filter_map(Result::err).collect();
        if !errors.is_empty() {
            return Err(EventError::HandlersFailed(errors));
        }
        
        Ok(())
    }
    
    fn matches_pattern(event_name: &str, pattern: &str) -> bool {
        // Simple wildcard matching: payment.* matches payment.charged, payment.failed, etc.
        let pattern_parts: Vec<&str> = pattern.split('.').collect();
        let event_parts: Vec<&str> = event_name.split('.').collect();
        
        if pattern_parts.len() > event_parts.len() {
            return false;
        }
        
        pattern_parts.iter().zip(event_parts.iter()).all(|(p, e)| {
            p == &"*" || p == e
        })
    }
}
```

**Deliverable**: Event registry with pattern matching

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared events::registry -- --nocapture
```

**Sub-Phase 2.3b: Event Consumer (Parallel Work 2)**

Implementation Location: `tasker-worker/src/worker/event_consumer.rs`

```rust
use tokio::sync::mpsc;
use tracing::{info, error};

pub struct EventConsumer {
    namespace: String,
    pool: PgPool,
    registry: Arc<EventRegistry>,
    config: EventConsumerConfig,
}

#[derive(Debug, Clone)]
pub struct EventConsumerConfig {
    pub polling_interval_ms: u64,
    pub batch_size: usize,
    pub max_concurrent_handlers: usize,
    pub handler_timeout_seconds: u64,
}

#[derive(Debug, Default)]
pub struct EventConsumerStats {
    pub polling_cycles: AtomicU64,
    pub events_processed: AtomicU64,
    pub handler_failures: AtomicU64,
    pub last_poll_at: Arc<Mutex<Option<Instant>>>,
}

impl EventConsumer {
    pub async fn start(self: Arc<Self>) -> Result<()> {
        let queue_name = format!("{}_domain_events", self.namespace);
        let mut interval = tokio::time::interval(Duration::from_millis(self.config.polling_interval_ms));
        
        loop {
            interval.tick().await;
            
            // OBSERVABILITY NOTE: Use atomic counters in polling loop, not metrics
            // See docs/ticket-specs/TAS-65/observability-guidelines.md
            self.stats.polling_cycles.fetch_add(1, Ordering::Relaxed);
            *self.stats.last_poll_at.lock().await = Some(Instant::now());
            
            debug!("Polling domain events queue");
            
            // Poll for events
            let events = self.fetch_events(&queue_name).await?;
            
            if events.is_empty() {
                continue;  // No work, no metrics
            }
            
            // Only log when we have work
            info!(
                count = events.len(),
                "Processing domain events batch"
            );
            
            // Process events concurrently with semaphore for backpressure
            let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_handlers));
            let mut tasks = Vec::new();
            
            for (msg_id, event) in events {
                let permit = semaphore.clone().acquire_owned().await?;
                let consumer = self.clone();
                
                tasks.push(tokio::spawn(async move {
                    let _permit = permit;  // Hold permit until done
                    consumer.process_event(msg_id, event).await
                }));
            }
            
            // Wait for all handlers to complete
            let results = futures::future::join_all(tasks).await;
            
            // Log failures (already sent to DLQ)
            for result in results {
                if let Err(e) = result {
                    error!("Event processing task failed: {}", e);
                }
            }
        }
    }
    
    async fn fetch_events(&self, queue_name: &str) -> Result<Vec<(i64, DomainEvent)>> {
        let records = sqlx::query!(
            r#"
            SELECT msg_id, message 
            FROM pgmq.read($1, $2, $3)
            "#,
            queue_name,
            self.config.handler_timeout_seconds as i32,
            self.config.batch_size as i32
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut events = Vec::new();
        for record in records {
            let event: DomainEvent = serde_json::from_value(record.message)?;
            events.push((record.msg_id, event));
        }
        
        Ok(events)
    }
    
    async fn process_event(&self, msg_id: i64, event: DomainEvent) -> Result<()> {
        let start = std::time::Instant::now();
        let event_name = event.event_name.clone();
        
        // Dispatch to registered handlers
        let result = tokio::time::timeout(
            Duration::from_secs(self.config.handler_timeout_seconds),
            self.registry.dispatch(event.clone())
        ).await;
        
        match result {
            Ok(Ok(())) => {
                // Success - delete message
                self.delete_message(msg_id).await?;
                
                metrics::histogram!(
                    "tasker.domain_events.handler_duration.seconds",
                    start.elapsed().as_secs_f64(),
                    "event_name" => event_name.as_str(),
                    "status" => "success"
                );
            }
            Ok(Err(e)) => {
                // Handler failed - send to DLQ
                error!("Event handler failed: {}", e);
                self.send_to_dlq(msg_id, event, e.to_string()).await?;
                
                metrics::counter!(
                    "tasker.domain_events.handler_failures.total",
                    1,
                    "event_name" => event_name.as_str()
                );
            }
            Err(_timeout) => {
                // Timeout - send to DLQ
                error!("Event handler timed out");
                self.send_to_dlq(msg_id, event, "Handler timeout".to_string()).await?;
                
                metrics::counter!(
                    "tasker.domain_events.handler_timeouts.total",
                    1,
                    "event_name" => event_name.as_str()
                );
            }
        }
        
        Ok(())
    }
    
    async fn send_to_dlq(&self, msg_id: i64, event: DomainEvent, reason: String) -> Result<()> {
        let dlq_name = format!("{}_domain_events_dlq", self.namespace);
        let dlq_message = serde_json::json!({
            "original_event": event,
            "failure_reason": reason,
            "failed_at": chrono::Utc::now(),
            "original_msg_id": msg_id
        });
        
        sqlx::query!("SELECT pgmq.send($1, $2)", dlq_name, dlq_message)
            .execute(&self.pool)
            .await?;
        
        // Delete from main queue
        self.delete_message(msg_id).await?;
        
        Ok(())
    }
    
    async fn delete_message(&self, msg_id: i64) -> Result<()> {
        let queue_name = format!("{}_domain_events", self.namespace);
        sqlx::query!("SELECT pgmq.delete($1, $2)", queue_name, msg_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

**Deliverable**: Event consumer with retry and DLQ support

**Observability Note**: The event consumer uses atomic counters in the polling loop (not OpenTelemetry metrics) to minimize overhead. Metrics are only emitted when events are processed, and detailed metrics are governed by namespace-aware sampling. See `observability-guidelines.md` for rationale and configuration.

**Testing Checkpoint**:
```bash
cargo test --package tasker-worker event_consumer -- --nocapture

# Integration test with real PGMQ
DATABASE_URL=postgresql://localhost/tasker_test cargo test event_consumer_integration
```

### Phase 2.4: Ruby FFI Integration (Days 6-10)

**Goal**: Expose event publication and subscription APIs to Ruby step handlers.

**Sub-Phase 2.4a: FFI Bindings (Parallel Work 1)**

Implementation Location: `workers/ruby/ext/tasker_core/src/events_ffi.rs`

```rust
use magnus::{Error, RHash, Symbol, Value};
use std::sync::Arc;

#[magnus::wrap(class = "TaskerCore::EventPublisher")]
struct RubyEventPublisher {
    inner: Arc<DomainEventPublisher>,
}

impl RubyEventPublisher {
    fn publish_event(
        &self,
        event_name: String,
        payload: RHash,
    ) -> Result<String, Error> {
        let payload_value: serde_json::Value = ruby_hash_to_json(payload)?;
        
        // Get metadata from execution context
        let metadata = get_current_execution_metadata()?;
        
        let event_id = tokio::runtime::Handle::current()
            .block_on(self.inner.publish_event(&event_name, payload_value, metadata))
            .map_err(|e| Error::new(magnus::exception::runtime_error(), e.to_string()))?;
        
        Ok(event_id.to_string())
    }
}

#[magnus::init]
fn init() -> Result<(), Error> {
    let module = magnus::define_module("TaskerCore")?;
    let class = module.define_class("EventPublisher", magnus::class::object())?;
    class.define_singleton_method("publish_event", function!(RubyEventPublisher::publish_event, 2))?;
    Ok(())
}
```

**Deliverable**: Rust→Ruby FFI bridge for event publication

**Testing Checkpoint**:
```bash
cd workers/ruby
bundle exec rake compile
bundle exec rspec spec/ffi/events_ffi_spec.rb
```

**Sub-Phase 2.4b: Ruby API Layer (Parallel Work 2)**

Implementation Location: `workers/ruby/lib/tasker_core/events.rb`

```ruby
module TaskerCore
  module Events
    class << self
      def subscribe(event_pattern, &block)
        # Register with Rust EventRegistry via FFI
        TaskerCore::Internal.register_event_handler(event_pattern, block)
      end
      
      def publish_event(event_name, payload)
        # Publish via FFI
        TaskerCore::EventPublisher.publish_event(event_name, payload)
      end
    end
  end
end

# Add to step handler base class
module TaskerCore
  module StepHandler
    class Base
      def publish_event(event_name, **payload)
        TaskerCore::Events.publish_event(event_name, payload)
      end
    end
  end
end
```

**Deliverable**: Ruby API for event publication and subscription

**Testing Checkpoint**:
```bash
bundle exec rspec spec/events_spec.rb --format documentation
```

### Phase 2 Success Criteria

- [ ] Domain event queues created per namespace with DLQ
- [ ] Event publication API working in Rust
- [ ] JSON Schema validation enforced
- [ ] Event consumer processes events with retry logic
- [ ] Event registry supports pattern matching
- [ ] Ruby FFI bindings complete and tested
- [ ] Ruby step handlers can publish and subscribe to events
- [ ] Integration tests show end-to-end event flow
- [ ] Documentation: `docs/ticket-specs/TAS-65/phase2-domain-events.md` complete

---

## Phase 3: Message Broker Abstraction

**Duration**: Week 4 (5 days)
**Dependencies**: Phase 2 structure in place
**Parallel Work**: Design and implementation can overlap with Phase 2

### Phase 3.1: Abstraction Layer Design (Days 1-2)

**Goal**: Design EventMessagingService trait for multi-broker support.

Implementation Location: `tasker-shared/src/events/messaging_service.rs`

```rust
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait EventMessagingService: Send + Sync {
    /// Publish an event to the appropriate queue/topic
    async fn publish_event(
        &self,
        namespace: &str,
        event: &DomainEvent,
    ) -> Result<String, EventError>;
    
    /// Subscribe to events matching a pattern
    async fn subscribe_pattern(
        &self,
        namespace: &str,
        pattern: &str,
        handler: EventHandler,
    ) -> Result<(), EventError>;
    
    /// Start consuming events for a namespace
    async fn start_consumer(
        &self,
        namespace: &str,
        config: EventConsumerConfig,
    ) -> Result<(), EventError>;
    
    /// Create event queues for a namespace
    async fn create_queues(&self, namespace: &str) -> Result<(), EventError>;
    
    /// Purge events from a namespace
    async fn purge_queues(&self, namespace: &str) -> Result<(), EventError>;
    
    /// Get queue statistics
    async fn get_stats(&self, namespace: &str) -> Result<EventQueueStats, EventError>;
}

pub struct EventRoutingConfig {
    pub default_provider: MessageServiceProvider,
    pub routes: Vec<EventRoute>,
}

pub struct EventRoute {
    pub pattern: String,
    pub provider: MessageServiceProvider,
    pub config: ProviderConfig,
}

pub enum MessageServiceProvider {
    Pgmq,
    RabbitMq,
    Sns,
    Kafka,
}
```

**Deliverable**: Trait design with routing configuration

**Testing Checkpoint**: Design review document created

### Phase 3.2: PGMQ Provider Implementation (Days 2-4)

**Goal**: Implement EventMessagingService for PGMQ.

```rust
pub struct PgmqEventMessagingService {
    pool: PgPool,
    queue_manager: Arc<DomainEventQueueManager>,
}

#[async_trait]
impl EventMessagingService for PgmqEventMessagingService {
    async fn publish_event(&self, namespace: &str, event: &DomainEvent) -> Result<String, EventError> {
        let queue_name = format!("{}_domain_events", namespace);
        let message = serde_json::to_value(event)?;
        
        let msg_id = sqlx::query_scalar!(
            "SELECT pgmq_send_with_notify($1, $2)",
            queue_name,
            message
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(msg_id.to_string())
    }
    
    // ... implement other trait methods
}
```

**Deliverable**: Full PGMQ implementation

**Testing Checkpoint**:
```bash
cargo test --package tasker-shared messaging_service::pgmq
```

### Phase 3.3: Configuration System (Days 3-5)

**Goal**: Add configuration for event routing and provider selection.

Implementation Location: `config/tasker/base/events.toml`

```toml
[events]
enabled = true

[events.domain]
default_provider = "pgmq"
queue_pattern = "{namespace}_domain_events"
dlq_pattern = "{namespace}_domain_events_dlq"

# Optional: Route specific events to different providers
[[events.domain.routes]]
pattern = "payment.*"
provider = "pgmq"

[[events.domain.routes]]
pattern = "analytics.*"
provider = "kafka"  # Future
```

**Deliverable**: Configuration system with provider selection

**Testing Checkpoint**:
```bash
cargo test config::events
```

### Phase 3 Success Criteria

- [ ] EventMessagingService trait defined
- [ ] PGMQ provider fully implemented
- [ ] Configuration system supports provider selection
- [ ] Routing config allows pattern-based provider selection
- [ ] Future provider stubs (RabbitMQ, SNS) documented
- [ ] Zero breaking changes to Phase 2 implementation
- [ ] Documentation: `docs/ticket-specs/TAS-65/phase3-abstraction.md` complete

---

## Phase 4: Testing and Documentation

**Duration**: Week 5 (5 days)
**Dependencies**: Phases 1-3 complete
**Parallel Work**: Multiple test suites can run in parallel

### Phase 4.1: Integration Tests (Days 1-3)

**Goal**: Comprehensive end-to-end testing.

Test Files to Create:
- `tests/integration/system_events_telemetry_test.rs`
- `tests/integration/domain_events_test.rs`
- `tests/integration/event_consumer_test.rs`
- `workers/ruby/spec/integration/domain_events_spec.rb`

**Sub-Phase 4.1a: System Events Testing (Parallel Work 1)**

```rust
#[tokio::test]
async fn test_task_lifecycle_spans() {
    // Initialize telemetry
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(TestSpanCollector::new());
    
    tracing::subscriber::set_global_default(subscriber).unwrap();
    
    // Execute task lifecycle
    let task = create_test_task().await;
    task_state_machine.transition(TaskEvent::Start).await.unwrap();
    
    // Verify spans were emitted
    let spans = TestSpanCollector::get_spans();
    assert!(spans.iter().any(|s| s.name == "task.orchestration"));
    assert!(spans.iter().any(|s| s.metadata.contains_key("task.start_requested")));
}
```

**Sub-Phase 4.1b: Domain Events Testing (Parallel Work 2)**

```rust
#[tokio::test]
async fn test_event_publication_and_consumption() {
    let namespace = "test_namespace";
    
    // Set up event consumer
    let registry = Arc::new(EventRegistry::new());
    let received_events = Arc::new(Mutex::new(Vec::new()));
    let captured = received_events.clone();
    
    registry.subscribe("payment.charged", move |event| {
        let captured = captured.clone();
        async move {
            captured.lock().await.push(event);
            Ok(())
        }
    }).await;
    
    // Publish event
    let publisher = DomainEventPublisher::new(pool);
    publisher.publish_event("payment.charged", json!({ "amount": 100 }), metadata).await?;
    
    // Wait for consumption
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Verify event received
    assert_eq!(received_events.lock().await.len(), 1);
}
```

**Sub-Phase 4.1c: Ruby Integration Testing (Parallel Work 3)**

```ruby
RSpec.describe "Domain Events", :integration do
  it "publishes and consumes events" do
    received_events = []
    
    # Subscribe to events
    TaskerCore::Events.subscribe('test.event') do |event|
      received_events << event
    end
    
    # Publish event from step handler
    handler = TestStepHandler.new
    handler.publish_event('test.event', value: 'test')
    
    # Wait for processing
    sleep 1
    
    expect(received_events.size).to eq(1)
    expect(received_events.first[:payload][:value]).to eq('test')
  end
end
```

**Testing Checkpoint**:
```bash
# Run all integration tests
cargo test --test integration_tests -- --test-threads=1
cd workers/ruby && bundle exec rspec spec/integration/
```

### Phase 4.2: Performance Testing (Days 2-4)

**Goal**: Benchmark event throughput and latency.

```rust
#[tokio::test]
#[ignore]  // Run separately with: cargo test --test perf_tests -- --ignored
async fn bench_event_throughput() {
    let publisher = DomainEventPublisher::new(pool);
    let start = Instant::now();
    let num_events = 10_000;
    
    for i in 0..num_events {
        publisher.publish_event(
            "bench.test",
            json!({ "index": i }),
            metadata.clone()
        ).await?;
    }
    
    let duration = start.elapsed();
    let throughput = num_events as f64 / duration.as_secs_f64();
    
    println!("Throughput: {:.0} events/sec", throughput);
    assert!(throughput > 1000.0, "Expected >1000 events/sec");
}
```

**Testing Checkpoint**: Performance report with benchmarks

### Phase 4.3: Documentation (Days 3-5)

**Goal**: Complete developer and operator documentation.

Documents to Create:
1. `docs/architecture-decisions/TAS-65-distributed-events.md` - ADR
2. `docs/development/domain-events-guide.md` - Developer guide
3. `docs/operations/event-monitoring.md` - Operator guide
4. `docs/ticket-specs/TAS-65/migration-guide.md` - Rails→Rust migration

**Testing Checkpoint**: All documentation reviewed and approved

### Phase 4 Success Criteria

- [ ] Integration tests >95% coverage
- [ ] Performance benchmarks meet targets (>1000 events/sec)
- [ ] Failure mode testing complete
- [ ] Documentation complete and reviewed
- [ ] Zero regressions in existing functionality
- [ ] Production deployment guide ready

---

## Appendix A: Parallel Work Opportunities

### Phase 1 Parallelization
- **Team A**: Event audit + Task state machine updates
- **Team B**: Workflow event audit + Step state machine updates  
- **Team C**: Metrics implementation + Span hierarchy

### Phase 2 Parallelization
- **Team A**: Queue management + Event publication API
- **Team B**: Schema validation + Event registry
- **Team C**: Event consumer + Ruby FFI
- **Team D**: Ruby API layer + Integration tests

### Phase 3 Parallelization
- **Team A**: Trait design + PGMQ implementation
- **Team B**: Configuration system + Future provider stubs

### Phase 4 Parallelization
- **Team A**: System events testing
- **Team B**: Domain events testing
- **Team C**: Ruby integration testing
- **Team D**: Documentation

---

## Appendix B: Testing Checkpoints

After each sub-phase, run the relevant test command to verify completion:

```bash
# Phase 1 checkpoints
cargo test --package tasker-shared state_machine::actions
cargo test --package tasker-shared metrics::
TELEMETRY_ENABLED=true cargo test orchestration::

# Phase 2 checkpoints
cargo test --package tasker-shared domain_events::
cargo test --package tasker-worker event_consumer::
cd workers/ruby && bundle exec rspec spec/integration/

# Phase 3 checkpoints
cargo test --package tasker-shared messaging_service::
cargo test config::events

# Phase 4 checkpoints
cargo test --test integration_tests -- --test-threads=1
cargo test --test perf_tests -- --ignored
```

---

## Appendix C: Observability Cost Awareness

**Critical Guideline**: See `observability-guidelines.md` for detailed instrumentation decisions.

**Hot Path Rules**:
1. **Polling loops**: Atomic counters only, no metrics/spans
2. **State transitions**: Full instrumentation (I/O already present)
3. **Event processing**: Full instrumentation (handler execution is expensive)
4. **Registry lookups**: No instrumentation (pure computation)

**Cardinality & Sampling Guidelines**:
- Task/step state metrics: Low cardinality (12 states) ✅
- Event name metrics: Medium cardinality (developer-defined) ⚠️ Document concerns; aggregate by namespace
- Step name metrics: High cardinality (100s-1000s) ❌ Use namespace-aware sampling from telemetry config

**Correlation IDs (TAS-29)**:
- Use existing `task.correlation_id` and optional `task.parent_correlation_id` in spans and logs
- Do not generate new correlation IDs inside spans or handlers
- Thread correlation_id across FFI boundaries (Ruby) and domain events

---

## Appendix D: Key Implementation Patterns

### Pattern 1: OpenTelemetry Span Events
```rust
use tracing::{span, event, Level};

let span = span!(Level::INFO, "operation", task_uuid = %uuid);
async move {
    event!(Level::INFO, "step_completed", step_name = %name);
    // ... work
}.instrument(span).await
```

### Pattern 2: PGMQ Event Publication
```rust
let msg_id = sqlx::query_scalar!(
    "SELECT pgmq_send_with_notify($1, $2)",
    queue_name,
    serde_json::to_value(&event)?
).fetch_one(&pool).await?;
```

### Pattern 3: Event Registry Subscription
```rust
registry.subscribe("payment.*", |event| async move {
    handle_payment_event(event).await
}).await;
```

### Pattern 4: FFI Bridge
```rust
#[magnus::wrap(class = "TaskerCore::EventPublisher")]
struct RubyEventPublisher {
    inner: Arc<DomainEventPublisher>,
}
```

---

**End of Plan Document**

For phase-specific detailed guides, see:
- `phase1-telemetry-detail.md` (to be created)
- `phase2-domain-events-detail.md` (to be created)
- `phase3-abstraction-detail.md` (to be created)
- `phase4-testing-detail.md` (to be created)
