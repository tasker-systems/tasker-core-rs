# TAS-48: Task Staleness Immediate Relief

## Executive Summary

Implement immediate relief for task discovery priority drowning caused by stale tasks accumulating in waiting states. During TAS-29 Phase 6 testing, we discovered 4,459 stale tasks were blocking discovery of legitimate ready tasks by filling the priority-based pre-filter buffer in `get_next_ready_tasks()`. This ticket implements two complementary solutions: (1A) staleness-aware filtering to exclude long-stuck tasks from discovery, and (1B) priority decay to reduce the computed priority of aging tasks, ensuring fresh tasks are always discoverable.

**Dependencies**: None (can be implemented immediately after TAS-29 merge)
**Impact**: High - Resolves production-critical discovery blocking
**Effort**: Low - 1-2 days implementation
**Type**: Quick Win / Bug Fix

---

## Problem Statement

### Discovery Blocking by Stale Tasks

The current `get_next_ready_tasks()` implementation uses a two-stage filtering approach:

1. **Pre-filter stage**: Select top `(p_limit * 10)` tasks by `computed_priority` (e.g., 50 requested → 500 pre-filtered)
2. **Execution status stage**: Apply `execution_status = 'has_ready_steps'` filter to pre-filtered candidates

**Critical Issue**: Stale tasks in `waiting_for_dependencies` or `waiting_for_retry` states with high computed priority fill the 500-slot buffer, preventing legitimate ready tasks from being discovered even when they have higher business priority.

### Investigation Context

During TAS-29 Phase 6 testing, the `test_retryable_failure_scenario` test failed with the task stuck in `has_ready_steps` state indefinitely:

```sql
-- Test task correctly identified as ready:
SELECT execution_status, ready_steps
FROM get_task_execution_context('0199c904-f81c-763d-a281-5b08be1c6655');
-- Result: has_ready_steps | 1

-- But never discovered by poller:
SELECT COUNT(*) FROM get_next_ready_tasks(50);
-- Result: 0

-- Root cause: 4,459 stale tasks with higher priority:
SELECT COUNT(*) FROM tasker_task_transitions
WHERE most_recent = true AND to_state = 'waiting_for_dependencies';
-- Result: 2,978

-- Priority comparison:
SELECT priority FROM tasker_tasks WHERE task_uuid = '0199c904-...';
-- Test task: 5.04
-- Stale tasks: 6.92-11.72 (filling top 500 slots)
```

The test task was legitimate and ready for processing but never discovered because the priority queue was saturated with stale tasks.

---

## Background

### Current Priority Computation

From `migrations/20250912000000_tas41_richer_task_states.sql` (lines 250-251):

```sql
-- Compute priority with age escalation
t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
```

This creates a **monotonically increasing priority** where:
- All tasks continuously gain priority as they age
- Stale tasks accumulate maximum age escalation
- No mechanism to de-prioritize stuck tasks
- Fresh tasks with lower base priority get drowned out

### Current State Filtering

From lines 262-266:

```sql
WHERE tt.most_recent = true
  -- Tasks that can be picked up for processing
  AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
  AND tt_processing.task_uuid IS NULL  -- Not already being processed
  ORDER BY computed_priority DESC, t.created_at ASC
```

**No staleness awareness**: A task stuck in `waiting_for_dependencies` for 24 hours has the same discoverability as one that just entered that state 5 minutes ago.

---

## Solution 1A: Staleness-Aware Discovery Filtering

### Overview

Add time-based exclusion criteria to `get_next_ready_tasks()` to prevent long-stuck tasks from consuming discovery slots.

### Implementation

#### Migration: `20251010000000_add_staleness_exclusion.sql`

