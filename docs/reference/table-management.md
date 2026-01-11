# Table Management and Growth Strategies

**Last Updated**: 2026-01-10
**Status**: Active Recommendation

## Problem Statement

In high-throughput workflow orchestration systems, the core task tables (`tasks`, `workflow_steps`, `task_transitions`, `workflow_step_transitions`) can grow to millions of rows over time. Without proper management, this growth can lead to:

> **Note**: As of TAS-128, all tables reside in the `tasker` schema with simplified names (e.g., `tasks` instead of `tasker_tasks`). With `search_path = tasker, public`, queries use unqualified table names.

- **Query Performance Degradation**: Even with proper indexes, very large tables require more I/O operations
- **Maintenance Overhead**: VACUUM, ANALYZE, and index maintenance become increasingly expensive
- **Backup/Recovery Challenges**: Larger tables increase backup windows and recovery times
- **Storage Costs**: Historical data that's rarely accessed still consumes storage resources

## Existing Performance Mitigations

The tasker-core system employs several strategies to maintain query performance even with large tables:

### 1. Strategic Indexing

#### Covering Indexes for Hot Paths

The most critical indexes use PostgreSQL's `INCLUDE` clause to create covering indexes that satisfy queries without table lookups:

**Active Task Processing** (`migrations/tasker/20250810140000_uuid_v7_initial_schema.sql`):
```sql
-- Covering index for active task queries with priority sorting
CREATE INDEX IF NOT EXISTS idx_tasks_active_with_priority_covering
    ON tasks (complete, priority, task_uuid)
    INCLUDE (named_task_uuid, requested_at)
    WHERE complete = false;
```
*Impact*: Task discovery queries can be satisfied entirely from the index without accessing the main table.

**Step Readiness Processing** (`migrations/tasker/20250810140000_uuid_v7_initial_schema.sql`):
```sql
-- Covering index for step readiness queries
CREATE INDEX IF NOT EXISTS idx_workflow_steps_ready_covering
    ON workflow_steps (task_uuid, processed, in_process)
    INCLUDE (workflow_step_uuid, attempts, max_attempts, retryable)
    WHERE processed = false;

-- Covering index for task-based step grouping
CREATE INDEX IF NOT EXISTS idx_workflow_steps_task_covering
    ON workflow_steps (task_uuid)
    INCLUDE (workflow_step_uuid, processed, in_process, attempts, max_attempts);
```
*Impact*: Step dependency resolution and retry logic queries avoid heap lookups.

**Transitive Dependency Optimization** (`migrations/tasker/20250810140000_uuid_v7_initial_schema.sql`):
```sql
-- Covering index for transitive dependency traversal
CREATE INDEX IF NOT EXISTS idx_workflow_steps_transitive_deps
    ON workflow_steps (workflow_step_uuid, named_step_uuid)
    INCLUDE (task_uuid, results, processed);
```
*Impact*: DAG traversal operations can read all needed columns from the index.

#### State Transition Lookups (Partial Indexes)

**Current State Resolution** (`migrations/tasker/20250810140000_uuid_v7_initial_schema.sql`):
```sql
-- Fast current state resolution (only indexes most_recent = true)
CREATE INDEX IF NOT EXISTS idx_task_transitions_state_lookup
    ON task_transitions (task_uuid, to_state, most_recent)
    WHERE most_recent = true;

CREATE INDEX IF NOT EXISTS idx_workflow_step_transitions_state_lookup
    ON workflow_step_transitions (workflow_step_uuid, to_state, most_recent)
    WHERE most_recent = true;
```
*Impact*: State lookups index only current state, not full audit history. Reduces index size by >90%.

#### Correlation and Tracing Indexes (TAS-41)

**Distributed Tracing Support** (`migrations/tasker/20251007000000_add_correlation_ids.sql`):
```sql
-- Primary correlation ID lookups
CREATE INDEX IF NOT EXISTS idx_tasks_correlation_id
    ON tasks(correlation_id);

-- Hierarchical workflow traversal (parent-child relationships)
CREATE INDEX IF NOT EXISTS idx_tasks_correlation_hierarchy
    ON tasks(parent_correlation_id, correlation_id)
    WHERE parent_correlation_id IS NOT NULL;
```
*Impact*: Enables efficient distributed tracing and workflow hierarchy queries.

#### Processor Ownership and Monitoring (TAS-41)

