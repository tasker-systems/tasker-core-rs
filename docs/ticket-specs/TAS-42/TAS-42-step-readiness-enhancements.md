# TAS-42: Step Readiness SQL Function Enhancements

## Overview

Refactor step readiness SQL functions to eliminate code duplication, fix logical inconsistencies, and create reusable helper functions for better maintainability.

## Problem Analysis

### Issue 1: Backoff Logic Duplication (3 instances)

The `get_step_readiness_status` function contains nearly identical backoff calculation logic in 3 places:

- **Lines 189-198**: Pending state backoff evaluation (conditional for readiness)
- **Lines 208-215**: WaitingForRetry state backoff evaluation (conditional for readiness)
- **Lines 227-233**: Next retry calculation (result field calculation)

All three contain essentially the same logic:
```sql
CASE
  WHEN ws.backoff_request_seconds IS NOT NULL AND ws.last_attempted_at IS NOT NULL THEN
    ws.last_attempted_at + (ws.backoff_request_seconds * interval '1 second')
  ELSE
    lf.failure_time + (LEAST(power(2, COALESCE(ws.attempts, 1)) * interval '1 second', interval '30 seconds'))
END
```

### Issue 2: Inconsistent State Logic

The readiness evaluation logic differs between `pending` and `waiting_for_retry` states:

- **Pending state (lines 182-185)**: Includes `processed` and `in_process` checks
- **WaitingForRetry state (lines 203-205)**: Missing `processed` and `in_process` checks

This inconsistency could lead to race conditions where steps in `waiting_for_retry` state are marked as ready even when they're already processed or being processed.

### Issue 3: Complex Nested Logic

The main readiness evaluation contains deeply nested CASE statements that are difficult to reason about and maintain.

## Solution Architecture

### Helper Functions Approach

Create **2 focused helper functions** to encapsulate business logic:

#### 1. Backoff Time Calculation Function
```sql
CREATE OR REPLACE FUNCTION calculate_step_next_retry_time(
    backoff_request_seconds INTEGER,
    last_attempted_at TIMESTAMP,
    failure_time TIMESTAMP,
    attempts INTEGER
) RETURNS TIMESTAMP
LANGUAGE SQL STABLE AS $$
    SELECT CASE
        WHEN backoff_request_seconds IS NOT NULL AND last_attempted_at IS NOT NULL THEN
            last_attempted_at + (backoff_request_seconds * interval '1 second')
        WHEN failure_time IS NOT NULL THEN
            failure_time + (LEAST(power(2, COALESCE(attempts, 1)) * interval '1 second', interval '30 seconds'))
        ELSE NULL
    END
$$;
```

**Purpose**: Single source of truth for all backoff time calculations

#### 2. Step State Readiness Evaluation Function
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
LANGUAGE SQL STABLE AS $$
    SELECT
        COALESCE(current_state, 'pending') IN ('pending', 'waiting_for_retry')
        AND (processed = false OR processed IS NULL)
        AND (in_process = false OR in_process IS NULL)
        AND dependencies_satisfied
        AND retry_eligible
        AND retryable
        AND (next_retry_time IS NULL OR next_retry_time <= NOW())
$$;
```

**Purpose**: Unified readiness evaluation with consistent logic for all ready-eligible states

### Simplified Main Function Structure

With helper functions, the main query becomes:

```sql
-- Add retry_times CTE for efficient calculation
retry_times AS (
  SELECT
    ws.workflow_step_uuid,
    calculate_step_next_retry_time(
        ws.backoff_request_seconds,
        ws.last_attempted_at,
        lf.failure_time,
        ws.attempts
    ) as next_retry_time
  FROM tasker_workflow_steps ws
  INNER JOIN task_steps ts ON ts.workflow_step_uuid = ws.workflow_step_uuid
  LEFT JOIN last_failures lf ON lf.workflow_step_uuid = ws.workflow_step_uuid
)

SELECT
    -- ... other fields ...

    -- Ready for execution (CLEAN!)
    evaluate_step_state_readiness(
        COALESCE(ss.to_state, 'pending'),
        ws.processed,
        ws.in_process,
        (dc.total_deps IS NULL OR dc.completed_deps = dc.total_deps),
        (COALESCE(ws.attempts, 0) < COALESCE(ws.retry_limit, 3)),
        COALESCE(ws.retryable, true),
        rt.next_retry_time
    ) as ready_for_execution,

    -- Next retry time (ALREADY CALCULATED!)
    rt.next_retry_time as next_retry_at
```

## Implementation Strategy

### In-Place Migration Update

Update the existing migration file `migrations/20250927000000_add_waiting_for_retry_state.sql`:

1. **Add helper functions** after the constraint updates (around line 80)
2. **Replace both main functions** (`get_step_readiness_status` and `get_step_readiness_status_batch`) with simplified versions
3. **Maintain all performance optimizations** (task-scoped CTEs, INNER JOINs)

### File Organization
```sql
-- 1. CHECK constraint updates (existing, unchanged)
-- 2. NEW: Helper function definitions
-- 3. UPDATED: Main readiness functions using helpers
-- 4. Documentation comments (existing, updated)
```

## Benefits

### Code Quality
✅ **Eliminates ALL duplication** - backoff logic exists in exactly one place
✅ **Fixes logical inconsistencies** - both states use identical readiness criteria
✅ **Dramatically improves readability** - main queries become declarative
✅ **Future-proof design** - easy to add new ready-eligible states

### Functionality
✅ **Prevents race conditions** - consistent processed/in_process checking
✅ **Single source of truth** for backoff calculations
✅ **Consistent behavior** between pending and waiting_for_retry states

### Maintainability
✅ **Reusable functions** for other parts of the system
✅ **Testable logic** - can unit test backoff and readiness separately
✅ **Simplified debugging** - isolated function concerns

### Performance
✅ **Maintains all existing optimizations** - task-scoped CTEs, efficient JOINs
✅ **Better performance** - calculate once, use everywhere
✅ **No additional overhead** - helper functions are SQL STABLE/IMMUTABLE

## Testing Strategy

### Function-Level Testing
- Test `calculate_step_next_retry_time` with various backoff scenarios
- Test `evaluate_step_state_readiness` with different state combinations
- Verify consistent behavior between pending and waiting_for_retry

### Integration Testing
- Validate main functions return identical results to previous implementation
- Test performance regression prevention
- Verify all existing error handling workflows continue to work

## Migration Safety

- **Single atomic migration** - no sequencing dependencies
- **Backward compatible** - same function signatures and return types
- **Performance preserving** - maintains all existing optimizations
- **Logic correcting** - fixes bugs while maintaining intended behavior

## Related Work

This refactoring builds upon:
- **TAS-41**: Enhanced state machines with WaitingForRetry state
- **Previous optimization work**: Task-scoped CTEs and performance improvements
- **Error handling enhancements**: Integration with backoff calculator and error classification

## Success Criteria

1. **Zero duplication** in backoff calculation logic
2. **Consistent readiness evaluation** across all ready-eligible states
3. **Maintained performance** - no regression in query execution time
4. **Improved maintainability** - helper functions enable easier modifications
5. **Fixed race conditions** - proper processed/in_process checking for all states