```sql
-- ============================================================================
-- TAS-48 Solution 1A: Add staleness-aware filtering to get_next_ready_tasks
-- ============================================================================

-- Replace get_next_ready_tasks with staleness-aware version
CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
DECLARE
    v_max_waiting_for_deps_minutes INTEGER;
    v_max_waiting_for_retry_minutes INTEGER;
    v_enable_staleness_exclusion BOOLEAN;
BEGIN
    -- Load configuration (default values if not set)
    -- TODO: Load from tasker_config table in future
    v_max_waiting_for_deps_minutes := 60;
    v_max_waiting_for_retry_minutes := 30;
    v_enable_staleness_exclusion := true;

    RETURN QUERY
    WITH task_candidates AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            tt.created_at as state_entered_at,
            -- Compute priority with age escalation
            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1) as computed_priority
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        -- Tasks that can be picked up for processing
        AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
        AND tt_processing.task_uuid IS NULL  -- Not already being processed
        -- TAS-48 Solution 1A: Staleness-aware filtering
        AND (
            NOT v_enable_staleness_exclusion  -- Feature flag disabled
            OR tt.to_state = 'pending'  -- New tasks always eligible
            OR (
                tt.to_state = 'waiting_for_dependencies'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_deps_minutes
            )
            OR (
                tt.to_state = 'waiting_for_retry'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_retry_minutes
            )
        )
        ORDER BY computed_priority DESC, t.created_at ASC
        LIMIT p_limit * 10  -- Pre-filter more for batch
    ),
    task_with_context AS (
        SELECT
            tc.task_uuid,
            tc.task_name,
            tc.priority,
            tc.namespace_name,
            ctx.ready_steps,
            tc.computed_priority,
            tc.current_state,
            ctx.execution_status
        FROM task_candidates tc
        CROSS JOIN LATERAL get_task_execution_context(tc.task_uuid) ctx
        -- Pending tasks don't have steps yet, others must have ready steps
        WHERE (tc.current_state = 'pending')
           OR (tc.current_state != 'pending' AND ctx.execution_status = 'has_ready_steps')
    )
    SELECT
        twc.task_uuid,
        twc.task_name,
        twc.priority,
        twc.namespace_name,
        COALESCE(twc.ready_steps, 0) as ready_steps_count,
        twc.computed_priority,
        twc.current_state
    FROM task_with_context twc
    ORDER BY twc.computed_priority DESC
    LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_next_ready_tasks IS
'TAS-48: Returns ready tasks with staleness-aware filtering to prevent stale tasks from drowning out fresh tasks.
Excludes tasks stuck in waiting states beyond configured thresholds.';
```

### Configuration

Add to `config/tasker/base/state_machine.toml`:

```toml
[state_machine.discovery]
# TAS-48: Staleness-aware discovery filtering
enable_stale_task_exclusion = true

# Exclude tasks from discovery if stuck in waiting states beyond threshold
max_waiting_for_dependencies_discovery_minutes = 60
max_waiting_for_retry_discovery_minutes = 30  # Based on max exponential backoff

# Discovery pool optimization
discovery_pool_multiplier = 10  # Current: p_limit * 10
enable_adaptive_pooling = false  # Future: adjust based on stale task ratio
min_pool_multiplier = 5
max_pool_multiplier = 20
```

Add environment overrides in `config/tasker/environments/test/state_machine.toml`:

```toml
[state_machine.discovery]
# Tighter thresholds for testing
max_waiting_for_dependencies_discovery_minutes = 5
max_waiting_for_retry_discovery_minutes = 2
```

### Benefits

1. **Immediate Relief**: Stale tasks automatically excluded from discovery pool
2. **Configurable Thresholds**: Per-environment tuning capability
3. **Feature Flag**: Can be disabled if issues arise
4. **Zero Breaking Changes**: Legitimate tasks unaffected
5. **Quick Implementation**: Single SQL function replacement

### Risks & Mitigation

**Risk**: Tasks legitimately waiting for external dependencies might be excluded
**Mitigation**: Conservative 60-minute default threshold; configurable per environment

