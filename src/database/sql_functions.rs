use sqlx::{PgPool, FromRow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Core SQL function executor with async/await support
/// Provides high-performance function delegation equivalent to Rails wrappers
pub struct SqlFunctionExecutor {
    pool: PgPool,
}

impl SqlFunctionExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Execute a SQL function returning a single result
    pub async fn execute_single<T>(&self, sql: &str) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .fetch_optional(&self.pool)
            .await
    }

    /// Execute a SQL function returning multiple results
    pub async fn execute_many<T>(&self, sql: &str) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .fetch_all(&self.pool)
            .await
    }

    /// Execute a SQL function with task_id parameter
    pub async fn execute_single_with_task_id<T>(&self, sql: &str, task_id: i64) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
    }

    /// Execute a SQL function with task_id parameter returning multiple results
    pub async fn execute_many_with_task_id<T>(&self, sql: &str, task_id: i64) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        sqlx::query_as::<_, T>(sql)
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
    }

    /// Execute a SQL function with task_ids array parameter
    pub async fn execute_many_with_task_ids<T>(&self, sql: &str, task_ids: &[i64]) -> Result<Vec<T>, sqlx::Error>
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

impl SqlFunctionExecutor {
    /// Calculate dependency levels for DAG analysis
    /// Equivalent to Rails: FunctionBasedDependencyLevels.for_task
    pub async fn calculate_dependency_levels(&self, task_id: i64) -> Result<Vec<DependencyLevel>, sqlx::Error> {
        let sql = "SELECT * FROM calculate_dependency_levels($1::BIGINT)";
        self.execute_many_with_task_id(sql, task_id).await
    }

