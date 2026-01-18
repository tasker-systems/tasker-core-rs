# TAS-73: Research Findings Report

**Date:** 2026-01-17
**Status:** Research Phase Complete
**Branch:** `jcoletaylor/tas-73-resiliency-and-redundancy-ensuring-atomicity-and-idempotency`

---

## Executive Summary

This report synthesizes the findings from systematic code audits of all concurrency-sensitive code paths in the Tasker Core system. The architecture demonstrates **mature defense-in-depth patterns** with most critical paths properly protected. However, **two significant issues** require immediate attention, and **test infrastructure gaps** must be addressed to validate horizontal scaling claims.

### Risk Assessment Summary

| Category | Critical | High | Medium | Low | Design |
|----------|----------|------|--------|-----|--------|
| Task Lifecycle | 0 | 0 | 1 | 3 | 1 |
| Step Lifecycle | 0 | 0 | 0 | 7 | 0 |
| Message Queue | 0 | 0 | 2 | 3 | 0 |
| Test Coverage | 0 | 5 | 2 | 0 | 0 |

---

## Part 1: Critical Findings

### Finding 1: Task Identity Strategy Required (DESIGN)

**Location:** `migrations/20260110000002_constraints_and_indexes.sql:183`

**Issue:** The `identity_hash` column on the `tasks` table has only a regular B-tree index (`idx_tasks_identity_hash`), not a UNIQUE constraint. This was a regression from the schema migration (original `public.tasker_tasks` had a unique index).

**Evidence:**
```sql
-- Current (line 183):
CREATE INDEX idx_tasks_identity_hash ON tasker.tasks USING btree (identity_hash);

-- Original archived migration had:
CREATE UNIQUE INDEX index_tasks_on_identity_hash ON public.tasker_tasks USING btree (identity_hash);

-- Documentation claims (idempotency-and-atomicity.md:38):
-- "tasker.tasks.identity_hash (UNIQUE)" - BUT NOT ACTUALLY IMPLEMENTED
```

**Nuance Discovered:**

Task identity is **domain-specific**. The current approach (`hash(named_task_uuid, context)`) assumes all domains want strict idempotency, but:

| Use Case | Same Template + Same Context | Desired Behavior |
|----------|------------------------------|------------------|
| Payment processing | Likely accidental duplicate | **Deduplicate** |
| Nightly batch job | Intentional repetition | **Allow** |
| Event-driven triggers | Often intentional | **Allow** |

**Resolution:** Design a **strategy pattern** for task identity:
- `STRICT` (default): Current behavior, hash of (named_task_uuid, context)
- `CALLER_PROVIDED`: Caller supplies idempotency_key (like Stripe)
- `ALWAYS_UNIQUE`: Every request creates new task (uuidv7)

**Detailed Design:** See `docs/ticket-specs/TAS-73/identity-hash-strategy.md`

**Action Items:**
1. Add UNIQUE constraint to existing migration file (not standalone migration)
2. Implement identity strategy pattern on named tasks
3. Add optional `idempotency_key` to TaskRequest

**Priority:** P1 - Design document created, implementation can follow TAS-73 core work

---

### Finding 2: Task Finalization Race Condition (MEDIUM)

**Location:** `tasker-orchestration/src/orchestration/lifecycle/task_finalization/completion_handler.rs:57-78`

**Issue:** Task finalization uses a check-then-act pattern without atomic locking. If two orchestrators simultaneously detect task completion, both may attempt finalization.

**Current Pattern:**
```rust
let current_state = state_machine.current_state().await?;  // T0: Query (no lock)
if current_state == TaskState::Complete {
    return Ok(...);  // Idempotent if already done
}
// ← GAP: Another orchestrator can be here simultaneously
state_machine.transition(TaskEvent::AllStepsSuccessful).await?;  // T2: Transition
```

**Race Scenario:**
1. Orchestrator A queries state → `EvaluatingResults`
2. Orchestrator B queries state → `EvaluatingResults`
3. A transitions to `Complete` (succeeds)
4. B attempts transition (gets state machine error, not graceful)

**Impact:**
- Not data corruption (state machine prevents invalid transitions)
- Operationally suboptimal (second orchestrator gets error instead of silent idempotency)
- Logged errors in multi-instance deployments

**Previous Attempt (Rejected):** Claim-based approach with `finalization_claimed_by` column was tried but created stuck claims when orchestrators crashed after claiming but before completing.

