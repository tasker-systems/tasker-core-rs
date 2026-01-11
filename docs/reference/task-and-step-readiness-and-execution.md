# Task and Step Readiness and Execution

**Last Updated**: 2026-01-10
**Audience**: Developers, Architects
**Status**: Active
**Related Docs**: [Documentation Hub](README.md) | [States and Lifecycles](states-and-lifecycles.md) | [Events and Commands](events-and-commands.md)

← Back to [Documentation Hub](README.md)

---

This document provides comprehensive documentation of the SQL functions and database logic that drives task and step readiness analysis, dependency resolution, and execution coordination in the tasker-core system.

## Overview

The tasker-core system relies heavily on sophisticated PostgreSQL functions to
perform complex workflow orchestration operations at the database level. This
approach provides significant performance benefits through set-based operations,
atomic transactions, and reduced network round trips while maintaining data
consistency.

The SQL function system supports several critical categories of operations:

1. **Step Readiness Analysis**: Complex dependency resolution and backoff calculations
2. **DAG Operations**: Cycle detection, depth calculation, and parallel execution discovery
3. **State Management**: Atomic state transitions with processor ownership tracking
4. **Analytics and Monitoring**: Performance metrics and system health analysis
5. **Task Execution Context**: Comprehensive execution metadata and results management

## SQL Function Architecture

### Function Categories

The SQL functions are organized into logical categories as defined in
`tasker-shared/src/database/sql_functions.rs`:

#### 1. Step Readiness Analysis
- **`get_step_readiness_status(task_uuid, step_uuids[])`**: Comprehensive dependency analysis
- **`calculate_backoff_delay(attempts, base_delay)`**: Exponential backoff calculation
- **`check_step_dependencies(step_uuid)`**: Parent completion validation
- **`get_ready_steps(task_uuid)`**: Parallel execution candidate discovery

#### 2. DAG Operations
- **`detect_cycle(from_step_uuid, to_step_uuid)`**: Cycle detection using recursive CTEs
- **`calculate_dependency_levels(task_uuid)`**: Topological depth calculation
- **`calculate_step_depth(step_uuid)`**: Individual step depth analysis
- **`get_step_transitive_dependencies(step_uuid)`**: Full dependency tree traversal

#### 3. State Management (TAS-41 Enhanced)
- **`transition_task_state_atomic(task_uuid, from_state, to_state, processor_uuid)`**:
  Atomic state transitions with ownership
- **`get_current_task_state(task_uuid)`**: Current task state resolution
- **`finalize_task_completion(task_uuid)`**: Task completion orchestration

#### 4. Analytics and Monitoring
- **`get_analytics_metrics(since_timestamp)`**: Comprehensive system analytics
- **`get_system_health_counts()`**: System-wide health and performance metrics
- **`get_slowest_steps(limit)`**: Performance optimization analysis
- **`get_slowest_tasks(limit)`**: Task performance analysis

#### 5. Task Discovery and Execution
- **`get_next_ready_task()`**: Single task discovery for orchestration
- **`get_next_ready_tasks(limit)`**: Batch task discovery for scaling
- **`get_task_ready_info(task_uuid)`**: Detailed task readiness information
- **`get_task_execution_context(task_uuid)`**: Complete execution metadata

## Database Schema Foundation

### Core Tables

The SQL functions operate on a comprehensive schema designed for UUID v7
performance and scalability. As of TAS-128, all tables reside in the `tasker` schema
with simplified names. With `search_path = tasker, public`, queries use unqualified
table names.

#### Primary Tables
- **`tasks`**: Main workflow instances with UUID v7 primary keys
- **`workflow_steps`**: Individual workflow steps with dependency relationships
- **`task_transitions`**: Task state change audit trail with processor tracking
- **`workflow_step_transitions`**: Step state change audit trail

#### Registry Tables
- **`task_namespaces`**: Workflow namespace definitions
- **`named_tasks`**: Task type templates and metadata
- **`named_steps`**: Step type definitions and handlers
- **`workflow_step_edges`**: Step dependency relationships (DAG structure)

#### TAS-41 Enhancements

The TAS-41 migration (`migrations/tasker/20251209000000_tas41_richer_task_states.sql`) enhanced the
schema with:

**Task State Management**:
```sql
-- 12 comprehensive task states
ALTER TABLE task_transitions
ADD CONSTRAINT chk_task_transitions_to_state
CHECK (to_state IN (
    'pending', 'initializing', 'enqueuing_steps', 'steps_in_process',
    'evaluating_results', 'waiting_for_dependencies', 'waiting_for_retry',
    'blocked_by_failures', 'complete', 'error', 'cancelled', 'resolved_manually'
));
```

