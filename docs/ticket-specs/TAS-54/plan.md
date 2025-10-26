# TAS-54 Research Plan: Processor Ownership Necessity Analysis

**Status**: In Progress
**Created**: 2025-10-26
**Branch**: jcoletaylor/tas-54-processor-uuid-ownership-analysis-and-stale-task-recovery

## Executive Summary

Comprehensive research plan to determine if processor UUID ownership is still necessary given our evolved architecture (TAS-37, TAS-40, TAS-46), or if pure state machine logic is sufficient for distributed orchestration consistency.

**User Context**:
- **Priority**: Jump into research phase (ownership necessity analysis)
- **Interest**: Start with idempotency audit first
- **Environment**: Can reproduce stale ownership issues (orchestrator restart scenario)
- **Risk Tolerance**: High - willing to remove ownership if idempotency audit passes
- **Key Insight**: "Task orchestration runs no business logic, each step is handled idempotently, and so even race conditions around steps being finished in parallel should not result in inconsistent task states"

## Phase 1: Comprehensive Idempotency Audit (Research - No Code Changes)

**Goal**: Validate that all orchestration operations are idempotent and state-driven

### 1.1 Actor Operations Audit

Review each of the 4 actors for idempotency guarantees:

#### TaskRequestActor
**File**: `tasker-orchestration/src/actors/task_request_actor.rs`

**Questions to Answer**:
- Can task initialization be called multiple times safely?
- Is namespace validation safe to retry?
- Does template registration have side effects?
- What happens if two orchestrators initialize same task?

**Analysis Focus**:
- Delegated service: TaskInitializer
- State requirements: Task must be in `pending` state
- Database operations: INSERT with conflict handling?
- External dependencies: Namespace validation service calls

**Deliverable**: Document showing task initialization is/isn't idempotent

#### ResultProcessorActor
**File**: `tasker-orchestration/src/actors/result_processor_actor.rs`

**Questions to Answer**:
- Can same result be processed multiple times?
- Is next step discovery deterministic?
- Does result processing have side effects?
- What happens if two orchestrators process same result?

**Analysis Focus**:
- Delegated service: OrchestrationResultProcessor
- State requirements: Step must be in specific state
- Step state transitions: Are they atomic?
- Next step discovery: Is it purely database-driven?

**Deliverable**: Document showing result processing is/isn't idempotent

#### StepEnqueuerActor
**File**: `tasker-orchestration/src/actors/step_enqueuer_actor.rs`

**Questions to Answer**:
- What happens if batch enqueueing partially succeeds?
- Does PGMQ provide deduplication?
- Can same step be enqueued multiple times?
- What happens if two orchestrators enqueue same steps?

**Analysis Focus**:
- Delegated service: StepEnqueuerService
- PGMQ behavior: Message deduplication guarantees
- Batch operations: All-or-nothing or best-effort?
- Step state: Does enqueuing change step state atomically?

**Deliverable**: Document showing step enqueueing is/isn't idempotent

#### TaskFinalizerActor
**File**: `tasker-orchestration/src/actors/task_finalizer_actor.rs`

**Questions to Answer**:
- Can finalization be called multiple times?
- Does atomic claiming prevent duplicates?
- Is lifecycle event publishing idempotent?
- What happens if two orchestrators try to finalize?

**Analysis Focus**:
- Atomic claiming: `claim_task_for_finalization` SQL function
- Service decomposition: 6 focused components
- Event publishing: At-least-once semantics?
- Terminal state transitions: Are they atomic?

**Deliverable**: Document showing task finalization is/isn't idempotent

### 1.2 SQL Function Atomicity Analysis

Review critical SQL functions for race condition safety:

#### transition_task_state_atomic
**File**: `migrations/20250912000000_tas41_richer_task_states.sql`

**Analysis Focus**:
```sql
ownership_check AS (
    SELECT
        CASE
            WHEN cs.to_state IN ('initializing', 'enqueuing_steps',
                                 'steps_in_process', 'evaluating_results')
            THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
            ELSE true
        END as can_transition
    FROM current_state cs
    WHERE cs.to_state = p_from_state
)
```

