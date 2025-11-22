# TAS-65 Event Audit: System Event Publication Analysis

**Last Updated**: 2025-01-22
**Status**: Phase 1.1 Complete (Analysis Complete)
**Purpose**: Comprehensive audit of all system events, mapping to state transitions, and identifying gaps in current event publication

## Executive Summary

This audit identifies all 34 system events defined in the tasker-core system and maps them to actual state transitions and publication points. Analysis shows that **only 13 of 34 events (38%) are currently published**, leaving 21 events (62%) unpublished.

### Event Categories
- **Task Lifecycle Events**: 14 events (6 published, 8 unpublished)
- **Step Lifecycle Events**: 12 events (7 published, 5 unpublished)
- **Workflow Orchestration Events**: 8 events (1 published, 7 unpublished)

**Note**: Step events overcounted initially - 7 events currently published including internal transitions

### Key Findings
1. State machine actions only publish a subset of lifecycle events
2. Workflow orchestration events have minimal publication infrastructure
3. No correlation_id integration in current event publishing
4. Event publishing uses in-process EventPublisher broadcast (Phase 2 will migrate to PGMQ)

---

## Task Lifecycle Events (14 Total)

### Currently Published (5 events)

#### ✅ task.started
- **Constant**: `events::TASK_START_REQUESTED` (misnomer - should be `TASK_STARTED`)
- **Mapping Function**: `determine_task_event_name()` line 850
- **State Transition**: Any → `in_progress`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:259`
- **Notes**: Uses legacy string-based mapping, not TaskState enum

#### ✅ task.completed
- **Constant**: `events::TASK_COMPLETED`
- **Mapping Function**: `determine_task_event_name()` line 851
- **State Transition**: Any → `complete`
- **Publication Points**:
  1. `PublishTransitionEventAction` in `actions.rs:259`
  2. `EventPublisher::publish_task_completed()` in `orchestration/lifecycle/task_finalization/event_publisher.rs:52`
- **Notes**: Dual publication - both state machine action and finalization service

#### ✅ task.failed
- **Constant**: `events::TASK_FAILED`
- **Mapping Function**: `determine_task_event_name()` line 852
- **State Transition**: Any → `error`
- **Publication Points**:
  1. `PublishTransitionEventAction` in `actions.rs:259`
  2. `EventPublisher::publish_task_failed()` in `orchestration/lifecycle/task_finalization/event_publisher.rs:98`
- **Notes**: Dual publication - both state machine action and finalization service

#### ✅ task.cancelled
- **Constant**: `events::TASK_CANCELLED`
- **Mapping Function**: `determine_task_event_name()` line 853
- **State Transition**: Any → `cancelled`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:259`

#### ✅ task.resolved_manually
- **Constant**: `events::TASK_RESOLVED_MANUALLY`
- **Mapping Function**: `determine_task_event_name()` line 854
- **State Transition**: Any → `resolved_manually`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:259`

#### ✅ task.reset (legacy)
- **Constant**: Not defined in constants.rs (legacy only)
- **Mapping Function**: `determine_task_event_name()` line 855
- **State Transition**: `error` → `pending`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:259`
- **Notes**: Legacy error recovery, not in TAS-41 enhanced state machine

### Not Currently Published (9 events)

#### ❌ task.initialize_requested
- **Constant**: `events::TASK_INITIALIZE_REQUESTED`
- **State Transition**: `Pending` → `Initializing`
- **Should Publish At**: Task initialization start
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ task.retry_requested
- **Constant**: `events::TASK_RETRY_REQUESTED`
- **State Transition**: `Error` → `Pending` (modern retry pattern)
- **Should Publish At**: Task retry initiation
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ task.before_transition
- **Constant**: `events::TASK_BEFORE_TRANSITION`
- **State Transition**: Before any state transition
- **Should Publish At**: State machine pre-transition hook
- **Missing From**: State machine actions
- **Phase 1 Action**: Add pre-transition hook in state machine

### TAS-41 Orchestration Lifecycle Events (Not Published)

