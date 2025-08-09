# TAS-32

In the current system, the workflow is this:

* The tasker core orchestration loop identifies ready tasks and makes claims against them to enqueue ready steps
* It transitions these steps to in progress and enqueues them
* Our workers take step handling requests from the namespaced worker queues, and determine if they can handle the steps
* If they can handle them, they delete the message from the queue, and process the steps, and send back results to the result processing queue
* The orchestration system polls for workflow step results on the results processing queue
* When the orchestration system receives results, it persists those results, transitions steps appropriately, and evaluates the task finalization workflow

There is an architectural gap that I have introduced here. In my goal to make workers as independent as possible, I have introduced the following situation:

* The orchestration system uses the state manager to transition steps into an in progress state before they are actually in progress. They are actually enqueued. In the migration to a distributed system, we introduced a new state for steps that did not exist before, but we didn't model it explicitly
* This means that we use the presence or absence of a message on the queue as the source of truth about whether a step is in a safe state to process
* If there is a failure to delete a message from the queue, more than one worker could pick up the step and process it
* It also means that we have to be very aggressive with deleting a message from the queue for step processing before we have actually processed it, because if our handler does not process the step within the visibility window of the message and we haven't preemptively deleted it, another worker could pick the message up from the queue

This is a violation of step level idempotency, and it means we are working against the strengths of the queue rather than with it.

What I have realized is that the orchestration core needs transition the steps to an enqueued status rather than in progress during our orchestration loop lifecycle when it takes a task claim.

Our workers now have new responsibilities as well:

* When a new namespace queue has a ready step, the worker needs to make use of our state machine logic to evaluate the step
* If the step is in an enqueued status, then we transition the step to in_progress when the worker claims the step for processing.
* We may want to add metadata to the transition to indicate some kind of worker uuid and timestamp not different from the task claim core id and timestamp, though in this case it is less of a claim than an indication that an ephemeral worker transitioned state to in progress last and when
* The worker can then remove the message from the queue because it is now the owner of processing the step
* The worker can then find the step's handler and run it's call methods just like it does today
* When the step is processed, success or failure, we can use the same result struct that we do today to save the results. If the step handler was successful and did not have an error, the results can be persisted to the workflow steps results column with the correct struct, and the step can be transitioned from in_progress to complete
* If the step has failed, then the handler needs to persist the error results struct to the step results, and transition the step to a new state machine state for workflow step that we would define called  RetryableError or PermanentError - the step handler logic would need to know what it wants to return based on domain logic
* Then the worker will send a message back to the "results" queue to indicate that the step has been handled, but it does not need to send the serialized results anymore, it simply needs to send a queue message to the results queue for the orchestration core to indicate that the step has been handled

Then we re-enter the orchestration core's results processing loop. It will be different now because it will no longer be responsible for persisting the step results. It will need to evaluate the step.

* If the step is complete, then we enter the standard task finalizer workflow.
* If it is not complete, then the orchestration core needs to evaluate the state, RetryableError vs PermanentError.
* PermanentError puts the whole task into a failed state and we transition the task itself into a failed state and that exempts it from any further task claim.
* RetryableError needs to be evaluated for the number of attempts (+1 from the recent handler failure) against the retryable limit, and if it is retryable, we need to investigate the step result struct metadata for information that would be fed into our backoff analysis logic
* Once we determine if the step is retryable and we understand how long it needs to back off, we can transition the step back into pending but with it's backoff time set correctly so that our task claim doesn't come back up until we are truly ready

By putting the responsibility of persisting results of the step and the transition of the workflow step to a completed or retryable / permanent error state to the worker, we isolate the idempotency of each step in a much more encapsulated manner, such that the state of the step and it's results are not indeterminate across a queue system. We use the queue for robust message passing but not implied state management.

## System Analysis & Alignment

### Current System State (August 7, 2025)
Based on analysis of the current tasker-core-rs implementation, this architectural change aligns well with the ongoing "Simple Message Implementation" phase:

**Current Strengths:**
- ✅ **Workflow Pattern Standardization Complete**: All 20 step handlers across 5 workflows use consistent `sequence.get_results()` pattern
- ✅ **UUID Schema Foundation**: Database schema includes UUID columns for both tasks and workflow steps
- ✅ **State Machine Infrastructure**: Robust Rust state machine with proper transition validation
- ✅ **Database-Driven Architecture**: Shared PostgreSQL database serves as API layer between Rust orchestration and Ruby workers

**Current Architectural Gap (TAS-32 Problem):**
- ❌ **State Inconsistency**: Steps are marked `in_progress` during enqueueing, not actual processing
- ❌ **Queue as State Manager**: Message presence/absence used as source of truth for processing state
- ❌ **Idempotency Violation**: Risk of multiple workers processing same step if message deletion fails
- ❌ **Aggressive Message Deletion**: Workers must delete messages before processing to avoid visibility timeout issues

### Alignment with Simple Message Architecture
TAS-32 complements the ongoing simple message implementation because:

1. **State Management Simplification**: Moves state responsibility from queue messages to database where it belongs
2. **Message Size Reduction**: Results no longer serialized in queue messages, only completion signals sent
3. **ActiveRecord Integration**: Workers get real AR models for state transitions and result persistence
4. **UUID-Based Processing**: Supports the planned 3-field message structure `{task_uuid, step_uuid, ready_dependency_step_uuids}`

## Plan of Action

### Phase 1: State Machine Enhancement
**Objective**: Add new `Enqueued` state and update transition logic

#### 1.1 Update Rust State Definitions
**File**: `src/state_machine/states.rs`
- Add `Enqueued` variant to `WorkflowStepState` enum
- Update `is_active()` method to include `Enqueued` state
- Update `satisfies_dependencies()` to exclude `Enqueued` state
- Add state transition validation for `Pending → Enqueued → InProgress`

#### 1.2 Update State Transition Maps
**File**: `src/constants.rs`
- Add transition mapping: `(Some(WorkflowStepState::Pending), WorkflowStepState::Enqueued)`
- Add transition mapping: `(Some(WorkflowStepState::Enqueued), WorkflowStepState::InProgress)`
- Update `build_step_transition_map()` with new transition events

#### 1.3 Add State Transition Events
**File**: `src/constants.rs` (events module)
- Add `STEP_ENQUEUE_REQUESTED` event for `Pending → Enqueued` transition
- Add `STEP_CLAIM_REQUESTED` event for `Enqueued → InProgress` transition

#### 1.4 Update Database Schema
**File**: `migrations/` (new migration)
- Update state validation constraints to include `enqueued` state
- Update existing transitions table to support new state
- Add database indexes for efficient `enqueued` state queries

**Specific SQL Functions & Views Requiring Updates:**
- `get_step_readiness_status()` - Update state logic to exclude `enqueued` steps from `ready_for_execution`
- `get_step_readiness_status_batch()` - Update batch version with same state exclusions
- `get_task_execution_context()` - Add `enqueued_steps` count to step aggregation
- `get_task_execution_contexts_batch()` - Add `enqueued_steps` count to batch aggregation
- `get_system_health_counts_v01()` - Add `enqueued_steps` to system health metrics
- `get_analytics_metrics_v01()` - Update active task detection to include `enqueued` state
- `tasker_ready_tasks` view - Update readiness logic to only include `pending` steps as ready
- State constraint validation - Ensure transition tables accept `enqueued` as valid state

#### 1.5 Update Ruby Constants
**File**: `bindings/ruby/lib/tasker_core/constants.rb`
- Add `ENQUEUED = 'enqueued'` to `WorkflowStepStatuses` module
- Update `STEP_TRANSITION_EVENT_MAP` with new transition mappings
- Update validation arrays to include `enqueued` state

### Phase 2: Orchestration Core Changes  
**Objective**: Update orchestration to transition steps to `enqueued` instead of `in_progress`

#### 2.1 Update Step Enqueuer Logic
**File**: `src/orchestration/step_enqueuer.rs`
- Replace `mark_step_in_progress()` call with `mark_step_enqueued()`
- Update logging messages to reflect `enqueued` state
- Update error handling for new state transition

