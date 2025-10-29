# TAS-57: Exponential Backoff Logic Consolidation and Verification

**Status**: In Progress
**Priority**: Critical
**Created**: 2025-10-29
**Branch**: `jcoletaylor/tas-57-exponential-backoff-verification`

## Problem Statement

The tasker-core distributed workflow orchestration system has multiple, potentially conflicting implementations of exponential backoff logic for step retry coordination. This creates risks of:

1. **Configuration Conflicts**: Three different maximum backoff values across the system
2. **Race Conditions**: No atomic guarantees on backoff updates from concurrent orchestrators
3. **Implementation Divergence**: Dual calculation paths (Rust vs SQL) that could produce different results
4. **Timing Issues**: Potential stale data in readiness checks

## Research Findings

### 1. Multiple Backoff Calculation Points

#### 1.1 SQL Function (Primary Issue)
**Location**: `migrations/20250927000000_add_waiting_for_retry_state.sql:87-101`

**Function**: `calculate_step_next_retry_time()`

```sql
SELECT CASE
    WHEN backoff_request_seconds IS NOT NULL AND last_attempted_at IS NOT NULL THEN
        last_attempted_at + (backoff_request_seconds * interval '1 second')
    WHEN failure_time IS NOT NULL THEN
        failure_time + (LEAST(power(2, COALESCE(attempts, 1)) * interval '1 second', interval '30 seconds'))
    ELSE NULL
END
```