#### ❌ task.steps_discovery_completed
- **Constant**: `events::TASK_STEPS_DISCOVERY_COMPLETED`
- **State Transition**: `Initializing` → `EnqueuingSteps` or `WaitingForDependencies`
- **Should Publish At**: After step discovery completes
- **Missing From**: Initialization services
- **Phase 1 Action**: Add to TaskInitializer or state machine action

#### ❌ task.steps_enqueued
- **Constant**: `events::TASK_STEPS_ENQUEUED`
- **State Transition**: `EnqueuingSteps` → `StepsInProcess`
- **Should Publish At**: After steps successfully enqueued to worker queues
- **Missing From**: StepEnqueuerActor/services
- **Phase 1 Action**: Add to step enqueuer completion

#### ❌ task.step_results_received
- **Constant**: `events::TASK_STEP_RESULTS_RECEIVED`
- **State Transition**: `StepsInProcess` → `EvaluatingResults`
- **Should Publish At**: When orchestration receives step results
- **Missing From**: ResultProcessorActor
- **Phase 1 Action**: Add to result processor

#### ❌ task.awaiting_dependencies
- **Constant**: `events::TASK_AWAITING_DEPENDENCIES`
- **State Transition**: Any → `WaitingForDependencies`
- **Should Publish At**: When task enters waiting state
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ task.retry_backoff_started
- **Constant**: `events::TASK_RETRY_BACKOFF_STARTED`
- **State Transition**: Any → `WaitingForRetry`
- **Should Publish At**: When task enters retry waiting
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ task.blocked_by_failures
- **Constant**: `events::TASK_BLOCKED_BY_FAILURES`
- **State Transition**: Any → `BlockedByFailures`
- **Should Publish At**: When task becomes blocked
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ task.dependencies_satisfied
- **Constant**: `events::TASK_DEPENDENCIES_SATISFIED`
- **State Transition**: `WaitingForDependencies` → `EvaluatingResults`
- **Should Publish At**: When dependencies become ready
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

---

## Step Lifecycle Events (12 Total)

### Currently Published (4 events)

#### ✅ step.started
- **Constant**: Not in constants (mapped as "step.started")
- **Mapping Function**: `determine_step_event_name()` line 862
- **State Transition**: Any → `in_progress`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`
- **Notes**: Uses legacy string-based mapping, not WorkflowStepState enum

#### ✅ step.completed
- **Constant**: `events::STEP_COMPLETED`
- **Mapping Function**: `determine_step_event_name()` line 864
- **State Transition**: Any → `complete`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`

#### ✅ step.failed
- **Constant**: `events::STEP_FAILED`
- **Mapping Function**: `determine_step_event_name()` line 865
- **State Transition**: Any → `error`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`

#### ✅ step.cancelled
- **Constant**: `events::STEP_CANCELLED`
- **Mapping Function**: `determine_step_event_name()` line 866
- **State Transition**: Any → `cancelled`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`

#### ✅ step.resolved_manually
- **Constant**: `events::STEP_RESOLVED_MANUALLY`
- **Mapping Function**: `determine_step_event_name()` line 867
- **State Transition**: Any → `resolved_manually`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`

#### ✅ step.enqueued_for_orchestration
- **Constant**: Not in constants (internal transition)
- **Mapping Function**: `determine_step_event_name()` line 863
- **State Transition**: Any → `enqueued_for_orchestration`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`
- **Notes**: Internal coordination event between worker and orchestration

#### ✅ step.retried
- **Constant**: Not in constants (legacy mapping)
- **Mapping Function**: `determine_step_event_name()` line 868
- **State Transition**: `error` → `pending`
- **Publication Point**: `PublishTransitionEventAction` in `actions.rs:290`
- **Notes**: Legacy error recovery, conflicts with TAS-42 `WaitingForRetry` semantics

### Not Currently Published (8 events)

#### ❌ step.initialize_requested
- **Constant**: `events::STEP_INITIALIZE_REQUESTED`
- **State Transition**: Creation → `Pending`
- **Should Publish At**: Step initialization
- **Missing From**: Step creation services
- **Phase 1 Action**: Add to step initialization

#### ❌ step.enqueue_requested
- **Constant**: `events::STEP_ENQUEUE_REQUESTED`
- **State Transition**: `Pending` → `Enqueued`
- **Should Publish At**: Before enqueueing to worker queue
- **Missing From**: Step enqueuer services
- **Phase 1 Action**: Add to StepEnqueuerActor

