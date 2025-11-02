//! # SQL Function Integration
//!
//! High-performance SQL function execution for workflow orchestration.
//!
//! ## Overview
//!
//! This module provides a unified interface for executing PostgreSQL functions that
//! contain critical workflow orchestration logic. It serves as the bridge between
//! Rust application code and sophisticated SQL domain logic.
//!
//! ## SQL Function Categories
//!
//! The module supports several categories of SQL functions:
//!
//! ### 1. Step Readiness Analysis
//! - `get_step_readiness_status(task_uuid, step_uuids[])`: Complex dependency analysis
//! - `calculate_backoff_delay(attempts, base_delay)`: Exponential backoff calculation
//! - `check_step_dependencies(step_uuid)`: Parent completion validation
//!
//! ### 2. DAG Operations
//! - `detect_cycle(from_step_uuid, to_step_uuid)`: Cycle detection with recursive CTEs
//! - `calculate_step_depth(step_uuid)`: Topological depth calculation
//! - `find_ready_steps(task_uuid)`: Parallel execution candidate discovery
//!
//! ### 3. State Management
//! - `transition_task_state(task_uuid, from_state, to_state)`: Atomic state transitions
//! - `finalize_task_completion(task_uuid)`: Task completion orchestration
//!
//! ## Performance Benefits
//!
//! SQL functions provide significant performance advantages:
//! - **Set-based Operations**: Process multiple records in single operations
//! - **Reduced Network Round Trips**: Execute complex logic at database level
//! - **Atomic Transactions**: Ensure consistency without application-level locking
//! - **PostgreSQL Optimization**: Leverage query planner and indexes
//!
//! ## Rails Heritage
//!
//! Migrated from `lib/tasker/functions.rb` (351B) and scattered Rails models.
//! Consolidates all SQL function calls into a unified, type-safe interface.
//!
//! ## Usage Pattern
//!
//! ```rust,no_run
//! use tasker_shared::database::sql_functions::SqlFunctionExecutor;
//! use sqlx::PgPool;
//! use uuid::Uuid;
//!
//! # async fn example(pool: PgPool, task_uuid: Uuid) -> Result<(), sqlx::Error> {
//! let executor = SqlFunctionExecutor::new(pool);
//! let ready_steps = executor.get_ready_steps(task_uuid).await?;
//! for step in ready_steps {
//!     println!("Step {} is ready for execution", step.workflow_step_uuid);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! For complete examples with test data setup, see `tests/database/sql_functions.rs`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, types::Uuid, FromRow, PgPool};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::state_machine::TaskState;

// Import the orchestration models for transitive dependencies
use crate::messaging::StepExecutionResult;
use crate::models::orchestration::StepTransitiveDependencies;

/// Core SQL function executor with type-safe async execution.
///
/// Provides a unified interface for executing PostgreSQL functions that contain
/// critical workflow orchestration logic. All functions are executed with proper
/// parameter binding and result type mapping.
///
/// # Design Principles
///
/// - **Type Safety**: All function calls use sqlx compile-time validation
/// - **Parameter Binding**: Prevents SQL injection through proper parameter binding
/// - **Error Handling**: Comprehensive error propagation from database layer
/// - **Performance**: Optimized for high-throughput workflow operations
///
/// # Connection Pooling
///
/// Uses SQLx connection pooling for optimal performance:
/// - Reuses connections across function calls
/// - Handles connection failures gracefully
/// - Supports concurrent execution from multiple threads
#[derive(Clone)]
pub struct SqlFunctionExecutor {
    pool: PgPool,
}

crate::debug_with_pgpool!(SqlFunctionExecutor { pool: PgPool });