**Issues**:
- Hardcoded 30-second maximum (conflicts with TOML's 300s)
- Hardcoded `power(2, attempts)` exponential base (not configurable)
- Fallback path uses different time source (`failure_time` vs `last_attempted_at`)

#### 1.2 Rust BackoffCalculator (Secondary)
**Location**: `tasker-orchestration/src/orchestration/backoff_calculator.rs:240-285`

**Method**: `apply_exponential_backoff()`

```rust
let exponential_delay = base_delay * multiplier.powi(attempts as i32);
let mut delay_seconds = exponential_delay.min(self.config.max_delay_seconds as f64) as u32;

if self.config.jitter_enabled {
    delay_seconds = self.apply_jitter(delay_seconds);
}

sqlx::query!(
    "UPDATE tasker_workflow_steps SET backoff_request_seconds = $1 WHERE workflow_step_uuid = $2",
    delay_seconds as i32,
    step_uuid
)
```

**Issues**:
- No row-level locking (race condition risk)
- Doesn't update `last_attempted_at` (timing inconsistency)
- Direct UPDATE with no transaction wrapper

#### 1.3 MetadataProcessor (Coordination Layer)
**Location**: `tasker-orchestration/src/orchestration/lifecycle/result_processing/metadata_processor.rs:29-159`

**Method**: `process_metadata()`

Correctly prioritizes worker-provided backoff hints, but delegates to BackoffCalculator which has the race condition issues.

### 2. Configuration Value Conflicts

| Source | Location | Max Backoff Value |
|--------|----------|-------------------|
| **TOML Config** | `config/tasker/base/orchestration.toml:6` | 300 seconds |
| **Rust Default** | `tasker-shared/src/config/tasker.rs` | 60 seconds |
| **SQL Hardcoded** | Migration line 98 | 30 seconds |

**Additional Conflicts**:
- Jitter percentage: TOML (10%) vs Rust default (50%)
- Exponential multiplier: Configurable in TOML (2.0) vs hardcoded in SQL (power(2, ...))

### 3. Backoff Flow Through System

```
WORKER EXECUTION FAILS
      ↓
StepExecutionResult with orchestration_metadata
  - headers: {"Retry-After": "60"}
  - backoff_hint: {type: "ServerRequested", delay_seconds: 120}
      ↓
OrchestrationResultProcessor.handle_step_result_message()
      ↓
MetadataProcessor.process_metadata()
      ↓
BackoffCalculator.calculate_and_apply_backoff()
      ↓
[RACE CONDITION POSSIBLE]
UPDATE backoff_request_seconds (no lock)
      ↓
READINESS CHECK (SQL)
      ↓
calculate_step_next_retry_time()
  - Path 1: backoff_request_seconds set → use it
  - Path 2: backoff_request_seconds NULL → SQL exponential with 30s max
      ↓
evaluate_step_state_readiness()
  - ready = (state IN pending/waiting_for_retry) AND next_retry_time <= NOW()
      ↓
STEP ENQUEUED (if ready)
```

### 4. Race Condition Scenarios

**Scenario 1: Concurrent Backoff Updates**
1. Orchestrator A processes step failure, calculates 4-second backoff
2. Orchestrator B processes same step failure, calculates 8-second backoff (different attempt count)
3. Both execute UPDATE without lock
4. Last write wins (either 4s or 8s, non-deterministic)
5. Lost update - one calculation is discarded

**Scenario 2: Readiness Check with Stale Data**
1. Worker sets step to WaitingForRetry state
2. Orchestrator A begins backoff calculation
3. Orchestrator B runs readiness check before A finishes
4. `backoff_request_seconds` is still NULL
5. SQL fallback uses hardcoded exponential (30s max)
6. Step becomes ready earlier than intended by Rust calculation

**Scenario 3: Timing Window**
1. Rust applies backoff_request_seconds = 60
2. Rust doesn't update last_attempted_at
3. Worker later updates last_attempted_at (different timestamp)
4. SQL calculation: `last_attempted_at + backoff_request_seconds`
5. Result: Incorrect retry time due to timestamp mismatch

## Proposed Solution

### Architecture Decision

Based on project context (greenfield alpha, no production dependencies):

1. **Single Source of Truth**: TOML configuration (`config/tasker/base/orchestration.toml`)
2. **Standard Max Backoff**: 60 seconds (balance between retry speed and system load)
3. **Atomic Updates**: Database row-level locking via `SELECT FOR UPDATE`
4. **SQL Parameterization**: Pass TOML config values to SQL functions as parameters
5. **No Backward Compatibility**: Direct rewrites, aggressive schema changes acceptable

### Implementation Phases

#### Phase 1: Configuration Alignment

**File**: `config/tasker/base/orchestration.toml`
```toml
[backoff]
default_backoff_seconds = [1, 2, 4, 8, 16, 32]
max_backoff_seconds = 60                 # CHANGED from 300
backoff_multiplier = 2.0
jitter_enabled = true
jitter_max_percentage = 0.1
```

**File**: `tasker-shared/src/config/tasker.rs`
```rust
impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            default_backoff_seconds: vec![1, 2, 4, 8, 16, 32],
            max_backoff_seconds: 60,        // Aligned with TOML
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            jitter_max_percentage: 0.1,     // CHANGED from 0.5
            reenqueue_delays: ReenqueueDelays::default(),
        }
    }
}
```

#### Phase 2: SQL Function Rewrite

**File**: `migrations/20250927000000_add_waiting_for_retry_state.sql` (REWRITE IN PLACE)

**Function 1**: `calculate_step_next_retry_time()` with parameters
```sql
CREATE OR REPLACE FUNCTION calculate_step_next_retry_time(
    backoff_request_seconds INTEGER,
    last_attempted_at TIMESTAMP,
    failure_time TIMESTAMP,
    attempts INTEGER,
    p_max_backoff_seconds INTEGER DEFAULT 60,
    p_backoff_multiplier NUMERIC DEFAULT 2.0
) RETURNS TIMESTAMP AS $$
BEGIN
    RETURN CASE
        -- Primary path: Use Rust-calculated backoff
        WHEN backoff_request_seconds IS NOT NULL AND last_attempted_at IS NOT NULL THEN
            last_attempted_at + (backoff_request_seconds * interval '1 second')

        -- Fallback path: Calculate exponentially with config params
        WHEN failure_time IS NOT NULL THEN
            failure_time + (
                LEAST(
                    power(p_backoff_multiplier, COALESCE(attempts, 1)) * interval '1 second',
                    p_max_backoff_seconds * interval '1 second'
                )
            )

        ELSE NULL
    END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

**Function 2**: New atomic update helper
```sql
CREATE OR REPLACE FUNCTION set_step_backoff_atomic(
    p_step_uuid UUID,
    p_backoff_seconds INTEGER
) RETURNS BOOLEAN AS $$
DECLARE
    v_updated BOOLEAN;
BEGIN
    UPDATE tasker_workflow_steps
    SET backoff_request_seconds = p_backoff_seconds,
        last_attempted_at = NOW()
    WHERE workflow_step_uuid = p_step_uuid;

    GET DIAGNOSTICS v_updated = FOUND;
    RETURN v_updated;
END;
$$ LANGUAGE plpgsql;
```

**Function 3**: Update `get_step_readiness_status_batch()` to pass parameters
- Extract max_backoff_seconds and multiplier from system config table or function parameters
- Pass to `calculate_step_next_retry_time()` calls

#### Phase 3: Rust BackoffCalculator Refactor

**File**: `tasker-orchestration/src/orchestration/backoff_calculator.rs`

**New Method**: Atomic update with row-level locking
```rust
async fn update_backoff_atomic(
    &self,
    step_uuid: &Uuid,
    delay_seconds: u32,
) -> Result<(), BackoffError> {
    let mut tx = self.pool.begin().await
        .map_err(BackoffError::Database)?;

    // Acquire row-level lock
    sqlx::query!(
        "SELECT workflow_step_uuid
         FROM tasker_workflow_steps
         WHERE workflow_step_uuid = $1
         FOR UPDATE",
        step_uuid
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(BackoffError::Database)?;

    // Update with both backoff and timestamp
    sqlx::query!(
        "UPDATE tasker_workflow_steps
         SET backoff_request_seconds = $1,
             last_attempted_at = NOW()
         WHERE workflow_step_uuid = $2",
        delay_seconds as i32,
        step_uuid
    )
    .execute(&mut *tx)
    .await
    .map_err(BackoffError::Database)?;

    tx.commit().await
        .map_err(BackoffError::Database)?;

    Ok(())
}
```

**Updated Method**: `apply_server_requested_backoff()`
```rust
async fn apply_server_requested_backoff(
    &self,
    step_uuid: &Uuid,
    retry_after_seconds: u32,
) -> Result<BackoffResult, BackoffError> {
    let delay_seconds = retry_after_seconds.min(self.config.max_delay_seconds);

    // Use atomic update instead of direct query
    self.update_backoff_atomic(step_uuid, delay_seconds).await?;

    Ok(BackoffResult {
        delay_seconds,
        backoff_type: BackoffType::ServerRequested,
        next_retry_at: Utc::now() + Duration::seconds(delay_seconds as i64),
    })
}
```

**Updated Method**: `apply_exponential_backoff()`
```rust
async fn apply_exponential_backoff(
    &self,
    step_uuid: &Uuid,
    _context: &BackoffContext,
) -> Result<BackoffResult, BackoffError> {
    let step = sqlx::query!(
        "SELECT attempts FROM tasker_workflow_steps WHERE workflow_step_uuid = $1",
        step_uuid
    )
    .fetch_one(&self.pool)
    .await
    .map_err(BackoffError::Database)?;

    let attempts = step.attempts.unwrap_or(0) as u32;
    let base_delay = self.config.base_delay_seconds as f64;
    let multiplier = self.config.multiplier;
    let exponential_delay = base_delay * multiplier.powi(attempts as i32);

    let mut delay_seconds = exponential_delay.min(self.config.max_delay_seconds as f64) as u32;

    if self.config.jitter_enabled {
        delay_seconds = self.apply_jitter(delay_seconds);
    }

    // Use atomic update instead of direct query
    self.update_backoff_atomic(step_uuid, delay_seconds).await?;

    Ok(BackoffResult {
        delay_seconds,
        backoff_type: BackoffType::Exponential,
        next_retry_at: Utc::now() + Duration::seconds(delay_seconds as i64),
    })
}
```

#### Phase 4: Testing

**Unit Tests**: `tasker-orchestration/src/orchestration/backoff_calculator.rs`
- Test exponential calculation accuracy
- Test jitter bounds (±10%)
- Test max cap enforcement (60 seconds)
- Test Retry-After header parsing

**SQL Function Tests**: New `tasker-shared/tests/sql_backoff_functions_test.rs`
- Test parameterization with various max values
- Test exponential calculation matches Rust
- Test boundary conditions (attempts 0, 10, 20)

**Concurrency Tests**: New `tasker-orchestration/tests/concurrent_backoff_test.rs`
- Simulate 10 concurrent orchestrators updating same step
- Verify no lost updates (all attempts recorded)
- Verify lock acquisition/release
- Test lock timeout behavior

**Integration Tests**: Existing integration test suites
- End-to-end: Worker failure → Backoff applied → Readiness respects delay
- Verify SQL fallback when backoff_request_seconds NULL
- Verify consistency between Rust and SQL calculations

#### Phase 5: Documentation

**Architecture Docs**:
- `docs/task-and-step-readiness-and-execution.md`: Update SQL function signatures
- `docs/states-and-lifecycles.md`: Document WaitingForRetry backoff timing

**ADR**: New `docs/architecture-decisions/TAS-57-backoff-consolidation.md`
- Problem: Configuration conflicts, race conditions
- Decision: TOML as single source, atomic updates, 60s max
- Consequences: Simplified config, guaranteed consistency, slight performance overhead from locking

**Code Comments**:
- Explain dual-path strategy (Rust primary, SQL fallback)
- Document locking strategy and retry behavior
- Clarify when each calculation path is used

## Success Criteria

### Functional Requirements
✅ Single maximum backoff value (60 seconds) enforced across all paths
✅ TOML configuration is single source of truth for all backoff parameters
✅ No race conditions on concurrent backoff updates (atomic transactions)
✅ SQL fallback calculation uses same configuration as Rust
✅ `last_attempted_at` updated consistently with `backoff_request_seconds`

### Testing Requirements
✅ All existing tests pass after changes
✅ New unit tests cover calculation edge cases
✅ Concurrency tests demonstrate no lost updates
✅ Integration tests verify end-to-end correctness

### Documentation Requirements
✅ Architecture docs reflect current implementation
✅ ADR documents decisions and rationale
✅ Inline comments explain complex logic

### Verification Steps
✅ Database reset and migration successful
✅ All SQL functions have expected signatures
✅ Manual Docker verification with failed steps
✅ Backoff timing verified via logs and database inspection

## Risk Assessment

### Low Risk
- Configuration alignment (simple value changes)
- Adding database locking (standard PostgreSQL feature)
- Documentation updates

### Medium Risk
- SQL function rewrite (breaking change, but alpha stage acceptable)
- Race condition fixes (complexity in transaction handling)

### Mitigation Strategies
- Comprehensive testing before database reset
- Manual verification with Docker integration tests
- Keep old migration file backed up during development

## Rollout Plan

Since this is greenfield alpha development:

1. **Implement all changes on feature branch**
2. **Drop and recreate test database** with updated migrations
3. **Run full test suite** - all tests must pass
4. **Manual verification** via Docker compose integration tests
5. **Merge to main** once validated
6. **Document in release notes** for future alpha users

No gradual rollout or feature flags needed - clean break acceptable at this stage.

## Future Enhancements (Post-Alpha)

- **Configuration Table**: Store backoff config in database table for runtime updates
- **Metrics**: Add OpenTelemetry metrics for backoff application and lock contention
- **Adaptive Backoff**: Adjust multiplier based on system load or error patterns
- **Backoff Policies**: Per-namespace or per-handler backoff configuration

## Related Tickets

- **TAS-41**: Enhanced state machines with processor tracking (provides state audit trail)
- **TAS-42**: WaitingForRetry state introduction (created the dual-path problem)
- **TAS-54**: Processor ownership removal (related to concurrent access patterns)

## References

- [States and Lifecycles Documentation](../states-and-lifecycles.md)
- [Task and Step Readiness Documentation](../task-and-step-readiness-and-execution.md)
- [Events and Commands Documentation](../events-and-commands.md)
- [BackoffCalculator Implementation](../../tasker-orchestration/src/orchestration/backoff_calculator.rs)
- [SQL Migration 20250927000000](../../migrations/20250927000000_add_waiting_for_retry_state.sql)
