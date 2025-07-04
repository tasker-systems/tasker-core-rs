# Rustdoc Guide for Tasker Core

This guide establishes documentation standards for the Tasker Core Rust project, with special attention to SQL-heavy domain logic.

## 📚 Documentation Philosophy

Since much of our domain logic lives in SQL functions and SQLx queries, our documentation must:
1. **Explain the Rust API** - Standard rustdoc for public interfaces
2. **Document SQL Logic** - Detailed explanations of embedded SQL
3. **Connect the Layers** - Show how Rust and SQL work together
4. **Preserve Rails Context** - Reference original Rails implementations

## 🎯 Documentation Standards

### Module-Level Documentation

Every module should have comprehensive documentation explaining its purpose and SQL dependencies:

```rust
//! # Step Readiness Status
//! 
//! This module manages the readiness state of workflow steps for execution.
//! 
//! ## Overview
//! 
//! The `StepReadinessStatus` model serves as both a cache and authoritative source
//! for step readiness calculations. It works in conjunction with the PostgreSQL
//! function `get_step_readiness_status()` for high-performance dependency resolution.
//! 
//! ## SQL Dependencies
//! 
//! This module relies on the following SQL function:
//! 
//! ### `get_step_readiness_status(task_id, step_ids[])`
//! 
//! A complex PostgreSQL function that calculates step readiness by:
//! - Analyzing parent-child relationships in the DAG
//! - Checking completion status of dependencies
//! - Evaluating retry eligibility based on attempts and limits
//! - Determining if backoff periods have expired
//! 
//! The function returns a composite result including:
//! - `dependencies_satisfied`: All parent steps are complete
//! - `retry_eligible`: Failed step can be retried (attempts < limit)
//! - `ready_for_execution`: Step can be immediately executed
//! - `next_retry_at`: When a failed step can be retried (backoff calculation)
//! 
//! ## Rails Heritage
//! 
//! Migrated from `app/models/tasker/step_readiness_status.rb` (2.1KB)
//! Preserves all ActiveRecord scopes and business logic.
```

### Function Documentation

For functions that execute SQL, document both the Rust interface and SQL logic:

```rust
/// Bulk update readiness statuses for all steps in a task.
/// 
/// This method leverages the PostgreSQL function `get_step_readiness_status()` to
/// perform high-performance dependency analysis at the database level.
/// 
/// # SQL Function Details
/// 
/// The underlying SQL function performs:
/// 
/// 1. **Parent Analysis** - Joins with `workflow_step_edges` to count dependencies
/// 2. **Completion Check** - Filters parents by `processed = true` status
/// 3. **Retry Logic** - Evaluates `attempts < retry_limit AND retryable = true`
/// 4. **State Determination** - Complex CASE statements for execution readiness
/// 
/// ```sql
/// WITH step_parent_analysis AS (
///     SELECT ws.workflow_step_id,
///            COUNT(parent_ws.*) as total_parents,
///            COUNT(*) FILTER (WHERE parent_ws.processed = true) as completed_parents,
///            -- Retry eligibility calculation
///            CASE WHEN ws.attempts < ws.retry_limit AND ws.retryable THEN true ELSE false END
///     FROM tasker_workflow_steps ws
///     LEFT JOIN tasker_workflow_step_edges wse ON wse.to_step_id = ws.workflow_step_id
///     LEFT JOIN tasker_workflow_steps parent_ws ON parent_ws.workflow_step_id = wse.from_step_id
///     WHERE ws.task_id = $1
///     GROUP BY ws.workflow_step_id
/// )
/// ```
/// 
/// # Performance
/// 
/// This approach is 10-100x faster than application-level dependency resolution
/// due to:
/// - Set-based operations instead of N+1 queries
/// - PostgreSQL's optimized join algorithms
/// - Elimination of network round trips
/// 
/// # Example
/// 
/// ```rust
/// let readiness_results = StepReadinessStatus::bulk_update_for_task(&pool, task_id).await?;
/// for result in readiness_results {
///     println!("Step {} ready: {}", result.workflow_step_id, result.ready_for_execution);
/// }
/// ```
pub async fn bulk_update_for_task(pool: &PgPool, task_id: i64) -> Result<Vec<StepReadinessResult>, sqlx::Error>
```

### Complex Query Documentation

For complex SQLx queries, break down the logic:

```rust
/// Find all sibling steps (steps that share exactly the same parent dependencies).
/// 
/// # SQL Logic
/// 
/// This query uses a sophisticated CTE-based approach:
/// 
/// 1. **Parent Aggregation**: Groups all parent IDs into JSONB arrays
/// 2. **Self-Join**: Finds steps with identical parent arrays
/// 3. **Exclusion**: Filters out the step comparing with itself
/// 
/// The query structure:
/// ```sql
/// WITH step_parents AS (
///     -- Aggregate parent IDs for each step
///     SELECT to_step_id, 
///            jsonb_agg(from_step_id ORDER BY from_step_id) as parent_ids
///     FROM workflow_step_edges
///     GROUP BY to_step_id
/// )
/// SELECT DISTINCT s2.workflow_step_id
/// FROM step_parents sp1
/// JOIN step_parents sp2 ON sp1.parent_ids = sp2.parent_ids
/// WHERE sp1.to_step_id = $1 AND sp2.to_step_id != $1
/// ```
/// 
/// # Business Logic
/// 
/// Sibling detection is crucial for:
/// - Parallel execution optimization (siblings can run concurrently)
/// - Dependency validation (siblings should have compatible requirements)
/// - Workflow visualization (grouping related steps)
```

### Type Documentation

For types that map to SQL results, document the mapping:

```rust
/// Result from the `get_step_readiness_status()` SQL function.
/// 
/// Maps directly to the PostgreSQL composite type returned by the function.
/// Each field corresponds to a calculated value from the dependency analysis.
/// 
/// # SQL Function Return Values
/// 
/// - `workflow_step_id`: Step being analyzed
/// - `dependencies_satisfied`: `COUNT(completed_parents) = COUNT(total_parents)`
/// - `retry_eligible`: `attempts < retry_limit AND retryable = true`
/// - `ready_for_execution`: `dependencies_satisfied AND (pending OR retry_eligible)`
/// - `next_retry_at`: Calculated using exponential backoff algorithm
/// - `backoff_request_seconds`: From `calculate_backoff()` SQL function
/// 
/// # Null Handling
/// 
/// All fields are `Option<T>` because the SQL function may return partial results
/// for steps that don't exist or have been deleted.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepReadinessResult {
    pub workflow_step_id: Option<i64>,
    pub task_id: Option<i64>,
    // ... etc
}
```

## 🔍 Special Documentation Cases

### 1. Recursive CTEs

Always explain the recursion logic:

```rust
/// Detects if adding an edge would create a cycle in the DAG.
/// 
/// # SQL Algorithm
/// 
/// Uses a recursive CTE to traverse the graph:
/// 
/// 1. **Base Case**: Start from the proposed destination step
/// 2. **Recursive Case**: Follow all outgoing edges
/// 3. **Termination**: Stop if we reach the source step (cycle!) or exhaust paths
/// 
/// The recursion is protected by a depth limit to prevent infinite loops
/// in case of existing cycles (data corruption).
```

### 2. JSONB Operations

Document the JSON structure and operations:

```rust
/// Updates step metadata using JSONB merge operations.
/// 
/// # JSONB Structure
/// 
/// The metadata field expects:
/// ```json
/// {
///   "retry_count": 0,
///   "last_error": null,
///   "custom_attributes": {},
///   "performance_metrics": {
///     "execution_time_ms": null,
///     "queue_time_ms": null
///   }
/// }
/// ```
/// 
/// # SQL Merge Logic
/// 
/// Uses PostgreSQL's `||` operator for shallow merge:
/// ```sql
/// UPDATE workflow_steps 
/// SET metadata = metadata || $2
/// WHERE workflow_step_id = $1
/// ```
```

### 3. Performance-Critical Paths

Always note performance characteristics:

```rust
/// High-performance batch update using PostgreSQL's UPDATE...FROM pattern.
/// 
/// # Performance Characteristics
/// 
/// - **O(n)** complexity instead of O(n²) for application-level updates
/// - Single round trip vs N round trips
/// - Leverages PostgreSQL's MVCC for consistency
/// - Typical performance: 10,000 steps updated in <100ms
```

## 📏 Documentation Checklist

For each module/type/function, ensure:

- [ ] Standard rustdoc summary line
- [ ] Detailed description of purpose
- [ ] SQL function/query documentation if applicable
- [ ] Performance characteristics noted
- [ ] Examples provided for complex usage
- [ ] Rails heritage referenced where relevant
- [ ] Edge cases and error conditions documented
- [ ] Links to related SQL functions/views

## 🚀 Generating Documentation

```bash
# Generate and open documentation
cargo doc --open

# Generate documentation with private items
cargo doc --document-private-items

# Generate and test documentation examples
cargo test --doc
```

## 🎓 Benefits

This comprehensive documentation approach:
1. **Preserves Domain Knowledge** - SQL logic isn't hidden
2. **Aids Debugging** - Clear understanding of data flow
3. **Enables Optimization** - Performance characteristics visible
4. **Supports Maintenance** - Future developers understand the full stack
5. **Facilitates Testing** - Clear contracts for test cases