**Questions to Answer**:
- What prevents concurrent transitions?
- Is the compare-and-swap pattern sufficient without ownership?
- Can multiple orchestrators transition same task simultaneously?
- What happens if ownership check is removed?

**Deliverable**: Analysis of atomicity guarantees with/without ownership

#### claim_task_for_finalization
**File**: `migrations/20250818000001_add_finalization_claiming.sql`

**Analysis Focus**:
- Atomic claiming mechanism
- Database-level locking (SELECT FOR UPDATE?)
- Duplicate finalization prevention
- Interaction with processor ownership

**Questions to Answer**:
- Does this eliminate need for processor ownership at finalization?
- Can two orchestrators claim same task?
- Is this TAS-37 feature sufficient for distributed safety?

**Deliverable**: Document showing finalization claiming atomicity

#### get_next_ready_tasks, get_ready_steps
**Files**: Various SQL function migrations

**Questions to Answer**:
- Can same task/step be returned to multiple orchestrators?
- Are batch operations consistent?
- What prevents duplicate discovery?

**Deliverable**: Analysis of discovery race conditions

### 1.3 State Machine Consistency Analysis

Review state machines for self-describing recovery:

#### Task State Machine (12 states)
**File**: `docs/states-and-lifecycles.md`

**Questions to Answer**:
- Can any orchestrator resume from any state?
- Are intermediate states self-sufficient?
- What information is lost on orchestrator crash?
- Can state machine recover without processor context?

**States to Analyze**:
- Initial: `pending`, `initializing`
- Active: `enqueuing_steps`, `steps_in_process`, `evaluating_results`
- Waiting: `waiting_for_dependencies`, `waiting_for_retry`, `blocked_by_failures`
- Terminal: `complete`, `error`, `cancelled`, `resolved_manually`

**Deliverable**: State-by-state recovery analysis

#### Step State Machine (8 states)
**File**: `docs/states-and-lifecycles.md`

**Questions to Answer**:
- Can step execution be retried safely?
- Are step results deterministic?
- Can steps be processed out of order?

**States to Analyze**:
- Pipeline: `pending`, `enqueued`, `in_progress`, `enqueued_for_orchestration`
- Terminal: `complete`, `error`, `cancelled`, `resolved_manually`

**Deliverable**: Step state recovery analysis

### 1.4 Crash Window Analysis

Identify operations in progress between state transitions:

#### Scenario 1: Crash During Step Enqueueing
```
T0: Task in steps_in_process (owned by Orch A)
T1: Orch A discovers 10 ready steps via SQL
T2: Orch A begins enqueueing: 5/10 enqueued to PGMQ
T3: CRASH - Orch A dies
T4: Orch B discovers same task in steps_in_process
T5: Orch B discovers 10 ready steps (same SQL query)
T6: Question: Re-enqueue all? Only missing 5? How to detect?
```

**Analysis Questions**:
- Do enqueued steps change state in database?
- Can Orch B detect which steps already enqueued?
- Is duplicate enqueueing harmful?
- What's the recovery mechanism?

#### Scenario 2: Crash During Result Processing
```
T0: Task in evaluating_results (owned by Orch A)
T1: Orch A processes step completion message
T2: Orch A updates step state to completed (atomic)
T3: Orch A begins discovering next steps
T4: CRASH - Orch A dies before discovering/enqueueing
T5: Orch B picks up task in evaluating_results
T6: Question: Re-process result? Continue from step state?
```

**Analysis Questions**:
- Is result processing resume-able?
- Can Orch B detect Orch A's progress?
- Is duplicate result processing harmful?
- What state has Orch A left in database?

#### Scenario 3: Crash During Finalization
```
T0: Task ready for finalization
T1: Orch A calls claim_task_for_finalization (SUCCESS)
T2: Orch A begins finalization operations
T3: CRASH - Orch A dies mid-finalization
T4: Orch B calls claim_task_for_finalization
T5: Question: Does B succeed or fail? Is task stuck?
```

