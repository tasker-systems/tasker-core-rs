# DLQ Refactoring Plan: Views Integration and Function Decomposition

**Created**: 2025-11-01
**Status**: Planned
**Related**: TAS-49 Phase 1 DLQ Implementation

## Overview

This document outlines a comprehensive refactoring plan for the DLQ (Dead Letter Queue) implementation addressing three key areas:

### 1. Performance Optimization (Part 2)
**Problem**: The monolithic `detect_and_transition_stale_tasks()` function (220 lines) performs expensive multi-table joins inside a FOR loop, causing O(n) I/O operations.

**Solution**:
- Extract joins into reusable `v_task_state_analysis` base view
- Create discovery function `get_stale_tasks_for_dlq()` that queries view ONCE
- Main function reduced to ~90 lines with expensive joins outside loop
- Replace NOT EXISTS with LEFT JOIN anti-join pattern for better query planner optimization
- **Expected**: 50-70% reduction in execution time

### 2. Type Safety (Part 0)
**Problem**: `action_taken` field is a String with hardcoded comparisons like `r.action_taken == "transition_failed"`.

**Solution**:
- Create `StalenessAction` enum with 5 variants
- sqlx Type derive for automatic SQL mapping
- Helper methods: `is_failure()`, `dlq_created()`, `transition_succeeded()`
- Compile-time validation prevents typos and missing cases

### 3. JSONB Indexing (Part 3)
**Problem**: Heavy reliance on JSONB path queries like `configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes'` with no indexes.

**Solution**:
- Start with GIN index on `configuration->'lifecycle'` (Option A)
- Low risk, no schema changes, maintains flexibility
- Benchmark and measure improvement
- Future option: Denormalize to relational columns if needed (Option B)

### 4. Application Integration (Part 1)
**Problem**: Three well-designed views exist but are completely unused by application code.

**Solution**:
- Integrate `v_dlq_dashboard` into `get_stats()` endpoint
- Add investigation queue endpoint using `v_dlq_investigation_queue`
- Add proactive staleness monitoring endpoint using `v_task_staleness_monitoring`

**Implementation Order**: Part 2 → Part 3 → Part 1
- Part 2 creates/refactors the views that Part 1 relies on
- Part 3 optimizes views before application integration
- Part 1 last ensures we're integrating finalized, tested, optimized views
- Part 0 can be done anytime (independent)

## Part 0: Type Safety Improvements (Rust Application)

### Problem: String-based action_taken Field

**Current Code** (`tasker-orchestration/src/orchestration/staleness_detector.rs:72`):
```rust
/// Action taken by SQL function
///
/// Values:
/// - "would_transition_to_dlq_and_error" (dry_run=true)
/// - "transitioned_to_dlq_and_error" (both succeeded)
/// - "moved_to_dlq_only" (DLQ succeeded, transition failed)
/// - "transitioned_to_error_only" (transition succeeded, DLQ failed)
/// - "transition_failed" (both failed)
pub action_taken: String,
```

**Issue**: String comparison at runtime (line 320):
```rust
let failures = results
    .iter()
    .filter(|r| r.action_taken == "transition_failed")
    .count();
```

### Proposed: Type-safe Enum

**New Enum** (`tasker-shared/src/models/orchestration/dlq.rs` or new `staleness_actions.rs`):
```rust
use serde::{Deserialize, Serialize};
use sqlx::Type;

/// Action taken by staleness detection SQL function
///
/// Maps to the `action_taken` column returned by `detect_and_transition_stale_tasks()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum StalenessAction {
    /// Dry run mode - would have transitioned to DLQ and Error state
    #[sqlx(rename = "would_transition_to_dlq_and_error")]
    WouldTransitionToDlqAndError,

    /// Successfully moved to DLQ and transitioned to Error state
    #[sqlx(rename = "transitioned_to_dlq_and_error")]
    TransitionedToDlqAndError,

    /// DLQ entry created but state transition failed
    #[sqlx(rename = "moved_to_dlq_only")]
    MovedToDlqOnly,

    /// State transitioned to Error but DLQ entry creation failed
    #[sqlx(rename = "transitioned_to_error_only")]
    TransitionedToErrorOnly,

    /// Both DLQ creation and state transition failed
    #[sqlx(rename = "transition_failed")]
    TransitionFailed,
}

impl StalenessAction {
    /// Returns true if the action represents any kind of failure
    #[must_use]
    pub const fn is_failure(self) -> bool {
        matches!(
            self,
            Self::MovedToDlqOnly | Self::TransitionedToErrorOnly | Self::TransitionFailed
        )
    }

    /// Returns true if DLQ entry was successfully created
    #[must_use]
    pub const fn dlq_created(self) -> bool {
        matches!(
            self,
            Self::TransitionedToDlqAndError | Self::MovedToDlqOnly
        )
    }

    /// Returns true if state transition succeeded
    #[must_use]
    pub const fn transition_succeeded(self) -> bool {
        matches!(
            self,
            Self::TransitionedToDlqAndError | Self::TransitionedToErrorOnly
        )
    }
}
```

**Updated StalenessResult**:
```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StalenessResult {
    pub task_uuid: Uuid,
    pub namespace_name: String,
    pub task_name: String,
    pub current_state: String,
    pub time_in_state_minutes: i32,
    pub staleness_threshold_minutes: i32,

    /// Action taken by SQL function (now type-safe)
    pub action_taken: StalenessAction,

    pub moved_to_dlq: bool,
    pub transition_success: bool,
}
```

**Updated Metrics Code**:
```rust
// Old (line 320)
let failures = results
    .iter()
    .filter(|r| r.action_taken == "transition_failed")
    .count();

// New (type-safe)
let failures = results
    .iter()
    .filter(|r| r.action_taken.is_failure())
    .count();