**Recommended Fix:** Transaction-based locking using `SELECT ... FOR UPDATE`:
- Wrap finalization in a transaction that acquires row lock
- Other orchestrators wait for lock instead of racing
- Crash = transaction rollback = lock released automatically
- Uses existing `TaskTransition::create_with_transaction()` method

**Detailed Design:** See `docs/ticket-specs/TAS-73/atomic-finalization-design.md`

**Priority:** P2 - Operational improvement, not correctness issue. Implement after multi-instance testing validates frequency of this race.

---

## Part 2: Architecture Validation Summary

### 2.1 Task Lifecycle Code Paths

| Code Path | Protection Mechanism | Risk | Status |
|-----------|---------------------|------|--------|
| Task creation (POST /v1/tasks) | `identity_hash` index (NOT unique) | **CRITICAL** | ⚠️ FIX REQUIRED |
| Task initialization | Transaction scoping | Low | ✓ SOLID |
| Task state transitions | `FOR UPDATE` + compare-and-swap | Low | ✓ SOLID |
| Task finalization | State guard (non-atomic claim) | Medium | ⚠️ IMPROVEMENT NEEDED |
| Task DLQ entry | `idx_dlq_unique_pending_task` partial unique | Low | ✓ SOLID |

### 2.2 Step Lifecycle Code Paths

| Code Path | Protection Mechanism | Risk | Status |
|-----------|---------------------|------|--------|
| Initial step creation | Transaction within task init | Low | ✓ SOLID |
| Decision point step creation | `uq_workflow_step_task_named_step` + find_or_create | Low | ✓ SOLID (TAS-151) |
| Batch worker creation | Idempotency check + `unique_edge_per_step_pair` | Low | ✓ SOLID |
| Step claiming (PGMQ read) | State machine transition guard | Low | ✓ SOLID |
| Step result processing | Downstream state validation | Low | ✓ SOLID |
| Step state transitions | State machine guards + immutable audit trail | Low | ✓ SOLID |
| Step retry scheduling | Max attempts guard + backoff calculator | Low | ✓ SOLID |

### 2.3 Message Queue Operations

| Code Path | Protection Mechanism | Risk | Status |
|-----------|---------------------|------|--------|
| Step enqueueing | State transition BEFORE PGMQ send | Low | ✓ SOLID |
| Result message processing | Downstream state checks | Medium | ⚠️ EXPLICIT CHECK RECOMMENDED |
| Notification handling | Hint-based architecture | Low | ✓ SOLID |
| Message acknowledgment | Visibility timeout model | Low | ✓ SOLID |
| Long-running steps (>30s) | No heartbeat implementation | Medium | ⚠️ DOCUMENTATION NEEDED |

---

## Part 3: Research Questions Answered

### 3.1 Task Finalization Race

**Q: If two orchestrators simultaneously detect that all steps are complete, what prevents both from finalizing the task?**

**A:** State machine guards prevent invalid transitions, but not gracefully:
- First finalization succeeds
- Second receives `StateTransitionFailed` error
- No data corruption, but suboptimal UX
- **Recommendation:** Implement atomic claiming pattern

### 3.2 Result Processing Idempotency

**Q: Can the same result be processed by multiple orchestrators? Is it idempotent?**

**A:** Yes, processing is idempotent:
- Workers mark steps `Complete` BEFORE sending result to orchestration
- SQL functions skip completed/in-progress steps
- Second result processing finds no matching "ready to process" steps
- **Note:** No explicit duplicate detection; relies on downstream state

### 3.3 Step Enqueueing Idempotency

**Q: Can the same step be enqueued to PGMQ multiple times?**

**A:** No, protected by state machine:
- `mark_step_enqueued()` transitions state `Pending` → `Enqueued` BEFORE PGMQ send
- If two orchestrators attempt same step:
  - First: succeeds, sends message
  - Second: state transition fails (already `Enqueued`), no message sent

### 3.4 State Machine Atomicity

**Q: Is the most_recent pattern atomic?**

**A:** Yes, verified:
- SQL function uses `FOR UPDATE` lock
- Compare-and-swap validates expected state
- Unique constraint on `most_recent` flag
- All operations within single transaction

### 3.5 Message Acknowledgment

**Q: Is pgmq.delete atomic with result recording?**

**A:** N/A - Tasker doesn't use explicit message deletion:
- Visibility timeout model handles recovery
- Messages automatically re-appear after timeout
- Duplicate processing prevented by step state checks

