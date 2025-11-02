# TAS-37: Task Finalizer Race Condition - Revised Plan with Dedicated SQL Function

## Recommended Solution: Dedicated `claim_task_for_finalization` Function

Create a separate SQL function specifically for finalization claiming, keeping it completely independent from the existing `claim_ready_tasks` function.

### Why This Approach is Better

1. **Transactional safety** - Claims are properly released on rollback
2. **Connection pool friendly** - No connection-level state to manage
3. **Consistent architecture** - Uses same proven pattern as step claiming
4. **Simpler debugging** - Claims visible in database
5. **No side effects** - Completely isolated from existing functionality

### Implementation

#### Step 1: Create New SQL Function

```sql
-- Migration: 20250818000001_add_finalization_claiming.sql

CREATE OR REPLACE FUNCTION public.claim_task_for_finalization(
    p_task_uuid uuid,
    p_processor_id character varying,
    p_timeout_seconds integer DEFAULT 30
)
RETURNS TABLE(
    claimed boolean,
    already_claimed_by character varying,
    task_state character varying,
    message character varying
)
LANGUAGE plpgsql
AS $$
DECLARE
    v_current_claimed_by character varying;
    v_current_claimed_at timestamp;
    v_task_complete boolean;
    v_task_exists boolean;
    v_has_completed_steps boolean;
    v_execution_status character varying;
BEGIN
    -- First check if task exists and get its current state
    SELECT
        EXISTS(t.task_uuid),
        t.complete,
        t.claimed_by,
        t.claimed_at,
        tec.execution_status,
        EXISTS(
            SELECT 1 FROM tasker_workflow_steps ws
            WHERE ws.task_uuid = t.task_uuid
            AND ws.state IN ('completed', 'failed')
        )
    INTO
        v_task_exists,
        v_task_complete,
        v_current_claimed_by,
        v_current_claimed_at,
        v_execution_status,
        v_has_completed_steps
    FROM tasker_tasks t
    LEFT JOIN LATERAL (
        SELECT execution_status
        FROM get_task_execution_context(t.task_uuid)
    ) tec ON true
    WHERE t.task_uuid = p_task_uuid;

    -- Task doesn't exist
    IF NOT v_task_exists THEN
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            'not_found'::character varying,
            'Task not found'::character varying;
        RETURN;
    END IF;

    -- Task already complete
    IF v_task_complete THEN
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            'complete'::character varying,
            'Task already complete'::character varying;
        RETURN;
    END IF;

    -- Task doesn't need finalization yet (no completed/failed steps)
    IF NOT v_has_completed_steps THEN
        RETURN QUERY SELECT
            false,
            NULL::character varying,
            v_execution_status,
            'Task has no completed or failed steps'::character varying;
        RETURN;
    END IF;

    -- Check if already claimed and claim still valid
    IF v_current_claimed_by IS NOT NULL AND
       v_current_claimed_at > (NOW() - (p_timeout_seconds || ' seconds')::interval) THEN
        -- Already claimed by someone else
        IF v_current_claimed_by != p_processor_id THEN
            RETURN QUERY SELECT
                false,
                v_current_claimed_by,
                v_execution_status,
                'Task already claimed for finalization'::character varying;
            RETURN;
        ELSE
            -- We already have the claim - extend it
            UPDATE tasker_tasks
            SET claimed_at = NOW(),
                updated_at = NOW()
            WHERE task_uuid = p_task_uuid
            AND claimed_by = p_processor_id;

            RETURN QUERY SELECT
                true,
                p_processor_id,
                v_execution_status,
                'Claim extended'::character varying;
            RETURN;
        END IF;
    END IF;

    -- Try to claim the task atomically
    UPDATE tasker_tasks
    SET claimed_by = p_processor_id,
        claimed_at = NOW(),
        claim_timeout_seconds = p_timeout_seconds,
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND (claimed_by IS NULL
             OR claimed_at < (NOW() - (claim_timeout_seconds || ' seconds')::interval)
             OR claimed_by = p_processor_id)
        AND complete = false
    RETURNING
        true,
        claimed_by,
        (SELECT execution_status FROM get_task_execution_context(task_uuid)),
        'Successfully claimed for finalization'::character varying
    INTO claimed, already_claimed_by, task_state, message;

    -- If no rows updated, someone else grabbed it
    IF NOT FOUND THEN
        RETURN QUERY SELECT
            false,
            v_current_claimed_by,
            v_execution_status,
            'Failed to claim - race condition'::character varying;
        RETURN;
    END IF;

    RETURN QUERY SELECT claimed, already_claimed_by, task_state, message;
END;
$$;

-- Companion function to release finalization claim
CREATE OR REPLACE FUNCTION public.release_finalization_claim(
    p_task_uuid uuid,
    p_processor_id character varying
)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NULL,
        claimed_by = NULL,
        updated_at = NOW()
    WHERE task_uuid = p_task_uuid
        AND claimed_by = p_processor_id;

    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;
    RETURN v_rows_updated > 0;
END;
$$;

-- Add index to support finalization claiming queries
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_finalization_claim
ON tasker_tasks(task_uuid, complete, claimed_by, claimed_at)
WHERE complete = false;

-- Add comments for documentation
COMMENT ON FUNCTION claim_task_for_finalization IS
'Atomically claim a task for finalization processing. Prevents multiple processors from finalizing the same task simultaneously.';

COMMENT ON FUNCTION release_finalization_claim IS
'Release a finalization claim when done processing or on error. Only releases if caller owns the claim.';
```