    /// Get dependency levels as a hash map for efficient lookup
    pub async fn dependency_levels_hash(&self, task_id: i64) -> Result<HashMap<i64, i32>, sqlx::Error> {
        let levels = self.calculate_dependency_levels(task_id).await?;
        Ok(levels.into_iter().map(|level| (level.workflow_step_id, level.dependency_level)).collect())
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
    pub async fn get_analytics_metrics(&self, since_timestamp: Option<DateTime<Utc>>) -> Result<AnalyticsMetrics, sqlx::Error> {
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
    pub named_step_id: i64,
    pub name: String,
    pub current_state: String,
    pub dependencies_satisfied: bool,
    pub retry_eligible: bool,
    pub ready_for_execution: bool,
    pub last_failure_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub total_parents: i32,
    pub completed_parents: i32,
    pub attempts: i32,
    pub retry_limit: i32,
    pub backoff_request_seconds: Option<i32>,
    pub last_attempted_at: Option<DateTime<Utc>>,
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
    pub async fn get_step_readiness_status(&self, task_id: i64, step_ids: Option<Vec<i64>>) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
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
    pub async fn get_step_readiness_status_batch(&self, task_ids: Vec<i64>) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let sql = "SELECT * FROM get_step_readiness_status_batch($1::BIGINT[])";
        self.execute_many_with_task_ids(sql, &task_ids).await
    }

    /// Get only ready steps for execution
    pub async fn get_ready_steps(&self, task_id: i64) -> Result<Vec<StepReadinessStatus>, sqlx::Error> {
        let all_steps = self.get_step_readiness_status(task_id, None).await?;
        Ok(all_steps.into_iter().filter(|step| step.ready_for_execution).collect())
    }
}

// ============================================================================
// 4. SYSTEM HEALTH COUNTS
// ============================================================================

/// System-wide health and performance counts
/// Equivalent to Rails: FunctionBasedSystemHealthCounts
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemHealthCounts {
    pub total_tasks: i32,
    pub pending_tasks: i32,
    pub in_progress_tasks: i32,
    pub complete_tasks: i32,
    pub error_tasks: i32,
    pub cancelled_tasks: i32,
    pub total_steps: i32,
    pub pending_steps: i32,
    pub in_progress_steps: i32,
    pub complete_steps: i32,
    pub error_steps: i32,
    pub retryable_error_steps: i32,
    pub exhausted_retry_steps: i32,
    pub in_backoff_steps: i32,
    pub active_connections: i32,
    pub max_connections: i32,
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
        let connection_health = 1.0 - (self.active_connections as f64 / self.max_connections as f64).min(1.0);

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
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskExecutionContext {
    pub task_id: i64,
    pub total_steps: i32,
    pub completed_steps: i32,
    pub pending_steps: i32,
    pub error_steps: i32,
    pub ready_steps: i32,
    pub blocked_steps: i32,
    pub completion_percentage: f64,
    pub estimated_duration_seconds: Option<i32>,
    pub recommended_action: String,
    pub next_steps_to_execute: Vec<i64>,
    pub critical_path_steps: Vec<i64>,
    pub bottleneck_steps: Vec<i64>,
}

impl TaskExecutionContext {
    /// Check if task is ready for execution
    pub fn can_proceed(&self) -> bool {
        self.ready_steps > 0
    }

    /// Check if task is complete
    pub fn is_complete(&self) -> bool {
        self.completion_percentage >= 100.0
    }

    /// Check if task is blocked
    pub fn is_blocked(&self) -> bool {
        self.ready_steps == 0 && self.pending_steps > 0
    }

    /// Get priority steps to execute next
    pub fn get_priority_steps(&self) -> &[i64] {
        if !self.critical_path_steps.is_empty() {
            &self.critical_path_steps
        } else {
            &self.next_steps_to_execute
        }
    }
}

impl SqlFunctionExecutor {
    /// Get task execution context
    /// Equivalent to Rails: FunctionBasedTaskExecutionContext.for_task
    pub async fn get_task_execution_context(&self, task_id: i64) -> Result<Option<TaskExecutionContext>, sqlx::Error> {
        let sql = "SELECT * FROM get_task_execution_context($1::BIGINT)";
        self.execute_single_with_task_id(sql, task_id).await
    }

    /// Get task execution contexts for multiple tasks (batch operation)
    /// Equivalent to Rails: FunctionBasedTaskExecutionContext.for_tasks_batch
    pub async fn get_task_execution_contexts_batch(&self, task_ids: Vec<i64>) -> Result<Vec<TaskExecutionContext>, sqlx::Error> {
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
    pub named_step_id: i64,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_readiness_blocking_reason() {
        let step = StepReadinessStatus {
            workflow_step_id: 1,
            task_id: 1,
            named_step_id: 1,
            name: "test".to_string(),
            current_state: "pending".to_string(),
            dependencies_satisfied: false,
            retry_eligible: true,
            ready_for_execution: false,
            last_failure_at: None,
            next_retry_at: None,
            total_parents: 2,
            completed_parents: 1,
            attempts: 0,
            retry_limit: 3,
            backoff_request_seconds: None,
            last_attempted_at: None,
        };

        assert_eq!(step.blocking_reason(), Some("dependencies_not_satisfied"));
        assert!(!step.can_execute_now());
    }

    #[test]
    fn test_system_health_score_calculation() {
        let health = SystemHealthCounts {
            total_tasks: 100,
            complete_tasks: 80,
            error_tasks: 5,
            active_connections: 20,
            max_connections: 100,
            ..Default::default()
        };

        let score = health.health_score();
        assert!(score > 0.0 && score <= 1.0);
        assert!(!health.is_under_heavy_load());
    }

    #[test]
    fn test_task_execution_context_status() {
        let context = TaskExecutionContext {
            task_id: 1,
            total_steps: 10,
            completed_steps: 8,
            pending_steps: 2,
            error_steps: 0,
            ready_steps: 2,
            blocked_steps: 0,
            completion_percentage: 80.0,
            estimated_duration_seconds: Some(300),
            recommended_action: "execute_ready_steps".to_string(),
            next_steps_to_execute: vec![9, 10],
            critical_path_steps: vec![9],
            bottleneck_steps: vec![],
        };

        assert!(context.can_proceed());
        assert!(!context.is_complete());
        assert!(!context.is_blocked());
        assert_eq!(context.get_priority_steps(), &[9]);
    }

    #[test]
    fn test_step_backoff_calculation() {
        let step = StepReadinessStatus {
            workflow_step_id: 1,
            task_id: 1,
            named_step_id: 1,
            name: "test".to_string(),
            current_state: "error".to_string(),
            dependencies_satisfied: true,
            retry_eligible: true,
            ready_for_execution: false,
            last_failure_at: None,
            next_retry_at: None,
            total_parents: 0,
            completed_parents: 0,
            attempts: 3,
            retry_limit: 5,
            backoff_request_seconds: None,
            last_attempted_at: None,
        };

        assert_eq!(step.effective_backoff_seconds(), 8); // 2^3 = 8
    }
}