impl SqlFunctionExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get access to the underlying database pool for monitoring
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Execute a SQL function returning a single result with compile-time validation.
    ///
    /// This method provides type-safe execution of SQL functions that return at most
    /// one row. It uses SQLx's compile-time query validation to ensure type safety.
    ///
    /// # Type Safety
    ///
    /// The generic type `T` must implement `FromRow` to enable automatic mapping
    /// from PostgreSQL result rows to Rust structs. SQLx validates the mapping
    /// at compile time when possible.
    ///
    /// # Error Handling
    ///
    /// Returns:
    /// - `Ok(Some(T))`: Function returned exactly one row
    /// - `Ok(None)`: Function returned no rows (common for conditional functions)
    /// - `Err(sqlx::Error)`: Database error, connection failure, or type mismatch
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_shared::database::sql_functions::{SqlFunctionExecutor, StepReadinessStatus};
    /// use sqlx::PgPool;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let executor = SqlFunctionExecutor::new(pool);
    /// let result: Option<StepReadinessStatus> = executor
    ///     .execute_single("SELECT * FROM get_step_readiness_status($1)")
    ///     .await?;
    ///
    /// if let Some(status) = result {
    ///     println!("Step {} readiness: {}", status.workflow_step_uuid, status.ready_for_execution);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For complete examples with test data setup, see `tests/database/sql_functions.rs`.
    pub async fn execute_single<T>(&self, sql: &str) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql).fetch_optional(&self.pool).await
    }

    /// Execute a SQL function returning multiple results
    pub async fn execute_many<T>(&self, sql: &str) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql).fetch_all(&self.pool).await
    }

    /// Execute a SQL function with task_uuid parameter
    pub async fn execute_single_with_task_uuid<T>(
        &self,
        sql: &str,
        task_uuid: Uuid,
    ) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_uuid)
            .fetch_optional(&self.pool)
            .await
    }

    /// Execute a SQL function with task_uuid parameter returning multiple results
    pub async fn execute_many_with_task_uuid<T>(
        &self,
        sql: &str,
        task_uuid: Uuid,
    ) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_uuid)
            .fetch_all(&self.pool)
            .await
    }

    /// Execute a SQL function with task_uuids array parameter
    pub async fn execute_many_with_task_uuids<T>(
        &self,
        sql: &str,
        task_uuids: &[Uuid],
    ) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_uuids)
            .fetch_all(&self.pool)
            .await
    }
}

// ============================================================================
// 1. DEPENDENCY LEVELS
// ============================================================================

/// Result structure for calculate_dependency_levels function
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DependencyLevel {
    pub workflow_step_uuid: Uuid,
    pub dependency_level: i32,
}

impl DependencyLevel {
    /// Create a new dependency level
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::database::sql_functions::DependencyLevel;
    /// use uuid::Uuid;
    ///
    /// let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let level = DependencyLevel::new(uuid, 2);
    /// assert_eq!(level.workflow_step_uuid, uuid);
    /// assert_eq!(level.dependency_level, 2);
    /// ```
    pub fn new(workflow_step_uuid: Uuid, dependency_level: i32) -> Self {
        Self {
            workflow_step_uuid,
            dependency_level,
        }
    }

    /// Check if this is a root level (no dependencies)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::database::sql_functions::DependencyLevel;
    /// use uuid::Uuid;
    ///
    /// let uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    ///
    /// let root_level = DependencyLevel::new(uuid1, 0);
    /// assert!(root_level.is_root_level());
    ///
    /// let dependent_level = DependencyLevel::new(uuid2, 2);
    /// assert!(!dependent_level.is_root_level());
    /// ```
    pub fn is_root_level(&self) -> bool {
        self.dependency_level == 0
    }

    /// Check if this level can run in parallel with another level
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_shared::database::sql_functions::DependencyLevel;
    /// use uuid::Uuid;
    ///
    /// let uuid1 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
    /// let uuid2 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
    /// let uuid3 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap();
    ///
    /// let level1 = DependencyLevel::new(uuid1, 2);
    /// let level2 = DependencyLevel::new(uuid2, 2);
    /// let level3 = DependencyLevel::new(uuid3, 3);
    ///
    /// assert!(level1.can_run_parallel_with(&level2));
    /// assert!(!level1.can_run_parallel_with(&level3));
    /// ```
    pub fn can_run_parallel_with(&self, other: &DependencyLevel) -> bool {
        self.dependency_level == other.dependency_level
    }
}

impl SqlFunctionExecutor {
    /// Calculate dependency levels for DAG analysis
    /// Equivalent to Rails: FunctionBasedDependencyLevels.for_task
    pub async fn calculate_dependency_levels(
        &self,
        task_uuid: Uuid,
    ) -> Result<Vec<DependencyLevel>, sqlx::Error> {
        let sql = "SELECT * FROM calculate_dependency_levels($1::UUID)";
        self.execute_many_with_task_uuid(sql, task_uuid).await
    }

    /// Get dependency levels as a hash map for efficient lookup
    pub async fn dependency_levels_hash(
        &self,
        task_uuid: Uuid,
    ) -> Result<HashMap<Uuid, i32>, sqlx::Error> {
        let levels = self.calculate_dependency_levels(task_uuid).await?;
        Ok(levels
            .into_iter()
            .map(|level| (level.workflow_step_uuid, level.dependency_level))
            .collect())
    }
}