#### Step 2: Create Rust Finalization Claimer

```rust
// src/orchestration/finalization_claimer.rs

use sqlx::PgPool;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FinalizationClaimResult {
    pub claimed: bool,
    pub already_claimed_by: Option<String>,
    pub task_state: Option<String>,
    pub message: Option<String>,
}

pub struct FinalizationClaimer {
    pool: PgPool,
    processor_id: String,
    default_timeout_seconds: i32,
}

impl FinalizationClaimer {
    pub fn new(pool: PgPool, processor_id: String) -> Self {
        Self {
            pool,
            processor_id,
            default_timeout_seconds: 30, // Short timeout for finalization
        }
    }

    /// Attempt to claim a task for finalization
    pub async fn claim_task(
        &self,
        task_uuid: Uuid,
    ) -> Result<FinalizationClaimResult, sqlx::Error> {
        let start = std::time::Instant::now();
        
        let result = sqlx::query_as::<_, FinalizationClaimResult>(
            "SELECT * FROM claim_task_for_finalization($1, $2, $3)"
        )
        .bind(task_uuid)
        .bind(&self.processor_id)
        .bind(self.default_timeout_seconds)
        .fetch_one(&self.pool)
        .await?;

        // Record metrics
        let duration = start.elapsed().as_secs_f64();
        metrics::histogram!("finalization_claim_duration_seconds", duration);
        metrics::counter!("finalization_claim_attempts_total", 1);
        
        if result.claimed {
            metrics::counter!("finalization_claim_success_total", 1);
            debug!(
                task_uuid = %task_uuid,
                processor_id = %self.processor_id,
                duration_ms = duration * 1000.0,
                message = ?result.message,
                "Successfully claimed task for finalization"
            );
        } else {
            metrics::counter!("finalization_claim_contention_total", 1);
            
            // Track specific contention reasons
            if let Some(ref msg) = result.message {
                let reason = match msg.as_str() {
                    "Task already claimed for finalization" => "already_claimed",
                    "Task already complete" => "already_complete",
                    "Task has no completed or failed steps" => "not_ready",
                    "Failed to claim - race condition" => "race_condition",
                    _ => "other",
                };
                metrics::counter!("finalization_claim_contention_by_reason_total", 1, "reason" => reason);
            }
            
            debug!(
                task_uuid = %task_uuid,
                already_claimed_by = ?result.already_claimed_by,
                task_state = ?result.task_state,
                duration_ms = duration * 1000.0,
                message = ?result.message,
                "Could not claim task for finalization"
            );
        }

        Ok(result)
    }

    /// Release a finalization claim
    pub async fn release_claim(
        &self,
        task_uuid: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let start = std::time::Instant::now();
        
        let released = sqlx::query_scalar::<_, bool>(
            "SELECT release_finalization_claim($1, $2)"
        )
        .bind(task_uuid)
        .bind(&self.processor_id)
        .fetch_one(&self.pool)
        .await?;

        // Record metrics
        let duration = start.elapsed().as_secs_f64();
        metrics::histogram!("finalization_claim_release_duration_seconds", duration);
        metrics::counter!("finalization_claim_release_attempts_total", 1);

        if released {
            metrics::counter!("finalization_claim_release_success_total", 1);
            debug!(
                task_uuid = %task_uuid,
                processor_id = %self.processor_id,
                duration_ms = duration * 1000.0,
                "Released finalization claim"
            );
        } else {
            metrics::counter!("finalization_claim_release_not_owned_total", 1);
            debug!(
                task_uuid = %task_uuid,
                processor_id = %self.processor_id,
                duration_ms = duration * 1000.0,
                "Claim was not owned by this processor"
            );
        }

        Ok(released)
    }

    /// Extend an existing claim (heartbeat)
    pub async fn extend_claim(
        &self,
        task_uuid: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let start = std::time::Instant::now();
        
        // The claim_task_for_finalization function handles extension
        // when called with same processor_id
        let result = self.claim_task(task_uuid).await?;
        let extended = result.claimed && result.message == Some("Claim extended".to_string());
        
        // Record metrics specifically for extensions
        let duration = start.elapsed().as_secs_f64();
        metrics::histogram!("finalization_claim_extend_duration_seconds", duration);
        metrics::counter!("finalization_claim_extend_attempts_total", 1);
        
        if extended {
            metrics::counter!("finalization_claim_extend_success_total", 1);
            debug!(
                task_uuid = %task_uuid,
                processor_id = %self.processor_id,
                duration_ms = duration * 1000.0,
                "Extended finalization claim"
            );
        } else {
            metrics::counter!("finalization_claim_extend_failed_total", 1);
            debug!(
                task_uuid = %task_uuid,
                processor_id = %self.processor_id,
                duration_ms = duration * 1000.0,
                "Failed to extend finalization claim"
            );
        }
        
        Ok(extended)
    }
}
```

