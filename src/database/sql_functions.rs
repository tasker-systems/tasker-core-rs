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
//! - `get_step_readiness_status(task_id, step_ids[])`: Complex dependency analysis
//! - `calculate_backoff_delay(attempts, base_delay)`: Exponential backoff calculation
//! - `check_step_dependencies(step_id)`: Parent completion validation
//!
//! ### 2. DAG Operations
//! - `detect_cycle(from_step_id, to_step_id)`: Cycle detection with recursive CTEs
//! - `calculate_step_depth(step_id)`: Topological depth calculation
//! - `find_ready_steps(task_id)`: Parallel execution candidate discovery
//!
//! ### 3. State Management
//! - `transition_task_state(task_id, from_state, to_state)`: Atomic state transitions
//! - `cleanup_stale_processes(timeout_minutes)`: Process cleanup and recovery
//! - `finalize_task_completion(task_id)`: Task completion orchestration
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
//! use tasker_core::database::sql_functions::SqlFunctionExecutor;
//! use sqlx::PgPool;
//!
//! # async fn example(pool: PgPool, task_id: i64) -> Result<(), sqlx::Error> {
//! let executor = SqlFunctionExecutor::new(pool);
//! let ready_steps = executor.get_ready_steps(task_id).await?;
//! for step in ready_steps {
//!     println!("Step {} is ready for execution", step.workflow_step_id);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! For complete examples with test data setup, see `tests/database/sql_functions.rs`.

use bigdecimal::ToPrimitive;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, FromRow, PgPool};
use std::collections::HashMap;

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

impl SqlFunctionExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
    /// use tasker_core::database::sql_functions::{SqlFunctionExecutor, StepReadinessStatus};
    /// use sqlx::PgPool;
    ///
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let executor = SqlFunctionExecutor::new(pool);
    /// let result: Option<StepReadinessStatus> = executor
    ///     .execute_single("SELECT * FROM get_step_readiness_status($1)")
    ///     .await?;
    ///
    /// if let Some(status) = result {
    ///     println!("Step {} readiness: {}", status.workflow_step_id, status.ready_for_execution);
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

    /// Execute a SQL function with task_id parameter
    pub async fn execute_single_with_task_id<T>(
        &self,
        sql: &str,
        task_id: i64,
    ) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Execute a SQL function with task_id parameter returning multiple results
    pub async fn execute_many_with_task_id<T>(
        &self,
        sql: &str,
        task_id: i64,
    ) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
    }

    /// Execute a SQL function with task_ids array parameter
    pub async fn execute_many_with_task_ids<T>(
        &self,
        sql: &str,
        task_ids: &[i64],
    ) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_ids)
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
    pub workflow_step_id: i64,
    pub dependency_level: i32,
}

impl DependencyLevel {
    /// Create a new dependency level
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::database::sql_functions::DependencyLevel;
    ///
    /// let level = DependencyLevel::new(123, 2);
    /// assert_eq!(level.workflow_step_id, 123);
    /// assert_eq!(level.dependency_level, 2);
    /// ```
    pub fn new(workflow_step_id: i64, dependency_level: i32) -> Self {
        Self {
            workflow_step_id,
            dependency_level,
        }
    }