// ============================================================================
// 2. ANALYTICS METRICS
// ============================================================================

/// Comprehensive analytics metrics
/// Equivalent to Rails: FunctionBasedAnalyticsMetrics
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

impl Default for AnalyticsMetrics {
    fn default() -> Self {
        Self {
            active_tasks_count: 0i64,
            total_namespaces_count: 0i64,
            unique_task_types_count: 0i64,
            system_health_score: BigDecimal::from(0),
            task_throughput: 0i64,
            completion_count: 0i64,
            error_count: 0i64,
            completion_rate: BigDecimal::from(0),
            error_rate: BigDecimal::from(0),
            avg_task_duration: BigDecimal::from(0),
            avg_step_duration: BigDecimal::from(0),
            step_throughput: 0i64,
            analysis_period_start: Utc::now(),
            calculated_at: Utc::now(),
        }
    }
}

impl SqlFunctionExecutor {
    /// Get comprehensive analytics metrics
    /// Equivalent to Rails: FunctionBasedAnalyticsMetrics.for_period
    pub async fn get_analytics_metrics(
        &self,
        since_timestamp: Option<DateTime<Utc>>,
    ) -> Result<AnalyticsMetrics, sqlx::Error> {
        let sql = if since_timestamp.is_some() {
            "SELECT * FROM get_analytics_metrics($1)"
        } else {
            "SELECT * FROM get_analytics_metrics()"
        };

        let result = if let Some(ts) = since_timestamp {
            sqlx::query_as::<_, AnalyticsMetrics>(sql)
                .bind(ts)
                .fetch_optional(&self.pool)
                .await?
        } else {
            self.execute_single(sql).await?
        };

        Ok(result.unwrap_or_default())
    }
}

// ============================================================================
// 3. STEP READINESS STATUS
// ============================================================================

/// Comprehensive step readiness analysis
/// Equivalent to Rails: FunctionBasedStepReadinessStatus
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct StepReadinessStatus {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub named_step_uuid: Uuid,
    pub name: String,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_failure_at: Option<chrono::NaiveDateTime>,
    pub next_retry_at: Option<chrono::NaiveDateTime>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<chrono::NaiveDateTime>,
}

impl StepReadinessStatus {
    /// Check if step can execute right now
    pub fn can_execute_now(&self) -> bool {
        self.ready_for_execution
    }

    /// Get the reason why step cannot execute (if any)
    pub fn blocking_reason(&self) -> Option<&'static str> {
        if self.ready_for_execution {
            return None;
        }
        if !self.dependencies_satisfied {
            return Some("dependencies_not_satisfied");
        }
        if !self.retry_eligible {
            return Some("retry_not_eligible");
        }
        if !["pending", "error"].contains(&self.current_state.as_str()) {
            return Some("invalid_state");
        }
        Some("unknown")
    }

    /// Calculate effective backoff seconds considering attempts
    pub fn effective_backoff_seconds(&self) -> i32 {
        if let Some(backoff) = self.backoff_request_seconds {
            backoff
        } else if self.attempts > 0 {
            // Exponential backoff: 2^attempts, max 30
            std::cmp::min(2_i32.pow(self.attempts as u32), 30)
        } else {
            0
        }
    }
}

impl SqlFunctionExecutor {
    /// Get step readiness status for a task
    /// Equivalent to Rails: FunctionBasedStepReadinessStatus.for_task
    pub async fn get_step_readiness_status(
        &self,
        task_uuid: Uuid,
        step_uuids: Option<Vec<Uuid>>,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let sql = if step_uuids.is_some() {
            "SELECT * FROM get_step_readiness_status($1::UUID, $2::UUID[])"
        } else {
            "SELECT * FROM get_step_readiness_status($1::UUID)"
        };

        if let Some(uuids) = step_uuids {
            sqlx::query_as::<_, StepReadinessStatus>(sql)
                .bind(task_uuid)
                .bind(&uuids)
                .fetch_all(&self.pool)
                .await
        } else {
            self.execute_many_with_task_uuid(sql, task_uuid).await
        }
    }

    /// Get step readiness status for multiple tasks (batch operation)
    /// Equivalent to Rails: FunctionBasedStepReadinessStatus.for_tasks_batch
    pub async fn get_step_readiness_status_batch(
        &self,
        task_uuids: Vec<Uuid>,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let sql = "SELECT * FROM get_step_readiness_status_batch($1::uuid[])";
        self.execute_many_with_task_uuids(sql, &task_uuids).await
    }