**Processor Ownership Tracking**:
```sql
ALTER TABLE task_transitions
ADD COLUMN processor_uuid UUID,
ADD COLUMN transition_metadata JSONB DEFAULT '{}';
```

**Atomic State Transitions**:
```sql
CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN
```

## Step Readiness Analysis

### Recent Enhancements

#### TAS-42: WaitingForRetry State Support (Migration 20250927000000)

The step readiness system was enhanced to support the new `WaitingForRetry` state, which distinguishes retryable failures from permanent errors:

**Key Changes**:
1. **Helper Functions**: Added `calculate_step_next_retry_time()` and `evaluate_step_state_readiness()` for consistent backoff logic
2. **State Recognition**: Updated readiness evaluation to treat `waiting_for_retry` as a ready-eligible state alongside `pending`
3. **Backoff Calculation**: Centralized exponential backoff logic with configurable backoff periods
4. **Performance Optimization**: Introduced task-scoped CTEs to eliminate table scans for batch operations

**Semantic Impact (TAS-42)**:
- **Before**: `error` state included both retryable and permanent failures
- **After**: `error` = permanent only, `waiting_for_retry` = awaiting backoff for retry

#### TAS-57: Backoff Logic Consolidation (October 2025)

The backoff calculation system was consolidated to eliminate configuration conflicts and race conditions:

**Key Changes**:
1. **Configuration Alignment**: Single source of truth (TOML config) with max_backoff_seconds = 60
2. **Parameterized SQL Functions**: `calculate_step_next_retry_time()` accepts configurable max delay and multiplier
3. **Atomic Updates**: Row-level locking prevents concurrent backoff update conflicts
4. **Timing Consistency**: `last_attempted_at` updated atomically with `backoff_request_seconds`

**Issues Resolved**:
- **Configuration Conflicts**: Eliminated three conflicting max values (30s SQL, 60s code, 300s TOML)
- **Race Conditions**: Added SELECT FOR UPDATE locking in BackoffCalculator
- **Hardcoded Values**: Removed hardcoded 30-second cap and power(2, attempts) in SQL

**Helper Functions Enhanced**:

1. **`calculate_step_next_retry_time()`**: Now parameterized with configuration values
   ```sql
   CREATE OR REPLACE FUNCTION calculate_step_next_retry_time(
       backoff_request_seconds INTEGER,
       last_attempted_at TIMESTAMP,
       failure_time TIMESTAMP,
       attempts INTEGER,
       p_max_backoff_seconds INTEGER DEFAULT 60,
       p_backoff_multiplier NUMERIC DEFAULT 2.0
   ) RETURNS TIMESTAMP
   ```
   - Respects custom backoff periods from step configuration (primary path)
   - Falls back to exponential backoff with configurable parameters
   - Defaults aligned with TOML config (60s max, 2.0 multiplier)
   - Used consistently across all readiness evaluation

2. **`set_step_backoff_atomic()`**: New atomic update function
   ```sql
   CREATE OR REPLACE FUNCTION set_step_backoff_atomic(
       p_step_uuid UUID,
       p_backoff_seconds INTEGER
   ) RETURNS BOOLEAN
   ```
   - Provides transactional guarantee for concurrent updates
   - Updates both `backoff_request_seconds` and `last_attempted_at`
   - Ensures timing consistency with SQL calculations

3. **`evaluate_step_state_readiness()`**: Determines if a step is ready for execution
   ```sql
   CREATE OR REPLACE FUNCTION evaluate_step_state_readiness(
       current_state TEXT,
       processed BOOLEAN,
       in_process BOOLEAN,
       dependencies_satisfied BOOLEAN,
       retry_eligible BOOLEAN,
       retryable BOOLEAN,
       next_retry_time TIMESTAMP
   ) RETURNS BOOLEAN
   ```
   - Recognizes both `pending` and `waiting_for_retry` as ready-eligible states
   - Validates backoff period has expired before allowing retry
   - Ensures dependencies are satisfied and retry limits not exceeded

### Step Readiness Status

The `get_step_readiness_status` function provides comprehensive analysis of step
execution eligibility:

```sql
CREATE OR REPLACE FUNCTION get_step_readiness_status(
    task_uuid UUID,
    step_uuids UUID[] DEFAULT NULL
) RETURNS TABLE(
    workflow_step_uuid UUID,
    task_uuid UUID,
    named_step_uuid UUID,
    name VARCHAR,
    current_state VARCHAR,
    dependencies_satisfied BOOLEAN,
    retry_eligible BOOLEAN,
    ready_for_execution BOOLEAN,
    last_failure_at TIMESTAMP,
    next_retry_at TIMESTAMP,
    total_parents INTEGER,
    completed_parents INTEGER,
    attempts INTEGER,
    retry_limit INTEGER,
    backoff_request_seconds INTEGER,
    last_attempted_at TIMESTAMP
)
```

