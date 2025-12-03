# TAS-65: Distributed Event System Architecture

**Status**: PLANNING
**Created**: 2025-11-20
**Author**: Platform Team
**Related**: TAS-41 (State Machines), TAS-29 (OpenTelemetry), TAS-47 (Blog Migration)

---

## Executive Summary

This specification defines the distributed event system architecture for tasker-core, establishing a clear separation between **system events** (internal telemetry) and **custom domain events** (developer-facing business logic).

### Problem Statement

The Rails engine (`tasker-engine`) exposed all 56+ lifecycle events to developers via `ActiveSupport::Notifications`. This made sense in a monolithic architecture where orchestration and workers existed in the same application scope. In tasker-core's distributed architecture (multiple orchestration instances, dozens of worker instances across namespaces), exposing system events to developers creates:

1. **Coupling risks**: Business logic depending on internal orchestration events
2. **Contract instability**: System events may change between versions
3. **Performance concerns**: Unnecessary event propagation to workers
4. **Complexity**: Developers needing to understand internal state machine mechanics

### Solution

A two-tier event architecture:

| Tier | Event Type | Audience | Delivery Mechanism | Purpose |
|------|-----------|----------|-------------------|---------|
| **Tier 1** | System Events | Ops/Platform | OpenTelemetry (spans, metrics, logs) | Internal observability |
| **Tier 2** | Custom Domain Events | Developers | PGMQ-backed pub/sub | Business side-effects |

### Non-Goals

- Building an enterprise event sourcing system (use Kafka, EventStoreDB, etc.)
- Event replay or projection capabilities
- Cross-service event mesh or schema registry
- Replacing workflow steps with events

---

## Part 1: System Events Architecture

### 1.1 Existing System Event Constants

Located in `tasker-shared/src/constants.rs`, these 30+ events are **internal only**:

#### Task Lifecycle Events (14 events)
| Event Constant | String Value | Purpose |
|---------------|--------------|---------|
| `TASK_INITIALIZE_REQUESTED` | `task.initialize_requested` | Task creation initiated |
| `TASK_START_REQUESTED` | `task.start_requested` | Task processing begins |
| `TASK_COMPLETED` | `task.completed` | Task successfully finished |
| `TASK_FAILED` | `task.failed` | Task failed permanently |
| `TASK_RETRY_REQUESTED` | `task.retry_requested` | Task queued for retry |
| `TASK_CANCELLED` | `task.cancelled` | Task cancelled by user/system |
| `TASK_RESOLVED_MANUALLY` | `task.resolved_manually` | Manual resolution applied |
| `TASK_BEFORE_TRANSITION` | `task.before_transition` | State transition starting |
| `TASK_STEPS_DISCOVERY_COMPLETED` | `task.steps_discovery_completed` | Ready steps identified |
| `TASK_STEPS_ENQUEUED` | `task.steps_enqueued` | Steps sent to worker queues |
| `TASK_STEP_RESULTS_RECEIVED` | `task.step_results_received` | Worker results received |
| `TASK_AWAITING_DEPENDENCIES` | `task.awaiting_dependencies` | Waiting for deps |
| `TASK_RETRY_BACKOFF_STARTED` | `task.retry_backoff_started` | In exponential backoff |
| `TASK_BLOCKED_BY_FAILURES` | `task.blocked_by_failures` | Blocked by step failures |

#### Step Lifecycle Events (12 events)
| Event Constant | String Value | Purpose |
|---------------|--------------|---------|
| `STEP_INITIALIZE_REQUESTED` | `step.initialize_requested` | Step created |
| `STEP_ENQUEUE_REQUESTED` | `step.enqueue_requested` | Queuing to worker |
| `STEP_EXECUTION_REQUESTED` | `step.execution_requested` | Worker claimed step |
| `STEP_BEFORE_HANDLE` | `step.before_handle` | Handler invocation starting |
| `STEP_HANDLE` | `step.handle` | Handler executing |
| `STEP_COMPLETED` | `step.completed` | Step finished successfully |
| `STEP_FAILED` | `step.failed` | Step failed |
| `STEP_RETRY_REQUESTED` | `step.retry_requested` | Step retry queued |
| `STEP_CANCELLED` | `step.cancelled` | Step cancelled |
| `STEP_RESOLVED_MANUALLY` | `step.resolved_manually` | Manual resolution |
| `STEP_BEFORE_TRANSITION` | `step.before_transition` | State transition starting |
| `STEP_RESULT_SUBMITTED` | `step.result_submitted` | Result sent to orchestration |