**Risk**: Excluded tasks become permanently undiscoverable
**Mitigation**: TAS-49 will implement staleness detection and automatic DLQ transitions

---

## Solution 1B: Weighted Priority with Decay

### Overview

Replace simple age escalation with **exponential decay** for aging tasks, ensuring fresh tasks always rise to the top while stale tasks sink to DLQ-candidate priority.

### Priority Decay Function

```sql
-- Compute priority with age escalation AND staleness decay
CASE
  -- Fresh tasks (< 1 hour): normal priority + age bonus
  WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (60 * 60) THEN
    t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1)

  -- Aging tasks (1 hour - 1 day): exponential decay
  WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (60 * 60 * 24) THEN
    t.priority * EXP(-1 * EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / (60 * 60 * 12))

  -- Stale tasks (> 1 day): minimal priority (DLQ candidates)
  ELSE
    0.1
END as computed_priority
```

### Mathematical Behavior

| Time in State | Base Priority | Computed Priority | Notes |
|---------------|---------------|-------------------|-------|
| 5 minutes | 5.0 | 5.008 | Normal age escalation |
| 30 minutes | 5.0 | 5.05 | Continues escalating |
| 1 hour | 5.0 | 5.1 | Peak priority (transition point) |
| 6 hours | 5.0 | 3.03 | Decay begins |
| 12 hours | 5.0 | 1.84 | Half-life reached |
| 24 hours | 5.0 | 0.68 | Approaching DLQ threshold |
| 48 hours | 5.0 | 0.1 | DLQ candidate (minimum priority) |

**Key Properties**:
- Fresh tasks get normal priority boost
- After 1 hour, priority decays exponentially
- 12-hour half-life prevents permanent starvation
- After 24 hours, tasks become DLQ candidates

### Implementation

#### Migration: `20251010000001_add_priority_decay.sql`