#### Key Analysis Features

**Dependency Satisfaction**:
- Validates all parent steps are in `complete` or `resolved_manually` states
- Handles complex DAG structures with multiple dependency paths
- Supports conditional dependencies based on parent results

**Retry Logic**:
- Exponential backoff calculation: `2^attempts` seconds (max 30)
- Custom backoff periods from step configuration
- Retry limit enforcement to prevent infinite loops
- Failure tracking with temporal analysis

**Execution Readiness**:
- State validation (must be `pending` or `error`)
- Dependency satisfaction confirmation
- Retry eligibility assessment
- Backoff period expiration checking

### Step Readiness Implementation

The Rust integration provides type-safe access to step readiness analysis:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepReadinessStatus {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub named_step_uuid: Uuid,
    pub name: String,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_failure_at: Option<NaiveDateTime>,
    pub next_retry_at: Option<NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub retry_limit: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<NaiveDateTime>,
}

impl StepReadinessStatus {
    pub fn can_execute_now(&self) -> bool {
        self.ready_for_execution
    }

    pub fn blocking_reason(&self) -> Option<&'static str> {
        if !self.dependencies_satisfied {
            return Some("dependencies_not_satisfied");
        }
        if !self.retry_eligible {
            return Some("retry_not_eligible");
        }
        Some("invalid_state")
    }

    pub fn effective_backoff_seconds(&self) -> i32 {
        self.backoff_request_seconds.unwrap_or_else(|| {
            if self.attempts > 0 {
                std::cmp::min(2_i32.pow(self.attempts as u32), 30)
            } else {
                0
            }
        })
    }
}
```

## DAG Operations and Dependency Resolution

### Dependency Level Calculation

The `calculate_dependency_levels` function uses recursive CTEs to perform
topological analysis of the workflow DAG:

```sql
CREATE OR REPLACE FUNCTION calculate_dependency_levels(input_task_uuid UUID)
RETURNS TABLE(workflow_step_uuid UUID, dependency_level INTEGER)
LANGUAGE plpgsql STABLE AS $$
BEGIN
  RETURN QUERY
  WITH RECURSIVE dependency_levels AS (
    -- Base case: Find root nodes (steps with no dependencies)
    SELECT
      ws.workflow_step_uuid,
      0 as level
    FROM workflow_steps ws
    WHERE ws.task_uuid = input_task_uuid
      AND NOT EXISTS (
        SELECT 1
        FROM workflow_step_edges wse
        WHERE wse.to_step_uuid = ws.workflow_step_uuid
      )

    UNION ALL

    -- Recursive case: Find children of current level nodes
    SELECT
      wse.to_step_uuid as workflow_step_uuid,
      dl.level + 1 as level
    FROM dependency_levels dl
    JOIN workflow_step_edges wse ON wse.from_step_uuid = dl.workflow_step_uuid
    JOIN workflow_steps ws ON ws.workflow_step_uuid = wse.to_step_uuid
    WHERE ws.task_uuid = input_task_uuid
  )
  SELECT
    dl.workflow_step_uuid,
    MAX(dl.level) as dependency_level  -- Use MAX to handle multiple paths
  FROM dependency_levels dl
  GROUP BY dl.workflow_step_uuid
  ORDER BY dependency_level, workflow_step_uuid;
END;
```

#### Dependency Level Benefits

**Parallel Execution Planning**:
- Steps at the same dependency level can execute in parallel
- Enables optimal resource utilization across workers
- Supports batch enqueueing for scalability

**Execution Ordering**:
- Level 0: Root steps (no dependencies) - can start immediately
- Level N: Steps requiring completion of level N-1 steps
- Topological ordering ensures dependency satisfaction

**Performance Optimization**:
- Single query provides complete dependency analysis
- Avoids N+1 query problems in dependency resolution
- Enables batch processing optimizations

### Transitive Dependencies

The `get_step_transitive_dependencies` function provides complete ancestor analysis:

```sql
CREATE OR REPLACE FUNCTION get_step_transitive_dependencies(step_uuid UUID)
RETURNS TABLE(
    step_name VARCHAR,
    step_uuid UUID,
    task_uuid UUID,
    distance INTEGER,
    processed BOOLEAN,
    results JSONB
)
```

This enables step handlers to access results from any ancestor step:

```rust
impl SqlFunctionExecutor {
    pub async fn get_step_dependency_results_map(
        &self,
        step_uuid: Uuid,
    ) -> Result<HashMap<String, StepExecutionResult>, sqlx::Error> {
        let dependencies = self.get_step_transitive_dependencies(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter_map(|dep| {
                if dep.processed && dep.results.is_some() {
                    let results: StepExecutionResult = dep.results.unwrap().into();
                    Some((dep.step_name, results))
                } else {
                    None
                }
            })
            .collect())
    }
}
```

## Task Execution Context

### Recent Enhancements

#### Permanently Blocked Detection Fix (Migration 20251001000000)

The `get_task_execution_context` function was enhanced to correctly identify tasks blocked by permanent errors:

**Problem**: The function only checked `attempts >= retry_limit` to detect permanently blocked steps, missing cases where workers marked errors as non-retryable (e.g., missing handlers, configuration errors).

**Solution**: Updated `permanently_blocked_steps` calculation to check both conditions:
```sql
COUNT(CASE WHEN sd.current_state = 'error'
            AND (sd.attempts >= retry_limit OR sd.retry_eligible = false) THEN 1 END)