**Analysis Questions**:
- Is claiming persistent across crashes?
- Can finalization be retried?
- What's the timeout mechanism?

**Deliverable**: Crash window documentation with recovery paths

## Phase 2: State Machine Sufficiency Analysis (Research)

**Goal**: Prove state machine alone prevents double-processing

### 2.1 Double-Processing Prevention

Analyze if state machine alone prevents:

#### Same Step Executed Twice
**Question**: Can worker execute same step multiple times?

**State Machine Protection**:
- Step must be in `enqueued` state to execute
- Worker transitions: `enqueued` → `in_progress` → `complete`
- Second execution attempt: Step in `complete` state, rejected

**Ownership Impact**:
- With ownership: Task-level lock prevents enqueueing
- Without ownership: Step state prevents execution
- **Analysis**: Step state sufficient?

#### Same Result Processed Twice
**Question**: Can orchestrator process same step result multiple times?

**State Machine Protection**:
- Result message references step UUID
- Step must be in `in_progress` or `enqueued_for_orchestration`
- After processing: Step in `complete` or `error` state
- Second processing: Step state check rejects

**Ownership Impact**:
- With ownership: Task-level lock during evaluation
- Without ownership: Step state prevents duplicate
- **Analysis**: Step state sufficient?

#### Task Finalized Twice
**Question**: Can two orchestrators finalize same task?

**State Machine Protection**:
- `claim_task_for_finalization` SQL function (TAS-37)
- Atomic database-level claiming
- Only one orchestrator succeeds

**Ownership Impact**:
- With ownership: Task must be owned by claimer
- Without ownership: Claiming is still atomic
- **Analysis**: TAS-37 claiming sufficient?

#### Steps Enqueued Multiple Times
**Question**: Can same step be enqueued to PGMQ multiple times?

**State Machine Protection**:
- Step must be in `pending` state
- Enqueueing transitions: `pending` → `enqueued`
- Re-enqueueing: Step state check rejects

**PGMQ Protection**:
- Message deduplication by step UUID?
- **Research Needed**: PGMQ guarantees

**Ownership Impact**:
- With ownership: Task locked during enqueueing
- Without ownership: Step state prevents duplicates
- **Analysis**: Combined protection sufficient?

**Deliverable**: Double-processing prevention matrix

### 2.2 Concurrent Orchestrator Scenarios

Document what happens with 5 orchestrators:

#### Scenario: All Try to Initialize Same Task
```
5 Orchestrators receive same TaskRequestMessage
All call TaskRequestActor.handle(ProcessTaskRequestMessage)
All attempt: pending → initializing transition
```

**Questions**:
- How many succeed?
- What does state machine prevent?
- What does ownership prevent?
- Is outcome deterministic?

**Analysis**:
- State machine: Only one transition succeeds (atomic SQL)
- Ownership: Winning orchestrator claims task
- **Without ownership**: State machine still allows only one success
- **Conclusion**: ???

#### Scenario: All Try to Process Same Step Result
```
5 Orchestrators poll orchestration_step_results queue
All receive same message (PGMQ visibility timeout allows this?)
All call ResultProcessorActor.handle(ProcessStepResultMessage)
```

**Questions**:
- How many process the result?
- What prevents duplicates?
- Is step state changed atomically?

**Analysis**:
- PGMQ: Message visibility timeout (only one sees message?)
- Step state: Transition prevents duplicates?
- **Research Needed**: PGMQ message claiming semantics

#### Scenario: All Try to Finalize Same Task
```
5 Orchestrators detect task ready for finalization
All call TaskFinalizerActor.handle(FinalizeTaskMessage)
All call claim_task_for_finalization()
```

**Questions**:
- How many succeed at claiming?
- Is claiming atomic?
- What happens to losers?

**Analysis**:
- TAS-37 atomic claiming (SELECT FOR UPDATE?)
- Only one orchestrator claims
- **Without ownership**: Claiming still works
- **Conclusion**: TAS-37 sufficient

