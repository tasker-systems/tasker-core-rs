//! # Analytics Handlers
//!
//! Read-only endpoints for performance metrics and bottleneck analysis.

use axum::extract::{Query, State};
use axum::Json;
use bigdecimal::ToPrimitive;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::database::sql_functions::SqlFunctionExecutor;
use crate::web::circuit_breaker::execute_with_circuit_breaker;
#[allow(unused_imports)]
use crate::web::response_types::{ApiError, ApiResult};
use crate::web::state::{AppState, DbOperationType};

#[cfg(feature = "web-api")]
use utoipa::ToSchema;

/// Query parameters for performance metrics
#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    /// Number of hours to look back (default: 24)
    pub hours: Option<u32>,
}

/// Query parameters for bottleneck analysis
#[derive(Debug, Deserialize)]
pub struct BottleneckQuery {
    /// Maximum number of slow steps to return (default: 10)
    pub limit: Option<i32>,
    /// Minimum number of executions for inclusion (default: 5)
    pub min_executions: Option<i32>,
}

/// Performance metrics response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct PerformanceMetrics {
    pub total_tasks: i64,
    pub active_tasks: i64,
    pub completed_tasks: i64,
    pub failed_tasks: i64,
    pub completion_rate: f64,
    pub error_rate: f64,
    pub average_task_duration_seconds: f64,
    pub average_step_duration_seconds: f64,
    pub tasks_per_hour: i64,
    pub steps_per_hour: i64,
    pub system_health_score: f64,
    pub analysis_period_start: String,
    pub calculated_at: String,
}

/// Bottleneck analysis response
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct BottleneckAnalysis {
    pub slow_steps: Vec<SlowStepInfo>,
    pub slow_tasks: Vec<SlowTaskInfo>,
    pub resource_utilization: ResourceUtilization,
    pub recommendations: Vec<String>,
}

/// Information about slow-performing steps
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowStepInfo {
    pub step_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Information about slow-performing tasks
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct SlowTaskInfo {
    pub task_name: String,
    pub average_duration_seconds: f64,
    pub max_duration_seconds: f64,
    pub execution_count: i32,
    pub average_step_count: f64,
    pub error_count: i32,
    pub error_rate: f64,
    pub last_executed_at: Option<String>,
}

/// Resource utilization metrics
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "web-api", derive(ToSchema))]
pub struct ResourceUtilization {
    pub database_connections_active: i64,
    pub database_connections_max: i64,
    pub database_pool_utilization: f64,
    pub pending_steps: i64,
    pub in_progress_steps: i64,
    pub error_steps: i64,
    pub retryable_error_steps: i64,
    pub exhausted_retry_steps: i64,
}

/// Get performance metrics: GET /v1/analytics/performance
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/analytics/performance",
    params(
        ("hours" = Option<u32>, Query, description = "Number of hours to look back (default: 24)")
    ),
    responses(
        (status = 200, description = "Performance metrics", body = PerformanceMetrics),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "analytics"
))]
pub async fn get_performance_metrics(
    State(state): State<AppState>,
    Query(params): Query<MetricsQuery>,
) -> ApiResult<Json<PerformanceMetrics>> {
    info!("Retrieving performance metrics");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);
        let executor = SqlFunctionExecutor::new(db_pool.clone());

        // Calculate the lookback period
        let hours = params.hours.unwrap_or(24);
        let since = if hours > 0 {
            Some(Utc::now() - Duration::hours(hours as i64))
        } else {
            None
        };

        // Get analytics metrics and system health counts in parallel
        let (analytics, health) = tokio::try_join!(
            executor.get_analytics_metrics(since),
            executor.get_system_health_counts()
        )?;

        // Build comprehensive performance metrics response
        let metrics = PerformanceMetrics {
            total_tasks: health.total_tasks,
            active_tasks: analytics.active_tasks_count,
            completed_tasks: analytics.completion_count,
            failed_tasks: analytics.error_count,
            completion_rate: analytics.completion_rate.to_f64().unwrap_or(0.0),
            error_rate: analytics.error_rate.to_f64().unwrap_or(0.0),
            average_task_duration_seconds: analytics.avg_task_duration.to_f64().unwrap_or(0.0),
            average_step_duration_seconds: analytics.avg_step_duration.to_f64().unwrap_or(0.0),
            tasks_per_hour: analytics.task_throughput,
            steps_per_hour: analytics.step_throughput,
            system_health_score: analytics.system_health_score.to_f64().unwrap_or(0.0),
            analysis_period_start: analytics.analysis_period_start.to_rfc3339(),
            calculated_at: analytics.calculated_at.to_rfc3339(),
        };

        Ok::<Json<PerformanceMetrics>, sqlx::Error>(Json(metrics))
    })
    .await
}