```

**Benefits**:
- Compile-time validation of all action_taken values
- Exhaustive pattern matching prevents missing cases
- Self-documenting through enum variants
- Helper methods for common checks
- sqlx Type derive handles SQL string mapping

**Implementation**: This can be done independently as Part 0, before or during Part 1/2.

---

## Part 1: DLQ Views Integration into Application Layer

**Note**: This should be implemented AFTER Part 2, once views are finalized.

### Current State Analysis

**Problem**: Three well-designed database views exist but are completely unused:
- `v_dlq_dashboard` (lines 17-34 in 20251122000002_add_dlq_views.sql)
- `v_task_staleness_monitoring` (lines 57-107)
- `v_dlq_investigation_queue` (lines 131-154)

Application code in `tasker-shared/src/models/orchestration/dlq.rs` duplicates view logic with direct table queries.

### Refactoring Opportunities

#### 1.1: Use v_dlq_dashboard in DlqEntry::get_stats()

**Current Code** (dlq.rs:447-467):
```rust
pub async fn get_stats(pool: &PgPool) -> TaskerResult<Vec<DlqStats>> {
    sqlx::query_as::<_, DlqStats>(
        r#"
        SELECT
            dlq_reason,
            COUNT(*) as total_entries,
            COUNT(*) FILTER (WHERE resolution_status = 'pending') as pending_count,
            // ... manual aggregation logic
        FROM tasker_tasks_dlq
        GROUP BY dlq_reason
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| TaskerError::Database(format!("Failed to get DLQ stats: {}", e)))
}
```

**Proposed Change**:
```rust
pub async fn get_stats(pool: &PgPool) -> TaskerResult<Vec<DlqStats>> {
    sqlx::query_as::<_, DlqStats>("SELECT * FROM v_dlq_dashboard")
        .fetch_all(pool)
        .await
        .map_err(|e| TaskerError::Database(format!("Failed to get DLQ stats: {}", e)))
}
```

**Benefits**:
- Eliminates 15 lines of duplicate SQL logic
- Single source of truth for DLQ statistics
- View is pre-optimized and tested
- Easier to maintain aggregation logic

**Risk**: DlqStats struct must match view columns exactly. Need to verify field mappings.

#### 1.2: Use v_dlq_investigation_queue for Prioritized Listings

**Current Code** (dlq.rs:271-309):
```rust
pub async fn list(pool: &PgPool, params: DlqListParams) -> TaskerResult<Vec<DlqEntry>> {
    let mut query = String::from(
        "SELECT ... FROM tasker_tasks_dlq WHERE 1=1"
    );

    if let Some(status) = &params.status {
        query.push_str(" AND resolution_status = $1");
    }
    // ... manual filtering and ordering
}
```

**Proposed Enhancement**:
Add new method for investigation queue:
```rust
pub async fn list_investigation_queue(pool: &PgPool) -> TaskerResult<Vec<DlqEntry>> {
    sqlx::query_as::<_, DlqEntry>(
        r#"
        SELECT dlq.*
        FROM v_dlq_investigation_queue viq
        JOIN tasker_tasks_dlq dlq USING (dlq_entry_uuid)
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| TaskerError::Database(format!("Failed to list investigation queue: {}", e)))
}
```

**Benefits**:
- Leverages view's built-in prioritization logic
- Pre-filtered to pending investigations only
- Consistent priority ordering across system
- New GET endpoint: `/v1/dlq/investigation-queue`

**Risk**: Need to decide if this replaces or augments existing list() method.

#### 1.3: Add Staleness Monitoring Endpoint

**New Feature**: Create endpoint using v_task_staleness_monitoring

**Proposed Addition** (dlq.rs):
```rust
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct StalenessMonitoring {
    pub current_state: String,
    pub task_count: i32,
    pub avg_minutes_in_state: i32,
    pub max_minutes_in_state: i32,
    pub approaching_threshold: i32,
    pub exceeds_threshold: i32,
    pub stale_task_uuids: Option<Vec<Uuid>>,
}