#### Scenario: All Try to Enqueue Steps for Same Task
```
5 Orchestrators discover same task has ready steps
All call StepEnqueuerActor.handle(EnqueueReadyStepsMessage)
All discover same 10 steps
All attempt to enqueue
```

**Questions**:
- Do all 10 steps get enqueued 5 times (50 messages)?
- Does step state prevent duplicates?
- Does PGMQ deduplicate?

**Analysis**:
- Step state transitions: `pending` → `enqueued` (atomic?)
- PGMQ behavior: Accept duplicates or reject?
- **Research Needed**: Step state atomicity during enqueueing

**Deliverable**: Concurrent orchestrator safety matrix

### 2.3 Ownership vs State Machine Comparison

Compare guarantees provided by each approach:

#### With Processor Ownership
**Guarantees**:
- Only owner can transition task during active states
- Prevents concurrent processing of same task
- Audit trail of which orchestrator processed task

**Costs**:
- Stale ownership on crash blocks progress
- Requires ownership transfer mechanism
- Additional complexity in SQL and code

**Failure Modes**:
- Orchestrator crashes → ownership stuck
- Ownership transfer race conditions
- Manual intervention required

#### With State Machine Only
**Guarantees**:
- Atomic state transitions prevent double-processing
- Any orchestrator can resume from any state
- Automatic recovery from crashes

**Costs**:
- Must audit all operations for idempotency
- Relies on database atomicity guarantees
- No protection if operations aren't truly idempotent

**Failure Modes**:
- Non-idempotent operation executed twice
- Race conditions in batch operations
- Partial state corruption

#### Hybrid: Audit-Only Processor UUID
**Guarantees**:
- State machine prevents double-processing
- Processor UUID kept for audit/debugging
- Automatic recovery from crashes

**Costs**:
- No additional cost (remove ownership enforcement)
- Keep audit trail for debugging

**Failure Modes**:
- Same as state machine only approach

**Deliverable**: Decision matrix comparing approaches

## Phase 3: Proof of Concept - Audit-Only Approach (Implementation)

**Goal**: Implement audit-only approach with feature flag for validation

### 3.1 SQL Function Modification

Create new migration: `20251026000000_tas54_audit_only_processor_uuid.sql`

**Modification Target**: `transition_task_state_atomic` function

**Current Ownership Check**:
```sql
ownership_check AS (
    SELECT
        CASE
            WHEN cs.to_state IN ('initializing', 'enqueuing_steps',
                                 'steps_in_process', 'evaluating_results')
            THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
            ELSE true
        END as can_transition
    FROM current_state cs
    WHERE cs.to_state = p_from_state
)
```

**Proposed Audit-Only Version**:
```sql
-- Option 1: Remove check entirely
ownership_check AS (
    SELECT true as can_transition  -- Always allow if state matches
)

-- Option 2: Configurable via database setting
ownership_check AS (
    SELECT
        CASE
            WHEN current_setting('tasker.enforce_ownership')::boolean
            THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
            ELSE true
        END as can_transition
    FROM current_state cs
    WHERE cs.to_state = p_from_state
)
```

**Implementation Decision**: Start with Option 1 (simpler), add Option 2 if needed

**Migration Steps**:
1. Create new version of `transition_task_state_atomic`
2. Keep `processor_uuid` parameter (for audit trail)
3. Remove ownership enforcement from ownership_check
4. Add migration rollback for safety

### 3.2 Configuration Flag

**File**: `config/tasker/base/orchestration.toml`

**New Configuration**:
```toml
[orchestration.processor_ownership]
# Enable processor UUID ownership enforcement
# Default: true (current behavior, backward compatible)
enabled = true

# Audit-only mode: Keep processor UUID for audit trail but don't enforce ownership
# When true, any orchestrator can transition any task (state machine provides safety)
# Default: false (current behavior)
audit_only = false
```