/// Get bottleneck analysis: GET /v1/analytics/bottlenecks
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/analytics/bottlenecks",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of slow steps to return (default: 10)"),
        ("min_executions" = Option<i32>, Query, description = "Minimum number of executions for inclusion (default: 5)")
    ),
    responses(
        (status = 200, description = "Bottleneck analysis", body = BottleneckAnalysis),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    tag = "analytics"
))]
pub async fn get_bottlenecks(
    State(state): State<AppState>,
    Query(params): Query<BottleneckQuery>,
) -> ApiResult<Json<BottleneckAnalysis>> {
    info!("Performing bottleneck analysis");

    execute_with_circuit_breaker(&state, || async {
        let db_pool = state.select_db_pool(DbOperationType::ReadOnly);
        let executor = SqlFunctionExecutor::new(db_pool.clone());

        let limit = params.limit.or(Some(10));
        let min_executions = params.min_executions.or(Some(5));

        // Get slow steps, slow tasks, and system health in parallel
        let (slow_steps, slow_tasks, health) = tokio::try_join!(
            executor.get_slowest_steps(limit, min_executions),
            executor.get_slowest_tasks(limit, min_executions),
            executor.get_system_health_counts()
        )?;

        // Convert slow steps to response format
        let slow_step_infos: Vec<SlowStepInfo> = slow_steps
            .into_iter()
            .map(|step| SlowStepInfo {
                step_name: step.step_name,
                average_duration_seconds: step.avg_duration_seconds,
                max_duration_seconds: step.max_duration_seconds,
                execution_count: step.execution_count,
                error_count: step.error_count,
                error_rate: step.error_rate,
                last_executed_at: step.last_executed_at.map(|dt| dt.to_rfc3339()),
            })
            .collect();

        // Convert slow tasks to response format
        let slow_task_infos: Vec<SlowTaskInfo> = slow_tasks
            .into_iter()
            .map(|task| SlowTaskInfo {
                task_name: task.task_name,
                average_duration_seconds: task.avg_duration_seconds,
                max_duration_seconds: task.max_duration_seconds,
                execution_count: task.execution_count,
                average_step_count: task.avg_step_count,
                error_count: task.error_count,
                error_rate: task.error_rate,
                last_executed_at: task.last_executed_at.map(|dt| dt.to_rfc3339()),
            })
            .collect();

        // Calculate resource utilization
        let pool_utilization = if health.max_connections > 0 {
            health.active_connections as f64 / health.max_connections as f64
        } else {
            0.0
        };

        let resource_utilization = ResourceUtilization {
            database_connections_active: health.active_connections,
            database_connections_max: health.max_connections,
            database_pool_utilization: pool_utilization,
            pending_steps: health.pending_steps,
            in_progress_steps: health.in_progress_steps,
            error_steps: health.error_steps,
            retryable_error_steps: health.retryable_error_steps,
            exhausted_retry_steps: health.exhausted_retry_steps,
        };

        // Generate recommendations based on findings
        let mut recommendations = Vec::new();

        // Check for high error rates
        if !slow_step_infos.is_empty() {
            let highest_error_rate = slow_step_infos
                .iter()
                .map(|s| s.error_rate)
                .fold(0.0_f64, f64::max);

            if highest_error_rate > 0.2 {
                recommendations.push(format!(
                    "High error rate detected ({:.1}%). Review error handling and retry logic.",
                    highest_error_rate * 100.0
                ));
            }
        }

        // Check for database pool pressure
        if pool_utilization > 0.8 {
            recommendations.push(format!(
                "Database pool utilization is high ({:.0}%). Consider increasing pool size or optimizing queries.",
                pool_utilization * 100.0
            ));
        }

        // Check for retry exhaustion
        if health.exhausted_retry_steps > 0 {
            recommendations.push(format!(
                "{} steps have exhausted retries. Review failure patterns and consider manual intervention.",
                health.exhausted_retry_steps
            ));
        }

        // Check for slow operations
        if !slow_step_infos.is_empty() && slow_step_infos[0].average_duration_seconds > 30.0 {
            recommendations.push(format!(
                "Step '{}' averages {:.1}s. Consider optimization or timeout adjustment.",
                slow_step_infos[0].step_name,
                slow_step_infos[0].average_duration_seconds
            ));
        }

        let analysis = BottleneckAnalysis {
            slow_steps: slow_step_infos,
            slow_tasks: slow_task_infos,
            resource_utilization,
            recommendations,
        };

        Ok::<Json<BottleneckAnalysis>, sqlx::Error>(Json(analysis))
    }).await
}