```sql
-- ============================================================================
-- TAS-48 Solution 1B: Add exponential priority decay for aging tasks
-- ============================================================================

CREATE OR REPLACE FUNCTION get_next_ready_tasks(p_limit INTEGER DEFAULT 5)
RETURNS TABLE(
    task_uuid UUID,
    task_name VARCHAR,
    priority INTEGER,
    namespace_name VARCHAR,
    ready_steps_count BIGINT,
    computed_priority NUMERIC,
    current_state VARCHAR
) AS $$
DECLARE
    v_max_waiting_for_deps_minutes INTEGER;
    v_max_waiting_for_retry_minutes INTEGER;
    v_enable_staleness_exclusion BOOLEAN;
    v_enable_priority_decay BOOLEAN;
    v_decay_start_hours NUMERIC;
    v_decay_half_life_hours NUMERIC;
    v_stale_threshold_hours NUMERIC;
BEGIN
    -- Load configuration (default values if not set)
    v_max_waiting_for_deps_minutes := 60;
    v_max_waiting_for_retry_minutes := 30;
    v_enable_staleness_exclusion := true;
    v_enable_priority_decay := true;
    v_decay_start_hours := 1.0;
    v_decay_half_life_hours := 12.0;
    v_stale_threshold_hours := 24.0;

    RETURN QUERY
    WITH task_candidates AS (
        SELECT
            t.task_uuid,
            nt.name as task_name,
            t.priority,
            tns.name as namespace_name,
            tt.to_state as current_state,
            tt.created_at as state_entered_at,
            -- TAS-48 Solution 1B: Priority with exponential decay
            CASE
                WHEN NOT v_enable_priority_decay THEN
                    -- Legacy: simple age escalation
                    t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1)
                ELSE
                    -- New: decay function
                    CASE
                        -- Fresh tasks: normal age escalation
                        WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (v_decay_start_hours * 3600) THEN
                            t.priority + (EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600 * 0.1)

                        -- Aging tasks: exponential decay
                        WHEN EXTRACT(EPOCH FROM (NOW() - tt.created_at)) < (v_stale_threshold_hours * 3600) THEN
                            t.priority * EXP(-1 * EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / (v_decay_half_life_hours * 3600))

                        -- Stale tasks: minimum priority (DLQ candidates)
                        ELSE
                            0.1
                    END
            END as computed_priority
        FROM tasker_tasks t
        JOIN tasker_task_transitions tt ON tt.task_uuid = t.task_uuid
        JOIN tasker_named_tasks nt on nt.named_task_uuid = t.named_task_uuid
        JOIN tasker_task_namespaces tns on tns.task_namespace_uuid = nt.task_namespace_uuid
        LEFT JOIN tasker_task_transitions tt_processing
            ON tt_processing.task_uuid = t.task_uuid
            AND tt_processing.most_recent = true
            AND tt_processing.processor_uuid IS NOT NULL
            AND tt_processing.to_state IN ('initializing', 'enqueuing_steps', 'steps_in_process', 'evaluating_results')
        WHERE tt.most_recent = true
        AND tt.to_state IN ('pending', 'waiting_for_dependencies', 'waiting_for_retry')
        AND tt_processing.task_uuid IS NULL
        -- TAS-48 Solution 1A: Staleness exclusion
        AND (
            NOT v_enable_staleness_exclusion
            OR tt.to_state = 'pending'
            OR (
                tt.to_state = 'waiting_for_dependencies'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_deps_minutes
            )
            OR (
                tt.to_state = 'waiting_for_retry'
                AND EXTRACT(EPOCH FROM (NOW() - tt.created_at)) / 60 <= v_max_waiting_for_retry_minutes
            )
        )
        ORDER BY computed_priority DESC, t.created_at ASC
        LIMIT p_limit * 10
    ),
    task_with_context AS (
        SELECT
            tc.task_uuid,
            tc.task_name,
            tc.priority,
            tc.namespace_name,
            ctx.ready_steps,
            tc.computed_priority,
            tc.current_state,
            ctx.execution_status
        FROM task_candidates tc
        CROSS JOIN LATERAL get_task_execution_context(tc.task_uuid) ctx
        WHERE (tc.current_state = 'pending')
           OR (tc.current_state != 'pending' AND ctx.execution_status = 'has_ready_steps')
    )
    SELECT
        twc.task_uuid,
        twc.task_name,
        twc.priority,
        twc.namespace_name,
        COALESCE(twc.ready_steps, 0) as ready_steps_count,
        twc.computed_priority,
        twc.current_state
    FROM task_with_context twc
    ORDER BY twc.computed_priority DESC
    LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION get_next_ready_tasks IS
'TAS-48: Returns ready tasks with staleness filtering (1A) and exponential priority decay (1B).
Fresh tasks get age escalation, aging tasks decay exponentially, stale tasks become DLQ candidates.';
```

### Configuration

Add to `config/tasker/base/state_machine.toml`:

```toml
[state_machine.priority_decay]
# TAS-48 Solution 1B: Exponential priority decay
enabled = true

# Decay parameters
decay_start_hours = 1.0      # Start decay after this duration
decay_half_life_hours = 12.0 # Priority halves every 12 hours
stale_threshold_hours = 24.0 # After this, tasks become DLQ candidates
minimum_priority = 0.1       # Floor for stale tasks
```

### Benefits

1. **Self-Healing**: Stale tasks automatically sink in priority
2. **Fair Scheduling**: Fresh tasks always rise to top
3. **DLQ Preparation**: Identifies DLQ candidates via priority
4. **Tunable**: All parameters configurable
5. **Feature Flag**: Can be disabled independently of 1A

### Risks & Mitigation

**Risk**: Legitimate long-running workflows might be de-prioritized
**Mitigation**: 1-hour grace period before decay starts; 12-hour half-life prevents rapid de-prioritization

**Risk**: Tasks might oscillate between discoverable and non-discoverable
**Mitigation**: Monotonic decay function ensures predictable behavior

---