**Environment Overrides**:
- `config/tasker/environments/test/orchestration.toml`: `audit_only = true` (test new approach)
- `config/tasker/environments/development/orchestration.toml`: `audit_only = false` (stable)
- `config/tasker/environments/production/orchestration.toml`: `audit_only = false` (stable)

### 3.3 Rust Configuration Integration

**File**: `tasker-shared/src/config/orchestration.rs`

**Add to OrchestrationConfig**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorOwnershipConfig {
    /// Enable processor UUID ownership enforcement
    #[serde(default = "default_ownership_enabled")]
    pub enabled: bool,

    /// Audit-only mode: keep UUID for audit but don't enforce
    #[serde(default)]
    pub audit_only: bool,
}

fn default_ownership_enabled() -> bool {
    true  // Backward compatible default
}

impl Default for ProcessorOwnershipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            audit_only: false,
        }
    }
}
```

**Integration**:
- Pass config to state machine operations
- SQL function respects audit_only setting
- Log configuration at startup

### 3.4 Backward Compatibility

**Default Behavior** (no config changes):
- `enabled = true`, `audit_only = false`
- Ownership enforcement active (current behavior)
- Zero breaking changes

**Audit-Only Mode** (opt-in):
- `enabled = true`, `audit_only = true`
- Processor UUID still written to database
- Ownership checks disabled
- Automatic recovery from crashes

**Future: Full Removal** (if audit-only succeeds):
- `enabled = false`
- Remove processor UUID from transitions table
- Simplify SQL functions
- Requires migration for old data

### 3.5 Integration Test Validation

**Test Strategy**: Run existing tests with audit-only mode

**Test Files to Validate**:
- `tests/e2e/rust/*.rs` - All Rust worker e2e tests
- `tests/e2e/ruby/*.rs` - All Ruby worker e2e tests
- `tests/integration/*.rs` - SQL function tests

**Validation Criteria**:
- ✅ All tests pass with audit-only enabled
- ✅ No race conditions detected
- ✅ No duplicate operations logged
- ✅ Task states remain consistent
- ✅ Step execution remains idempotent

**Additional Tests**:
- Create multi-orchestrator test (if doesn't exist)
- Simulate orchestrator crashes
- Verify stale message handling

**Deliverable**: Test results document showing audit-only mode works

## Phase 4: Graceful Stale Task Handling (Immediate Fixes)

**Goal**: Handle stale tasks gracefully regardless of ownership decision

### 4.1 Terminal State Handling

**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/task_coordinator.rs`

**Current Code** (lines ~95-135):
```rust
let should_finalize = if current_state == TaskState::StepsInProcess {
    task_state_machine.transition(TaskEvent::StepCompleted(*step_uuid)).await?
} else if current_state == TaskState::EvaluatingResults {
    true
} else {
    false  // ❌ Tasks in blocked_by_failures fall here
};

if should_finalize {
    self.finalize_task(...)
} else {
    error!("Failed to transition state machine");  // ❌ Misleading
    Err(OrchestrationError::DatabaseError { /* ... */ })
}
```

**Proposed Fix**:
```rust
let should_finalize = if current_state == TaskState::StepsInProcess {
    task_state_machine.transition(TaskEvent::StepCompleted(*step_uuid)).await?
} else if current_state == TaskState::EvaluatingResults {
    true
} else if matches!(
    current_state,
    TaskState::BlockedByFailures
        | TaskState::Complete
        | TaskState::Error
        | TaskState::Cancelled
        | TaskState::ResolvedManually
) {
    // Task already in terminal state; step completion message is stale
    info!(
        correlation_id = %correlation_id,
        task_uuid = %workflow_step.task_uuid,
        step_uuid = %step_uuid,
        current_state = ?current_state,
        execution_status = %task.execution_status,
        "Step completed but task already in terminal state; acknowledging gracefully"
    );
    return Ok(());  // ✅ Graceful acknowledgment, message won't retry
} else {
    // Unexpected state for step completion
    debug!(
        correlation_id = %correlation_id,
        task_uuid = %workflow_step.task_uuid,
        step_uuid = %step_uuid,
        current_state = ?current_state,
        "Task in unexpected state for step completion"
    );
    false
};
```

**Benefits**:
- Prevents infinite message retries
- Clear logging of terminal state scenario
- Graceful message acknowledgment
- Reduces error noise in logs

### 4.2 Improved Error Messages

**File**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/task_coordinator.rs`