**Processor Tracking** (`migrations/tasker/20250912000000_tas41_richer_task_states.sql`):
```sql
-- Index for processor ownership queries (audit trail only after TAS-54)
CREATE INDEX IF NOT EXISTS idx_task_transitions_processor
    ON task_transitions(processor_uuid)
    WHERE processor_uuid IS NOT NULL;

-- Index for timeout monitoring using JSONB metadata
CREATE INDEX IF NOT EXISTS idx_task_transitions_timeout
    ON task_transitions((transition_metadata->>'timeout_at'))
    WHERE most_recent = true;
```
*Impact*: Enables processor-level debugging and timeout monitoring. Processor ownership enforcement removed in TAS-54 but audit trail preserved.

#### Dependency Graph Navigation

**Step Edges for DAG Operations** (`migrations/tasker/20250810140000_uuid_v7_initial_schema.sql`):
```sql
-- Parent-to-child navigation for dependency resolution
CREATE INDEX IF NOT EXISTS idx_workflow_step_edges_from_step
    ON workflow_step_edges (from_step_uuid);

-- Child-to-parent navigation for completion propagation
CREATE INDEX IF NOT EXISTS idx_workflow_step_edges_to_step
    ON workflow_step_edges (to_step_uuid);
```
*Impact*: Bidirectional DAG traversal for readiness checks and completion propagation.

### 2. Partial Indexes

Many indexes use `WHERE` clauses to index only active/relevant rows:

```sql
-- Only index tasks that are actively being processed
WHERE current_state IN ('pending', 'initializing', 'steps_in_process')

-- Only index the current state transition
WHERE most_recent = true
```

This significantly reduces index size and maintenance overhead while keeping lookups fast.

### 3. SQL Function Optimizations

Complex orchestration queries are implemented as PostgreSQL functions that leverage:

- **Lateral Joins**: For efficient correlated subqueries
- **CTEs with Materialization**: For complex dependency analysis
- **Targeted Filtering**: Early elimination of irrelevant rows using index scans

Example from `get_next_ready_tasks()`:
```sql
-- First filter to active tasks with priority sorting (uses index)
WITH prioritized_tasks AS (
  SELECT task_uuid, priority
  FROM tasks
  WHERE current_state IN ('pending', 'steps_in_process')
  ORDER BY priority DESC, created_at ASC
  LIMIT $1 * 2  -- Get more candidates than needed for filtering
)
-- Then apply complex staleness/readiness checks only on candidates
...
```

### 4. Staleness Exclusion (TAS-48)

The system automatically excludes stale tasks from active processing queues:

- Tasks stuck in `waiting_for_dependencies` > 60 minutes
- Tasks stuck in `waiting_for_retry` > 30 minutes
- Tasks with lifecycle timeouts exceeded

This prevents the active query set from growing indefinitely, even if old tasks aren't archived.

## Archive-and-Delete Strategy (Considered, Not Implemented)

### What We Considered

During TAS-49 implementation, we initially designed an archive-and-delete strategy:

**Architecture**:
- Mirror tables: `tasker.archived_tasks`, `tasker.archived_workflow_steps`, `tasker.archived_task_transitions`, `tasker.archived_workflow_step_transitions`
- Background service running every 24 hours
- Batch processing: 1000 tasks per run
- Transactional archival: INSERT into archive tables → DELETE from main tables
- Retention policies: Configurable per task state (completed, error, cancelled)

**Implementation Details**:
```rust
// Archive tasks in terminal states older than retention period
pub async fn archive_completed_tasks(
    pool: &PgPool,
    retention_days: i32,
    batch_size: i32,
) -> Result<ArchiveStats> {
    // 1. INSERT INTO archived_tasks SELECT * FROM tasks WHERE ...
    // 2. INSERT INTO archived_workflow_steps SELECT * WHERE task_uuid IN (...)
    // 3. INSERT INTO archived_task_transitions SELECT * WHERE task_uuid IN (...)
    // 4. DELETE FROM workflow_step_transitions WHERE ...
    // 5. DELETE FROM task_transitions WHERE ...
    // 6. DELETE FROM workflow_steps WHERE ...
    // 7. DELETE FROM tasks WHERE ...
}
```

### Why We Decided Against It

After implementation and analysis, we identified critical performance issues:

#### 1. Write Amplification

Every archived task results in:
- **2× writes per row**: INSERT into archive table + original row still exists until DELETE
- **1× delete per row**: DELETE from main table triggers index updates
- **Cascade costs**: Foreign key relationships require multiple DELETE operations in sequence