#### Step 3: Update OrchestrationResultProcessor

```rust
// src/orchestration/result_processor.rs

use crate::orchestration::finalization_claimer::{FinalizationClaimer, FinalizationClaimResult};

pub struct OrchestrationResultProcessor {
    pool: PgPool,
    task_finalizer: Arc<TaskFinalizer>,
    finalization_claimer: FinalizationClaimer,
    processor_id: String,
    // ... other fields
}

impl OrchestrationResultProcessor {
    pub fn new(/* params */) -> Self {
        let processor_id = format!("result_processor_{}", uuid::Uuid::new_v4());
        let finalization_claimer = FinalizationClaimer::new(
            pool.clone(),
            processor_id.clone()
        );

        Self {
            pool,
            task_finalizer,
            finalization_claimer,
            processor_id,
            // ... other fields
        }
    }

    pub async fn handle_step_result(
        &self,
        step_uuid: Uuid,
        status: String,
        execution_time_ms: u64,
        worker_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ... existing step processing code ...

        if matches!(status.as_str(), "success" | "failed") {
            if let Ok(Some(workflow_step)) = WorkflowStep::find_by_id(&self.pool, step_uuid).await {
                // Try to claim the task for finalization
                let claim_result = self.finalization_claimer
                    .claim_task(workflow_step.task_uuid)
                    .await?;

                if claim_result.claimed {
                    // We got the claim - proceed with finalization
                    info!(
                        task_uuid = %workflow_step.task_uuid,
                        processor_id = %self.processor_id,
                        "Claimed task for finalization"
                    );

                    // Perform finalization
                    let finalization_result = match self.task_finalizer
                        .finalize_task(workflow_step.task_uuid, false)
                        .await
                    {
                        Ok(result) => {
                            info!(
                                task_uuid = %workflow_step.task_uuid,
                                action = ?result.action,
                                "Task finalization completed"
                            );
                            result
                        }
                        Err(e) => {
                            error!(
                                task_uuid = %workflow_step.task_uuid,
                                error = %e,
                                "Task finalization failed"
                            );
                            // Release claim on error
                            let _ = self.finalization_claimer
                                .release_claim(workflow_step.task_uuid)
                                .await;
                            return Err(e.into());
                        }
                    };

                    // Release the claim after finalization
                    let _ = self.finalization_claimer
                        .release_claim(workflow_step.task_uuid)
                        .await;

                } else {
                    // Another processor is handling or will handle finalization
                    debug!(
                        task_uuid = %workflow_step.task_uuid,
                        already_claimed_by = ?claim_result.already_claimed_by,
                        reason = ?claim_result.message,
                        "Task finalization not needed or already claimed"
                    );
                }
            }
        }

        Ok(())
    }
}
```

#### Step 4: Add Monitoring Table (Optional but Recommended)

```sql
-- Track finalization attempts for debugging and metrics
CREATE TABLE IF NOT EXISTS tasker_finalization_attempts (
    id BIGSERIAL PRIMARY KEY,
    task_uuid uuid NOT NULL,
    processor_id character varying NOT NULL,
    claimed boolean NOT NULL,
    already_claimed_by character varying,
    task_state character varying,
    message character varying,
    attempted_at timestamp DEFAULT NOW() NOT NULL
);

CREATE INDEX idx_finalization_attempts_task_uuid
ON tasker_finalization_attempts(task_uuid, attempted_at DESC);

CREATE INDEX idx_finalization_attempts_processor
ON tasker_finalization_attempts(processor_id, attempted_at DESC);

-- Modify claim_task_for_finalization to log attempts
-- Add before the RETURN statements:
INSERT INTO tasker_finalization_attempts
    (task_uuid, processor_id, claimed, already_claimed_by, task_state, message)
VALUES
    (p_task_uuid, p_processor_id, claimed, already_claimed_by, task_state, message);
```