```

**Impact**:
- **execution_status**: Now correctly returns `blocked_by_failures` instead of `waiting_for_dependencies` for tasks with non-retryable errors
- **recommended_action**: Returns `handle_failures` instead of `wait_for_dependencies`
- **health_status**: Returns `blocked` instead of `recovering` when appropriate

This fix ensures the orchestration system properly identifies when manual intervention is needed versus when a task is simply waiting for retry backoff.

## Task Discovery and Orchestration

### Task Readiness Discovery

The system provides multiple functions for task discovery based on orchestration needs:

#### Single Task Discovery

```sql
CREATE OR REPLACE FUNCTION get_next_ready_task()
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
)
```

#### Batch Task Discovery

```sql
CREATE OR REPLACE FUNCTION get_next_ready_tasks(limit_count INTEGER)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
)
```

### Task Ready Information

The `ReadyTaskInfo` structure provides comprehensive task metadata for
orchestration decisions:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReadyTaskInfo {
    pub task_uuid: Uuid,
    pub task_name: String,
    pub priority: i32,
    pub namespace_name: String,
    pub ready_steps_count: i64,
    pub computed_priority: Option<BigDecimal>,
    pub current_state: String,
}
```

**Priority Calculation**:
- Base priority from task configuration
- Dynamic priority adjustment based on age, retry attempts
- Namespace-based priority modifiers
- SLA-based priority escalation

**Ready Steps Count**:
- Real-time count of steps eligible for execution
- Used for batch size optimization
- Influences orchestration scheduling decisions

## State Management and Atomic Transitions

### TAS-41 Atomic State Transitions

The enhanced state machine provides atomic transitions with processor ownership:

```sql
CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN AS $$
DECLARE
    v_sort_key INTEGER;
    v_transitioned BOOLEAN := FALSE;
BEGIN
    -- Get next sort key
    SELECT COALESCE(MAX(sort_key), 0) + 1 INTO v_sort_key
    FROM task_transitions
    WHERE task_uuid = p_task_uuid;

    -- Atomically transition only if in expected state
    WITH current_state AS (
        SELECT to_state, processor_uuid
        FROM task_transitions
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        FOR UPDATE
    ),
    ownership_check AS (
        SELECT
            CASE
                -- States requiring ownership
                WHEN cs.to_state IN ('initializing', 'enqueuing_steps',
                                   'steps_in_process', 'evaluating_results')
                THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
                -- Other states don't require ownership
                ELSE true
            END as can_transition
        FROM current_state cs
        WHERE cs.to_state = p_from_state
    ),
    do_update AS (
        UPDATE task_transitions
        SET most_recent = false
        WHERE task_uuid = p_task_uuid
        AND most_recent = true
        AND EXISTS (SELECT 1 FROM ownership_check WHERE can_transition)
        RETURNING task_uuid
    )
    INSERT INTO task_transitions (
        task_uuid, from_state, to_state,
        processor_uuid, transition_metadata,
        sort_key, most_recent, created_at, updated_at
    )
    SELECT
        p_task_uuid, p_from_state, p_to_state,
        p_processor_uuid, p_metadata,
        v_sort_key, true, NOW(), NOW()
    WHERE EXISTS (SELECT 1 FROM do_update);

    GET DIAGNOSTICS v_transitioned = ROW_COUNT;
    RETURN v_transitioned > 0;
END;
$$ LANGUAGE plpgsql;
```

#### Key Features

**Atomic Operation**:
- Single transaction with row-level locking
- Compare-and-swap semantics prevent race conditions
- Returns boolean indicating success/failure

**Ownership Validation**:
- Processor ownership required for active states
- Prevents concurrent processing by multiple orchestrators
- Supports ownership claiming for unowned tasks

**State Consistency**:
- Validates current state matches expected `from_state`
- Maintains audit trail with complete transition history
- Updates `most_recent` flags atomically

### Current State Resolution

Fast current state lookups are provided through optimized queries:

```rust
impl SqlFunctionExecutor {
    pub async fn get_current_task_state(&self, task_uuid: Uuid)
        -> Result<TaskState, sqlx::Error> {
        let state_str = sqlx::query_scalar!(
            r#"SELECT get_current_task_state($1) as "state""#,
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| sqlx::Error::RowNotFound)?;

        match state_str {
            Some(state) => TaskState::try_from(state.as_str())
                .map_err(|_| sqlx::Error::Decode("Invalid task state".into())),
            None => Err(sqlx::Error::RowNotFound),
        }
    }
}
```

## Analytics and System Health

### System Health Monitoring

The `get_system_health_counts` function provides comprehensive system visibility:

```sql
CREATE OR REPLACE FUNCTION get_system_health_counts()
RETURNS TABLE(
    pending_tasks BIGINT,
    initializing_tasks BIGINT,
    enqueuing_steps_tasks BIGINT,
    steps_in_process_tasks BIGINT,
    evaluating_results_tasks BIGINT,
    waiting_for_dependencies_tasks BIGINT,
    waiting_for_retry_tasks BIGINT,
    blocked_by_failures_tasks BIGINT,
    complete_tasks BIGINT,
    error_tasks BIGINT,
    cancelled_tasks BIGINT,
    resolved_manually_tasks BIGINT,
    total_tasks BIGINT,
    -- step counts...
) AS $$
```

#### Health Score Calculation

The Rust implementation provides derived health metrics:

```rust
impl SystemHealthCounts {
    pub fn health_score(&self) -> f64 {
        if self.total_tasks == 0 {
            return 1.0;
        }

        let success_rate = self.complete_tasks as f64 / self.total_tasks as f64;
        let error_rate = self.error_tasks as f64 / self.total_tasks as f64;
        let connection_health = 1.0 -
            (self.active_connections as f64 / self.max_connections as f64).min(1.0);

        // Weighted combination: 50% success rate, 30% error rate, 20% connection health
        (success_rate * 0.5) + ((1.0 - error_rate) * 0.3) + (connection_health * 0.2)
    }

    pub fn is_under_heavy_load(&self) -> bool {
        let connection_pressure =
            self.active_connections as f64 / self.max_connections as f64;
        let error_rate = if self.total_tasks > 0 {
            self.error_tasks as f64 / self.total_tasks as f64
        } else {
            0.0
        };

        connection_pressure > 0.8 || error_rate > 0.2
    }
}
```

### Analytics Metrics

The `get_analytics_metrics` function provides comprehensive performance analysis:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AnalyticsMetrics {
    pub active_tasks_count: i64,
    pub total_namespaces_count: i64,
    pub unique_task_types_count: i64,
    pub system_health_score: BigDecimal,
    pub task_throughput: i64,
    pub completion_count: i64,
    pub error_count: i64,
    pub completion_rate: BigDecimal,
    pub error_rate: BigDecimal,
    pub avg_task_duration: BigDecimal,
    pub avg_step_duration: BigDecimal,
    pub step_throughput: i64,
    pub analysis_period_start: DateTime<Utc>,
    pub calculated_at: DateTime<Utc>,
}
```

## Performance Optimization Analysis

### Slowest Steps Analysis

The system provides performance optimization guidance through detailed analysis:

```sql
CREATE OR REPLACE FUNCTION get_slowest_steps(
    since_timestamp TIMESTAMP WITH TIME ZONE,
    limit_count INTEGER,
    namespace_filter VARCHAR,
    task_name_filter VARCHAR,
    version_filter VARCHAR
) RETURNS TABLE(
    named_step_uuid INTEGER,
    step_name VARCHAR,
    avg_duration_seconds NUMERIC,
    max_duration_seconds NUMERIC,
    min_duration_seconds NUMERIC,
    execution_count INTEGER,
    error_count INTEGER,
    error_rate NUMERIC,
    last_executed_at TIMESTAMP WITH TIME ZONE
)
```

### Slowest Tasks Analysis

Similar analysis is available at the task level:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestTaskAnalysis {
    pub named_task_uuid: Uuid,
    pub task_name: String,
    pub avg_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub min_duration_seconds: f64,
    pub execution_count: i32,
    pub avg_step_count: f64,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<DateTime<Utc>>,
}
```

## Critical Problem-Solving SQL Functions

### PGMQ Message Race Condition Prevention

#### Problem: Multiple Workers Claiming Same Message

When multiple workers simultaneously try to process steps from the same queue, PGMQ's standard
`pgmq.read()` function randomly selects messages, potentially causing workers to miss messages
they were specifically notified about. This creates inefficiency and potential race conditions.

#### Solution: pgmq_read_specific_message()