    /// Get only ready steps for execution
    pub async fn get_ready_steps(
        &self,
        task_uuid: Uuid,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let all_steps = self.get_step_readiness_status(task_uuid, None).await?;
        Ok(all_steps
            .into_iter()
            .filter(|step| step.ready_for_execution)
            .collect())
    }
}

// ============================================================================
// 4. SYSTEM HEALTH COUNTS
// ============================================================================

/// System-wide health and performance counts
/// Equivalent to Rails: FunctionBasedSystemHealthCounts
///
/// **IMPORTANT**: Field names match the SQL function `get_system_health_counts()` output exactly.
/// The SQL function returns detailed task state columns, not an aggregated `in_progress_tasks`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
pub struct SystemHealthCounts {
    // Task counts by state - matches SQL function output
    pub pending_tasks: i64,
    pub initializing_tasks: i64,
    pub enqueuing_steps_tasks: i64,
    pub steps_in_process_tasks: i64,
    pub evaluating_results_tasks: i64,
    pub waiting_for_dependencies_tasks: i64,
    pub waiting_for_retry_tasks: i64,
    pub blocked_by_failures_tasks: i64,
    pub complete_tasks: i64,
    pub error_tasks: i64,
    pub cancelled_tasks: i64,
    pub resolved_manually_tasks: i64,
    pub total_tasks: i64,

    // Step counts by state - matches SQL function output
    pub pending_steps: i64,
    pub enqueued_steps: i64,
    pub in_progress_steps: i64,
    pub enqueued_for_orchestration_steps: i64,
    pub enqueued_as_error_for_orchestration_steps: i64,
    pub waiting_for_retry_steps: i64,
    pub complete_steps: i64,
    pub error_steps: i64,
    pub cancelled_steps: i64,
    pub resolved_manually_steps: i64,
    pub total_steps: i64,
}

impl SystemHealthCounts {
    /// Calculate error rate (0.0 to 1.0)
    pub fn error_rate(&self) -> f64 {
        if self.total_tasks > 0 {
            self.error_tasks as f64 / self.total_tasks as f64
        } else {
            0.0
        }
    }

    /// Calculate success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_tasks > 0 {
            self.complete_tasks as f64 / self.total_tasks as f64
        } else {
            0.0
        }
    }
}

impl SqlFunctionExecutor {
    /// Get system health counts
    /// Equivalent to Rails: FunctionBasedSystemHealthCounts.current
    pub async fn get_system_health_counts(&self) -> Result<SystemHealthCounts, sqlx::Error> {
        let sql = "SELECT * FROM get_system_health_counts()";
        let result = self.execute_single(sql).await?;
        Ok(result.unwrap_or_default())
    }
}

// ============================================================================
// 5. TASK EXECUTION CONTEXT (UNIFIED VERSION)
// ============================================================================

// Use unified TaskExecutionContext from models::orchestration
// This replaces the duplicate definition that existed here
pub use crate::models::orchestration::TaskExecutionContext;

impl SqlFunctionExecutor {
    /// Get task execution context (DEPRECATED - use TaskExecutionContext::get_for_task)
    pub async fn get_task_execution_context(
        &self,
        task_uuid: Uuid,
    ) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        TaskExecutionContext::get_for_task(&self.pool, task_uuid).await
    }

    /// Get task execution contexts for multiple tasks (DEPRECATED - use TaskExecutionContext::get_for_tasks)
    pub async fn get_task_execution_contexts_batch(
        &self,
        task_uuids: Vec<Uuid>,
    ) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        TaskExecutionContext::get_for_tasks(&self.pool, &task_uuids).await
    }
}

// ============================================================================
// 6. PERFORMANCE ANALYTICS
// ============================================================================

/// Slowest steps analysis for performance optimization
/// Equivalent to Rails: FunctionBasedSlowestSteps
/// Returns individual step executions sorted by duration
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestStepAnalysis {
    pub workflow_step_uuid: Uuid,
    pub task_uuid: Uuid,
    pub step_name: String,
    pub task_name: String,
    pub namespace_name: String,
    pub version: String,
    pub duration_seconds: BigDecimal,
    pub attempts: i32,
    pub created_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub retryable: bool,
    pub step_status: String,
}

/// Slowest tasks analysis for performance optimization
/// Equivalent to Rails: FunctionBasedSlowestTasks
/// Returns individual task executions sorted by duration
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestTaskAnalysis {
    pub task_uuid: Uuid,
    pub task_name: String,
    pub namespace_name: String,
    pub version: String,
    pub duration_seconds: BigDecimal,
    pub step_count: i64,
    pub completed_steps: i64,
    pub error_steps: i64,
    pub created_at: chrono::NaiveDateTime,
    pub completed_at: Option<chrono::NaiveDateTime>,
    pub initiator: String,
    pub source_system: String,
}

