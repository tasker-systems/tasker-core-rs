# Checkpoint Operations Guide

**Last Updated**: 2026-01-06
**Status**: Active (TAS-125)
**Related**: [Batch Processing - Checkpoint Yielding](../guides/batch-processing.md#checkpoint-yielding-tas-125)

---

## Overview

This guide covers operational concerns for checkpoint yielding in production environments, including monitoring, troubleshooting, and maintenance tasks.

---

## Monitoring Checkpoints

### Key Metrics

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| Checkpoint history size | Length of `history` array | >100 entries |
| Checkpoint age | Time since last checkpoint | >10 minutes (step-dependent) |
| Accumulated results size | Size of `accumulated_results` JSONB | >1MB |
| Checkpoint frequency | Checkpoints per step execution | <1 per minute (may indicate issues) |

### SQL Queries for Monitoring

**Steps with large checkpoint history:**

```sql
SELECT
    ws.workflow_step_uuid,
    ws.name,
    t.task_uuid,
    jsonb_array_length(ws.checkpoint->'history') as history_length,
    ws.checkpoint->>'timestamp' as last_checkpoint
FROM tasker.workflow_steps ws
JOIN tasker.tasks t ON ws.task_uuid = t.task_uuid
WHERE ws.checkpoint IS NOT NULL
  AND jsonb_array_length(ws.checkpoint->'history') > 50
ORDER BY history_length DESC
LIMIT 20;
```

**Steps with stale checkpoints (in progress but not checkpointed recently):**

```sql
SELECT
    ws.workflow_step_uuid,
    ws.name,
    ws.current_state,
    ws.checkpoint->>'timestamp' as last_checkpoint,
    NOW() - (ws.checkpoint->>'timestamp')::timestamptz as checkpoint_age
FROM tasker.workflow_steps ws
WHERE ws.current_state = 'in_progress'
  AND ws.checkpoint IS NOT NULL
  AND (ws.checkpoint->>'timestamp')::timestamptz < NOW() - INTERVAL '10 minutes'
ORDER BY checkpoint_age DESC;
```

**Large accumulated results:**

```sql
SELECT
    ws.workflow_step_uuid,
    ws.name,
    pg_column_size(ws.checkpoint->'accumulated_results') as results_size_bytes,
    pg_size_pretty(pg_column_size(ws.checkpoint->'accumulated_results')::bigint) as results_size
FROM tasker.workflow_steps ws
WHERE ws.checkpoint->'accumulated_results' IS NOT NULL
  AND pg_column_size(ws.checkpoint->'accumulated_results') > 100000
ORDER BY results_size_bytes DESC
LIMIT 20;
```

### Logging

Checkpoint operations emit structured logs:

```
INFO checkpoint_yield_step_event step_uuid=abc-123 cursor=1000 items_processed=1000
INFO checkpoint_saved step_uuid=abc-123 history_length=5
```

**Log fields to monitor:**
- `step_uuid` - Step being checkpointed
- `cursor` - Current position
- `items_processed` - Total items at checkpoint
- `history_length` - Number of checkpoint entries

---

## Troubleshooting

### Step Not Resuming from Checkpoint

**Symptoms**: Step restarts from beginning instead of checkpoint position.

**Checks**:
1. Verify checkpoint exists:
   ```sql
   SELECT checkpoint FROM tasker.workflow_steps WHERE workflow_step_uuid = 'uuid';
   ```
2. Check handler uses `BatchWorkerContext` accessors:
   - `has_checkpoint?` / `has_checkpoint()` / `hasCheckpoint()`
   - `checkpoint_cursor` / `checkpointCursor`
3. Verify handler respects checkpoint in processing loop

### Checkpoint Not Persisting

**Symptoms**: `checkpoint_yield()` returns but data not in database.

**Checks**:
1. Check for errors in worker logs
2. Verify FFI bridge is healthy
3. Check database connectivity

### Excessive Checkpoint History Growth

**Symptoms**: Steps have hundreds or thousands of checkpoint history entries.

**Causes**:
- Very long-running processes with frequent checkpoints
- Small checkpoint intervals relative to work

**Remediation**:
1. Increase checkpoint interval (process more items between checkpoints)
2. Clear history for completed steps (see Maintenance section)
3. Monitor with history size query above

### Large Accumulated Results

**Symptoms**: Database bloat, slow step queries.

**Causes**:
- Storing full result sets instead of summaries
- Unbounded accumulation without size checks

**Remediation**:
1. Modify handler to store summaries, not full data
2. Use external storage for large intermediate results
3. Add size checks before checkpoint

---

## Maintenance Tasks

### Clear Checkpoint for Completed Steps

Completed steps retain checkpoint data for debugging. To clear:

```sql
-- Clear checkpoints for completed steps older than 7 days
UPDATE tasker.workflow_steps
SET checkpoint = NULL
WHERE current_state = 'complete'
  AND checkpoint IS NOT NULL
  AND updated_at < NOW() - INTERVAL '7 days';
```

### Truncate History Array

For steps with excessive history:

```sql
-- Keep only last 10 history entries
UPDATE tasker.workflow_steps
SET checkpoint = jsonb_set(
    checkpoint,
    '{history}',
    (SELECT jsonb_agg(elem)
     FROM (
         SELECT elem
         FROM jsonb_array_elements(checkpoint->'history') elem
         ORDER BY (elem->>'timestamp')::timestamptz DESC
         LIMIT 10
     ) sub)
)
WHERE jsonb_array_length(checkpoint->'history') > 10;
```

### Clear Checkpoint for Manual Reset

When manually resetting a step to reprocess from scratch:

```sql
-- Clear checkpoint to force reprocessing from beginning
UPDATE tasker.workflow_steps
SET checkpoint = NULL
WHERE workflow_step_uuid = 'step-uuid-here';
```

**Warning**: Only clear checkpoints if you want the step to restart from the beginning.

---

## Capacity Planning

### Database Sizing

**Checkpoint column considerations:**
- Each checkpoint: ~1-10KB typical (cursor, timestamp, metadata)
- History array: ~100 bytes per entry
- Accumulated results: Variable (handler-dependent)

**Formula for checkpoint storage:**
```
Storage = Active Steps × (Base Checkpoint Size + History Entries × 100 bytes + Accumulated Size)
```

**Example**: 10,000 active steps with 50-entry history and 5KB accumulated results:
```
10,000 × (5KB + 50 × 100B + 5KB) = 10,000 × 15KB = 150MB
```

### Performance Impact

**Checkpoint write**: ~1-5ms per checkpoint (single row UPDATE)

**Checkpoint read**: Included in step data fetch (no additional query)

**Recommendations**:
- Checkpoint every 1000-10000 items or every 1-5 minutes
- Too frequent: Excessive database writes
- Too infrequent: Lost progress on failure

---

## Alerting Recommendations

### Prometheus/Grafana Metrics

If exporting to Prometheus:

```yaml
# Alert on stale checkpoints
- alert: StaleCheckpoint
  expr: tasker_checkpoint_age_seconds > 600
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Step checkpoint is stale"

# Alert on large history
- alert: CheckpointHistoryGrowth
  expr: tasker_checkpoint_history_size > 100
  for: 1h
  labels:
    severity: warning
  annotations:
    summary: "Checkpoint history exceeding threshold"
```

### Database-Based Alerting

For periodic SQL-based monitoring:

```sql
-- Return non-zero if any issues detected
SELECT COUNT(*)
FROM tasker.workflow_steps
WHERE (
    -- Stale in-progress checkpoints
    (current_state = 'in_progress'
     AND checkpoint IS NOT NULL
     AND (checkpoint->>'timestamp')::timestamptz < NOW() - INTERVAL '10 minutes')
    OR
    -- Excessive history
    (checkpoint IS NOT NULL
     AND jsonb_array_length(checkpoint->'history') > 100)
);
```

---

## See Also

- [Batch Processing Guide](../guides/batch-processing.md) - Full checkpoint yielding documentation
- [DLQ System](../guides/dlq-system.md) - Dead letter queue for failed steps
- [Retry Semantics](../guides/retry-semantics.md) - Retry behavior with checkpoints