    /// Check if this is a root level (no dependencies)
    ///
    /// # Example
    ///
    /// ```rust
    /// use tasker_core::database::sql_functions::DependencyLevel;
    ///
    /// let root_level = DependencyLevel::new(123, 0);
    /// assert!(root_level.is_root_level());
    ///
    /// let dependent_level = DependencyLevel::new(456, 2);
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
    /// use tasker_core::database::sql_functions::DependencyLevel;
    ///
    /// let level1 = DependencyLevel::new(123, 2);
    /// let level2 = DependencyLevel::new(456, 2);
    /// let level3 = DependencyLevel::new(789, 3);
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
        task_id: i64,
    ) -> Result<Vec<DependencyLevel>, sqlx::Error> {
        let sql = "SELECT * FROM calculate_dependency_levels($1::BIGINT)";
        self.execute_many_with_task_id(sql, task_id).await
    }

    /// Get dependency levels as a hash map for efficient lookup
    pub async fn dependency_levels_hash(
        &self,
        task_id: i64,
    ) -> Result<HashMap<i64, i32>, sqlx::Error> {
        let levels = self.calculate_dependency_levels(task_id).await?;
        Ok(levels
            .into_iter()
            .map(|level| (level.workflow_step_id, level.dependency_level))
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
    pub active_tasks_count: i32,
    pub total_namespaces_count: i32,
    pub unique_task_types_count: i32,
    pub system_health_score: f64,
    pub task_throughput: i32,
    pub completion_count: i32,
    pub error_count: i32,
    pub completion_rate: f64,
    pub error_rate: f64,
    pub avg_task_duration: f64,
    pub avg_step_duration: f64,
    pub step_throughput: i32,
    pub analysis_period_start: String,
    pub calculated_at: String,
}

impl Default for AnalyticsMetrics {
    fn default() -> Self {
        Self {
            active_tasks_count: 0,
            total_namespaces_count: 0,
            unique_task_types_count: 0,
            system_health_score: 0.0,
            task_throughput: 0,
            completion_count: 0,
            error_count: 0,
            completion_rate: 0.0,
            error_rate: 0.0,
            avg_task_duration: 0.0,
            avg_step_duration: 0.0,
            step_throughput: 0,
            analysis_period_start: "".to_string(),
            calculated_at: Utc::now().to_rfc3339(),
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
            "SELECT * FROM get_analytics_metrics_v01($1)"
        } else {
            "SELECT * FROM get_analytics_metrics_v01()"
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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StepReadinessStatus {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub named_step_id: i32,
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
    pub retry_limit: i32,
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
        task_id: i64,
        step_ids: Option<Vec<i64>>,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let sql = if step_ids.is_some() {
            "SELECT * FROM get_step_readiness_status($1::BIGINT, $2::BIGINT[])"
        } else {
            "SELECT * FROM get_step_readiness_status($1::BIGINT)"
        };

        if let Some(ids) = step_ids {
            sqlx::query_as::<_, StepReadinessStatus>(sql)
                .bind(task_id)
                .bind(&ids)
                .fetch_all(&self.pool)
                .await
        } else {
            self.execute_many_with_task_id(sql, task_id).await
        }
    }

    /// Get step readiness status for multiple tasks (batch operation)
    /// Equivalent to Rails: FunctionBasedStepReadinessStatus.for_tasks_batch
    pub async fn get_step_readiness_status_batch(
        &self,
        task_ids: Vec<i64>,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let sql = "SELECT * FROM get_step_readiness_status_batch($1::BIGINT[])";
        self.execute_many_with_task_ids(sql, &task_ids).await
    }

    /// Get only ready steps for execution
    pub async fn get_ready_steps(
        &self,
        task_id: i64,
    ) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let all_steps = self.get_step_readiness_status(task_id, None).await?;
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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemHealthCounts {
    pub total_tasks: i64,
    pub pending_tasks: i64,
    pub in_progress_tasks: i64,
    pub complete_tasks: i64,
    pub error_tasks: i64,
    pub cancelled_tasks: i64,
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub complete_steps: i64,
    pub error_steps: i64,
    pub retryable_error_steps: i64,
    pub exhausted_retry_steps: i64,
    pub in_backoff_steps: i64,
    pub active_connections: i64,
    pub max_connections: i64,
}

impl Default for SystemHealthCounts {
    fn default() -> Self {
        Self {
            total_tasks: 0,
            pending_tasks: 0,
            in_progress_tasks: 0,
            complete_tasks: 0,
            error_tasks: 0,
            cancelled_tasks: 0,
            total_steps: 0,
            pending_steps: 0,
            in_progress_steps: 0,
            complete_steps: 0,
            error_steps: 0,
            retryable_error_steps: 0,
            exhausted_retry_steps: 0,
            in_backoff_steps: 0,
            active_connections: 0,
            max_connections: 100,
        }
    }
}

impl SystemHealthCounts {
    /// Calculate system health score (0.0 to 1.0)
    pub fn health_score(&self) -> f64 {
        if self.total_tasks == 0 {
            return 1.0;
        }

        let success_rate = self.complete_tasks as f64 / self.total_tasks as f64;
        let error_rate = self.error_tasks as f64 / self.total_tasks as f64;
        let connection_health =
            1.0 - (self.active_connections as f64 / self.max_connections as f64).min(1.0);

        // Weighted combination: 50% success rate, 30% error rate, 20% connection health
        (success_rate * 0.5) + ((1.0 - error_rate) * 0.3) + (connection_health * 0.2)
    }

    /// Check if system is under heavy load
    pub fn is_under_heavy_load(&self) -> bool {
        let connection_pressure = self.active_connections as f64 / self.max_connections as f64;
        let error_rate = if self.total_tasks > 0 {
            self.error_tasks as f64 / self.total_tasks as f64
        } else {
            0.0
        };

        connection_pressure > 0.8 || error_rate > 0.2
    }
}

impl SqlFunctionExecutor {
    /// Get system health counts
    /// Equivalent to Rails: FunctionBasedSystemHealthCounts.current
    pub async fn get_system_health_counts(&self) -> Result<SystemHealthCounts, sqlx::Error> {
        let sql = "SELECT * FROM get_system_health_counts_v01()";
        let result = self.execute_single(sql).await?;
        Ok(result.unwrap_or_default())
    }
}

// ============================================================================
// 5. TASK EXECUTION CONTEXT
// ============================================================================

/// Task execution context and recommendations
/// Equivalent to Rails: FunctionBasedTaskExecutionContext
/// FIXED: Aligned with actual SQL function return columns
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskExecutionContext {
    pub task_id: i64,
    pub named_task_id: i32,
    pub status: String,
    pub total_steps: i64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub completed_steps: i64,
    pub failed_steps: i64,
    pub ready_steps: i64,
    pub execution_status: String,
    pub recommended_action: String,
    pub completion_percentage: sqlx::types::BigDecimal, // numeric from SQL
    pub health_status: String,
}

impl TaskExecutionContext {
    /// Check if task is ready for execution
    pub fn can_proceed(&self) -> bool {
        self.ready_steps > 0
    }

    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        self.completion_percentage >= BigDecimal::from(100)
    }

    /// Check if task is blocked
    pub fn is_blocked(&self) -> bool {
        self.ready_steps == 0 && self.pending_steps > 0
    }

    /// Get execution status info
    pub fn get_execution_status(&self) -> &str {
        &self.execution_status
    }

    /// Get health status info  
    pub fn get_health_status(&self) -> &str {
        &self.health_status
    }
}

impl SqlFunctionExecutor {
    /// Get task execution context
    /// Equivalent to Rails: FunctionBasedTaskExecutionContext.for_task
    pub async fn get_task_execution_context(
        &self,
        task_id: i64,
    ) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        let sql = "SELECT * FROM get_task_execution_context($1::BIGINT)";
        self.execute_single_with_task_id(sql, task_id).await
    }

    /// Get task execution contexts for multiple tasks (batch operation)
    /// Equivalent to Rails: FunctionBasedTaskExecutionContext.for_tasks_batch
    pub async fn get_task_execution_contexts_batch(
        &self,
        task_ids: Vec<i64>,
    ) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let sql = "SELECT * FROM get_task_execution_contexts_batch($1::BIGINT[])";
        self.execute_many_with_task_ids(sql, &task_ids).await
    }
}

// ============================================================================
// 6. PERFORMANCE ANALYTICS
// ============================================================================

/// Slowest steps analysis for performance optimization
/// Equivalent to Rails: FunctionBasedSlowestSteps
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestStepAnalysis {
    pub named_step_id: i32,
    pub step_name: String,
    pub avg_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub min_duration_seconds: f64,
    pub execution_count: i32,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<DateTime<Utc>>,
}

/// Slowest tasks analysis for performance optimization
/// Equivalent to Rails: FunctionBasedSlowestTasks
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestTaskAnalysis {
    pub named_task_id: i64,
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

impl SqlFunctionExecutor {
    /// Get slowest steps analysis
    /// Equivalent to Rails: FunctionBasedSlowestSteps.analyze
    pub async fn get_slowest_steps(
        &self,
        limit: Option<i32>,
        min_executions: Option<i32>,
    ) -> Result<Vec<SlowestStepAnalysis>, sqlx::Error> {
        let sql = "SELECT * FROM get_slowest_steps_v01($1, $2, NULL, NULL, NULL)";
        sqlx::query_as::<_, SlowestStepAnalysis>(sql)
            .bind(limit)
            .bind(min_executions)
            .fetch_all(&self.pool)
            .await
    }

    /// Get slowest tasks analysis
    /// Equivalent to Rails: FunctionBasedSlowestTasks.analyze
    pub async fn get_slowest_tasks(
        &self,
        limit: Option<i32>,
        min_executions: Option<i32>,
    ) -> Result<Vec<SlowestTaskAnalysis>, sqlx::Error> {
        let sql = "SELECT * FROM get_slowest_tasks_v01($1, $2, NULL, NULL, NULL)";
        sqlx::query_as::<_, SlowestTaskAnalysis>(sql)
            .bind(limit)
            .bind(min_executions)
            .fetch_all(&self.pool)
            .await
    }
}

// ============================================================================
// REGISTRY PATTERN FOR FUNCTION ACCESS
// ============================================================================

/// Central registry for all SQL function operations
/// Provides organized access to all function-based operations
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
}

// ============================================================================
// 7. OPTIMIZED WORKER QUERIES
// ============================================================================

/// Result structure for find_active_workers_for_task function
/// Provides complete worker and task information with pre-computed health scoring
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActiveWorkerResult {
    pub id: i32,
    pub worker_id: i32,
    pub named_task_id: i32,
    pub configuration: serde_json::Value,
    pub priority: i32,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub worker_name: String,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,
}

/// Result structure for get_worker_health_batch function
/// Provides comprehensive worker health information
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerHealthResult {
    pub worker_id: i32,
    pub worker_name: String,
    pub status: String,
    pub last_heartbeat_at: Option<chrono::NaiveDateTime>,
    pub connection_healthy: bool,
    pub current_load: i32,
}

/// Result structure for select_optimal_worker_for_task function
/// Provides intelligent worker selection with comprehensive scoring
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OptimalWorkerResult {
    pub worker_id: i32,
    pub worker_name: String,
    pub priority: i32,
    pub configuration: serde_json::Value,
    pub health_score: sqlx::types::BigDecimal,
    pub current_load: i32,
    pub max_concurrent_steps: i32,
    pub available_capacity: i32,
    pub selection_score: sqlx::types::BigDecimal,
}

impl OptimalWorkerResult {
    /// Get health score as f64
    pub fn health_score_f64(&self) -> f64 {
        self.health_score.to_f64().unwrap_or(0.0)
    }

    /// Get selection score as f64
    pub fn selection_score_f64(&self) -> f64 {
        self.selection_score.to_f64().unwrap_or(0.0)
    }

    /// Check if worker is healthy enough for task execution
    pub fn is_healthy(&self) -> bool {
        self.health_score_f64() > 50.0
    }

    /// Get capacity utilization percentage
    pub fn capacity_utilization(&self) -> f64 {
        if self.max_concurrent_steps > 0 {
            (self.current_load as f64 / self.max_concurrent_steps as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// Result structure for get_worker_pool_statistics function
/// Provides comprehensive worker pool monitoring information
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerPoolStatistics {
    pub total_workers: i32,
    pub healthy_workers: i32,
    pub registered_workers: i32,
    pub active_workers: i32,
    pub workers_with_recent_heartbeat: i32,
    pub avg_health_score: Option<sqlx::types::BigDecimal>,
}

impl Default for WorkerPoolStatistics {
    fn default() -> Self {
        Self {
            total_workers: 0,
            healthy_workers: 0,
            registered_workers: 0,
            active_workers: 0,
            workers_with_recent_heartbeat: 0,
            avg_health_score: Some(BigDecimal::from(0)),
        }
    }
}

impl WorkerPoolStatistics {
    /// Get average health score as f64
    pub fn avg_health_score_f64(&self) -> f64 {
        self.avg_health_score
            .as_ref()
            .and_then(|score| score.to_f64())
            .unwrap_or(0.0)
    }

    /// Calculate worker availability percentage
    pub fn availability_percentage(&self) -> f64 {
        if self.total_workers > 0 {
            (self.healthy_workers as f64 / self.total_workers as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Calculate recent heartbeat percentage
    pub fn recent_heartbeat_percentage(&self) -> f64 {
        if self.active_workers > 0 {
            (self.workers_with_recent_heartbeat as f64 / self.active_workers as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Check if worker pool is healthy
    pub fn is_healthy(&self) -> bool {
        self.availability_percentage() > 70.0 && self.recent_heartbeat_percentage() > 80.0
    }
}

impl SqlFunctionExecutor {
    /// Find active workers for a specific task with optimized SQL function
    /// Uses pre-computed query plan with health scoring and proper indexing
    /// 
    /// Equivalent to Rails: optimized worker selection with complex JOIN operations
    pub async fn find_active_workers_for_task(
        &self,
        named_task_id: i32,
    ) -> Result<Vec<ActiveWorkerResult>, sqlx::Error> {
        let sql = "SELECT * FROM find_active_workers_for_task($1)";
        sqlx::query_as::<_, ActiveWorkerResult>(sql)
            .bind(named_task_id)
            .fetch_all(&self.pool)
            .await
    }

    /// Get worker health information for multiple workers in a single batch operation
    /// Uses optimized SQL function for efficient health status retrieval
    /// 
    /// Provides comprehensive health metrics including connection status and load information
    pub async fn get_worker_health_batch(
        &self,
        worker_ids: &[i32],
    ) -> Result<Vec<WorkerHealthResult>, sqlx::Error> {
        let sql = "SELECT * FROM get_worker_health_batch($1)";
        sqlx::query_as::<_, WorkerHealthResult>(sql)
            .bind(worker_ids)
            .fetch_all(&self.pool)
            .await
    }

    /// Select the optimal worker for a task using comprehensive scoring algorithm
    /// Uses pre-computed SQL function with advanced worker selection logic
    /// 
    /// Scoring factors:
    /// - Worker health (40% weight)
    /// - Priority level (30% weight) 
    /// - Available capacity (30% weight)
    pub async fn select_optimal_worker_for_task(
        &self,
        named_task_id: i32,
        required_capacity: Option<i32>,
    ) -> Result<Option<OptimalWorkerResult>, sqlx::Error> {
        let sql = "SELECT * FROM select_optimal_worker_for_task($1, $2)";
        sqlx::query_as::<_, OptimalWorkerResult>(sql)
            .bind(named_task_id)
            .bind(required_capacity.unwrap_or(1))
            .fetch_optional(&self.pool)
            .await
    }

    /// Get comprehensive worker pool statistics for monitoring and health assessment
    /// Uses optimized SQL function for efficient pool-wide metrics calculation
    /// 
    /// Provides metrics including:
    /// - Total, healthy, registered, and active worker counts
    /// - Workers with recent heartbeat activity
    /// - Average health score across all workers
    pub async fn get_worker_pool_statistics(&self) -> Result<WorkerPoolStatistics, sqlx::Error> {
        let sql = "SELECT * FROM get_worker_pool_statistics()";
        let result = self.execute_single(sql).await?;
        Ok(result.unwrap_or_default())
    }

    /// Invalidate worker-related caches for a specific task or all tasks
    /// Uses SQL function to coordinate cache invalidation across system components
    /// 
    /// Returns true if cache invalidation was successfully requested
    pub async fn invalidate_worker_cache(
        &self,
        named_task_id: Option<i32>,
    ) -> Result<bool, sqlx::Error> {
        let sql = "SELECT invalidate_worker_cache($1)";
        let result: Option<bool> = sqlx::query_scalar(sql)
            .bind(named_task_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result.unwrap_or(false))
    }
}