impl SqlFunctionExecutor {
    /// Get slowest steps analysis
    /// Equivalent to Rails: FunctionBasedSlowestSteps.analyze
    pub async fn get_slowest_steps(
        &self,
        limit: Option<i32>,
        _min_executions: Option<i32>,
    ) -> Result<Vec<SlowestStepAnalysis>, sqlx::Error> {
        // Database function signature: get_slowest_steps(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)
        // We don't use min_executions parameter as it doesn't exist in the database function
        let sql =
            "SELECT * FROM get_slowest_steps(NULL::timestamp with time zone, $1, NULL, NULL, NULL)";
        sqlx::query_as::<_, SlowestStepAnalysis>(sql)
            .bind(limit.unwrap_or(10))
            .fetch_all(&self.pool)
            .await
    }

    /// Get slowest tasks analysis
    /// Equivalent to Rails: FunctionBasedSlowestTasks.analyze
    pub async fn get_slowest_tasks(
        &self,
        limit: Option<i32>,
        _min_executions: Option<i32>,
    ) -> Result<Vec<SlowestTaskAnalysis>, sqlx::Error> {
        // Database function signature: get_slowest_tasks(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)
        // We don't use min_executions parameter as it doesn't exist in the database function
        let sql =
            "SELECT * FROM get_slowest_tasks(NULL::timestamp with time zone, $1, NULL, NULL, NULL)";
        sqlx::query_as::<_, SlowestTaskAnalysis>(sql)
            .bind(limit.unwrap_or(10))
            .fetch_all(&self.pool)
            .await
    }
}

// ============================================================================
// REGISTRY PATTERN FOR FUNCTION ACCESS
// ============================================================================

/// Central registry for all SQL function operations
/// Provides organized access to all function-based operations
#[derive(Debug)]
pub struct FunctionRegistry {
    executor: SqlFunctionExecutor,
}

impl FunctionRegistry {
    pub fn new(pool: PgPool) -> Self {
        Self {
            executor: SqlFunctionExecutor::new(pool),
        }
    }