---

## Part 4: Test Coverage Analysis

### 4.1 Existing Test Coverage (60+ tests)

| Category | Count | Coverage Quality |
|----------|-------|------------------|
| Integration tests (single-instance) | 40+ | Strong |
| E2E tests (language-specific) | 60+ | Strong |
| PGMQ/messaging tests | 5 | Moderate |
| Retry boundary tests | 6 | Strong |
| DLQ lifecycle tests | 4 | Strong |
| Concurrency tests | 1 | **WEAK** |

### 4.2 Critical Test Gaps

| Gap | Risk | Priority | Recommended Test |
|-----|------|----------|------------------|
| No thundering herd test | High | P0 | Submit N=50 identical tasks simultaneously |
| No multi-worker contention | High | P0 | 5 workers competing for same queue |
| No concurrent orchestrator test | High | P0 | 2+ orchestration instances processing same tasks |
| No duplicate result test | High | P1 | Same result message processed twice |
| No concurrent finalization test | High | P1 | 2+ orchestrators finalizing same task |
| No chaos injection | Medium | P2 | Random service restarts during processing |
| No graceful shutdown test | Medium | P2 | Validate cleanup on shutdown |

### 4.3 Test Infrastructure Available

Strong foundation exists:
- `LifecycleTestManager`: Task/step lifecycle operations
- `IntegrationTestManager`: Docker Compose coordination
- `ActorTestHarness`: Actor system initialization
- `IntegrationTestUtils`: Task creation helpers

Missing:
- Multi-instance deployment scripts
- OrchestrationCluster abstraction for round-robin endpoints
- Chaos injection framework
- Aggregated metrics collection across instances

---

## Part 5: Recommended Implementation Plan

### Phase 1: Task Identity Strategy (Parallel Track)

**TAS-73a: Identity Hash Strategy Implementation**

See: `docs/ticket-specs/TAS-73/identity-hash-strategy.md`

1. Add UNIQUE constraint to existing migration file (`20260110000002_constraints_and_indexes.sql`)
2. Add `identity_strategy` column to `named_tasks` table
3. Implement strategy pattern (STRICT, CALLER_PROVIDED, ALWAYS_UNIQUE)
4. Add optional `idempotency_key` to TaskRequest
5. Update documentation to match implementation

**Note:** This can be developed in parallel with multi-instance testing infrastructure. The UNIQUE constraint is the minimum requirement; full strategy implementation enhances the design.

### Phase 2: Infrastructure Development

**TAS-73b: Multi-Instance Deployment Scripts**
1. Create `cargo-make/scripts/multi-deploy/start-orchestration-cluster.sh`
2. Create `cargo-make/scripts/multi-deploy/start-worker-cluster.sh`
3. Add environment variable support for `TASKER_INSTANCE_ID`
4. Configure unique ports (3000, 3001, 3002...)
5. Shared DATABASE_URL and PGMQ_DATABASE_URL

**TAS-73c: Test Infrastructure Enhancement**
1. Add `OrchestrationCluster` abstraction to test utils
2. Extend `IntegrationTestManager` with cluster support
3. Add test categorization (`#[cfg_attr(feature = "multi-instance", ...)]`)

### Phase 3: Test Development

**TAS-73d: Concurrency Test Suite**

Priority order:
1. `tests/integration/concurrency/thundering_herd_test.rs`
2. `tests/integration/concurrency/step_claiming_contention_test.rs`
3. `tests/integration/concurrency/orchestrator_race_test.rs`
4. `tests/integration/concurrency/duplicate_result_test.rs`
5. `tests/integration/concurrency/concurrent_finalization_test.rs`

**TAS-73e: Chaos Test Suite** (if time permits)
1. Service restart during processing
2. Database connectivity loss
3. Long-running step timeout

### Phase 4: Operational Improvements

**TAS-73f: Atomic Finalization Claiming** (if needed after testing)
1. Add `finalization_claimed_at` and `finalization_claimed_by` columns
2. Implement `claim_task_for_finalization()` SQL function
3. Update completion handler to use atomic claiming

---

## Part 6: Files to Create/Modify

### New Files