**Current Error** (line ~129):
```rust
error!("Failed to transition state machine");
```

**Proposed Improvement**:
```rust
error!(
    correlation_id = %correlation_id,
    task_uuid = %workflow_step.task_uuid,
    step_uuid = %step_uuid,
    current_state = ?current_state,
    current_processor = ?current_task_processor_uuid,
    our_processor = %self.processor_uuid,
    ownership_match = %(current_task_processor_uuid == Some(self.processor_uuid)),
    "Cannot process step completion: task state {:?} incompatible with step completion, \
     owned by processor {:?} (ours: {})",
    current_state,
    current_task_processor_uuid,
    self.processor_uuid
);
```

**Benefits**:
- Clear diagnosis of issue (state vs ownership)
- Processor UUID comparison visible
- Enables debugging of stale ownership
- Structured logging for metrics

### 4.3 Stale Ownership Detection

**New Helper Function**:

**File**: `tasker-orchestration/src/orchestration/lifecycle/stale_ownership.rs` (new file)

```rust
/// Detect if task ownership is stale (owned by old/dead processor)
pub async fn is_ownership_stale(
    pool: &PgPool,
    task_uuid: Uuid,
    current_processor_uuid: Uuid,
    max_age_seconds: i64,
) -> TaskerResult<StaleOwnershipInfo> {
    let result = sqlx::query_as::<_, StaleOwnershipInfo>(
        r#"
        SELECT
            t.task_uuid,
            tt.processor_uuid,
            tt.to_state,
            tt.created_at,
            EXTRACT(EPOCH FROM (NOW() - tt.created_at))::bigint as age_seconds,
            tt.processor_uuid != $2 as different_processor,
            EXTRACT(EPOCH FROM (NOW() - tt.created_at))::bigint > $3 as is_stale
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON t.task_uuid = tt.task_uuid
        WHERE t.task_uuid = $1
          AND tt.most_recent = true
        "#,
    )
    .bind(task_uuid)
    .bind(current_processor_uuid)
    .bind(max_age_seconds)
    .fetch_one(pool)
    .await?;

    Ok(result)
}

#[derive(Debug, sqlx::FromRow)]
pub struct StaleOwnershipInfo {
    pub task_uuid: Uuid,
    pub processor_uuid: Option<Uuid>,
    pub to_state: String,
    pub created_at: chrono::NaiveDateTime,
    pub age_seconds: i64,
    pub different_processor: bool,
    pub is_stale: bool,
}
```

**Usage in Error Path**:
```rust
} else {
    // Check if ownership is stale
    let stale_info = is_ownership_stale(
        &self.pool,
        workflow_step.task_uuid,
        self.processor_uuid,
        300,  // 5 minutes
    ).await?;

    if stale_info.is_stale {
        warn!(
            task_uuid = %workflow_step.task_uuid,
            old_processor = ?stale_info.processor_uuid,
            age_seconds = %stale_info.age_seconds,
            "Task owned by stale processor; consider enabling audit-only mode for automatic recovery"
        );
    }

    error!(/* ... improved error message ... */);
    false
}
```

**Benefits**:
- Explicit stale ownership detection
- Actionable warnings in logs
- Metrics for monitoring
- Evidence for audit-only mode decision

**Deliverable**: Graceful handling implementation with tests

## Phase 5: Testing Strategy

### 5.1 Manual Reproduction Test

**Goal**: Create reproducible stale ownership scenario