#### ❌ step.execution_requested
- **Constant**: `events::STEP_EXECUTION_REQUESTED`
- **State Transition**: `Enqueued` → `InProgress` (worker claims)
- **Should Publish At**: Worker claims step from queue
- **Missing From**: Worker command processor
- **Phase 1 Action**: Add to worker step execution

#### ❌ step.before_handle
- **Constant**: `events::STEP_BEFORE_HANDLE`
- **State Transition**: Before handler execution
- **Should Publish At**: Worker pre-handler hook
- **Missing From**: Worker Ruby/Rust handler integration
- **Phase 1 Action**: Add to FFI handler wrappers

#### ❌ step.handle
- **Constant**: `events::STEP_HANDLE`
- **State Transition**: During handler execution
- **Should Publish At**: Handler execution start
- **Missing From**: Worker Ruby/Rust handler integration
- **Phase 1 Action**: Add to FFI handler wrappers

#### ❌ step.retry_requested
- **Constant**: `events::STEP_RETRY_REQUESTED`
- **State Transition**: `Error` or `WaitingForRetry` → `Pending`
- **Should Publish At**: Retry initiation
- **Missing From**: State machine actions
- **Phase 1 Action**: Add to `PublishTransitionEventAction`

#### ❌ step.before_transition
- **Constant**: `events::STEP_BEFORE_TRANSITION`
- **State Transition**: Before any state transition
- **Should Publish At**: State machine pre-transition hook
- **Missing From**: State machine actions
- **Phase 1 Action**: Add pre-transition hook in state machine

#### ❌ step.result_submitted
- **Constant**: `events::STEP_RESULT_SUBMITTED`
- **State Transition**: Worker submits result to orchestration
- **Should Publish At**: Result submission to orchestration_step_results queue
- **Missing From**: Worker result submission
- **Phase 1 Action**: Add to worker result processor

---

## Workflow Orchestration Events (8 Total)

### Currently Published (1 event)

#### ✅ workflow.viable_steps_discovered (via OrchestrationEvent)
- **Constant**: `constants::workflow_events::VIABLE_STEPS_DISCOVERED`
- **Publication Point**: `ViableStepDiscovery::find_viable_steps()` in `viable_step_discovery.rs:142`
- **Publication Method**: `EventPublisher::publish_viable_steps_discovered()` (line 270 in publisher.rs)
- **Event Type**: `OrchestrationEvent::ViableStepsDiscovered` (structured event)
- **Notes**: Published to in-process broadcast channel, includes full step details

### Not Currently Published (7 events)

#### ❌ workflow.task_started
- **Constant**: `events::WORKFLOW_TASK_STARTED`
- **State Transition**: `Pending` → `Initializing` (task orchestration begins)
- **Should Publish At**: `TaskInitializer::create_task_from_request()` completion
- **File**: `tasker-orchestration/src/orchestration/lifecycle/task_initialization/service.rs`
- **Notes**: TODO comment exists at line 349: "Implement event publishing once EventPublisher interface is finalized"
- **Phase 1 Action**: Implement `publish_task_initialized()` method and call from task creation flow

#### ❌ workflow.task_completed
- **Constant**: `events::WORKFLOW_TASK_COMPLETED`
- **State Transition**: Any → `Complete` (task orchestration completes successfully)
- **Should Publish At**: `TaskFinalizer::finalize_task()` after successful completion
- **File**: `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs`
- **Existing**: Generic "task.completed" event published (line 52 in event_publisher.rs)
- **Phase 1 Action**: Add workflow-level event alongside state machine event

#### ❌ workflow.task_failed
- **Constant**: `events::WORKFLOW_TASK_FAILED`
- **State Transition**: Any → `Error` (task orchestration fails permanently)
- **Should Publish At**: `TaskFinalizer::finalize_task()` after failure determination
- **File**: `tasker-orchestration/src/orchestration/lifecycle/task_finalization/service.rs`
- **Existing**: Generic "task.failed" event published (line 98 in event_publisher.rs)
- **Phase 1 Action**: Add workflow-level event alongside state machine event

