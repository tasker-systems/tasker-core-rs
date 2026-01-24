//! # Analytics Handlers
//!
//! Read-only endpoints for performance metrics and bottleneck analysis.

use axum::extract::{Query, State};
use axum::Json;
use bigdecimal::ToPrimitive;
use chrono::{Duration, Utc};
use tracing::info;

use crate::web::circuit_breaker::execute_with_circuit_breaker;
use crate::web::middleware::permission::require_permission;
use crate::web::state::AppState;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::types::api::orchestration::{
    BottleneckAnalysis, BottleneckQuery, MetricsQuery, PerformanceMetrics, ResourceUtilization,
    SlowStepInfo, SlowTaskInfo,
};
use tasker_shared::types::permissions::Permission;
use tasker_shared::types::security::SecurityContext;
#[allow(unused_imports)]
use tasker_shared::types::web::{ApiError, ApiResult, DbOperationType};

/// Get performance metrics: GET /v1/analytics/performance
#[cfg_attr(feature = "web-api", utoipa::path(
    get,
    path = "/v1/analytics/performance",
    params(
        ("hours" = Option<u32>, Query, description = "Number of hours to look back (default: 24)")
    ),
    responses(
        (status = 200, description = "Performance metrics", body = PerformanceMetrics),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "analytics"
))]
pub async fn get_performance_metrics(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<MetricsQuery>,
) -> ApiResult<Json<PerformanceMetrics>> {
    require_permission(&security, Permission::AnalyticsRead)?;

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
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 403, description = "Insufficient permissions", body = ApiError),
        (status = 503, description = "Service unavailable", body = ApiError)
    ),
    security(("bearer_auth" = []), ("api_key_auth" = [])),
    tag = "analytics"
))]
pub async fn get_bottlenecks(
    State(state): State<AppState>,
    security: SecurityContext,
    Query(params): Query<BottleneckQuery>,
) -> ApiResult<Json<BottleneckAnalysis>> {
    require_permission(&security, Permission::AnalyticsRead)?;

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

        // Aggregate slow steps by step name
        use bigdecimal::ToPrimitive;
        use std::collections::HashMap;

        // Type aliases for complex aggregation data structures
        // (durations, error_count, last_executed)
        type StepAggregateData = (Vec<f64>, i32, Option<chrono::NaiveDateTime>);
        // (durations, step_counts, total_errors, last_executed)
        type TaskAggregateData = (Vec<f64>, Vec<i64>, i64, Option<chrono::NaiveDateTime>);

        let mut step_aggregates: HashMap<String, StepAggregateData> = HashMap::new();

        for step in slow_steps {
            let entry = step_aggregates.entry(step.step_name.clone()).or_insert((Vec::new(), 0, None));
            if let Some(duration) = step.duration_seconds.to_f64() {
                entry.0.push(duration);
            }
            if step.step_status == "error" {
                entry.1 += 1;
            }
            if let Some(completed) = step.completed_at {
                if entry.2.is_none() || Some(&completed) > entry.2.as_ref() {
                    entry.2 = Some(completed);
                }
            }
        }

        let slow_step_infos: Vec<SlowStepInfo> = step_aggregates
            .into_iter()
            .map(|(step_name, (durations, error_count, last_executed))| {
                let execution_count = durations.len() as i32;
                let avg = durations.iter().sum::<f64>() / (execution_count as f64);
                let max = durations.iter().fold(0.0_f64, |a, &b| f64::max(a, b));
                SlowStepInfo {
                    step_name,
                    average_duration_seconds: avg,
                    max_duration_seconds: max,
                    execution_count,
                    error_count,
                    error_rate: error_count as f64 / execution_count as f64,
                    last_executed_at: last_executed.map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc3339()),
                }
            })
            .collect();

        // Aggregate slow tasks by task name
        let mut task_aggregates: HashMap<String, TaskAggregateData> = HashMap::new();

        for task in slow_tasks {
            let entry = task_aggregates.entry(task.task_name.clone()).or_insert((Vec::new(), Vec::new(), 0, None));
            if let Some(duration) = task.duration_seconds.to_f64() {
                entry.0.push(duration);
            }
            entry.1.push(task.step_count);
            entry.2 += task.error_steps;
            if let Some(completed) = task.completed_at {
                if entry.3.is_none() || Some(&completed) > entry.3.as_ref() {
                    entry.3 = Some(completed);
                }
            }
        }

        let slow_task_infos: Vec<SlowTaskInfo> = task_aggregates
            .into_iter()
            .map(|(task_name, (durations, step_counts, total_errors, last_executed))| {
                let execution_count = durations.len() as i32;
                let avg_duration = durations.iter().sum::<f64>() / (execution_count as f64);
                let max_duration = durations.iter().fold(0.0_f64, |a, &b| f64::max(a, b));
                let avg_step_count = step_counts.iter().sum::<i64>() as f64 / (execution_count as f64);
                SlowTaskInfo {
                    task_name,
                    average_duration_seconds: avg_duration,
                    max_duration_seconds: max_duration,
                    execution_count,
                    average_step_count: avg_step_count,
                    error_count: total_errors as i32,
                    error_rate: total_errors as f64 / execution_count as f64,
                    last_executed_at: last_executed.map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc3339()),
                }
            })
            .collect();

        // Calculate resource utilization from database pool statistics
        // TAS-142: Implement real pool utilization calculation
        let pool_utilization = {
            let size = db_pool.size() as f64;
            let idle = db_pool.num_idle() as f64;
            if size > 0.0 {
                (size - idle) / size
            } else {
                0.0
            }
        };

        let resource_utilization = ResourceUtilization {
            database_pool_utilization: pool_utilization,
            system_health: health
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