## Combined Effect: Solutions 1A + 1B

### Complementary Behavior

| Scenario | Solution 1A Effect | Solution 1B Effect | Combined Result |
|----------|-------------------|-------------------|-----------------|
| Fresh task (5 min) | Included in discovery | Priority 5.008 | Discovered immediately |
| Waiting task (30 min) | Included | Priority 5.05 | Normal escalation |
| Aging task (6 hours) | Included | Priority 3.03 (decaying) | Still discoverable, lower priority |
| Long-wait task (2 hours) | Excluded (>60min) | Priority 2.5 | **Not discovered** (1A wins) |
| Stale task (48 hours) | Excluded | Priority 0.1 | **Not discovered** (both agree) |

**Key Insight**: Solution 1A provides hard cutoff, Solution 1B provides soft landing. Together they ensure:
- Fresh tasks always discovered
- Aging tasks gradually de-prioritized
- Stale tasks excluded from discovery entirely
- Clear candidates for DLQ migration (TAS-49)

---

## Testing Strategy

### Unit Tests

#### Test 1: Staleness Exclusion (1A)

```sql
-- Setup: Create tasks in various states
INSERT INTO tasker_tasks (task_uuid, named_task_uuid, priority, created_at)
VALUES
  ('fresh-task', ..., 5, NOW() - INTERVAL '5 minutes'),
  ('aging-task', ..., 5, NOW() - INTERVAL '30 minutes'),
  ('stale-waiting-deps', ..., 5, NOW() - INTERVAL '90 minutes'),
  ('stale-waiting-retry', ..., 5, NOW() - INTERVAL '45 minutes');

-- Set states with created_at reflecting time in state
INSERT INTO tasker_task_transitions (task_uuid, to_state, created_at, most_recent)
VALUES
  ('fresh-task', 'pending', NOW() - INTERVAL '5 minutes', true),
  ('aging-task', 'waiting_for_dependencies', NOW() - INTERVAL '30 minutes', true),
  ('stale-waiting-deps', 'waiting_for_dependencies', NOW() - INTERVAL '90 minutes', true),
  ('stale-waiting-retry', 'waiting_for_retry', NOW() - INTERVAL '45 minutes', true);

-- Test: Only fresh and aging tasks should be discovered
SELECT COUNT(*) FROM get_next_ready_tasks(10);
-- Expected: 2 (fresh-task, aging-task)

-- Verify stale tasks excluded
SELECT task_uuid FROM get_next_ready_tasks(100);
-- Should NOT include: stale-waiting-deps, stale-waiting-retry
```

#### Test 2: Priority Decay (1B)

```sql
-- Test priority computation at various ages
WITH test_tasks AS (
  SELECT
    'fresh' as label,
    5 as base_priority,
    NOW() - INTERVAL '30 minutes' as created_at,
    NOW() - INTERVAL '30 minutes' as state_entered_at
  UNION ALL
  SELECT 'decay-start', 5, NOW() - INTERVAL '1 hour', NOW() - INTERVAL '1 hour'
  UNION ALL
  SELECT 'half-life', 5, NOW() - INTERVAL '12 hours', NOW() - INTERVAL '12 hours'
  UNION ALL
  SELECT 'stale', 5, NOW() - INTERVAL '48 hours', NOW() - INTERVAL '48 hours'
)
SELECT
  label,
  CASE
    WHEN EXTRACT(EPOCH FROM (NOW() - state_entered_at)) < 3600 THEN
      base_priority + (EXTRACT(EPOCH FROM (NOW() - created_at)) / 3600 * 0.1)
    WHEN EXTRACT(EPOCH FROM (NOW() - state_entered_at)) < (24 * 3600) THEN
      base_priority * EXP(-1 * EXTRACT(EPOCH FROM (NOW() - state_entered_at)) / (12 * 3600))
    ELSE 0.1
  END as computed_priority
FROM test_tasks;

-- Expected results:
-- fresh: ~5.05
-- decay-start: ~5.1 (peak)
-- half-life: ~1.84
-- stale: 0.1
```