impl StalenessMonitoring {
    pub async fn get_monitoring(pool: &PgPool) -> TaskerResult<Vec<Self>> {
        sqlx::query_as::<_, StalenessMonitoring>(
            "SELECT * FROM v_task_staleness_monitoring"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| TaskerError::Database(format!("Failed to get staleness monitoring: {}", e)))
    }
}
```

**New Web Handler** (tasker-orchestration/src/web/handlers/dlq.rs):
```rust
pub async fn get_staleness_monitoring(
    State(app_state): State<AppState>,
    Extension(auth_context): Extension<Option<AuthContext>>,
) -> Result<Json<Vec<StalenessMonitoring>>, ApiError> {
    validate_read_access(&auth_context)?;

    let monitoring = StalenessMonitoring::get_monitoring(&app_state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(monitoring))
}
```

**Benefits**:
- Proactive monitoring before tasks hit DLQ
- Identifies systemic staleness issues by state
- Enables alerting at 75% threshold (approaching_threshold field)
- Ops team can prevent DLQ entries by addressing staleness early

**Risk**: New public API surface - need to document and version appropriately.

### Implementation Order for Part 1

1. **Phase 1** (Low Risk): Replace get_stats() with v_dlq_dashboard view
   - Verify DlqStats struct matches view columns
   - Update query to use view
   - Test with existing test_dlq_stats test
   - Estimated: 30 minutes

2. **Phase 2** (Medium Risk): Add list_investigation_queue() using v_dlq_investigation_queue
   - Create new model method
   - Add new web handler GET /v1/dlq/investigation-queue
   - Add integration test
   - Update API documentation
   - Estimated: 1-2 hours

3. **Phase 3** (New Feature): Add staleness monitoring endpoint
   - Create StalenessMonitoring struct
   - Implement get_monitoring() method
   - Add web handler GET /v1/dlq/staleness
   - Add integration test
   - Document alerting use cases
   - Estimated: 2-3 hours

---

## Part 2: detect_and_transition_stale_tasks() Function Decomposition

### Current State Analysis

**Problem**: 220-line monolithic function (lines 16-221 in 20251115000001_add_dlq_functions.sql) with multiple responsibilities:
- Stale task discovery
- Threshold calculation (duplicated logic)
- DLQ entry creation
- State transition execution
- Dry run reporting

### Refactoring Opportunities

#### 2.1: Extract Threshold Calculation Function

**Duplicated Code**: Lines 53-70 and lines 85-99 have identical CASE logic.

**Proposed Function**:
```sql
CREATE OR REPLACE FUNCTION calculate_staleness_threshold(
    p_state VARCHAR,
    p_template_config JSONB,
    p_default_waiting_deps INTEGER,
    p_default_waiting_retry INTEGER,
    p_default_steps_in_process INTEGER
)
RETURNS INTEGER AS $$
BEGIN
    RETURN CASE p_state
        WHEN 'waiting_for_dependencies' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER,
                p_default_waiting_deps
            )
        WHEN 'waiting_for_retry' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER,
                p_default_waiting_retry
            )
        WHEN 'steps_in_process' THEN
            COALESCE(
                (p_template_config->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER,
                p_default_steps_in_process
            )
        ELSE 1440  -- 24 hours for other states
    END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION calculate_staleness_threshold IS
'TAS-49: Calculate state-specific staleness threshold.

Reads per-template lifecycle configuration and falls back to defaults.
IMMUTABLE allows PostgreSQL to optimize repeated calls.

Parameters:
- p_state: Current task state
- p_template_config: Template configuration JSONB
- p_default_*: Default thresholds when template does not specify

Returns: Threshold in minutes for the given state';
```

**Benefits**:
- DRY principle - single source of truth for thresholds
- Reusable across multiple functions
- IMMUTABLE optimization for query planner
- Easier to unit test threshold logic
- Reduces main function by ~40 lines

#### 2.2: Extract DLQ Entry Creation Function

**Current Code**: Lines 134-152 (INSERT INTO tasker_tasks_dlq)

**Proposed Function**:
```sql
CREATE OR REPLACE FUNCTION create_dlq_entry(
    p_task_uuid UUID,
    p_current_state VARCHAR,
    p_dlq_reason dlq_reason,
    p_task_snapshot JSONB,
    p_time_in_state_minutes NUMERIC,
    p_threshold_minutes INTEGER
)
RETURNS UUID AS $$
DECLARE
    v_dlq_entry_uuid UUID;
BEGIN
    INSERT INTO tasker_tasks_dlq (
        task_uuid,
        original_state,
        dlq_reason,
        dlq_timestamp,
        task_snapshot,
        metadata
    ) VALUES (
        p_task_uuid,
        p_current_state,
        p_dlq_reason,
        NOW(),
        p_task_snapshot,
        jsonb_build_object(
            'detection_method', 'automatic_staleness_detection',
            'time_in_state_minutes', p_time_in_state_minutes,
            'threshold_minutes', p_threshold_minutes
        )
    )
    RETURNING dlq_entry_uuid INTO v_dlq_entry_uuid;

    RETURN v_dlq_entry_uuid;
EXCEPTION
    WHEN unique_violation THEN
        -- Already has pending DLQ entry (race condition)
        RETURN NULL;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION create_dlq_entry IS
'TAS-49: Create DLQ investigation entry for a stale task.

Handles race condition where multiple detectors try to create DLQ entry
for same task (unique constraint on task_uuid + pending status).

Parameters:
- p_task_uuid: Task to send to DLQ
- p_current_state: Task state when detected as stale
- p_dlq_reason: Why task is being sent to DLQ
- p_task_snapshot: Full task state snapshot for investigation
- p_time_in_state_minutes: How long task was stuck
- p_threshold_minutes: Threshold that was exceeded

Returns: UUID of created DLQ entry, or NULL if already exists';
```

**Benefits**:
- Encapsulates DLQ creation logic
- Handles race condition (unique_violation) centrally
- Returns UUID for auditing
- Reusable for manual DLQ creation
- Reduces main function by ~20 lines

#### 2.3: Extract State Transition Function

**Current Code**: Lines 162-199 (INSERT INTO tasker_task_transitions)

**Proposed Function**:
```sql
CREATE OR REPLACE FUNCTION transition_stale_task_to_error(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_time_in_state_minutes NUMERIC,
    p_threshold_minutes INTEGER,
    p_dlq_entry_created BOOLEAN
)
RETURNS BOOLEAN AS $$
BEGIN
    -- Mark old transition as not most_recent
    UPDATE tasker_task_transitions
    SET most_recent = false
    WHERE task_uuid = p_task_uuid
      AND most_recent = true;

    -- Insert new error transition
    INSERT INTO tasker_task_transitions (
        task_uuid,
        from_state,
        to_state,
        most_recent,
        created_at,
        reason,
        transition_metadata
    ) VALUES (
        p_task_uuid,
        p_from_state,
        'error',
        true,
        NOW(),
        'staleness_timeout',
        jsonb_build_object(
            'time_in_state_minutes', p_time_in_state_minutes,
            'threshold_minutes', p_threshold_minutes,
            'automatic_transition', true,
            'dlq_entry_created', p_dlq_entry_created
        )
    );

    RETURN true;
EXCEPTION
    WHEN OTHERS THEN
        RAISE WARNING 'Failed to transition task % to error state: %',
            p_task_uuid, SQLERRM;
        RETURN false;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION transition_stale_task_to_error IS
'TAS-49: Transition stale task to Error state using TAS-41 state machine.

Performs atomic state transition:
1. Mark current transition as not most_recent
2. Insert new Error state transition with staleness metadata

Parameters:
- p_task_uuid: Task to transition
- p_from_state: Current state before transition
- p_time_in_state_minutes: How long stuck in current state
- p_threshold_minutes: Threshold that was exceeded
- p_dlq_entry_created: Whether DLQ entry was successfully created

Returns: true on success, false on error (logged as warning)';
```

**Benefits**:
- Encapsulates atomic state transition logic
- Consistent error handling
- Reusable for other staleness scenarios
- Clear success/failure signaling via BOOLEAN
- Reduces main function by ~40 lines

#### 2.4: Extract Stale Task Discovery to Separate Function

**Current Code**: Lines 42-113 (complex FOR loop with inline SELECT)

**Problem**: Expensive multi-table joins and calculations happen inside loop initialization, causing poor performance.

**Better Architecture**: Separate DISCOVERY (expensive, I/O-bound) from PROCESSING (loop over results).

**Step 1: Create base view for task state analysis**

This extracts the `current_states` CTE from `v_task_staleness_monitoring` into a reusable view:

```sql
CREATE OR REPLACE VIEW v_task_state_analysis AS
SELECT
    t.task_uuid,
    nt.named_task_uuid,
    nt.name as task_name,
    tns.name as namespace_name,
    tt.to_state as current_state,
    EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 as minutes_in_state,
    EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60 as task_age_minutes,
    t.priority,
    t.created_at as task_created_at,
    tt.created_at as state_created_at,
    nt.configuration as template_config,
    -- Use calculate_staleness_threshold() helper
    calculate_staleness_threshold(
        tt.to_state,
        nt.configuration,
        60,  -- default waiting_for_dependencies
        30,  -- default waiting_for_retry
        30   -- default steps_in_process
    ) as threshold_minutes
FROM tasker_tasks t
JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid AND tt.most_recent = true
JOIN tasker_named_tasks nt ON nt.named_task_uuid = t.named_task_uuid
JOIN tasker_task_namespaces tns ON tns.task_namespace_uuid = nt.task_namespace_uuid
WHERE tt.to_state NOT IN ('complete', 'error', 'cancelled', 'resolved_manually');

COMMENT ON VIEW v_task_state_analysis IS
'TAS-49: Base view for task state analysis with threshold calculations.

Provides individual task records with:
- Current state and time in state
- Calculated staleness thresholds
- Template configuration
- Task metadata

Used by:
- v_task_staleness_monitoring (aggregated metrics)
- get_stale_tasks_for_dlq() (DLQ detection)
- Other monitoring/alerting systems

Performance: Expensive joins materialized once, reusable across queries.';
```

**Step 2: Update v_task_staleness_monitoring to use base view**

```sql
CREATE OR REPLACE VIEW v_task_staleness_monitoring AS
SELECT
    current_state,
    COUNT(*) as task_count,
    AVG(minutes_in_state)::INTEGER as avg_minutes_in_state,
    MAX(minutes_in_state)::INTEGER as max_minutes_in_state,
    MIN(threshold_minutes)::INTEGER as min_threshold,
    MAX(threshold_minutes)::INTEGER as max_threshold,
    COUNT(*) FILTER (WHERE minutes_in_state > threshold_minutes * 0.75) as approaching_threshold,
    COUNT(*) FILTER (WHERE minutes_in_state > threshold_minutes) as exceeds_threshold,
    array_agg(
        task_uuid ORDER BY minutes_in_state DESC
    ) FILTER (WHERE minutes_in_state > threshold_minutes) as stale_task_uuids
FROM v_task_state_analysis
GROUP BY current_state
ORDER BY exceeds_threshold DESC, approaching_threshold DESC;

-- Comment remains the same
```

**Step 3: Create function to get stale tasks for DLQ**

```sql
CREATE OR REPLACE FUNCTION get_stale_tasks_for_dlq(
    p_batch_size INTEGER DEFAULT 100,
    p_default_waiting_deps_threshold INTEGER DEFAULT 60,
    p_default_waiting_retry_threshold INTEGER DEFAULT 30,
    p_default_steps_in_process_threshold INTEGER DEFAULT 30,
    p_default_task_max_lifetime_hours INTEGER DEFAULT 24
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes NUMERIC,
    task_age_minutes NUMERIC,
    threshold_minutes INTEGER,
    template_config JSONB
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        tsa.task_uuid,
        tsa.namespace_name,
        tsa.task_name,
        tsa.current_state,
        tsa.minutes_in_state,
        tsa.task_age_minutes,
        -- Recalculate with provided defaults (not hardcoded 60/30/30)
        calculate_staleness_threshold(
            tsa.current_state,
            tsa.template_config,
            p_default_waiting_deps_threshold,
            p_default_waiting_retry_threshold,
            p_default_steps_in_process_threshold
        ) as threshold_minutes,
        tsa.template_config
    FROM v_task_state_analysis tsa
    -- Anti-join: exclude tasks with pending DLQ entries
    LEFT JOIN tasker_tasks_dlq dlq
        ON dlq.task_uuid = tsa.task_uuid
        AND dlq.resolution_status = 'pending'
    WHERE
        dlq.dlq_entry_uuid IS NULL  -- No pending DLQ entry exists
        AND (
            -- Stale in current state
            tsa.minutes_in_state > calculate_staleness_threshold(
                tsa.current_state,
                tsa.template_config,
                p_default_waiting_deps_threshold,
                p_default_waiting_retry_threshold,
                p_default_steps_in_process_threshold
            )
            OR
            -- Exceeded max lifetime
            tsa.task_age_minutes > COALESCE(
                (tsa.template_config->'lifecycle'->>'max_duration_minutes')::INTEGER,
                p_default_task_max_lifetime_hours * 60
            )
        )
    ORDER BY tsa.state_created_at ASC
    LIMIT p_batch_size;
END;
$$ LANGUAGE plpgsql STABLE;

COMMENT ON FUNCTION get_stale_tasks_for_dlq IS
'TAS-49: Get stale tasks ready for DLQ transition.

Queries v_task_state_analysis (expensive joins happen once) and filters to:
- Tasks exceeding state-specific thresholds
- Tasks exceeding max lifetime
- Tasks without pending DLQ entries (anti-join using LEFT JOIN + IS NULL)

Performance optimizations:
- LEFT JOIN anti-join pattern more optimizable than NOT EXISTS
- STABLE marking allows PostgreSQL to optimize multiple calls in same transaction
- Uses existing index on tasker_tasks_dlq(task_uuid, resolution_status)

Parameters: Same as detect_and_transition_stale_tasks()
Returns: Individual stale task records ready for DLQ processing';
```

**Step 4: Refactor main function to use discovery function**

Now the main function simply calls the discovery function and processes results:

```sql
CREATE OR REPLACE FUNCTION detect_and_transition_stale_tasks(
    p_dry_run BOOLEAN DEFAULT true,
    p_batch_size INTEGER DEFAULT 100,
    p_default_waiting_deps_threshold INTEGER DEFAULT 60,
    p_default_waiting_retry_threshold INTEGER DEFAULT 30,
    p_default_steps_in_process_threshold INTEGER DEFAULT 30,
    p_default_task_max_lifetime_hours INTEGER DEFAULT 24
)
RETURNS TABLE(
    task_uuid UUID,
    namespace_name VARCHAR,
    task_name VARCHAR,
    current_state VARCHAR,
    time_in_state_minutes INTEGER,
    staleness_threshold_minutes INTEGER,
    action_taken VARCHAR,
    moved_to_dlq BOOLEAN,
    transition_success BOOLEAN
) AS $$
DECLARE
    v_task_record RECORD;
    v_dlq_entry_uuid UUID;
    v_transition_success BOOLEAN;
    v_task_snapshot JSONB;
BEGIN
    -- Discovery happens ONCE before loop (not inside loop initialization)
    FOR v_task_record IN
        SELECT * FROM get_stale_tasks_for_dlq(
            p_batch_size,
            p_default_waiting_deps_threshold,
            p_default_waiting_retry_threshold,
            p_default_steps_in_process_threshold,
            p_default_task_max_lifetime_hours
        )
    LOOP
        v_dlq_entry_uuid := NULL;
        v_transition_success := false;

        -- Build task snapshot for investigation
        v_task_snapshot := jsonb_build_object(
            'task_uuid', v_task_record.task_uuid,
            'namespace', v_task_record.namespace_name,
            'task_name', v_task_record.task_name,
            'current_state', v_task_record.current_state,
            'time_in_state_minutes', v_task_record.time_in_state_minutes,
            'threshold_minutes', v_task_record.threshold_minutes,
            'task_age_minutes', v_task_record.task_age_minutes,
            'template_config', v_task_record.template_config,
            'detection_time', NOW()
        );

        IF NOT p_dry_run THEN
            -- Create DLQ entry using helper function
            v_dlq_entry_uuid := create_dlq_entry(
                v_task_record.task_uuid,
                v_task_record.current_state,
                'staleness_timeout',
                v_task_snapshot,
                v_task_record.time_in_state_minutes,
                v_task_record.threshold_minutes
            );

            -- Transition to error using helper function
            v_transition_success := transition_stale_task_to_error(
                v_task_record.task_uuid,
                v_task_record.current_state,
                v_task_record.time_in_state_minutes,
                v_task_record.threshold_minutes,
                v_dlq_entry_uuid IS NOT NULL
            );
        END IF;

        -- Return result for this task
        RETURN QUERY SELECT
            v_task_record.task_uuid,
            v_task_record.namespace_name,
            v_task_record.task_name,
            v_task_record.current_state,
            v_task_record.time_in_state_minutes::INTEGER,
            v_task_record.threshold_minutes::INTEGER,
            (CASE
                WHEN p_dry_run THEN 'would_transition_to_dlq_and_error'
                WHEN v_dlq_entry_uuid IS NOT NULL AND v_transition_success THEN 'transitioned_to_dlq_and_error'
                WHEN v_dlq_entry_uuid IS NOT NULL THEN 'moved_to_dlq_only'
                WHEN v_transition_success THEN 'transitioned_to_error_only'
                ELSE 'transition_failed'
            END)::VARCHAR,
            v_dlq_entry_uuid IS NOT NULL,
            v_transition_success;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION detect_and_transition_stale_tasks IS
'TAS-49: Detect stale tasks and create DLQ investigation entries.

Architecture:
1. Calls get_stale_tasks_for_dlq() for discovery (expensive joins happen ONCE)
2. Loops over pre-identified tasks (cheap processing)
3. Creates DLQ entries via create_dlq_entry()
4. Transitions to Error state via transition_stale_task_to_error()

This function does NOT resolve tasks - it only:
1. Identifies tasks stuck beyond thresholds
2. Transitions them to Error state (TAS-41 state machine)
3. Creates DLQ entries for operator investigation

Parameters:
- p_dry_run: If true, only returns what would be done without making changes
- p_batch_size: Maximum number of stale tasks to process in single call
- p_default_*_threshold: Default thresholds when template doesn''t specify

Returns: Results for each task processed with action taken';
```

**Benefits of This Architecture**:

1. **Performance**: Multiple optimizations for speed
   - Expensive joins happen ONCE (in get_stale_tasks_for_dlq), not on every loop iteration
   - LEFT JOIN anti-join pattern instead of NOT EXISTS (query planner can optimize better)
   - Uses existing partial index on tasker_tasks_dlq for pending DLQ check
   - Expected: 50-70% reduction in execution time vs original

2. **Separation of Concerns**: Discovery (I/O-bound) separated from processing (CPU-bound)
   - Discovery: Read-heavy, uses views and indexes
   - Processing: Write-heavy, uses helper functions

3. **Reusability**:
   - `v_task_state_analysis` can be used by other monitoring systems
   - `get_stale_tasks_for_dlq()` can be called independently for dry-run analysis
   - `v_task_staleness_monitoring` now built on shared base view

4. **Testability**: Each component independently testable:
   - Query v_task_state_analysis for correctness
   - Test get_stale_tasks_for_dlq() filtering logic
   - Test main function orchestration

5. **Maintainability**: Main function reduced from 220 lines to ~90 lines

6. **DRY**: Eliminates duplicated join logic and threshold calculations

#### 2.5: Architecture Summary

**New Component Hierarchy**:

```
v_task_state_analysis (base view)
    ├── v_task_staleness_monitoring (aggregated metrics)
    ├── get_stale_tasks_for_dlq() (filtered individual tasks)
    │       │
    │       └── detect_and_transition_stale_tasks() (main orchestration)
    │               ├── create_dlq_entry() (DLQ creation)
    │               └── transition_stale_task_to_error() (state transition)
    │
    └── [future monitoring/alerting systems]
```

**Separation of Responsibilities**:

1. **v_task_state_analysis**: Expensive joins + threshold calculation (materialized once)
2. **get_stale_tasks_for_dlq()**: Filtering logic (which tasks are stale?)
3. **detect_and_transition_stale_tasks()**: Orchestration (what to do with stale tasks?)
4. **create_dlq_entry()**: DLQ creation with race condition handling
5. **transition_stale_task_to_error()**: Atomic state transition
6. **calculate_staleness_threshold()**: Threshold calculation logic

**Key Improvements**:
- Main function: 220 lines → ~90 lines
- Expensive computation: Inside loop → Before loop (single execution)
- Duplicate code: Multiple CASE statements → Single calculate_staleness_threshold() function
- Duplicate joins: Scattered → Single v_task_state_analysis view
- Testability: Monolithic → Each component independently testable
- Reusability: Single-use → Shared across monitoring systems

### Implementation Order for Part 2

1. **Phase 1** (Foundation - Views): Create reusable views
   - Create v_task_state_analysis base view
   - Update v_task_staleness_monitoring to use base view
   - Test views return expected data
   - Verify no performance regression vs current implementation
   - Estimated: 1-2 hours

2. **Phase 2** (Foundation - Helper Functions): Create atomic operations
   - Create calculate_staleness_threshold() function with COMMENT
   - Create create_dlq_entry() function with COMMENT
   - Create transition_stale_task_to_error() function with COMMENT
   - Add validation checks in migration
   - Unit test each function independently
   - Estimated: 2-3 hours

3. **Phase 3** (Discovery Function): Extract expensive discovery logic
   - Create get_stale_tasks_for_dlq() function using v_task_state_analysis
   - Test function returns same tasks as current inline SELECT
   - Benchmark performance (should be equal or better)
   - Estimated: 1-2 hours

4. **Phase 4** (Main Function Refactor): Update orchestration
   - Replace inline SELECT with get_stale_tasks_for_dlq() call
   - Replace threshold calculation with calculate_staleness_threshold() call
   - Replace DLQ INSERT with create_dlq_entry() call
   - Replace transition INSERT with transition_stale_task_to_error() call
   - Update function COMMENT to reflect new architecture
   - Estimated: 1-2 hours

5. **Phase 5** (Validation): Comprehensive testing
   - Run existing integration tests (should pass unchanged)
   - Side-by-side comparison: old vs new implementation results
   - Verify identical behavior in dry run mode
   - Performance testing: measure improvement from moving joins outside loop
   - Add tests for new helper functions
   - Estimated: 2-3 hours

---

## Part 3: JSONB Indexing Strategy

### Problem: Heavy Reliance on JSONB Path Queries

The refactored views and functions heavily query lifecycle configuration from JSONB:

```sql
-- Used extensively in v_task_state_analysis and functions
(nt.configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes')::INTEGER
(nt.configuration->'lifecycle'->>'max_waiting_for_retry_minutes')::INTEGER
(nt.configuration->'lifecycle'->>'max_steps_in_process_minutes')::INTEGER
(nt.configuration->'lifecycle'->>'max_duration_minutes')::INTEGER
```

**Current Situation**:
- No indexes on JSONB paths
- Every staleness check performs JSONB path extraction
- `v_task_state_analysis` will be queried frequently by monitoring systems

### Options Analysis

#### Option A: JSONB Path Indexes (PostgreSQL 12+)

**Approach**: Create indexes on specific JSONB paths:

```sql
-- Index for lifecycle threshold lookups
CREATE INDEX IF NOT EXISTS idx_named_tasks_lifecycle_thresholds
    ON tasker_named_tasks
    USING GIN ((configuration->'lifecycle'));

-- Or more specific expression indexes
CREATE INDEX IF NOT EXISTS idx_named_tasks_waiting_deps_threshold
    ON tasker_named_tasks ((configuration->'lifecycle'->>'max_waiting_for_dependencies_minutes'));

CREATE INDEX IF NOT EXISTS idx_named_tasks_waiting_retry_threshold
    ON tasker_named_tasks ((configuration->'lifecycle'->>'max_waiting_for_retry_minutes'));

CREATE INDEX IF NOT EXISTS idx_named_tasks_steps_process_threshold
    ON tasker_named_tasks ((configuration->'lifecycle'->>'max_steps_in_process_minutes'));

CREATE INDEX IF NOT EXISTS idx_named_tasks_max_duration
    ON tasker_named_tasks ((configuration->'lifecycle'->>'max_duration_minutes'));
```

**Pros**:
- Template configuration remains fully flexible
- No schema changes required
- No task handler registry changes required
- JSONB can store arbitrary additional configuration
- Indexes allow fast lookups on JSONB paths

**Cons**:
- JSONB path extraction still has overhead (even with index)
- Expression indexes can be large
- Query planner may not always use JSONB indexes optimally
- More complex to understand query plans

#### Option B: Denormalize Lifecycle Columns

**Approach**: Add relational columns alongside JSONB:

```sql
-- New migration
ALTER TABLE tasker_named_tasks
    ADD COLUMN lifecycle_max_waiting_deps_minutes INTEGER,
    ADD COLUMN lifecycle_max_waiting_retry_minutes INTEGER,
    ADD COLUMN lifecycle_max_steps_process_minutes INTEGER,
    ADD COLUMN lifecycle_max_duration_minutes INTEGER;

-- Create standard B-tree indexes
CREATE INDEX IF NOT EXISTS idx_named_tasks_lifecycle_waiting_deps
    ON tasker_named_tasks (lifecycle_max_waiting_deps_minutes)
    WHERE lifecycle_max_waiting_deps_minutes IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_named_tasks_lifecycle_waiting_retry
    ON tasker_named_tasks (lifecycle_max_waiting_retry_minutes)
    WHERE lifecycle_max_waiting_retry_minutes IS NOT NULL;

-- Similar for other columns
```

**Updated View**:
```sql
CREATE OR REPLACE VIEW v_task_state_analysis AS
SELECT
    -- ... other columns
    COALESCE(
        nt.lifecycle_max_waiting_deps_minutes,
        60  -- default
    ) as threshold_minutes_waiting_deps
    -- ... etc
FROM tasker_tasks t
-- ... joins
```

**Pros**:
- Standard B-tree indexes (query planner understands perfectly)
- No JSONB extraction overhead in queries
- Faster query execution
- Clearer query plans and EXPLAIN output
- Partial indexes for NULL handling (templates without overrides)

**Cons**:
- Schema changes required
- Task template registry loading changes required
- Must keep JSONB and relational columns in sync
- Denormalization = potential data inconsistency
- Migration complexity for existing templates

### Recommendation

**Start with Option A (JSONB Path Indexes)** for the following reasons:

1. **Lower Risk**: No schema changes, no application changes, no migration complexity
2. **Sufficient Performance**: For staleness detection (runs every 5 minutes), JSONB path extraction overhead is acceptable
3. **Maintain Flexibility**: Configuration can remain fully flexible without schema constraints
4. **Measurable**: Can benchmark and switch to Option B later if performance is insufficient

**Implementation for Option A**:

```sql
-- Add to migration: 20251122000002_add_dlq_views.sql
-- Or new migration: 20251201000001_add_jsonb_indexes.sql

-- GIN index for lifecycle configuration lookups
CREATE INDEX IF NOT EXISTS idx_named_tasks_lifecycle_config
    ON tasker_named_tasks
    USING GIN ((configuration->'lifecycle'));

COMMENT ON INDEX idx_named_tasks_lifecycle_config IS
'TAS-49: JSONB GIN index for lifecycle configuration path queries.
Accelerates queries like:
- configuration->''lifecycle''->>''max_waiting_for_dependencies_minutes''
- Used by v_task_state_analysis and staleness detection functions';
```

**Performance Testing Plan**:

1. Implement JSONB GIN index
2. Run `EXPLAIN ANALYZE` on `v_task_state_analysis` query
3. Benchmark `get_stale_tasks_for_dlq()` execution time
4. Monitor staleness detection cycle duration in production
5. If performance is insufficient (<1s for 100 task batch), consider Option B

**Migration Path to Option B** (if needed in future):

1. Add relational columns (nullable initially)
2. Backfill existing templates from JSONB
3. Update task handler registry to populate both JSONB and relational columns
4. Update views to use relational columns
5. Monitor for inconsistencies
6. Once stable, consider making JSONB optional (or keep for backwards compatibility)

---

## Risk Assessment and Mitigation

### Part 1 Risks (View Integration)

**Risk 1**: Struct field mismatch with views
- **Mitigation**: Use sqlx compile-time checks, add explicit mapping tests
- **Impact**: Low - caught at compile time

**Risk 2**: View performance vs direct queries
- **Mitigation**: EXPLAIN ANALYZE comparison before/after, add indexes if needed
- **Impact**: Low - views use same indexes as direct queries

**Risk 3**: Breaking API changes for new endpoints
- **Mitigation**: Version API endpoints, document in changelog, add to API docs
- **Impact**: Low - new endpoints don't break existing ones

### Part 2 Risks (Function Decomposition)

**Risk 1**: Behavioral differences in refactored function
- **Mitigation**: Comprehensive test coverage, dry run comparison, side-by-side testing
- **Impact**: High - must guarantee identical behavior

**Risk 2**: Performance regression from function calls
- **Mitigation**: EXPLAIN ANALYZE, benchmark before/after, PostgreSQL inlines small functions
- **Impact**: Low - PostgreSQL optimizes function calls well

**Risk 3**: Transaction boundary issues with helper functions
- **Mitigation**: All helper functions in same transaction as main function
- **Impact**: Low - PostgreSQL function nesting preserves transaction semantics

**Risk 4**: Breaking existing callers of detect_and_transition_stale_tasks()
- **Mitigation**: Preserve function signature exactly, only change implementation
- **Impact**: None - internal refactoring only

---

## Testing Strategy

### Part 1 Testing (View Integration)

1. **Unit Tests**: Verify view queries return expected data shape
2. **Integration Tests**: Test web endpoints using views
3. **Comparison Tests**: Verify view results match original direct query results
4. **Performance Tests**: Benchmark view vs direct query execution time

### Part 2 Testing (Function Decomposition)

1. **Unit Tests**: Test each helper function independently
   - calculate_staleness_threshold() with various states and configs
   - create_dlq_entry() with race condition simulation
   - transition_stale_task_to_error() with error scenarios
2. **Integration Tests**: Run existing detect_and_transition_stale_tasks() tests
3. **Side-by-Side Tests**: Run both old and new implementation, compare results
4. **Dry Run Tests**: Verify dry run mode produces identical results
5. **Performance Tests**: Compare execution time and query plans

---

## Success Criteria

### Part 0 (Type Safety)
- [ ] StalenessAction enum created with all 5 variants
- [ ] sqlx Type derive working correctly
- [ ] Helper methods implemented (is_failure, dlq_created, transition_succeeded)
- [ ] StalenessResult.action_taken updated to use enum
- [ ] staleness_detector.rs updated to use enum methods
- [ ] All tests pass with enum changes
- [ ] Clippy warnings resolved

### Part 2 (Function Decomposition)
- [ ] All existing integration tests pass unchanged
- [ ] Main function reduced from 220 lines to ~90 lines
- [ ] Base view v_task_state_analysis created and documented
- [ ] v_task_staleness_monitoring refactored to use base view
- [ ] Four helper functions created with COMMENT documentation:
  - [ ] calculate_staleness_threshold()
  - [ ] create_dlq_entry()
  - [ ] transition_stale_task_to_error()
  - [ ] get_stale_tasks_for_dlq()
- [ ] Discovery function replaces inline SELECT (expensive joins outside loop)
- [ ] LEFT JOIN anti-join pattern used instead of NOT EXISTS
- [ ] No behavioral differences in output (side-by-side verification)
- [ ] Performance improvement from moving joins outside loop (measured)
- [ ] Migration validates all functions and views created

### Part 3 (JSONB Indexing)
- [ ] GIN index created on configuration->'lifecycle'
- [ ] EXPLAIN ANALYZE confirms index usage in v_task_state_analysis
- [ ] Benchmark shows acceptable performance (<1s for 100 task batch)
- [ ] Index COMMENT documentation added
- [ ] Performance metrics documented for future comparison
- [ ] Migration includes index validation

### Part 1 (View Integration)
- [ ] All existing DLQ tests pass
- [ ] get_stats() uses v_dlq_dashboard view
- [ ] New investigation queue endpoint added and tested
- [ ] New staleness monitoring endpoint added and tested
- [ ] API documentation updated
- [ ] No performance regression vs direct queries

---

## Estimated Total Effort

- **Part 0 (Type Safety)**: 1-2 hours
  - Create StalenessAction enum: 30 minutes
  - Update StalenessResult struct: 15 minutes
  - Update staleness_detector.rs: 30 minutes
  - Testing and validation: 30 minutes

- **Part 2 (Function Decomposition)**: 8-12 hours
  - Phase 1 (Views): 1-2 hours
  - Phase 2 (Helper Functions): 2-3 hours
  - Phase 3 (Discovery Function): 1-2 hours
  - Phase 4 (Main Refactor): 1-2 hours
  - Phase 5 (Validation): 2-3 hours

- **Part 3 (JSONB Indexing)**: 1-2 hours
  - Create GIN index: 15 minutes
  - EXPLAIN ANALYZE testing: 30 minutes
  - Benchmark staleness detection: 30 minutes
  - Documentation: 15 minutes

- **Part 1 (View Integration)**: 4-6 hours
  - Phase 1: 30 minutes
  - Phase 2: 1-2 hours
  - Phase 3: 2-3 hours

- **Total**: 14-22 hours

## Recommended Approach

**CRITICAL**: Parts must be executed in this order due to dependencies:

### Phase 1: Type Safety (Part 0)
1. **Start with Part 0** (StalenessAction enum): Independent, can be done anytime
   - Creates type safety for existing code
   - No dependencies on other parts
   - Can be merged independently

### Phase 2: Foundation - Views and Functions (Part 2)
2. **Part 2, Phase 1** (Create base view v_task_state_analysis)
   - Extracts expensive joins into reusable view
   - Foundation for all other views and functions
   - Test view returns expected data

3. **Part 2, Phase 2** (Create helper functions)
   - calculate_staleness_threshold()
   - create_dlq_entry()
   - transition_stale_task_to_error()
   - Unit test each function independently

4. **Part 2, Phase 3** (Create discovery function)
   - get_stale_tasks_for_dlq() using v_task_state_analysis
   - Benchmark against current implementation

5. **Part 2, Phase 4** (Refactor main function)
   - Update detect_and_transition_stale_tasks() to use helpers
   - Expensive joins now outside loop

6. **Part 2, Phase 5** (Comprehensive validation)
   - Run all existing integration tests
   - Side-by-side comparison
   - Performance benchmarking

### Phase 3: Performance Optimization (Part 3)
7. **Part 3** (Add JSONB indexing)
   - Create GIN index on configuration->'lifecycle'
   - EXPLAIN ANALYZE to verify index usage
   - Benchmark improvement
   - Document for future monitoring

### Phase 4: Application Integration (Part 1)
8. **Part 1, Phase 1** (get_stats → v_dlq_dashboard)
   - Now views are finalized and tested
   - Low risk, validates integration pattern

9. **Part 1, Phase 2** (Investigation queue endpoint)
   - Uses v_dlq_investigation_queue view

10. **Part 1, Phase 3** (Staleness monitoring endpoint)
    - Uses v_task_staleness_monitoring view
    - Enables proactive monitoring

**Rationale for This Order**:
- Part 2 creates/refactors the views that Part 1 relies on
- Part 3 optimizes the views before application integration
- Part 1 last ensures we're integrating finalized, tested, optimized views
- Each phase builds on previous phases
- Easy rollback at any phase boundary

This incremental approach allows validation at each step and easy rollback if issues arise.

---

## Key Performance Insight

**Critical Discovery**: The original refactoring plan (section 2.4 first draft) placed the expensive CTE inside the FOR loop declaration:

```sql
FOR v_task_record IN
    WITH stale_tasks AS (
        -- Complex joins here
    )
    SELECT * FROM stale_tasks
LOOP
    -- Process task
END LOOP;
```

**Problem**: This is computationally and disk I/O expensive because:
1. Complex multi-table joins (4 tables: tasks, transitions, named_tasks, namespaces)
2. JSONB path operations for configuration extraction
3. Multiple calculations (time_in_state, thresholds, etc.)
4. All of this happens as part of **loop initialization** - not before the loop

**Solution**: Separate DISCOVERY from PROCESSING:

```sql
-- Discovery happens ONCE (expensive joins execute once)
CREATE FUNCTION get_stale_tasks_for_dlq(...) RETURNS TABLE(...) AS $$
    SELECT * FROM v_task_state_analysis WHERE [stale conditions]
$$;

-- Main function just processes pre-discovered tasks
FOR v_task_record IN SELECT * FROM get_stale_tasks_for_dlq(...)
LOOP
    -- Cheap processing: create DLQ entry, transition state
END LOOP;
```

**Performance Impact**:
- Before: O(n) expensive joins where n = loop iterations
- After: O(1) expensive joins + O(n) cheap processing
- Expected improvement: 50-70% reduction in function execution time for batch processing

**Additional Optimization - Anti-Join Pattern**:

The refactoring also replaces `NOT EXISTS` with `LEFT JOIN + IS NULL` for excluding tasks with pending DLQ entries:

```sql
-- Original (NOT EXISTS)
WHERE NOT EXISTS (
    SELECT 1 FROM tasker_tasks_dlq dlq
    WHERE dlq.task_uuid = tsa.task_uuid
      AND dlq.resolution_status = 'pending'
)

-- Refactored (LEFT JOIN + IS NULL)
LEFT JOIN tasker_tasks_dlq dlq
    ON dlq.task_uuid = tsa.task_uuid
    AND dlq.resolution_status = 'pending'
WHERE dlq.dlq_entry_uuid IS NULL
```

**Benefits**:
1. Query planner can optimize joins more effectively than subqueries
2. Makes anti-join explicit (clearer query plan)
3. Can leverage existing partial index on `(task_uuid, resolution_status) WHERE resolution_status = 'pending'`
4. More declarative - shows intent clearly

**Lesson**: In SQL functions, treat loops the same as any programming language - expensive I/O should happen OUTSIDE loops, not inside loop initialization. CTEs inside FOR loops still execute as part of the loop machinery. Additionally, prefer explicit JOIN operations over subqueries for better query planner optimization.

---

## Implementation Notes

### File Locations

**Part 1 (View Integration)**:
- Model changes: `tasker-shared/src/models/orchestration/dlq.rs`
- Web handler changes: `tasker-orchestration/src/web/handlers/dlq.rs`
- Route additions: `tasker-orchestration/src/web/routes.rs`
- Integration tests: `tasker-orchestration/tests/web/handlers/dlq.rs`

**Part 2 (Function Decomposition)**:
- View changes: `migrations/20251122000002_add_dlq_views.sql` (update existing)
- New functions: `migrations/20251115000001_add_dlq_functions.sql` (update existing)
- Integration tests: `tasker-shared/tests/integration/sql_functions/` (existing tests should pass)

### Migration Strategy

**Part 1**: Non-breaking changes
- View integration is transparent to callers
- New endpoints are additive (no breaking changes)
- Can deploy incrementally

**Part 2**: Requires two migration updates

**Migration 1**: Update views (update `migrations/20251122000002_add_dlq_views.sql`)
- Add v_task_state_analysis base view
- Update v_task_staleness_monitoring to SELECT from base view
- Add validation checks for new view
- Rollback: Drop v_task_state_analysis, restore original v_task_staleness_monitoring

**Migration 2**: Refactor functions (update `migrations/20251115000001_add_dlq_functions.sql`)
- Add calculate_staleness_threshold() helper function
- Add create_dlq_entry() helper function
- Add transition_stale_task_to_error() helper function
- Add get_stale_tasks_for_dlq() discovery function
- Replace detect_and_transition_stale_tasks() implementation (preserve signature)
- Add validation checks for all new functions
- Rollback: Drop helper functions, restore original detect_and_transition_stale_tasks()

**Important**: Both migrations update existing files to maintain chronological order and keep related functionality together.

### Validation Checklist

Before considering refactoring complete:
1. [ ] All existing tests pass (zero regressions)
2. [ ] New helper functions have unit tests
3. [ ] EXPLAIN ANALYZE shows no performance regression
4. [ ] Dry run mode produces identical results
5. [ ] Migration includes validation checks for all functions
6. [ ] Documentation updated (function COMMENTs, API docs)
7. [ ] Code review completed
8. [ ] Deployed to staging environment
9. [ ] Monitoring confirms no errors or performance issues
10. [ ] Production deployment approved