```
docs/ticket-specs/TAS-73/
├── research-findings.md (this file)
├── implementation-plan.md (next step)
└── test-results.md (after testing)

cargo-make/scripts/multi-deploy/
├── start-orchestration-cluster.sh
├── start-worker-cluster.sh
├── stop-all-clusters.sh
└── README.md

tests/integration/concurrency/
├── mod.rs
├── thundering_herd_test.rs
├── step_claiming_contention_test.rs
├── orchestrator_race_test.rs
├── duplicate_result_test.rs
└── concurrent_finalization_test.rs
```

### Modified Files

```
migrations/
└── 20260117XXXXXX_tas_73_identity_hash_unique.sql (new migration)

tests/common/
├── mod.rs (export new types)
├── orchestration_cluster.rs (new)
└── integration_test_manager.rs (extend for clusters)

config/tasker/base/
└── orchestration.toml (add instance_id support if needed)
```

---

## Part 7: Success Criteria

TAS-73 will be considered complete when:

1. **No data corruption:** Identity hash uniqueness enforced at database level
2. **Exactly-once semantics:** Each step processed exactly once across N workers
3. **Correct convergence:** Fan-out/fan-in produces correct results with multiple orchestrators
4. **Graceful degradation:** Service failures don't corrupt state
5. **Test validation:** All new concurrency tests pass with N=2 orchestration + N=2 workers

---

## Part 8: Code References

### Critical Locations

| Finding | File | Line(s) |
|---------|------|---------|
| Missing identity_hash constraint | `migrations/20260110000002_constraints_and_indexes.sql` | 183 |
| Task finalization handler | `tasker-orchestration/src/orchestration/lifecycle/task_finalization/completion_handler.rs` | 57-78 |
| State transition atomic function | `migrations/20260110000003_sql_functions.sql` | 1687-1718 |
| Step state machine guards | `tasker-shared/src/state_machine/step_state_machine.rs` | 63-136 |
| Step enqueueing | `tasker-orchestration/src/orchestration/lifecycle/step_enqueuer.rs` | 517-570 |
| Decision point find_or_create | `tasker-shared/src/models/core/workflow_step.rs` | 222-302 |
| Batch worker idempotency | `tasker-orchestration/src/orchestration/lifecycle/batch_processing/service.rs` | 127-151 |

### Documentation Cross-References

- `docs/architecture/idempotency-and-atomicity.md` - Design patterns and contracts
- `docs/architecture/states-and-lifecycles.md` - State machine definitions
- `docs/architecture/actors.md` - Orchestration actor pattern
- `docs/architecture/worker-event-systems.md` - Worker architecture

---

## Appendix A: Defense-in-Depth Layers

The Tasker system uses five layers of protection:

1. **Database Layer:** Unique constraints, partial indexes, foreign keys
2. **Transaction Layer:** Atomic multi-step operations within single transaction
3. **State Machine Layer:** Guards prevent invalid state transitions
4. **Application Layer:** Idempotency checks (find_or_create, existence checks)
5. **Message Queue Layer:** PGMQ visibility timeout + state-based claiming

This layered approach means most operations have 2-3 independent protection mechanisms.

---

## Appendix B: Concurrent Scenario Analysis

### Scenario: Two Orchestrators Initialize Same Task

```
T0: A receives TaskRequest (identity_hash = "abc123")
T0: B receives TaskRequest (identity_hash = "abc123")
T1: A: BEGIN transaction
T1: B: BEGIN transaction
T2: A: INSERT INTO tasks (identity_hash = "abc123")  ← SUCCEEDS
T2: B: INSERT INTO tasks (identity_hash = "abc123")  ← CURRENTLY SUCCEEDS (BUG)
T3: A: COMMIT
T3: B: COMMIT
T4: Two tasks exist with same identity_hash ← DATA CORRUPTION

WITH FIX:
T2: B: INSERT INTO tasks ← UNIQUE CONSTRAINT VIOLATION
T3: B: Catches error, returns existing task
```

### Scenario: Two Workers Claim Same Step

```
T0: Worker A reads step from PGMQ (vt=30s)
T0: Worker B reads step from PGMQ → EMPTY (message claimed by A)
T1: A transitions Pending → InProgress (succeeds)
T2: A processes step
T3: A sends result to orchestration
T4: Result processed, step marked Complete

FALLBACK CASE (vt expires before A completes):
T30: Message visible again
T31: Worker C reads message
T32: C attempts transition → FAILS (already InProgress)
T33: C discards message (idempotent)
```

---

**Report Status:** Complete
**Next Step:** Create implementation plan based on findings