    /// Access dependency level operations
    pub fn dependency_levels(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access analytics operations
    pub fn analytics(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access step readiness operations
    pub fn step_readiness(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access system health operations
    pub fn system_health(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access task execution context operations
    pub fn task_execution(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access performance analytics operations
    pub fn performance(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Get the underlying executor for custom operations
    pub fn executor(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access optimized worker query operations
    pub fn worker_optimization(&self) -> &SqlFunctionExecutor {
        &self.executor
    }

    /// Access step transitive dependencies operations
    pub fn transitive_dependencies(&self) -> &SqlFunctionExecutor {
        &self.executor
    }
}

// ============================================================================
// 7. STEP TRANSITIVE DEPENDENCIES
// ============================================================================

impl SqlFunctionExecutor {
    /// Get all transitive dependencies (ancestors) for a step using recursive SQL function
    ///
    /// This function calls the `get_step_transitive_dependencies` SQL function which uses
    /// recursive CTEs to find all ancestor steps in the dependency graph.
    ///
    /// # Parameters
    ///
    /// - `step_uuid`: The workflow step ID to find dependencies for
    ///
    /// # Returns
    ///
    /// Vector of `StepTransitiveDependencies` ordered by distance (direct parents first)
    ///
    /// # Usage
    ///
    /// ```rust,no_run
    /// use tasker_shared::database::sql_functions::SqlFunctionExecutor;
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// # async fn example(pool: PgPool, step_uuid: Uuid) -> Result<(), sqlx::Error> {
    /// let executor = SqlFunctionExecutor::new(pool);
    /// let dependencies = executor.get_step_transitive_dependencies(step_uuid).await?;
    ///
    /// for dep in dependencies {
    ///     println!("Step {} depends on {} (distance: {})",
    ///              step_uuid, dep.step_name, dep.distance);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Equivalent to Rails: Enhanced dependency resolution with full DAG traversal
    pub async fn get_step_transitive_dependencies(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let sql = "SELECT * FROM get_step_transitive_dependencies($1::UUID)";
        sqlx::query_as::<_, StepTransitiveDependencies>(sql)
            .bind(step_uuid)
            .fetch_all(&self.pool)
            .await
    }

    /// Get transitive dependencies as a results map for step handler consumption
    ///
    /// This method fetches all transitive dependencies and converts them into a map
    /// of step_name -> results, which matches the pattern used by Ruby step handlers
    /// with `sequence.get_results('step_name')`.
    ///
    /// # Parameters
    ///
    /// - `step_uuid`: The workflow step ID to find dependencies for
    ///
    /// # Returns
    ///
    /// HashMap mapping step names to their JSON results for completed dependencies
    ///
    /// # Usage
    ///
    /// ```rust,no_run
    /// use tasker_shared::database::sql_functions::SqlFunctionExecutor;
    /// use sqlx::PgPool;
    /// use uuid::Uuid;
    ///
    /// # async fn example(pool: PgPool, step_uuid: Uuid) -> Result<(), sqlx::Error> {
    /// let executor = SqlFunctionExecutor::new(pool);
    /// let results = executor.get_step_dependency_results_map(step_uuid).await?;
    ///
    /// if let Some(validation_result) = results.get("validate_order") {
    ///     println!("Validation result: {:?}", validation_result);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_step_dependency_results_map(
        &self,
        step_uuid: Uuid,
    ) -> Result<HashMap<String, StepExecutionResult>, sqlx::Error> {
        let dependencies = self.get_step_transitive_dependencies(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter_map(|dep| {
                // Only include steps that are processed AND have non-null results
                if dep.processed && dep.results.is_some() {
                    let json_results = dep.results.unwrap();
                    let results: StepExecutionResult = json_results.into();
                    Some((dep.step_name, results))
                } else {
                    None
                }
            })
            .collect())
    }

    /// Get only completed transitive dependencies (those with results)
    ///
    /// This filters the transitive dependencies to only include steps that have
    /// been processed and have results available.
    pub async fn get_completed_step_dependencies(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let dependencies = self.get_step_transitive_dependencies(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter(|dep| dep.processed && dep.results.is_some())
            .collect())
    }

    /// Get only direct parent dependencies (distance = 1)
    ///
    /// This returns only the immediate parent steps, equivalent to the existing
    /// immediate dependency queries but using the transitive function for consistency.
    pub async fn get_direct_parent_dependencies(
        &self,
        step_uuid: Uuid,
    ) -> Result<Vec<StepTransitiveDependencies>, sqlx::Error> {
        let dependencies = self.get_step_transitive_dependencies(step_uuid).await?;
        Ok(dependencies
            .into_iter()
            .filter(|dep| dep.distance == 1)
            .collect())
    }
}

// ============================================================================
// 8. TAS-41 QUERY METHODS
// ============================================================================

/// Task info for batch processing with current state
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

impl SqlFunctionExecutor {
    // Get next ready task with type-safe result
    pub async fn get_task_ready_info(
        &self,
        task_uuid: Uuid,
    ) -> Result<Option<ReadyTaskInfo>, sqlx::Error> {
        sqlx::query_as!(
            ReadyTaskInfo,
            r#"
            SELECT
                task_uuid as "task_uuid!",
                task_name as "task_name!",
                priority as "priority!",
                namespace_name as "namespace_name!",
                ready_steps_count as "ready_steps_count!",
                computed_priority as "computed_priority!",
                current_state as "current_state!"
            FROM get_task_ready_info($1::UUID)
            "#,
            task_uuid
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Get next ready task with type-safe result
    pub async fn get_next_ready_task(&self) -> Result<Option<ReadyTaskInfo>, sqlx::Error> {
        sqlx::query_as!(
            ReadyTaskInfo,
            r#"
            SELECT
                task_uuid as "task_uuid!",
                task_name as "task_name!",
                priority as "priority!",
                namespace_name as "namespace_name!",
                ready_steps_count as "ready_steps_count!",
                computed_priority as "computed_priority!",
                current_state as "current_state!"
            FROM get_next_ready_task()
            "#
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Get batch of ready tasks with their current states
    pub async fn get_next_ready_tasks(
        &self,
        limit: i32,
    ) -> Result<Vec<ReadyTaskInfo>, sqlx::Error> {
        sqlx::query_as!(
            ReadyTaskInfo,
            r#"
            SELECT
                task_uuid as "task_uuid!",
                task_name as "task_name!",
                priority as "priority!",
                namespace_name as "namespace_name!",
                ready_steps_count as "ready_steps_count!",
                computed_priority as "computed_priority!",
                current_state as "current_state!"
            FROM get_next_ready_tasks($1)
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Get current state of a task
    pub async fn get_current_task_state(&self, task_uuid: Uuid) -> Result<TaskState, sqlx::Error> {
        let state_str =
            sqlx::query_scalar!(r#"SELECT get_current_task_state($1) as "state""#, task_uuid)
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

// ============================================================================
// 9. DLQ (DEAD LETTER QUEUE) OPERATIONS (TAS-49)
// ============================================================================

/// Stale task record from discovery function
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StaleTaskRecord {
    pub task_uuid: Uuid,
    pub namespace_name: String,
    pub task_name: String,
    pub current_state: String,
    pub time_in_state_minutes: BigDecimal,
    pub threshold_minutes: i32,
    pub task_age_minutes: BigDecimal,
}

/// Result from detect_and_transition_stale_tasks function
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StalenessDetectionResult {
    pub task_uuid: Uuid,
    pub namespace_name: String,
    pub task_name: String,
    pub current_state: String,
    pub time_in_state_minutes: i32,
    pub staleness_threshold_minutes: i32,
    pub action_taken: String,
    pub moved_to_dlq: bool,
    pub transition_success: bool,
}

impl SqlFunctionExecutor {
    /// Discover stale tasks exceeding staleness thresholds
    ///
    /// This function queries the `get_stale_tasks_for_dlq()` SQL function which uses
    /// the `v_task_state_analysis` base view for O(1) expensive join optimization.
    ///
    /// # Parameters
    ///
    /// - `default_waiting_deps`: Default threshold for waiting_for_dependencies (minutes)
    /// - `default_waiting_retry`: Default threshold for waiting_for_retry (minutes)
    /// - `default_steps_process`: Default threshold for steps_in_process (minutes)
    /// - `max_lifetime_hours`: Maximum task lifetime regardless of state (hours)
    /// - `batch_size`: Maximum number of stale tasks to return
    ///
    /// # Returns
    ///
    /// Vector of `StaleTaskRecord` with task details and staleness information
    ///
    /// # TAS-49 Phase 2 Refactoring
    ///
    /// This discovery function enables O(1) optimization - expensive multi-table joins
    /// happen ONCE in the base view query, not O(n) times inside a loop.
    pub async fn get_stale_tasks_for_dlq(
        &self,
        default_waiting_deps: i32,
        default_waiting_retry: i32,
        default_steps_process: i32,
        max_lifetime_hours: i32,
        batch_size: i32,
    ) -> Result<Vec<StaleTaskRecord>, sqlx::Error> {
        sqlx::query_as::<_, StaleTaskRecord>(
            r#"
            SELECT *
            FROM get_stale_tasks_for_dlq($1, $2, $3, $4, $5)
            "#,
        )
        .bind(default_waiting_deps)
        .bind(default_waiting_retry)
        .bind(default_steps_process)
        .bind(max_lifetime_hours)
        .bind(batch_size)
        .fetch_all(&self.pool)
        .await
    }

    /// Detect and transition stale tasks to DLQ
    ///
    /// This is the main staleness detection function that:
    /// 1. Discovers stale tasks using `get_stale_tasks_for_dlq()`
    /// 2. Creates DLQ entries using `create_dlq_entry()`
    /// 3. Transitions tasks to Error state using `transition_stale_task_to_error()`
    ///
    /// # Parameters
    ///
    /// - `dry_run`: If true, only reports what would happen without making changes
    /// - `batch_size`: Maximum tasks to process per run
    /// - `default_waiting_deps_threshold`: Default for waiting_for_dependencies (minutes)
    /// - `default_waiting_retry_threshold`: Default for waiting_for_retry (minutes)
    /// - `default_steps_in_process_threshold`: Default for steps_in_process (minutes)
    /// - `default_task_max_lifetime_hours`: Maximum task lifetime (hours)
    ///
    /// # Returns
    ///
    /// Vector of `StalenessDetectionResult` showing what happened to each detected task
    ///
    /// # TAS-49 Phase 2 Refactoring
    ///
    /// Refactored from 220 lines to 87 lines using helper functions:
    /// - `get_stale_tasks_for_dlq()` for discovery (O(1) optimization)
    /// - `create_dlq_entry()` for DLQ entry creation
    /// - `transition_stale_task_to_error()` for state transitions
    ///
    /// # Usage
    ///
    /// ```rust,no_run
    /// use tasker_shared::database::sql_functions::SqlFunctionExecutor;
    /// use sqlx::PgPool;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let executor = SqlFunctionExecutor::new(pool);
    ///
    /// // Dry run (safe, no changes)
    /// let results = executor.detect_and_transition_stale_tasks(
    ///     true,  // dry_run
    ///     10,    // batch_size
    ///     60,    // waiting_deps
    ///     30,    // waiting_retry
    ///     30,    // steps_process
    ///     24     // max_lifetime_hours
    /// ).await?;
    ///
    /// for result in results {
    ///     println!("Task {} would be moved to DLQ", result.task_uuid);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn detect_and_transition_stale_tasks(
        &self,
        dry_run: bool,
        batch_size: i32,
        default_waiting_deps_threshold: i32,
        default_waiting_retry_threshold: i32,
        default_steps_in_process_threshold: i32,
        default_task_max_lifetime_hours: i32,
    ) -> Result<Vec<StalenessDetectionResult>, sqlx::Error> {
        sqlx::query_as::<_, StalenessDetectionResult>(
            r#"
            SELECT *
            FROM detect_and_transition_stale_tasks($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(dry_run)
        .bind(batch_size)
        .bind(default_waiting_deps_threshold)
        .bind(default_waiting_retry_threshold)
        .bind(default_steps_in_process_threshold)
        .bind(default_task_max_lifetime_hours)
        .fetch_all(&self.pool)
        .await
    }

    /// Calculate staleness threshold for a task state
    ///
    /// Helper function that determines the appropriate staleness threshold by checking
    /// template configuration first, then falling back to provided defaults.
    ///
    /// # Parameters
    ///
    /// - `task_state`: Current task state (e.g., 'waiting_for_dependencies')
    /// - `template_config`: JSONB template configuration
    /// - `default_waiting_deps`: Default for waiting_for_dependencies (minutes)
    /// - `default_waiting_retry`: Default for waiting_for_retry (minutes)
    /// - `default_steps_process`: Default for steps_in_process (minutes)
    ///
    /// # Returns
    ///
    /// Threshold in minutes (i32)
    pub async fn calculate_staleness_threshold(
        &self,
        task_state: &str,
        template_config: serde_json::Value,
        default_waiting_deps: i32,
        default_waiting_retry: i32,
        default_steps_process: i32,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar::<_, i32>(
            r#"
            SELECT calculate_staleness_threshold($1, $2, $3, $4, $5)
            "#,
        )
        .bind(task_state)
        .bind(template_config)
        .bind(default_waiting_deps)
        .bind(default_waiting_retry)
        .bind(default_steps_process)
        .fetch_one(&self.pool)
        .await
    }

    /// Create DLQ entry for a stale task
    ///
    /// Helper function that encapsulates DLQ entry creation with comprehensive snapshot.
    ///
    /// # Parameters
    ///
    /// - `task_uuid`: Task being moved to DLQ
    /// - `namespace_name`: Namespace for context
    /// - `task_name`: Template name for context
    /// - `current_state`: State when detected as stale
    /// - `time_in_state_minutes`: How long in state
    /// - `threshold_minutes`: Threshold that was exceeded
    /// - `dlq_reason`: Why entering DLQ (default: 'staleness_timeout')
    ///
    /// # Returns
    ///
    /// UUID of created DLQ entry, or None if creation failed
    pub async fn create_dlq_entry(
        &self,
        task_uuid: Uuid,
        namespace_name: &str,
        task_name: &str,
        current_state: &str,
        time_in_state_minutes: i32,
        threshold_minutes: i32,
        dlq_reason: Option<&str>,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Option<Uuid>>(
            r#"
            SELECT create_dlq_entry($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(task_uuid)
        .bind(namespace_name)
        .bind(task_name)
        .bind(current_state)
        .bind(time_in_state_minutes)
        .bind(threshold_minutes)
        .bind(dlq_reason.unwrap_or("staleness_timeout"))
        .fetch_one(&self.pool)
        .await
    }

    /// Transition stale task to error state
    ///
    /// Helper function that handles state transition with staleness-specific context.
    ///
    /// # Parameters
    ///
    /// - `task_uuid`: Task to transition
    /// - `current_state`: Current state for validation
    /// - `namespace_name`: Namespace for audit trail
    /// - `task_name`: Template name for audit trail
    ///
    /// # Returns
    ///
    /// Boolean indicating whether transition succeeded
    pub async fn transition_stale_task_to_error(
        &self,
        task_uuid: Uuid,
        current_state: &str,
        namespace_name: &str,
        task_name: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT transition_stale_task_to_error($1, $2, $3, $4)
            "#,
        )
        .bind(task_uuid)
        .bind(current_state)
        .bind(namespace_name)
        .bind(task_name)
        .fetch_one(&self.pool)
        .await
    }
}