#### Workflow Orchestration Events (8 events)
| Event Constant | String Value | Purpose |
|---------------|--------------|---------|
| `WORKFLOW_TASK_STARTED` | `workflow.task_started` | Workflow processing started |
| `WORKFLOW_TASK_COMPLETED` | `workflow.task_completed` | Workflow finished |
| `WORKFLOW_TASK_FAILED` | `workflow.task_failed` | Workflow failed |
| `WORKFLOW_STEP_COMPLETED` | `workflow.step_completed` | Single step done |
| `WORKFLOW_STEP_FAILED` | `workflow.step_failed` | Single step failed |
| `WORKFLOW_VIABLE_STEPS_DISCOVERED` | `workflow.viable_steps_discovered` | Ready steps found |
| `WORKFLOW_NO_VIABLE_STEPS` | `workflow.no_viable_steps` | No steps ready |
| `WORKFLOW_ORCHESTRATION_REQUESTED` | `workflow.orchestration_requested` | Orchestration cycle triggered |

### 1.2 OpenTelemetry Mapping Plan

Each system event should map to the appropriate telemetry primitive:

#### Spans (Distributed Tracing)
```
Task Span: task.orchestration
├── task.initialize_requested     → Span start
├── task.steps_discovery_completed → Span event
├── task.steps_enqueued           → Span event
├── Step Span: step.execution
│   ├── step.execution_requested  → Span start
│   ├── step.before_handle        → Span event
│   ├── step.handle               → Span event
│   ├── step.completed            → Span end (ok)
│   └── step.failed               → Span end (error)
├── task.step_results_received    → Span event
├── task.completed                → Span end (ok)
└── task.failed                   → Span end (error)
```

#### Counters (Metrics)
| System Event | Counter Name | Labels |
|-------------|--------------|--------|
| `task.initialize_requested` | `tasker.tasks.requests.total` | namespace |
| `task.completed` | `tasker.tasks.completions.total` | namespace |
| `task.failed` | `tasker.tasks.failures.total` | namespace, error_code |
| `step.completed` | `tasker.steps.completions.total` | namespace, step_name |
| `step.failed` | `tasker.steps.failures.total` | namespace, step_name, error_type |

#### Gauges (Real-time State)
| System Event | Gauge Name | Purpose |
|-------------|------------|---------|
| `task.awaiting_dependencies` | `tasker.tasks.waiting` | Tasks blocked on deps |
| `task.blocked_by_failures` | `tasker.tasks.blocked` | Tasks blocked by errors |
| `workflow.viable_steps_discovered` | `tasker.steps.ready` | Steps ready to execute |

### 1.3 Current OpenTelemetry Integration

**Location**: `tasker-shared/src/metrics/`

**Existing Modules**:
- `orchestration.rs` - 20+ task/step metrics
- `worker.rs` - Step execution metrics
- `database.rs` - SQL query duration
- `messaging.rs` - PGMQ latency
- `channels.rs` - TAS-51 MPSC saturation

**Configuration** (environment variables):
```bash
TELEMETRY_ENABLED=true
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
OTEL_SERVICE_NAME=tasker-core
DEPLOYMENT_ENVIRONMENT=production
```

### 1.4 Phase 1 Implementation: System Event → Telemetry Audit

**Goal**: Ensure all 34 system events have appropriate telemetry coverage.

**Tasks**:
1. Audit each event constant for existing span/metric coverage
2. Add missing span events for lifecycle transitions
3. Standardize attribute naming (semantic conventions)
4. Remove any developer-facing hooks from system events
5. Document telemetry output for each event

---

## Part 2: Custom Domain Events Architecture

### 2.1 Conceptual Foundation

**Key Distinction**: Workflow Steps vs Custom Event Handlers

| Aspect | Workflow Step | Custom Event Handler |
|--------|--------------|---------------------|
| **Part of DAG?** | Yes - has dependencies | No - side-car pattern |
| **Affects task success?** | Yes - failure = task failure | No - failure is logged, task continues |
| **Blocking?** | Yes - task waits for completion | No - fire-and-forget or async |
| **Retryable?** | Yes - built-in retry system | Optional - depends on delivery mode |
| **State tracked?** | Yes - step state machine | Minimal - delivery status only |

**Use Cases for Custom Events**:
- Slack/Teams notifications on task completion
- PagerDuty alerts on critical failures
- Sentry breadcrumbs during workflow execution
- Datadog custom metrics
- Audit log entries for compliance
- CRM/billing system updates
- Custom analytics pipelines

### 2.2 Event Declaration (YAML Extension)

Extend existing task template schema with explicit `publishes_events` semantics:

```yaml
name: process_payment
namespace_name: payments
version: 1.0.0
steps:
  - name: charge_card
    handler:
      callable: Payments::ChargeCardHandler
    publishes_events:
      - name: payment.charged
        description: "Fired when payment is successfully charged"
        schema:
          type: object
          properties:
            payment_id:
              type: string
            amount:
              type: number
            currency:
              type: string
            customer_id:
              type: string
          required: [payment_id, amount, currency]
        delivery: fire_and_forget  # or: durable, acknowledged

  - name: handle_failure
    handler:
      callable: Payments::HandleFailureHandler
    publishes_events:
      - name: payment.failed
        description: "Fired when payment fails"
        schema:
          type: object
          properties:
            payment_id:
              type: string
            error_code:
              type: string
            error_message:
              type: string
          required: [payment_id, error_code]
        delivery: durable  # Ensure delivery for audit
```

### 2.3 Event Handler Registration (Developer API)

#### Ruby FFI API

```ruby
# In worker initialization or Rails initializer
TaskerCore::Events.subscribe('payment.charged') do |event|
  # event is a hash with :name, :payload, :metadata
  SlackNotifier.send(
    channel: '#payments',
    text: "Payment #{event[:payload][:payment_id]} charged: $#{event[:payload][:amount]}"
  )
end

TaskerCore::Events.subscribe('payment.failed', delivery: :durable) do |event|
  # Durable delivery ensures at-least-once processing
  SentryClient.capture_message(
    "Payment failed: #{event[:payload][:error_code]}",
    extra: event[:payload]
  )
  PagerDutyClient.alert(
    summary: "Payment failure: #{event[:payload][:error_code]}",
    severity: 'high'
  )
end

# Pattern-based subscription
TaskerCore::Events.subscribe('payment.*') do |event|
  AuditLog.record(
    event_name: event[:name],
    payload: event[:payload],
    timestamp: event[:metadata][:fired_at]
  )
end
```

#### Rust Native API (Future)

```rust
// Native event handler registration
EventRegistry::subscribe("payment.charged", |event: DomainEvent| async move {
    // Handle event
    metrics::counter!("custom_events.handled", 1, "event_name" => event.name);
    Ok(())
});
```

### 2.4 Event Publishing (Handler API)

Step handlers can publish events via the sequence object:

```ruby
class ChargeCardHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    result = process_payment(task.context)

    # Publish domain event (non-blocking)
    sequence.publish_event(
      'payment.charged',
      payment_id: result[:payment_id],
      amount: result[:amount],
      currency: result[:currency],
      customer_id: task.context[:customer_id]
    )

    TaskerCore::Types::StepHandlerCallResult.success(result: result)
  end
end
```

### 2.5 Delivery Modes

| Mode | Guarantee | Use Case | Implementation |
|------|-----------|----------|----------------|
| **fire_and_forget** | Best effort | Notifications, metrics | Broadcast channel |
| **durable** | At-least-once | Audit logs, compliance | PGMQ queue |
| **acknowledged** | Exactly-once (future) | Critical integrations | PGMQ + ACK tracking |

**Infrastructure**:
- Fire-and-forget: Use existing `EventPublisher` broadcast channels
- Durable: New PGMQ queue per namespace: `{namespace}_domain_events`
- Dead letter: Failed events go to `{namespace}_domain_events_dlq`

### 2.6 Event Metadata

All domain events include standard metadata:

```json
{
  "name": "payment.charged",
  "payload": {
    "payment_id": "pay_123",
    "amount": 99.99,
    "currency": "USD"
  },
  "metadata": {
    "task_uuid": "019abc...",
    "step_uuid": "019def...",
    "step_name": "charge_card",
    "namespace": "payments",
    "correlation_id": "corr_xyz",
    "fired_at": "2025-11-20T12:00:00Z",
    "fired_by": "worker_001"
  }
}
```

---

## Part 3: Implementation Phases

### Phase 1: System Events → OpenTelemetry (TAS-65a)

**Scope**: Audit and enhance telemetry for all 34 system events

**Tasks**:
1. Create telemetry mapping table (event → span/metric/log)
2. Add missing span events for lifecycle transitions
3. Standardize OpenTelemetry attribute naming
4. Remove any developer-facing hooks from system events
5. Add integration tests for telemetry output

**Files to Modify**:
- `tasker-shared/src/metrics/orchestration.rs`
- `tasker-shared/src/metrics/worker.rs`
- `tasker-orchestration/src/orchestration/` (add span events)
- `tasker-worker/src/worker/` (add span events)

**Deliverables**:
- Telemetry audit document
- Updated metrics with event correlation
- Span hierarchy documentation