For a system processing 100,000 tasks/day with 30-day retention:
- Daily archival: ~100,000 tasks × 2 write operations = 200,000 write I/Os
- Plus associated workflow_steps (typically 5-10 per task): 500,000-1,000,000 additional writes

#### 2. Index Maintenance Overhead

PostgreSQL must maintain indexes during both INSERT and DELETE operations:

**During INSERT to archive tables**:
- Build index entries for all archive table indexes
- Update statistics for query planner

**During DELETE from main tables**:
- Mark deleted tuples in main table indexes
- Update free space maps
- Trigger VACUUM requirements

**Result**: Periodic severe degradation (2-5 seconds) during archival runs, even with batch processing.

#### 3. Lock Contention

Large DELETE operations require:
- **Row-level locks** on deleted rows
- **Table-level locks** during index updates
- **Lock escalation risk** with large batch sizes

This creates a "stop-the-world" effect where active task processing is blocked during archival.

#### 4. VACUUM Pressure

Frequent large DELETEs create dead tuples that require aggressive VACUUMing:
- Increases I/O load during off-hours
- Can't be fully eliminated even with proper tuning
- Competes with active workload for resources

#### 5. The "Garbage Collector" Anti-Pattern

The archive-and-delete strategy essentially implements a manual garbage collector:
- Periodic runs with performance impact
- Tuning trade-offs (frequency vs. batch size vs. impact)
- Operational complexity (monitoring, alerting, recovery)

## Recommended Solution: PostgreSQL Native Partitioning

### Overview

PostgreSQL's native table partitioning with `pg_partman` provides zero-runtime-cost table management:

**Key Advantages**:
- **No write amplification**: Data stays in place, partitions are logical divisions
- **No DELETE operations**: Old partitions are DETACHed and dropped as units
- **Instant partition drops**: Dropping a partition is O(1), not O(rows)
- **Transparent to application**: Queries work identically on partitioned tables
- **Battle-tested**: Used by pgmq (our queue infrastructure) and thousands of production systems

### How It Works

```sql
-- 1. Create partitioned parent table (in tasker schema)
CREATE TABLE tasker.tasks (
    task_uuid UUID NOT NULL,
    created_at TIMESTAMP NOT NULL,
    -- ... other columns
) PARTITION BY RANGE (created_at);

-- 2. pg_partman automatically creates child partitions
-- tasker.tasks_p2025_01  (Jan 2025)
-- tasker.tasks_p2025_02  (Feb 2025)
-- tasker.tasks_p2025_03  (Mar 2025)
-- ... etc

-- 3. Queries transparently use appropriate partitions
SELECT * FROM tasks WHERE task_uuid = $1;
-- → PostgreSQL automatically queries correct partition

-- 4. Dropping old partitions is instant
ALTER TABLE tasker.tasks DETACH PARTITION tasker.tasks_p2024_12;
DROP TABLE tasker.tasks_p2024_12;  -- Instant, no row-by-row deletion
```

### Performance Characteristics

| Operation | Archive-and-Delete | Native Partitioning |
|-----------|-------------------|---------------------|
| Write path | INSERT + DELETE (2× I/O) | INSERT only (1× I/O) |
| Index maintenance | On INSERT + DELETE | On INSERT only |
| Lock contention | Row locks during DELETE | No locks for drops |
| VACUUM pressure | High (dead tuples) | None (partition drops) |
| Old data removal | O(rows) per deletion | O(1) partition detach |
| Query performance | Scans entire table | Partition pruning |
| Runtime impact | Periodic degradation | Zero |

### Implementation with pg_partman

#### Installation

```sql
CREATE EXTENSION pg_partman;
```

#### Setup for tasks

```sql
-- 1. Create partitioned table structure
-- (Include all existing columns and indexes)

-- 2. Initialize pg_partman for monthly partitions
SELECT partman.create_parent(
    p_parent_table := 'tasker.tasks',
    p_control := 'created_at',
    p_type := 'native',
    p_interval := 'monthly',
    p_premake := 3  -- Pre-create 3 future months
);

-- 3. Configure retention (keep 90 days)
UPDATE partman.part_config
SET retention = '90 days',
    retention_keep_table = false  -- Drop old partitions entirely
WHERE parent_table = 'tasker.tasks';

-- 4. Enable automatic maintenance
SELECT partman.run_maintenance(p_parent_table := 'tasker.tasks');
```