```sql
CREATE OR REPLACE FUNCTION pgmq_read_specific_message(
    queue_name text,
    target_msg_id bigint,
    vt_seconds integer DEFAULT 30
) RETURNS TABLE (
    msg_id bigint,
    read_ct integer,
    enqueued_at timestamp with time zone,
    vt timestamp with time zone,
    message jsonb
) AS $$
```

**Key Problem-Solving Logic**:

1. **Atomic Claim with Visibility Timeout**: Uses UPDATE...RETURNING pattern to atomically:
   - Check if message is available (`vt <= now()`)
   - Set new visibility timeout preventing other workers from claiming
   - Increment read count for monitoring retry attempts
   - Return message data only if successfully claimed

2. **Race Condition Prevention**: The `WHERE vt <= now()` clause ensures only one worker
   can claim a message. If two workers try simultaneously, only one UPDATE succeeds.

3. **Graceful Failure Handling**: Returns empty result set if message is:
   - Already claimed by another worker (vt > now())
   - Non-existent (deleted or never existed)
   - Archived (moved to archive table)

4. **Security**: Validates queue name to prevent SQL injection in dynamic query construction.

**Real-World Impact**: Eliminates "message not found" errors when workers are notified
about specific messages but can't retrieve them due to random selection in standard read.

### Task State Ownership and Atomic Transitions

#### Problem: Concurrent Orchestrators Processing Same Task

In distributed deployments, multiple orchestrator instances might try to process the same
task simultaneously, leading to duplicate work, inconsistent state, and race conditions.

#### Solution: transition_task_state_atomic()

```sql
CREATE OR REPLACE FUNCTION transition_task_state_atomic(
    p_task_uuid UUID,
    p_from_state VARCHAR,
    p_to_state VARCHAR,
    p_processor_uuid UUID,
    p_metadata JSONB DEFAULT '{}'
) RETURNS BOOLEAN AS $$
```

**Key Problem-Solving Logic**:

1. **Compare-and-Swap Pattern**:
   - Reads current state with `FOR UPDATE` lock
   - Only transitions if current state matches expected `from_state`
   - Returns false if state has changed, allowing caller to retry with fresh state

2. **Processor Ownership Enforcement**:
   ```sql
   CASE
       WHEN cs.to_state IN ('initializing', 'enqueuing_steps',
                           'steps_in_process', 'evaluating_results')
       THEN cs.processor_uuid = p_processor_uuid OR cs.processor_uuid IS NULL
       ELSE true
   END
   ```
   - Active processing states require ownership match
   - Allows claiming unowned tasks (NULL processor_uuid)
   - Terminal states (complete, error) don't require ownership

3. **Audit Trail Preservation**:
   - Updates previous transition's `most_recent = false`
   - Inserts new transition with `most_recent = true`
   - Maintains complete history with sort_key ordering

4. **Atomic Success/Failure**: Returns boolean indicating whether transition succeeded,
   enabling callers to handle contention gracefully.

**Real-World Impact**: Enables safe distributed orchestration where multiple instances
can operate without conflicts, automatically distributing work through ownership claiming.

### Batch Task Discovery with Priority

#### Problem: Efficient Work Distribution Across Orchestrators

Orchestrators need to discover ready tasks efficiently without creating hotspots or
missing tasks, while respecting priority and avoiding claimed tasks.

#### Solution: get_next_ready_tasks()

```sql
CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name TEXT,
    priority INTEGER,
    namespace_name TEXT,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
)
```

**Key Problem-Solving Logic**:

1. **Ready Step Discovery**:
   ```sql
   WITH ready_steps AS (
       SELECT task_uuid, COUNT(*) as ready_count
       FROM workflow_steps
       WHERE current_state IN ('pending', 'error')
       AND [dependency checks]
       GROUP BY task_uuid
   )
   ```
   - Pre-aggregates ready steps per task for efficiency
   - Considers both new steps and retryable errors

2. **State-Based Filtering**:
   - Only returns tasks in states that need processing
   - Excludes terminal states (complete, cancelled)
   - Includes waiting states that might have become ready

3. **Priority Computation**:
   ```sql
   computed_priority = base_priority +
                      (age_factor * hours_waiting) +
                      (retry_factor * retry_count)
   ```
   - Dynamic priority based on age and retry attempts
   - Prevents task starvation through age escalation

4. **Batch Efficiency**:
   - Returns multiple tasks in single query
   - Reduces database round trips
   - Enables parallel processing across orchestrators

**Real-World Impact**: Enables efficient work distribution where each orchestrator
can claim a batch of tasks, reducing contention and improving throughput.

### Complex Dependency Resolution

#### Problem: Determining Step Execution Readiness

Workflow steps have complex dependencies involving parent completion, retry logic, backoff
timing, and state validation. Determining which steps are ready for execution requires
sophisticated dependency analysis that must handle:

- Multiple parent dependencies with conditional logic
- Exponential backoff after failures
- Retry limits and attempt tracking
- State consistency across distributed workers

#### Solution: get_step_readiness_status()

```sql
CREATE OR REPLACE FUNCTION get_step_readiness_status(
    input_task_uuid UUID,
    step_uuids UUID[] DEFAULT NULL
) RETURNS TABLE(
    workflow_step_uuid UUID,
    task_uuid UUID,
    named_step_uuid UUID,
    name VARCHAR,
    current_state VARCHAR,
    dependencies_satisfied BOOLEAN,
    retry_eligible BOOLEAN,
    ready_for_execution BOOLEAN,
    -- ... additional metadata
)
```

**Key Problem-Solving Logic**:

1. **Dependency Satisfaction Analysis**:
   ```sql
   WITH parent_completion AS (
       SELECT
           edge.to_step_uuid,
           COUNT(*) as total_parents,
           COUNT(CASE WHEN parent.current_state = 'complete' THEN 1 END) as completed_parents
       FROM workflow_step_edges edge
       JOIN workflow_steps parent ON parent.workflow_step_uuid = edge.from_step_uuid
       WHERE parent.task_uuid = input_task_uuid
       GROUP BY edge.to_step_uuid
   )
   ```
   - Counts total vs. completed parent dependencies
   - Handles conditional dependencies based on parent results
   - Supports complex DAG structures with multiple paths

2. **Retry Eligibility Assessment**:
   ```sql
   retry_eligible = (
       current_state = 'error' AND
       attempts < retry_limit AND
       (last_attempted_at IS NULL OR
        last_attempted_at + backoff_interval <= NOW())
   )
   ```
   - Enforces retry limits to prevent infinite loops
   - Calculates exponential backoff: `2^attempts` seconds (max 30)
   - Respects custom backoff periods from step configuration
   - Considers temporal constraints for retry timing

3. **State Validation**:
   ```sql
   ready_for_execution = (
       current_state IN ('pending', 'error') AND
       dependencies_satisfied AND
       retry_eligible
   )
   ```
   - Only pending or retryable error steps can execute
   - Requires all dependencies satisfied
   - Must pass retry eligibility checks
   - Prevents execution of steps in terminal states

4. **Backoff Calculation**:
   ```sql
   next_retry_at = CASE
       WHEN current_state = 'error' AND attempts > 0
       THEN last_attempted_at + INTERVAL '1 second' *
            COALESCE(backoff_request_seconds, LEAST(POW(2, attempts), 30))
       ELSE NULL
   END
   ```
   - Custom backoff from step configuration takes precedence
   - Default exponential backoff with maximum cap
   - Temporal precision for scheduling retry attempts

**Real-World Impact**: Enables complex workflow orchestration with sophisticated dependency
management, retry logic, and backoff handling, supporting enterprise-grade reliability
patterns while maintaining high performance through set-based operations.

## Integration with Event and State Systems

### PostgreSQL LISTEN/NOTIFY Integration

The SQL functions integrate with the event-driven architecture through PostgreSQL
notifications:

#### PGMQ Wrapper Functions for Atomic Operations

The system uses wrapper functions that combine PGMQ message sending with PostgreSQL notifications atomically:

```sql
-- Atomic wrapper that sends message AND notification
CREATE OR REPLACE FUNCTION pgmq_send_with_notify(
    queue_name TEXT,
    message JSONB,
    delay_seconds INTEGER DEFAULT 0
) RETURNS BIGINT AS $$
DECLARE
    msg_id BIGINT;
    namespace_name TEXT;
    event_payload TEXT;
    namespace_channel TEXT;
    global_channel TEXT := 'pgmq_message_ready';
BEGIN
    -- Send message using PGMQ's native function
    SELECT pgmq.send(queue_name, message, delay_seconds) INTO msg_id;

    -- Extract namespace from queue name using robust helper
    namespace_name := extract_queue_namespace(queue_name);

    -- Build namespace-specific channel name
    namespace_channel := 'pgmq_message_ready.' || namespace_name;

    -- Build event payload
    event_payload := json_build_object(
        'event_type', 'message_ready',
        'msg_id', msg_id,
        'queue_name', queue_name,
        'namespace', namespace_name,
        'ready_at', NOW()::timestamptz,
        'delay_seconds', delay_seconds
    )::text;

    -- Send notifications in same transaction
    PERFORM pg_notify(namespace_channel, event_payload);

    -- Also send to global channel if different
    IF namespace_channel != global_channel THEN
        PERFORM pg_notify(global_channel, event_payload);
    END IF;

    RETURN msg_id;
END;
$$ LANGUAGE plpgsql;
```

#### Namespace Extraction Helper