### Integration Tests

#### Test 3: End-to-End Discovery with Stale Tasks

```rust
#[tokio::test]
async fn test_stale_tasks_do_not_block_fresh_tasks() {
    let pool = setup_test_db().await;

    // Create 100 stale tasks with high priority
    for i in 0..100 {
        create_stale_task(&pool, i, priority: 10).await;
    }

    // Create 1 fresh task with low priority
    let fresh_task = create_task(&pool, "fresh", priority: 2).await;

    // Discovery should find the fresh task despite lower priority
    let ready_tasks = get_next_ready_tasks(&pool, 10).await?;

    assert!(ready_tasks.iter().any(|t| t.task_uuid == fresh_task.task_uuid));
    assert_eq!(ready_tasks.iter().filter(|t| is_stale(t)).count(), 0);
}
```

#### Test 4: Priority Ordering Respects Decay

```rust
#[tokio::test]
async fn test_priority_ordering_with_decay() {
    let pool = setup_test_db().await;

    // Create tasks at different ages with same base priority
    let fresh = create_task_with_age(&pool, "fresh", priority: 5, age_minutes: 5).await;
    let aging = create_task_with_age(&pool, "aging", priority: 5, age_hours: 6).await;
    let stale = create_task_with_age(&pool, "stale", priority: 5, age_hours: 48).await;

    let ready_tasks = get_next_ready_tasks(&pool, 10).await?;

    // Fresh should be first
    assert_eq!(ready_tasks[0].task_uuid, fresh.task_uuid);

    // Stale should not be included
    assert!(!ready_tasks.iter().any(|t| t.task_uuid == stale.task_uuid));
}
```

### Regression Tests

Ensure existing behavior preserved:

```rust
#[tokio::test]
async fn test_normal_priority_ordering_unchanged() {
    // Tasks that are NOT stale should still follow priority ordering
    let pool = setup_test_db().await;

    let high = create_task(&pool, "high", priority: 10).await;
    let low = create_task(&pool, "low", priority: 2).await;

    let ready_tasks = get_next_ready_tasks(&pool, 10).await?;

    assert_eq!(ready_tasks[0].task_uuid, high.task_uuid);
    assert_eq!(ready_tasks[1].task_uuid, low.task_uuid);
}
```

---

## Success Metrics

### Pre-Implementation Baseline
- Task discovery rate: X tasks/sec (with stale task blocking)
- Average discovery latency: Y ms
- Stale task count: 4,459 (from investigation)
- Stale task ratio: Z%

### Post-Implementation Targets
1. **Discovery Rate**: No impact on fresh tasks (within 5% of baseline)
2. **Stale Task Exclusion**: >95% of stale tasks excluded from discovery pool
3. **Priority Ordering**: Fresh tasks with priority 2 discoverable within 1 second even with 10,000 stale tasks at priority 10
4. **Decay Effectiveness**: Tasks >24 hours old have computed_priority < 1.0
5. **Zero False Positives**: No legitimate tasks incorrectly excluded

### Monitoring

Add metrics to `tasker-shared/src/metrics/orchestration.rs`:

```rust
pub struct TaskDiscoveryMetrics {
    pub tasks_discovered_total: Counter,
    pub tasks_excluded_staleness: Counter,
    pub tasks_excluded_decay: Counter,
    pub computed_priority_histogram: Histogram,
    pub task_age_at_discovery_histogram: Histogram,
    pub discovery_pool_saturation: Gauge,
}
```

### Alerts

- Alert if `tasks_excluded_staleness` > 1000 (indicates staleness detection needed - see TAS-49)
- Alert if `discovery_pool_saturation` > 0.8 (may need to increase multiplier)
- Alert if fresh tasks (age < 5 min) take >10 seconds to be discovered

---