### Phase 2: Custom Event Declaration (TAS-65b)

**Scope**: Extend YAML parsing for `publishes_events`

**Tasks**:
1. Update `StepDefinition` struct (field exists, add schema)
2. Add JSON Schema validation for event payload schemas
3. Validate event names at template load time
4. Generate typed event structs from schemas (optional)

**Files to Modify**:
- `tasker-shared/src/models/domain/step_definition.rs`
- `tasker-shared/src/config/templates/` (YAML parsing)

**Deliverables**:
- Updated StepDefinition with event schema
- Event name validation
- Example templates with events

### Phase 3: Event Handler Registration (TAS-65c)

**Scope**: Developer-facing API for event subscription

**Tasks**:
1. Create `EventRegistry` for subscription management
2. Add Ruby FFI API: `TaskerCore::Events.subscribe()`
3. Implement pattern matching for wildcards
4. Integrate with worker startup

**Files to Create**:
- `tasker-shared/src/events/registry.rs`
- `workers/ruby/lib/tasker_core/events.rb`
- `workers/ruby/ext/tasker_core/src/events_ffi.rs`

**Deliverables**:
- EventRegistry implementation
- Ruby FFI bindings
- Example event handlers

### Phase 4: Durable Delivery (TAS-65d)

**Scope**: PGMQ-backed event delivery

**Tasks**:
1. Create domain event queues: `{namespace}_domain_events`
2. Implement event consumer in workers
3. Add dead letter handling
4. Implement retry logic for failed handlers
5. Add metrics for event delivery

**Files to Modify**:
- `tasker-worker/src/worker/` (event consumer)
- SQL migrations for event queues

**Deliverables**:
- Domain event queues
- Event consumer implementation
- DLQ for failed events
- Delivery metrics

---

## Part 4: Architecture Diagrams

### 4.1 System Events Flow (Telemetry Only)

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ORCHESTRATION                               │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │
│  │ Task Init   │───>│ Step Disc   │───>│ Step Enqueue│             │
│  │             │    │             │    │             │             │
│  │ ○ Span      │    │ ○ Span evt  │    │ ○ Span evt  │             │
│  │ ○ Counter   │    │ ○ Gauge     │    │ ○ Counter   │             │
│  └─────────────┘    └─────────────┘    └─────────────┘             │
│         │                                     │                     │
│         │              PGMQ                   │                     │
│         └─────────────────────────────────────┘                     │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                            WORKER                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │
│  │ Step Exec   │───>│ Handler     │───>│ Result Sub  │             │
│  │             │    │             │    │             │             │
│  │ ○ Span      │    │ ○ Span evt  │    │ ○ Counter   │             │
│  │ ○ Counter   │    │ ○ Histogram │    │ ○ Span end  │             │
│  └─────────────┘    └─────────────┘    └─────────────┘             │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │  OTLP Exporter  │
                    │  ────────────── │
                    │  Jaeger/Tempo   │
                    │  Prometheus     │
                    │  Datadog        │
                    └─────────────────┘
```

### 4.2 Custom Domain Events Flow (Developer-Facing)

```
┌─────────────────────────────────────────────────────────────────────┐
│                            WORKER                                   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────┐       │
│  │                  STEP HANDLER                           │       │
│  │  def call(task, sequence, step)                         │       │
│  │    result = process_payment(...)                        │       │
│  │                                                         │       │
│  │    # Publish domain event                               │       │
│  │    sequence.publish_event('payment.charged', ...)  ─────┼───┐   │
│  │                                                         │   │   │
│  │    success(result: result)                              │   │   │
│  │  end                                                    │   │   │
│  └─────────────────────────────────────────────────────────┘   │   │
│                                                                │   │
│  ┌─────────────────────────────────────────────────────────┐   │   │
│  │              EVENT PUBLISHER                            │   │   │
│  │  ┌──────────────────┐    ┌──────────────────┐          │<──┘   │
│  │  │ fire_and_forget  │    │     durable      │          │       │
│  │  │                  │    │                  │          │       │
│  │  │ Broadcast Chan   │    │ PGMQ Queue       │          │       │
│  │  │      │           │    │      │           │          │       │
│  │  └──────┼───────────┘    └──────┼───────────┘          │       │
│  └─────────┼───────────────────────┼──────────────────────┘       │
│            │                       │                               │
└────────────┼───────────────────────┼───────────────────────────────┘
             │                       │
             ▼                       ▼