**Test Script**: `scripts/test-stale-ownership.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

echo "🧪 Testing Stale Ownership Scenario"

# 1. Start orchestration + workers
echo "1. Starting orchestration and workers..."
docker-compose -f docker/docker-compose.test.yml up -d

# 2. Create tasks that will fail
echo "2. Creating tasks with intentional failures..."
# Use error_scenarios namespace with permanent failures
cargo run --bin tasker-cli task create \
  --namespace test_errors \
  --name permanent_error_only \
  --input '{"error_type": "permanent", "worker": "ruby"}'

# Wait for task to reach blocked_by_failures
sleep 5

# 3. Kill orchestration (creates stale ownership)
echo "3. Killing orchestration (creating stale ownership)..."
docker-compose -f docker/docker-compose.test.yml stop orchestration

# 4. Restart orchestration (new processor UUID)
echo "4. Restarting orchestration (new processor UUID)..."
docker-compose -f docker/docker-compose.test.yml up -d orchestration

# 5. Observe logs
echo "5. Observing logs for stale ownership errors..."
sleep 10
docker-compose -f docker/docker-compose.test.yml logs orchestration | \
  grep -E "(Failed to transition|owned by processor|stale)" || \
  echo "✅ No stale ownership errors!"

# 6. Cleanup
echo "6. Cleaning up..."
docker-compose -f docker/docker-compose.test.yml down
```

**Expected Results**:
- **With ownership enabled**: Errors about stale processor UUID
- **With audit-only mode**: Tasks process successfully

### 5.2 Chaos Test (If Audit-Only Passes)

**Goal**: Validate distributed orchestration safety under chaos

**Test Script**: `tests/chaos/multi_orchestrator_chaos_test.rs`

**Scenario**:
1. Deploy 5 orchestrators (different processor UUIDs)
2. Submit 100 tasks with various patterns:
   - Linear workflows
   - Diamond workflows (parallel steps)
   - Error scenarios (retries)
3. During execution:
   - Randomly kill 2 orchestrators every 30 seconds
   - Restart killed orchestrators with new UUIDs
   - Continue for 5 minutes
4. Verify integrity:
   - No duplicate step executions (check step state transitions)
   - No corrupted task states (check state machine invariants)
   - All tasks eventually complete or fail appropriately
   - No orphaned tasks (stuck forever)

**Metrics to Collect**:
- Task completion rate
- Average task execution time
- Orchestrator crash recovery time
- Duplicate operation attempts (should be 0)
- State machine transition conflicts
- PGMQ message redelivery rate

**Success Criteria**:
- ✅ 100% tasks reach terminal state (complete/error/blocked)
- ✅ 0 duplicate step executions
- ✅ 0 corrupted task states
- ✅ Recovery time < 5 seconds after orchestrator restart
- ✅ No manual intervention required

**Deliverable**: Chaos test results document

### 5.3 Performance Comparison

**Goal**: Measure performance impact of removing ownership

**Test Scenarios**:
1. **Baseline**: Current ownership-enabled configuration
2. **Audit-Only**: Ownership checks removed
3. **Multi-Orchestrator**: 5 orchestrators competing

**Metrics**:
- Task throughput (tasks/second)
- Step execution latency (ms)
- State transition latency (ms)
- Database contention (lock waits)
- PGMQ message processing rate

**Test Script**: `tests/benchmarks/ownership_performance_test.rs`

**Deliverable**: Performance comparison report

## Success Criteria

### Phase 1 Complete (Audit)
- ✅ Comprehensive documentation of all actor operations
- ✅ SQL function atomicity analysis complete
- ✅ State machine recovery paths documented
- ✅ Crash window scenarios analyzed
- ✅ Clear idempotency assessment for each operation
- **Deliverable**: `docs/ticket-specs/TAS-54/idempotency-audit.md`

### Phase 2 Complete (Analysis)
- ✅ Double-processing prevention matrix complete
- ✅ Concurrent orchestrator scenarios documented
- ✅ Ownership vs state machine comparison
- ✅ Clear recommendation: Keep or remove ownership?
- **Deliverable**: `docs/ticket-specs/TAS-54/ownership-analysis.md`