## Implementation Plan

### Phase 1: Solution 1A (Day 1)
1. Create migration `20251010000000_add_staleness_exclusion.sql`
2. Add configuration to `config/tasker/base/state_machine.toml`
3. Write unit tests for exclusion logic
4. Deploy to staging, verify metrics
5. Deploy to production with monitoring

### Phase 2: Solution 1B (Day 2)
1. Create migration `20251010000001_add_priority_decay.sql`
2. Extend configuration with decay parameters
3. Write unit tests for decay math
4. Write integration tests for combined 1A+1B behavior
5. Deploy to staging with A/B testing
6. Deploy to production with gradual rollout

### Phase 3: Monitoring & Tuning (Ongoing)
1. Monitor discovery metrics for 1 week
2. Tune decay parameters based on observed behavior
3. Document learnings for TAS-49 (comprehensive DLQ)

---

## Configuration Reference

### Complete Configuration

```toml
# config/tasker/base/state_machine.toml

[state_machine.discovery]
# TAS-48 Solution 1A: Staleness-aware discovery filtering
enable_stale_task_exclusion = true

# Exclude tasks from discovery if stuck in waiting states beyond threshold
max_waiting_for_dependencies_discovery_minutes = 60
max_waiting_for_retry_discovery_minutes = 30

# Discovery pool optimization
discovery_pool_multiplier = 10
enable_adaptive_pooling = false  # Future enhancement
min_pool_multiplier = 5
max_pool_multiplier = 20

[state_machine.priority_decay]
# TAS-48 Solution 1B: Exponential priority decay
enabled = true

# Decay parameters
decay_start_hours = 1.0
decay_half_life_hours = 12.0
stale_threshold_hours = 24.0
minimum_priority = 0.1
```

### Environment Overrides

```toml
# config/tasker/environments/test/state_machine.toml
[state_machine.discovery]
max_waiting_for_dependencies_discovery_minutes = 5
max_waiting_for_retry_discovery_minutes = 2

[state_machine.priority_decay]
decay_start_hours = 0.1      # 6 minutes
decay_half_life_hours = 0.5  # 30 minutes
stale_threshold_hours = 1.0  # 1 hour
```

---

## Dependencies & Risks

### Dependencies
- **TAS-29**: Must be merged first (discovery issue found during TAS-29 testing)
- **Database**: PostgreSQL 12+ for EXP() function support
- **Configuration System**: TAS-34 component-based configuration

### Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Legitimate tasks excluded by staleness threshold | Medium | High | Conservative 60-minute default; per-environment tuning |
| Priority decay too aggressive | Low | Medium | 1-hour grace period; 12-hour half-life; feature flag |
| Performance impact of decay calculation | Low | Low | EXP() is O(1); tested on 10k+ tasks |
| Configuration not loaded correctly | Low | High | Default values in SQL; validation tests |

### Rollback Plan

Both solutions have feature flags:
1. Set `enable_stale_task_exclusion = false` to disable 1A
2. Set `priority_decay.enabled = false` to disable 1B
3. Redeploy orchestration service (no migration rollback needed)

---

## Future Work (TAS-49)

This ticket provides immediate relief, but TAS-49 will add:
1. **Automatic staleness detection** with state transitions to DLQ
2. **DLQ table** for manual review and requeue workflows
3. **Table partitioning** for long-term growth management
4. **Archival strategy** for completed tasks
5. **Per-template staleness configuration**

Tasks excluded by TAS-48 become **DLQ candidates** that TAS-49 will automatically transition and manage.

---

## References

- **Investigation**: TAS-29 Phase 6 test failure diagnosis
- **Current Implementation**: `migrations/20250912000000_tas41_richer_task_states.sql` lines 231-297
- **Related Tickets**: TAS-49 (comprehensive DLQ), TAS-41 (state machine), TAS-34 (configuration)
- **Configuration System**: Component-based TOML configuration with environment overrides