#### ❌ workflow.step_completed
- **Constant**: `events::WORKFLOW_STEP_COMPLETED`
- **State Transition**: Step orchestration processing completes
- **Should Publish At**: `OrchestrationResultProcessor::process_step_result()` after successful step processing
- **File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/message_handler.rs`
- **Phase 1 Action**: Add to result processor workflow event publishing

#### ❌ workflow.step_failed
- **Constant**: `events::WORKFLOW_STEP_FAILED`
- **State Transition**: Step orchestration processing fails
- **Should Publish At**: `OrchestrationResultProcessor::process_step_result()` after step failure processing
- **File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/message_handler.rs`
- **Phase 1 Action**: Add to result processor workflow event publishing

#### ❌ workflow.no_viable_steps
- **Constant**: `events::WORKFLOW_NO_VIABLE_STEPS`
- **State Transition**: Step discovery finds zero ready steps
- **Should Publish At**: `ViableStepDiscovery::find_viable_steps()` when `viable_steps.is_empty()`
- **File**: `tasker-orchestration/src/orchestration/viable_step_discovery.rs:146`
- **Phase 1 Action**: Add conditional event publishing after step discovery

#### ❌ workflow.orchestration_requested
- **Constant**: `events::WORKFLOW_ORCHESTRATION_REQUESTED`
- **State Transition**: Orchestration command received
- **Should Publish At**: `OrchestrationProcessor::handle_command()` at command receipt
- **File**: `tasker-orchestration/src/orchestration/command_processor.rs`
- **Phase 1 Action**: Add to command processor event publishing

---

## Event Publication Infrastructure

### Current Implementation (In-Process Broadcast)

**EventPublisher** (`tasker-shared/src/events/publisher.rs`):
- In-process broadcast channels
- Subscribers listen via `subscribe()` method
- Used across FFI boundaries (Ruby/Rust)
- **Phase 2 will replace with PGMQ**

### State Machine Event Actions

**PublishTransitionEventAction** (`tasker-shared/src/state_machine/actions.rs:239-309`):
- Implements `StateAction<Task>` and `StateAction<WorkflowStep>`
- Uses helper functions for event name mapping:
  - `determine_task_event_name()` (line 848-858)
  - `determine_step_event_name()` (line 860-871)
- Publishes to EventPublisher broadcast channels
- **Issue**: Uses legacy string-based state matching, not TaskState/WorkflowStepState enums

### Task Finalization Event Publishing

**EventPublisher** (`orchestration/lifecycle/task_finalization/event_publisher.rs`):
- Specialized for task completion/failure events
- Dual publication strategy:
  1. Structured `OrchestrationEvent` via `publish_event()`
  2. Generic JSON event via `publish()`
- Only handles terminal state events

---

## Phase 1 Implementation Plan

### Phase 1.2a: Update Task State Machine Events

**File**: `tasker-shared/src/state_machine/actions.rs`

1. **Replace `determine_task_event_name()` with enum-based mapping**:
   ```rust
   fn determine_task_event_name_from_states(from: TaskState, to: TaskState) -> Option<&'static str> {
       use TaskState::*;
       match (from, to) {
           (Pending, Initializing) => Some("task.initialize_requested"),
           (Initializing, EnqueuingSteps) => Some("task.steps_discovery_completed"),
           (EnqueuingSteps, StepsInProcess) => Some("task.steps_enqueued"),
           (StepsInProcess, EvaluatingResults) => Some("task.step_results_received"),
           (_, WaitingForDependencies) => Some("task.awaiting_dependencies"),
           (WaitingForDependencies, EvaluatingResults) => Some("task.dependencies_satisfied"),
           (_, WaitingForRetry) => Some("task.retry_backoff_started"),
           (_, BlockedByFailures) => Some("task.blocked_by_failures"),
           (_, Complete) => Some("task.completed"),
           (_, Error) => Some("task.failed"),
           (_, Cancelled) => Some("task.cancelled"),
           (_, ResolvedManually) => Some("task.resolved_manually"),
           (Error, Pending) => Some("task.retry_requested"),
           _ => None,
       }
   }
   ```