### Phase 3 Complete (Proof of Concept)
- ✅ SQL migration for audit-only mode
- ✅ Configuration flag implementation
- ✅ All integration tests pass with audit-only
- ✅ No race conditions detected
- **Deliverable**: Working audit-only mode implementation

### Phase 4 Complete (Immediate Fixes)
- ✅ Terminal state handling deployed
- ✅ Error messages improved
- ✅ Stale ownership detection implemented
- ✅ Logs provide clear diagnostics
- **Deliverable**: Graceful stale task handling

### Phase 5 Complete (Validation)
- ✅ Manual reproduction test passes
- ✅ Chaos test shows zero corruption
- ✅ Performance comparison complete
- ✅ Production readiness assessment
- **Deliverable**: Validation report with recommendations

## Deliverables

### Documentation
1. **Idempotency Audit** (`docs/ticket-specs/TAS-54/idempotency-audit.md`)
   - Actor operation analysis
   - SQL function atomicity
   - Crash window documentation

2. **Ownership Analysis** (`docs/ticket-specs/TAS-54/ownership-analysis.md`)
   - Double-processing prevention
   - Concurrent orchestrator scenarios
   - Recommendation with evidence

3. **Migration Guide** (`docs/ticket-specs/TAS-54/migration-guide.md`)
   - Steps to enable audit-only mode
   - Rollback procedure
   - Production deployment plan

4. **Test Results** (`docs/ticket-specs/TAS-54/test-results.md`)
   - Integration test results
   - Chaos test results
   - Performance comparison

### Code Changes
1. **SQL Migration**: `20251026000000_tas54_audit_only_processor_uuid.sql`
2. **Configuration**: Processor ownership config in `tasker-shared/src/config/orchestration.rs`
3. **Immediate Fixes**: Terminal state handling, error messages, stale detection
4. **Tests**: Chaos test suite, reproduction scripts

### Architecture Updates
1. **`docs/actors.md`**: Updated with ownership findings
2. **`docs/states-and-lifecycles.md`**: State machine recovery documentation
3. **`docs/deployment-patterns.md`**: Multi-orchestrator deployment guidance

## Timeline Estimate

### Phase 1: Idempotency Audit
- **1.1 Actor Operations**: 2 hours (30 min per actor)
- **1.2 SQL Functions**: 1 hour
- **1.3 State Machines**: 1 hour
- **1.4 Crash Windows**: 1 hour
- **Subtotal**: ~5 hours

### Phase 2: State Machine Analysis
- **2.1 Double-Processing**: 1 hour
- **2.2 Concurrent Orchestrators**: 1 hour
- **2.3 Comparison & Decision**: 1 hour
- **Subtotal**: ~3 hours

### Phase 3: Proof of Concept
- **3.1 SQL Migration**: 1 hour
- **3.2 Configuration**: 1 hour
- **3.3 Integration**: 1 hour
- **3.4 Testing**: 2 hours
- **Subtotal**: ~5 hours

### Phase 4: Immediate Fixes
- **4.1 Terminal State Handling**: 30 min
- **4.2 Error Messages**: 30 min
- **4.3 Stale Detection**: 1 hour
- **Subtotal**: ~2 hours

### Phase 5: Validation
- **5.1 Reproduction Test**: 1 hour
- **5.2 Chaos Test**: 3 hours
- **5.3 Performance**: 2 hours
- **Subtotal**: ~6 hours

**Total Estimated Time**: ~21 hours of focused work

**Realistic Timeline**:
- Research Phases (1-2): 2-3 days
- Implementation (3-4): 1-2 days
- Validation (5): 1-2 days
- **Total**: 4-7 days depending on findings

## Next Steps

**Immediate**:
1. Start with Phase 1.1: TaskRequestActor idempotency audit
2. Document findings in `docs/ticket-specs/TAS-54/idempotency-audit.md`
3. Create structured analysis document as we go

**Questions Before Starting**:
- Should we create the audit document structure first?
- Want to review any specific actor before diving in?
- Preference for documentation format (checklist, table, narrative)?

---

**Last Updated**: 2025-10-26
**Status**: Ready to begin Phase 1.1