┌────────────────────────┐  ┌────────────────────────┐
│   REGISTERED HANDLERS  │  │   REGISTERED HANDLERS  │
│                        │  │                        │
│  TaskerCore::Events    │  │  TaskerCore::Events    │
│    .subscribe(...)     │  │    .subscribe(         │
│                        │  │      delivery: :durable│
│  - Slack notification  │  │    )                   │
│  - Metrics push        │  │                        │
│  - Quick logging       │  │  - Audit log           │
│                        │  │  - PagerDuty           │
│  (best effort)         │  │  - Sentry              │
│                        │  │                        │
│                        │  │  (guaranteed delivery) │
└────────────────────────┘  └────────────────────────┘
```

---

## Part 5: Migration from Rails Engine

### 5.1 Concepts That Translate

| Rails Concept | tasker-core Equivalent |
|--------------|------------------------|
| Event naming (`domain.action`) | Same convention |
| Payload schemas | JSON Schema in YAML |
| Correlation IDs | `metadata.correlation_id` |
| Event subscribers | `EventRegistry.subscribe()` |

### 5.2 Concepts That Need Reimagining

| Rails Concept | Why It Changes | tasker-core Approach |
|--------------|----------------|---------------------|
| Synchronous subscribers | Can't block distributed workers | Async event handlers |
| In-process delivery | Multiple workers/orchestrations | PGMQ-based delivery |
| System events visible | Coupling risk in distributed system | Telemetry-only |
| All events same API | Different reliability needs | Delivery modes |

### 5.3 Rails EVENT_SYSTEM.md Concepts

The Rails engine documented several event categories:

1. **Lifecycle Events** → System Events (telemetry-only in tasker-core)
2. **Custom Events** → Domain Events (developer-facing API)
3. **Event Subscribers** → Event Handlers via `EventRegistry`

---

## Part 6: Configuration

### 6.1 TOML Configuration

```toml
# config/tasker/base/events.toml

[events]
# Enable domain event system
enabled = true

# Default delivery mode for events without explicit mode
default_delivery = "fire_and_forget"

[events.fire_and_forget]
# Broadcast channel buffer size
buffer_size = 1000

# Drop events if buffer full (vs block)
overflow_strategy = "drop_oldest"

[events.durable]
# PGMQ queue settings for durable events
visibility_timeout = 30
max_delivery_attempts = 3

# Dead letter queue settings
dlq_enabled = true
dlq_retention_days = 7

[events.metrics]
# Event delivery metrics
enabled = true
histogram_buckets = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
```

### 6.2 Environment Overrides

```toml
# config/tasker/environments/production/events.toml

[events.fire_and_forget]
buffer_size = 10000

[events.durable]
max_delivery_attempts = 5
dlq_retention_days = 30
```

---

## Part 7: Success Criteria

### Phase 1 (System Events → Telemetry)
- [ ] All 34 system events audited and mapped
- [ ] Missing spans/metrics added
- [ ] No developer-facing hooks for system events
- [ ] Telemetry documentation complete

### Phase 2 (Event Declaration)
- [ ] `publishes_events` schema defined
- [ ] YAML validation working
- [ ] Example templates created
- [ ] Schema validation tests passing

### Phase 3 (Handler Registration)
- [ ] `EventRegistry` implemented
- [ ] Ruby FFI API working
- [ ] Pattern matching functional
- [ ] Integration tests passing

### Phase 4 (Durable Delivery)
- [ ] Domain event queues created
- [ ] Event consumer implemented
- [ ] DLQ handling working
- [ ] Metrics exposed

---

## Appendix A: Existing Infrastructure

### A.1 Event Publisher (`tasker-shared/src/events/publisher.rs`)

Already supports:
- FFI bridge for cross-language events
- External callback registration
- Async processing with bounded channels (TAS-51)
- Event correlation tracking

### A.2 StepDefinition (`tasker-shared/src/models/domain/step_definition.rs`)

Already has field:
```rust
pub struct StepDefinition {
    // ...
    pub publishes_events: Vec<String>,  // Exists but minimal
}
```

### A.3 Metrics Module (`tasker-shared/src/metrics/`)

Ready for event metrics:
- Counter, Histogram, Gauge support
- OTLP export configured
- Per-domain metric namespacing

---

## Appendix B: References

- `tasker-shared/src/constants.rs` - System event constants
- `tasker-shared/src/events/` - Event publisher implementation
- `config/system_events.yaml` - Event metadata schemas
- `tasker-shared/src/metrics/` - OpenTelemetry integration
- Rails `tasker-engine/docs/EVENT_SYSTEM.md` - Original patterns
- Rails `tasker-engine/spec/blog/post_05_production_observability/` - Observability examples