## Implementation Plan

### Phase 1: Database Changes (Day 1 Morning)
1. Create and test SQL functions in development
2. Verify claim/release behavior
3. Test timeout expiration
4. Add monitoring table

### Phase 2: Rust Implementation (Day 1 Afternoon)
1. Implement `FinalizationClaimer` module
2. Update `OrchestrationResultProcessor`
3. Add proper error handling and logging
4. Unit tests for claiming logic

### Phase 3: Integration Testing (Day 2)
1. Test with multiple processors
2. Simulate concurrent step completions
3. Verify claim timeout and release
4. Load testing with high concurrency

### Phase 4: Deployment (Day 3)
1. Deploy SQL migration
2. Deploy updated processors with feature flag
3. Monitor in staging for 24 hours
4. Production rollout

## Comprehensive Metrics Collection

### Finalization Claiming Metrics

The implementation includes comprehensive metrics collection for observability and debugging:

#### Core Claiming Metrics
- **`finalization_claim_duration_seconds`** (histogram): Time taken to attempt claiming
- **`finalization_claim_attempts_total`** (counter): Total claim attempts across all processors  
- **`finalization_claim_success_total`** (counter): Successfully acquired claims
- **`finalization_claim_contention_total`** (counter): Failed claim attempts due to contention

#### Contention Analysis Metrics
- **`finalization_claim_contention_by_reason_total`** (counter with labels): Contention breakdown by reason:
  - `reason="already_claimed"`: Another processor owns the claim
  - `reason="already_complete"`: Task was already finalized
  - `reason="not_ready"`: Task has no completed/failed steps
  - `reason="race_condition"`: Lost race condition during claim attempt
  - `reason="other"`: Unknown contention reason

#### Claim Management Metrics
- **`finalization_claim_release_duration_seconds`** (histogram): Time to release claims
- **`finalization_claim_release_attempts_total`** (counter): Total release attempts
- **`finalization_claim_release_success_total`** (counter): Successful releases
- **`finalization_claim_release_not_owned_total`** (counter): Release attempts on non-owned claims

#### Heartbeat Extension Metrics
- **`finalization_claim_extend_duration_seconds`** (histogram): Time to extend claims
- **`finalization_claim_extend_attempts_total`** (counter): Total extension attempts
- **`finalization_claim_extend_success_total`** (counter): Successful extensions
- **`finalization_claim_extend_failed_total`** (counter): Failed extensions

### Monitoring Dashboard Queries

#### Claim Success Rate
```promql
rate(finalization_claim_success_total[5m]) / rate(finalization_claim_attempts_total[5m]) * 100
```

#### Contention Rate by Reason
```promql
rate(finalization_claim_contention_by_reason_total[5m])
```

#### Average Claim Duration
```promql
histogram_quantile(0.95, rate(finalization_claim_duration_seconds_bucket[5m]))
```

#### Processor Activity
```promql
rate(finalization_claim_attempts_total[5m]) by (processor_id)
```

### Alert Conditions

#### High Contention Alert
```yaml
alert: HighFinalizationContention
expr: rate(finalization_claim_contention_total[5m]) / rate(finalization_claim_attempts_total[5m]) > 0.5
for: 2m
labels:
  severity: warning
annotations:
  summary: "High finalization claim contention detected"
```

#### Slow Claim Operations
```yaml
alert: SlowFinalizationClaims
expr: histogram_quantile(0.95, rate(finalization_claim_duration_seconds_bucket[5m])) > 0.5
for: 1m
labels:
  severity: warning
annotations:
  summary: "Finalization claims taking longer than expected"
```

## Advantages Over Advisory Locks

1. **Transactional Safety**: Claims automatically released on transaction rollback
2. **Visibility**: Can query database to see current claims
3. **Debugging**: Full audit trail in `tasker_finalization_attempts` table
4. **Connection Pool Safe**: No connection-level state
5. **Consistent Pattern**: Same approach as existing step claiming
6. **Comprehensive Metrics**: Built-in observability for monitoring and alerting

## Success Metrics

- **Zero "unclear state" logs** after implementation
- **Claim success rate > 95%** for first processor attempting
- **No duplicate finalizations** in production
- **Claim duration < 100ms** for typical finalization

## Rollback Plan

Simple and safe:
1. Stop calling `claim_task_for_finalization`
2. Functions remain in database (harmless)
3. No data cleanup needed