#### 2.2 Add New State Manager Method
**File**: `src/orchestration/state_manager.rs`
- Add `mark_step_enqueued()` method (similar to `mark_step_in_progress()`)
- Update state machine transition to use `StepEvent::Enqueue`
- Add database column update for `enqueued_at` timestamp

#### 2.3 Update Viable Step Discovery
**File**: `src/orchestration/viable_step_discovery.rs`
- Update SQL queries to exclude `enqueued` and `in_progress` steps from ready step detection
- Add efficient indexes for state-based queries

### Phase 3: Ruby Worker State Management
**Objective**: Workers transition `enqueued → in_progress` and persist results directly

#### 3.1 Add Worker State Transition Logic
**File**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`
- Add `claim_step_for_processing()` method to transition `enqueued → in_progress`
- Add worker UUID and timestamp metadata to transition
- Update `process_simple_step_message()` to claim step before processing
- Add error handling for state transition failures

#### 3.2 Add Result Persistence Logic
**File**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`
- Add `persist_step_results()` method to save results directly to database
- Add `transition_step_to_complete()` for successful executions
- Add `transition_step_to_retryable_error()` and `transition_step_to_permanent_error()` for failures
- Update handler result processing to determine error type (retryable vs permanent)

#### 3.3 Simplify Results Queue Messages
**File**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`
- Update `send_result_to_orchestration()` to send only completion signal
- Remove result serialization from queue messages
- Send simple message: `{step_uuid, task_uuid, completion_status}` 

#### 3.4 Add Step Handler Error Classification
**File**: `bindings/ruby/lib/tasker_core/types/step_handler_call_result.rb`
- Add `error_type` field to distinguish retryable vs permanent errors
- Add `max_retry_attempts` configuration
- Add `backoff_metadata` for retry timing hints

### Phase 4: Orchestration Result Processing Updates
**Objective**: Update orchestration to handle new completion-only messages and state evaluation

#### 4.1 Update Result Processor
**File**: `src/orchestration/result_processor.rs`
- Remove result persistence logic (now handled by workers)
- Update `handle_step_result_with_metadata()` to only evaluate step state
- Add logic to query step state and results from database
- Add retry/backoff evaluation for error states

#### 4.2 Add Retry Logic
**File**: `src/orchestration/result_processor.rs`  
- Add `evaluate_retryable_error()` method
- Add `calculate_backoff_time()` integration
- Add `transition_step_to_pending_with_backoff()` method
- Add `transition_task_to_failed()` for permanent errors

#### 4.3 Update Task Finalizer
**File**: `src/orchestration/task_finalizer.rs`
- Update completion detection to query step states from database
- Remove dependency on queue message results
- Add efficient state aggregation queries

### Phase 5: Testing & Validation
**Objective**: Ensure all integration tests pass with new state management

#### 5.1 Update Integration Tests
**Files**: `bindings/ruby/spec/integration/*_spec.rb`
- Update test expectations for new state transitions
- Add test coverage for `enqueued` state
- Add test coverage for worker state claiming
- Add test coverage for direct result persistence

#### 5.2 Update Factories
**File**: `tasker-core-rs/tests/factories/`
- Add `enqueued()` factory method for WorkflowStep
- Update state transition factories
- Add worker metadata factories for state transitions

#### 5.3 Performance Testing
- Measure impact of additional state transition
- Validate message size reduction benefits
- Test idempotency guarantees under failure conditions

### Phase 6: Documentation & Migration
**Objective**: Document new architecture and provide migration path

#### 6.1 Update Architecture Documentation
**File**: `docs/simple-messages.md`
- Document new state machine flow
- Update sequence diagrams
- Document worker responsibilities

#### 6.2 Add Migration Scripts
- Add database migration for new state constraints
- Add data migration for existing `in_progress` steps  
- Add rollback procedures

## Success Metrics

### Immediate Benefits
- ✅ **Idempotency Guarantee**: Steps can only be processed once due to database state management
- ✅ **Queue Efficiency**: No more aggressive message deletion before processing
- ✅ **Message Size Reduction**: Results no longer serialized in queue messages (~80% reduction)
- ✅ **State Consistency**: Database is single source of truth for step processing state

### Long-term Benefits  
- ✅ **Simplified Architecture**: Queue used for messaging, database used for state management
- ✅ **Better Error Handling**: Retryable vs permanent error classification at worker level
- ✅ **Improved Observability**: Clear state transitions with worker metadata and timestamps
- ✅ **ActiveRecord Integration**: Workers use real AR models instead of hash conversion

### Dependencies & Risks

### Dependencies
- **Simple Message Implementation**: TAS-32 builds on UUID-based message architecture
- **State Machine Infrastructure**: Requires robust state transition validation
- **Database Performance**: Additional state transitions may impact query performance
- **SQL Function Updates**: Critical readiness and health functions must support new `enqueued` state

### Risks & Mitigation
- **Migration Complexity**: Existing `in_progress` steps need careful migration → Add migration scripts with rollback
- **Performance Impact**: Additional database writes for state transitions → Benchmark and optimize critical paths  
- **Worker Coordination**: Multiple workers may attempt to claim same step → Use database-level locking/constraints
- **SQL Function Consistency**: 8+ SQL functions need coordinated updates → Update all in single migration to maintain consistency

## Implementation Timeline

### Week 1-2: Core State Machine (Phase 1)
- Rust state definitions and transitions
- Database schema updates with SQL function modifications
- Ruby constant updates

**SQL Migration Requirements:**
- Update `get_step_readiness_status()` L354-374: Change state logic from `IN ('pending', 'error')` to `IN ('pending', 'error')` and exclude `enqueued`
- Update `get_step_readiness_status_batch()` L466-486: Apply same exclusion logic for batch processing
- Update `get_task_execution_context()` L658-666: Add `COUNT(CASE WHEN sd.current_state = 'enqueued' THEN 1 END) as enqueued_steps`
- Update `get_task_execution_contexts_batch()` L753-762: Add enqueued step counting to batch version
- Update `get_system_health_counts_v01()` L570-590: Add `COUNT(*) FILTER (WHERE step_state.to_state = 'enqueued') as enqueued_steps`
- Update `get_analytics_metrics_v01()` L82-83: Change active task detection from `NOT IN ('complete', 'error', 'skipped', 'resolved_manually')` to include `enqueued`
- Update `tasker_ready_tasks` view readiness calculation: Only `pending` steps should be considered ready, not `enqueued`

### Week 3-4: Orchestration Changes (Phase 2)  
- Step enqueuer updates
- State manager enhancements
- Viable step discovery updates

### Week 5-6: Worker Updates (Phase 3)
- Worker state claiming logic
- Result persistence implementation
- Simplified queue messaging

### Week 7-8: Result Processing (Phase 4)
- Orchestration result processor updates
- Retry and backoff logic
- Task finalizer updates

### Week 9-10: Testing & Validation (Phase 5)
- Integration test updates
- Performance testing
- Factory updates

### Week 11-12: Documentation & Finalization (Phase 6)
- Documentation updates
- Migration scripts
- Final integration validation

This plan addresses the architectural gap identified in TAS-32 while leveraging the foundation established by the recent workflow pattern standardization and aligning with the ongoing simple message architecture implementation.

## State Machine Implementation Analysis

### Ruby Statesman vs Rust State Machine Comparison

After reviewing both implementations, there are **critical discrepancies** between the Ruby Statesman-based state machines and the Rust implementation that must be addressed for TAS-32:

#### Current State Definitions

**Ruby Statesman (`step_state_machine.rb`)**:
- States: `pending`, `in_progress`, `complete`, `error`, `cancelled`, `resolved_manually`
- Transitions: `pending → [in_progress, error, cancelled, resolved_manually]`
- Missing: No `enqueued` state defined

**Rust Implementation (`states.rs`)**:
- States: `Pending`, `InProgress`, `Complete`, `Error`, `Cancelled`, `ResolvedManually`  
- Transitions: Defined in `constants.rs` with event mapping
- Missing: No `Enqueued` state defined

#### Critical Discrepancies Identified

1. **Missing `Enqueued` State**: Neither implementation has the required intermediate state for TAS-32
2. **Transition Logic Inconsistency**: Ruby allows `pending → in_progress` directly, but TAS-32 requires `pending → enqueued → in_progress`
3. **Event Mapping Differences**: Ruby uses custom `TRANSITION_EVENT_MAP`, Rust uses `build_step_transition_map()`
4. **Guard Logic**: Ruby has complex dependency checking, Rust delegates to SQL functions

### Required Changes for TAS-32 Implementation

#### 1. Ruby State Machine Updates (`step_state_machine.rb`)

**Add `Enqueued` State**:
- Line 28: Add `state Constants::WorkflowStepStatuses::ENQUEUED` 
- Line 35: Update transitions: `pending → [enqueued, error, cancelled, resolved_manually]`
- Line 37: Add transition: `enqueued → [in_progress, error, cancelled]`

**Update Transition Event Map**:
- Line 111: Add `[nil, ENQUEUED] => StepEvents::ENQUEUE_REQUESTED`
- Line 115: Add `[PENDING, ENQUEUED] => StepEvents::ENQUEUE_REQUESTED`  
- Line 120: Add `[ENQUEUED, IN_PROGRESS] => StepEvents::EXECUTION_REQUESTED`
- Line 122: Add `[ENQUEUED, ERROR] => StepEvents::FAILED`
- Line 124: Add `[ENQUEUED, CANCELLED] => StepEvents::CANCELLED`

#### 2. Rust State Machine Updates

**Add `Enqueued` State** (`states.rs`):
- Add `Enqueued` variant to `WorkflowStepState` enum
- Update `is_active()` to include `Enqueued` state  
- Update `satisfies_dependencies()` to exclude `Enqueued` state

**Update Transition Maps** (`constants.rs`):
- Add `(Some(WorkflowStepState::Pending), WorkflowStepState::Enqueued)` transition
- Add `(Some(WorkflowStepState::Enqueued), WorkflowStepState::InProgress)` transition
- Add error transitions from `Enqueued` state

#### 3. Constants Synchronization

**Ruby Constants** (`constants.rb`):
- Add `ENQUEUED = 'enqueued'` to `WorkflowStepStatuses`
- Add `ENQUEUE_REQUESTED` to `StepEvents`
- Update `STEP_TRANSITION_EVENT_MAP` with new mappings

**Rust Constants** (`constants.rs`):
- Add `STEP_ENQUEUE_REQUESTED` event
- Update `build_step_transition_map()` with new transitions

### State Machine Consistency Requirements

For distributed reliability, both Ruby and Rust implementations must:

1. **Identical State Names**: Use exact string matching (`"enqueued"` in both systems)
2. **Consistent Transition Rules**: Same allowed transitions in both state machines  
3. **Synchronized Event Mapping**: Event names must match between Ruby and Rust
4. **Guard Logic Alignment**: Dependency checking logic must produce identical results

### Implementation Priority for TAS-32

**Phase 1 (Weeks 1-2)**: Update both state machine implementations simultaneously
- Add `Enqueued` state to both Ruby and Rust
- Synchronize transition maps and event definitions
- Update constants in both systems
- Add comprehensive state machine tests

**Phase 2 (Weeks 3-4)**: Implement orchestration changes  
- Update Rust orchestration to use `pending → enqueued` transition
- Update Ruby workers to use `enqueued → in_progress` transition

**Critical Success Factor**: State machine synchronization must be completed before any orchestration logic changes to prevent inconsistent behavior in the distributed system.

### Testing Strategy

**State Machine Consistency Tests**:
- Cross-system state transition validation
- Event mapping consistency verification  
- Guard logic equivalence testing
- Serialization/deserialization compatibility

The distributed nature of the system requires **absolute state machine consistency** between Ruby workers and Rust orchestration to prevent split-brain scenarios and ensure reliable workflow execution.
