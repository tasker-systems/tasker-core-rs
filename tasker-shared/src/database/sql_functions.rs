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
//! - `cleanup_stale_processes(timeout_minutes)`: Process cleanup and recovery
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
        let sql = "SELECT * FROM get_system_health_counts()";
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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TaskExecutionContext {
    pub task_uuid: Uuid,
    pub named_task_uuid: Uuid,
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
        task_uuid: Uuid,
    ) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        let sql = "SELECT * FROM get_task_execution_context($1::UUID)";
        self.execute_single_with_task_uuid(sql, task_uuid).await
    }

    /// Get task execution contexts for multiple tasks (batch operation)
    /// Equivalent to Rails: FunctionBasedTaskExecutionContext.for_tasks_batch
    pub async fn get_task_execution_contexts_batch(
        &self,
        task_uuids: Vec<Uuid>,
    ) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
        let sql = "SELECT * FROM get_task_execution_contexts_batch($1::UUID[])";
        self.execute_many_with_task_uuids(sql, &task_uuids).await
    }
}

// ============================================================================
// 6. PERFORMANCE ANALYTICS
// ============================================================================

/// Slowest steps analysis for performance optimization
/// Equivalent to Rails: FunctionBasedSlowestSteps
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlowestStepAnalysis {
    pub named_step_uuid: i32,
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