```sql
-- Robust namespace extraction helper function
CREATE OR REPLACE FUNCTION extract_queue_namespace(queue_name TEXT)
RETURNS TEXT AS $$
BEGIN
    -- Handle orchestration queues
    IF queue_name ~ '^orchestration' THEN
        RETURN 'orchestration';
    END IF;

    -- Handle worker queues: worker_namespace_queue -> namespace
    IF queue_name ~ '^worker_.*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^worker_(.+?)_queue$'))[1],
            'worker'
        );
    END IF;

    -- Handle standard namespace_queue pattern
    IF queue_name ~ '^[a-zA-Z][a-zA-Z0-9_]*_queue$' THEN
        RETURN COALESCE(
            (regexp_match(queue_name, '^([a-zA-Z][a-zA-Z0-9_]*)_queue$'))[1],
            'default'
        );
    END IF;

    -- Fallback for any other pattern
    RETURN 'default';
END;
$$ LANGUAGE plpgsql;
```

#### Fallback Polling for Task Readiness

Instead of database triggers for task readiness notifications, the system uses a fallback polling mechanism to ensure no ready tasks are missed:

**FallbackPoller Configuration**:
- Default polling interval: 30 seconds
- Runs `StepEnqueuerService::process_batch()` periodically
- Catches tasks that may have been missed by primary PGMQ notification system
- Configurable enable/disable via TOML configuration

**Key Benefits**:
- **Resilience**: Ensures no tasks are permanently stuck if notifications fail
- **Simplicity**: No complex database triggers or state tracking required
- **Observability**: Clear metrics on fallback discovery vs. event-driven discovery
- **Safety Net**: Primary event-driven system + fallback polling provides redundancy

### PGMQ Message Queue Integration

SQL functions coordinate with PGMQ for reliable message processing:

#### Queue Management Functions

```sql
-- Ensure queue exists with proper configuration
CREATE OR REPLACE FUNCTION ensure_task_queue(queue_name VARCHAR)
RETURNS BOOLEAN AS $$
BEGIN
    -- Create queue if it doesn't exist
    PERFORM pgmq.create_queue(queue_name);

    -- Ensure headers column exists (pgmq-rs compatibility)
    PERFORM pgmq_ensure_headers_column(queue_name);

    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;
```

#### Message Processing Support

```sql
-- Get queue statistics for monitoring
CREATE OR REPLACE FUNCTION get_queue_statistics(queue_name VARCHAR)
RETURNS TABLE(
    queue_name VARCHAR,
    queue_length BIGINT,
    oldest_msg_age_seconds INTEGER,
    newest_msg_age_seconds INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        queue_name,
        pgmq.queue_length(queue_name),
        EXTRACT(EPOCH FROM (NOW() - MIN(enqueued_at)))::INTEGER,
        EXTRACT(EPOCH FROM (NOW() - MAX(enqueued_at)))::INTEGER
    FROM pgmq.messages(queue_name);
END;
$$ LANGUAGE plpgsql;
```

### Transaction Safety

All SQL functions are designed with transaction safety in mind:

**Atomic Operations**:
- State transitions use row-level locking (`FOR UPDATE`)
- Compare-and-swap patterns prevent race conditions
- Rollback safety for partial failures

**Consistency Guarantees**:
- Foreign key constraints maintained across all operations
- Check constraints validate state transitions
- Audit trails preserved for debugging and compliance

**Performance Optimization**:
- Efficient indexes for common query patterns
- Materialized views for expensive analytics queries
- Connection pooling for high concurrency

## Usage Patterns and Best Practices

### Rust Integration Patterns

The `SqlFunctionExecutor` provides type-safe access to all SQL functions:

```rust
use tasker_shared::database::sql_functions::{SqlFunctionExecutor, FunctionRegistry};

// Direct executor usage
let executor = SqlFunctionExecutor::new(pool);
let ready_steps = executor.get_ready_steps(task_uuid).await?;

// Registry pattern for organized access
let registry = FunctionRegistry::new(pool);
let analytics = registry.analytics().get_analytics_metrics(None).await?;
let health = registry.system_health().get_system_health_counts().await?;
```

### Batch Processing Optimization

For high-throughput scenarios, the system supports efficient batch operations:

```rust
// Batch step readiness analysis
let task_uuids = vec![task1_uuid, task2_uuid, task3_uuid];
let batch_readiness = executor.get_step_readiness_status_batch(task_uuids).await?;

// Batch task discovery
let ready_tasks = executor.get_next_ready_tasks(50).await?;
```

### Error Handling Best Practices

SQL function errors are properly propagated through the type system:

```rust
match executor.get_current_task_state(task_uuid).await {
    Ok(state) => {
        // Process state
    }
    Err(sqlx::Error::RowNotFound) => {
        // Handle missing task
    }
    Err(e) => {
        // Handle other database errors
    }
}
```