#### Automation

Add to cron or pg_cron:

```sql
-- Run maintenance every hour
SELECT cron.schedule('partman-maintenance', '0 * * * *',
    $$SELECT partman.run_maintenance()$$
);
```

This automatically:
- Creates new partitions before they're needed
- Detaches and drops partitions older than retention period
- Updates partition constraints for query optimization

### Real-World Example: pgmq

The pgmq message queue system (which tasker-core uses for orchestration) implements partitioned queues for high-throughput scenarios:

**Reference**: [pgmq Partitioned Queues](https://github.com/pgmq/pgmq?tab=readme-ov-file#partitioned-queues)

**pgmq's Rationale** (from their docs):
> "For very high-throughput queues, you may want to partition the queue table by time. This allows you to drop old partitions instead of deleting rows, which is much faster and doesn't cause table bloat."

**pgmq's Approach**:
```sql
-- pgmq uses pg_partman for message queues
SELECT pgmq.create_partitioned(
    queue_name := 'high_throughput_queue',
    partition_interval := '1 day',
    retention_interval := '7 days'
);
```

**Benefits They Report**:
- **10× faster** old message cleanup vs. DELETE
- **Zero bloat** from message deletion
- **Consistent performance** even at millions of messages per day

**Applying to Tasker**:
Our use case is nearly identical to pgmq:
- High-throughput append-heavy workload
- Time-series data (created_at is natural partition key)
- Need to retain recent data, drop old data
- Performance-critical read path

If pgmq chose partitioning over archive-and-delete for these reasons, we should too.

## Migration Path

### Phase 1: Analysis (Current State)

Before implementing partitioning:

1. **Analyze Current Growth Rate**:
```sql
SELECT
    pg_size_pretty(pg_total_relation_size('tasker.tasks')) as total_size,
    count(*) as row_count,
    min(created_at) as oldest_task,
    max(created_at) as newest_task,
    count(*) / EXTRACT(day FROM (max(created_at) - min(created_at))) as avg_tasks_per_day
FROM tasks;
```

2. **Determine Partition Strategy**:
   - Daily partitions: For > 1M tasks/day
   - Weekly partitions: For 100K-1M tasks/day
   - Monthly partitions: For < 100K tasks/day

3. **Plan Retention Period**:
   - Legal/compliance requirements
   - Analytics/reporting needs
   - Typical task investigation window

### Phase 2: Implementation

1. **Create Partitioned Tables** (requires downtime or blue-green deployment)
2. **Migrate Existing Data** using `pg_partman.partition_data_proc()`
3. **Update Application** (no code changes needed if using same table names)
4. **Configure Automation** (pg_cron for maintenance)

### Phase 3: Monitoring

Track partition management effectiveness:

```sql
-- Check partition sizes
SELECT
    schemaname || '.' || tablename as partition_name,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
FROM pg_tables
WHERE schemaname = 'tasker' AND tablename LIKE 'tasks_p%'
ORDER BY tablename;

-- Verify partition pruning is working
EXPLAIN SELECT * FROM tasks
WHERE created_at > NOW() - INTERVAL '7 days';
-- Should show: "Seq Scan on tasker.tasks_p2025_11" (only current partition)
```

## Decision Summary

**Decision**: Use PostgreSQL native partitioning with pg_partman for table growth management.

**Rationale**:
- Zero runtime performance impact vs. periodic degradation with archive-and-delete
- Operationally simpler (set-and-forget vs. monitoring archive jobs)
- Battle-tested solution used by pgmq and thousands of production systems
- Aligns with PostgreSQL best practices and community recommendations

**Not Recommended**: Archive-and-delete strategy due to write amplification, lock contention, and periodic performance degradation.

## References

- [PostgreSQL Partitioning Documentation](https://www.postgresql.org/docs/current/ddl-partitioning.html)
- [pg_partman Extension](https://github.com/pgpartman/pg_partman)
- [pgmq Partitioned Queues](https://github.com/pgmq/pgmq?tab=readme-ov-file#partitioned-queues)
- [TAS-49](https://linear.app/tasker-systems/issue/TAS-49)

## See Also

- [States and Lifecycles](./states-and-lifecycles.md) - Task and step state management
- [Task and Step Readiness](./task-and-step-readiness-and-execution.md) - SQL function optimizations
- [Observability README](./observability/README.md) - Monitoring table growth and query performance