2. **Update `PublishTransitionEventAction` to use TaskState enum**:
   - Change signature to accept `TaskState` instead of `Option<String>`
   - Use new enum-based mapping function
   - Add correlation_id to event context

3. **Add pre-transition hook for `task.before_transition`**:
   - Add to `TaskStateMachine::transition()` before state change
   - Include correlation_id in event context

### Phase 1.2b: Update Step State Machine Events

**File**: `tasker-shared/src/state_machine/actions.rs`

1. **Replace `determine_step_event_name()` with enum-based mapping**:
   ```rust
   fn determine_step_event_name_from_states(from: WorkflowStepState, to: WorkflowStepState) -> Option<&'static str> {
       use WorkflowStepState::*;
       match (from, to) {
           (Pending, Enqueued) => Some("step.enqueue_requested"),
           (Enqueued, InProgress) => Some("step.execution_requested"),
           (_, InProgress) => Some("step.handle"),
           (_, EnqueuedForOrchestration) => Some("step.enqueued_for_orchestration"),
           (_, Complete) => Some("step.completed"),
           (_, Error) => Some("step.failed"),
           (_, Cancelled) => Some("step.cancelled"),
           (_, ResolvedManually) => Some("step.resolved_manually"),
           (Error | WaitingForRetry, Pending) => Some("step.retry_requested"),
           _ => None,
       }
   }
   ```

2. **Update `PublishTransitionEventAction` to use WorkflowStepState enum**

3. **Add pre-transition hook for `step.before_transition`**

### Phase 1.3: Implement OpenTelemetry Metrics

**Task Metrics** (`tasker-shared/src/metrics/orchestration.rs`):
- `task_state_transitions_total` - Counter by from_state, to_state, namespace
- `task_state_duration_seconds` - Histogram by state, namespace
- `task_completion_duration_seconds` - Histogram by namespace, outcome

**Step Metrics** (`tasker-shared/src/metrics/worker.rs`):
- `step_state_transitions_total` - Counter by from_state, to_state, namespace
- `step_execution_duration_seconds` - Histogram by handler_name, namespace, outcome
- `step_attempts_total` - Counter by handler_name, namespace, outcome

### Phase 1.4: Implement Span Hierarchy

**Trace Hierarchy**:
```
task_orchestration (correlation_id = task.correlation_id)
├── task_state_transition (from_state, to_state)
│   ├── step_discovery
│   ├── step_enqueuing
│   └── result_evaluation
└── step_execution (step_uuid)
    ├── step_state_transition (from_state, to_state)
    ├── handler_execution (handler_name)
    └── result_submission
```

---

## Testing Strategy

### Unit Tests
- Test event name mapping functions with all state combinations
- Test correlation_id propagation in event contexts
- Test OpenTelemetry span attributes

### Integration Tests
- Test complete task lifecycle publishes all 14 task events
- Test complete step lifecycle publishes all 12 step events
- Test workflow orchestration publishes all 8 workflow events
- Test Jaeger UI shows complete trace hierarchy

### Manual Verification
- Run sample workflows and inspect Jaeger traces
- Verify Prometheus metrics endpoint exposes all counters/histograms
- Verify event payloads include correlation_id

---

## Success Criteria

✅ **Phase 1 Complete When**:
1. All 34 system events (14 task + 12 step + 8 workflow) have publication points
2. Zero system events remain in EventPublisher broadcast channels (Phase 1 migrates to OpenTelemetry spans only)
3. All state transitions emit OpenTelemetry span events with correlation_id
4. Task and step metrics modules complete and tested
5. Jaeger UI shows complete trace hierarchy
6. Prometheus endpoint exposes all defined metrics
7. All existing tests pass (zero breaking changes)

---

## Phase 2 Preview: PGMQ Domain Events

Phase 2 will introduce PGMQ-backed domain events (developer-facing) for business logic integration:
- New PGMQ queues: `domain_events_{namespace}`
- Domain event types: `TaskInitiated`, `StepCompleted`, `WorkflowProgressed`
- Consumer API for subscribing to domain events
- Namespace-based routing and filtering

**Phase 1 focuses on internal observability (OpenTelemetry)**
**Phase 2 focuses on external integration (PGMQ domain events)**
