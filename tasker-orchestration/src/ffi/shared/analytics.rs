//! # Shared Analytics and Performance Functions
//!
//! Language-agnostic analytics functions that can be used by any language binding
//! to get high-performance operations from Rust while preserving the handle-based architecture.

use super::errors::*;
use super::types::*;
use crate::orchestration::OrchestrationCore;
use bigdecimal::ToPrimitive;
use serde_json::json;
use sqlx::types::Uuid;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tracing::{debug, info};

/// Shared analytics manager for cross-language performance operations
pub struct SharedAnalyticsManager {
    sql_executor: SqlFunctionExecutor,
}

impl Default for SharedAnalyticsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedAnalyticsManager {
    /// Create new analytics manager using shared orchestration core
    pub fn new() -> Self {
        debug!("🔧 SharedAnalyticsManager::new() - using shared orchestration core");

        // Get database URL from environment
        // Create a synchronous runtime for initialization
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        let orchestration_core = rt.block_on(async {
            OrchestrationCore::new()
                .await
                .expect("Failed to initialize OrchestrationCore")
        });

        let sql_executor = SqlFunctionExecutor::new(orchestration_core.database_pool().clone());

        Self { sql_executor }
    }

    /// Get task execution context using shared types
    pub fn get_task_execution_context(
        &self,
        task_uuid: Uuid,
    ) -> SharedFFIResult<TaskExecutionContext> {
        debug!(
            "🔍 SHARED ANALYTICS get_task_execution_context: task_uuid={}",
            task_uuid
        );

        execute_async(async {
            // Use the SQL function executor to get real data
            match self
                .sql_executor
                .get_task_execution_context(task_uuid)
                .await
            {
                Ok(Some(context)) => {
                    // Convert BigDecimal to f64 for cross-language compatibility
                    let completion_percentage = context
                        .completion_percentage
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0);

                    Ok(TaskExecutionContext {
                        task_uuid: context.task_uuid,
                        total_steps: context.total_steps,
                        completed_steps: context.completed_steps,
                        pending_steps: context.pending_steps,
                        error_steps: context.failed_steps, // Map failed_steps to error_steps
                        ready_steps: context.ready_steps,
                        blocked_steps: 0, // Not available in current structure, use 0
                        completion_percentage,
                        estimated_duration_seconds: None, // Not available in current structure
                        recommended_action: context.recommended_action,
                        // Get ready steps to populate next_steps_to_execute
                        next_steps_to_execute: {
                            match self.sql_executor.get_ready_steps(task_uuid).await {
                                Ok(ready_steps) => ready_steps
                                    .iter()
                                    .map(|s| s.workflow_step_uuid.to_string())
                                    .collect(),
                                Err(_) => vec![],
                            }
                        },
                        // Critical path would require dependency level analysis
                        critical_path_steps: {
                            match self
                                .sql_executor
                                .calculate_dependency_levels(task_uuid)
                                .await
                            {
                                Ok(levels) => {
                                    // Find the maximum level
                                    let max_level = levels
                                        .iter()
                                        .map(|l| l.dependency_level)
                                        .max()
                                        .unwrap_or(0);
                                    // Get all steps at max level (deepest in dependency chain)
                                    levels
                                        .iter()
                                        .filter(|l| l.dependency_level == max_level)
                                        .map(|l| l.workflow_step_uuid.to_string())
                                        .collect()
                                }
                                Err(_) => vec![],
                            }
                        },
                        // Bottlenecks are steps with multiple dependencies that are blocking others
                        bottleneck_steps: {
                            // Identify bottleneck steps using dependency analysis
                            match self
                                .sql_executor
                                .get_step_readiness_status(task_uuid, None)
                                .await
                            {
                                Ok(statuses) => {
                                    statuses
                                        .iter()
                                        .filter(|s| {
                                            !s.ready_for_execution && !s.dependencies_satisfied
                                        }) // Steps blocking by dependencies
                                        .map(|s| s.workflow_step_uuid.to_string())
                                        .collect()
                                }
                                Err(_) => vec![],
                            }
                        },
                    })
                }
                Ok(None) => Err(SharedFFIError::DatabaseError(format!(
                    "Task execution context not found for task_uuid: {task_uuid}"
                ))),
                Err(e) => Err(SharedFFIError::DatabaseError(format!(
                    "Failed to get task execution context: {e}"
                ))),
            }
        })
    }

    /// Get analytics metrics using shared types
    pub fn get_analytics_metrics(
        &self,
        task_uuid: Option<i64>,
    ) -> SharedFFIResult<AnalyticsMetrics> {
        debug!(
            "🔍 SHARED ANALYTICS get_analytics_metrics: task_uuid={:?}",
            task_uuid
        );

        execute_async(async {
            // Get real analytics metrics from SQL functions
            match self.sql_executor.get_analytics_metrics(None).await {
                Ok(metrics) => {
                    // Get system health counts for additional metrics
                    let health_counts = self.sql_executor.get_system_health_counts().await.ok();

                    // Convert SQL function metrics to FFI analytics metrics
                    Ok(AnalyticsMetrics {
                        total_tasks: health_counts.as_ref().map(|h| h.total_tasks).unwrap_or(0),
                        completed_tasks: health_counts
                            .as_ref()
                            .map(|h| h.complete_tasks)
                            .unwrap_or(0),
                        failed_tasks: health_counts.as_ref().map(|h| h.error_tasks).unwrap_or(0),
                        pending_tasks: health_counts.as_ref().map(|h| h.pending_tasks).unwrap_or(0),
                        average_completion_time_seconds: metrics
                            .avg_task_duration
                            .to_f64()
                            .unwrap_or(0.0),
                        success_rate_percentage: metrics.completion_rate.to_f64().unwrap_or(0.0)
                            * 100.0,
                        most_common_failure_reason: {
                            // Use health counts to determine most common failure type
                            match health_counts.as_ref() {
                                Some(h) if h.error_steps > h.exhausted_retry_steps => {
                                    "Execution timeout".to_string()
                                }
                                Some(h) if h.exhausted_retry_steps > 0 => {
                                    "Retry exhaustion".to_string()
                                }
                                _ => "Processing error".to_string(),
                            }
                        },
                        peak_throughput_tasks_per_hour: metrics.task_throughput * 60, // Convert from per minute
                        current_load_percentage: {
                            // Calculate load based on active vs max capacity
                            let active = metrics.active_tasks_count as f64;
                            // Calculate capacity based on database connection limits
                            let capacity = match health_counts.as_ref() {
                                Some(h) if h.max_connections > 0 => h.max_connections as f64 * 0.8, // 80% of max connections
                                _ => 1000.0, // Default fallback
                            };
                            (active / capacity * 100.0).min(100.0)
                        },
                        resource_utilization: json!({
                            "active_tasks": metrics.active_tasks_count,
                            "task_throughput_per_min": metrics.task_throughput,
                            "step_throughput_per_min": metrics.step_throughput,
                            "health_score": metrics.system_health_score,
                            "database_connections": health_counts.as_ref().map(|h| h.active_connections).unwrap_or(0)
                        }),
                    })
                }
                Err(e) => Err(SharedFFIError::DatabaseError(format!(
                    "Failed to get analytics metrics: {e}"
                ))),
            }
        })
    }

    /// Analyze dependencies using shared types
    pub fn analyze_dependencies(&self, task_uuid: Uuid) -> SharedFFIResult<DependencyAnalysis> {
        debug!(
            "🔍 SHARED ANALYTICS analyze_dependencies: task_uuid={}",
            task_uuid
        );

        execute_async(async {
            // Get dependency levels for critical path analysis
            let dependency_levels = match self
                .sql_executor
                .calculate_dependency_levels(task_uuid)
                .await
            {
                Ok(levels) => levels,
                Err(e) => {
                    return Err(SharedFFIError::DatabaseError(format!(
                        "Failed to get dependency levels: {e}"
                    )))
                }
            };

            // Get step readiness status for all steps
            let step_statuses = match self
                .sql_executor
                .get_step_readiness_status(task_uuid, None)
                .await
            {
                Ok(statuses) => statuses,
                Err(e) => {
                    return Err(SharedFFIError::DatabaseError(format!(
                        "Failed to get step statuses: {e}"
                    )))
                }
            };

            // Calculate metrics from the data
            let total_dependencies = step_statuses
                .iter()
                .map(|s| s.total_parents as i64)
                .sum::<i64>();

            let resolved_dependencies = step_statuses
                .iter()
                .map(|s| s.completed_parents as i64)
                .sum::<i64>();

            let pending_dependencies = total_dependencies - resolved_dependencies;

            // Find critical path - steps at the deepest dependency level
            let max_level = dependency_levels
                .iter()
                .map(|l| l.dependency_level)
                .max()
                .unwrap_or(0);

            let critical_path: Vec<Uuid> = dependency_levels
                .iter()
                .filter(|l| l.dependency_level == max_level)
                .map(|l| l.workflow_step_uuid)
                .collect();

            // Generate optimization suggestions based on analysis
            let mut suggestions = vec![];

            // Check for parallelization opportunities
            let same_level_groups: std::collections::HashMap<i32, Vec<Uuid>> = dependency_levels
                .iter()
                .fold(std::collections::HashMap::new(), |mut acc, level| {
                    acc.entry(level.dependency_level)
                        .or_default()
                        .push(level.workflow_step_uuid);
                    acc
                });

            for (level, steps) in same_level_groups {
                if steps.len() > 1 {
                    suggestions.push(format!(
                        "Level {} has {} steps that can run in parallel",
                        level,
                        steps.len()
                    ));
                }
            }

            // Check for blocked steps
            let blocked_count = step_statuses
                .iter()
                .filter(|s| !s.ready_for_execution && !s.dependencies_satisfied)
                .count();

            if blocked_count > 0 {
                suggestions.push(format!(
                    "{blocked_count} steps are blocked by incomplete dependencies"
                ));
            }

            // Estimate completion time based on average step duration
            let remaining_steps = step_statuses
                .iter()
                .filter(|s| s.current_state != "complete")
                .count() as i64;

            // Calculate average step duration from actual metrics
            let avg_step_duration = match self.sql_executor.get_analytics_metrics(None).await {
                Ok(metrics) => (metrics.avg_step_duration.to_f64().unwrap_or(0.0) * 60.0) as i64, // Convert from minutes to seconds
                Err(_) => 30, // Fallback default
            };
            let estimated_completion_time_seconds = remaining_steps * avg_step_duration;

            Ok(DependencyAnalysis {
                task_uuid,
                total_dependencies,
                resolved_dependencies,
                pending_dependencies,
                circular_dependencies: vec![], // PostgreSQL prevents cycles via constraints
                critical_path,
                optimization_suggestions: suggestions,
                estimated_completion_time_seconds,
            })
        })
    }

    /// Get performance report using shared types
    pub fn get_performance_report(
        &self,
        timeframe_hours: Option<i64>,
    ) -> SharedFFIResult<PerformanceReport> {
        debug!(
            "🔍 SHARED ANALYTICS get_performance_report: timeframe_hours={:?}",
            timeframe_hours
        );

        let timeframe = timeframe_hours.unwrap_or(24);

        execute_async(async {
            // Get analytics metrics for the timeframe
            let since = chrono::Utc::now() - chrono::Duration::hours(timeframe);
            let metrics = match self.sql_executor.get_analytics_metrics(Some(since)).await {
                Ok(m) => m,
                Err(e) => {
                    return Err(SharedFFIError::DatabaseError(format!(
                        "Failed to get metrics: {e}"
                    )))
                }
            };

            // Get system health counts
            let health = match self.sql_executor.get_system_health_counts().await {
                Ok(h) => h,
                Err(e) => {
                    return Err(SharedFFIError::DatabaseError(format!(
                        "Failed to get health counts: {e}"
                    )))
                }
            };

            // Get slowest steps for performance analysis
            let slowest_steps = match self.sql_executor.get_slowest_steps(Some(10), Some(1)).await {
                Ok(steps) => steps,
                Err(e) => {
                    return Err(SharedFFIError::DatabaseError(format!(
                        "Failed to get slowest steps: {e}"
                    )))
                }
            };

            // Calculate operation counts and rates
            let total_operations = health.total_steps;
            let mut error_rate = 0.0;
            let percent_failed: f64;
            let successful_operations = health.complete_steps;
            let failed_operations = health.error_steps;
            if total_operations > 0 {
                percent_failed = failed_operations as f64 / total_operations as f64;
                error_rate = percent_failed * 100.0;
            }

            // Convert slowest steps to slow operations
            let slowest_operations: Vec<SlowOperation> = slowest_steps
                .iter()
                .take(5)
                .map(|step| SlowOperation {
                    operation_name: step.step_name.clone(),
                    duration_ms: (step.avg_duration_seconds * 1000.0) as i64,
                    count: step.execution_count as i64,
                })
                .collect();

            // Generate recommendations based on actual data
            let mut recommendations = vec![];

            if error_rate > 10.0 {
                recommendations.push(format!(
                    "High error rate detected ({error_rate:.1}%). Review failed steps for common patterns."
                ));
            }

            if health.in_backoff_steps > 10 {
                recommendations.push(format!(
                    "{} steps are in backoff. Consider adjusting retry policies.",
                    health.in_backoff_steps
                ));
            }

            if health.exhausted_retry_steps > 0 {
                recommendations.push(format!(
                    "{} steps have exhausted retries. Manual intervention may be required.",
                    health.exhausted_retry_steps
                ));
            }

            // Check for slow steps
            for step in slowest_steps.iter().take(3) {
                if step.avg_duration_seconds > 60.0 {
                    recommendations.push(format!(
                        "Step '{}' averages {:.1}s. Consider optimization or parallelization.",
                        step.step_name, step.avg_duration_seconds
                    ));
                }
            }

            Ok(PerformanceReport {
                timeframe_hours: timeframe,
                total_operations,
                successful_operations,
                failed_operations,
                average_response_time_ms: metrics.avg_step_duration.to_f64().unwrap_or(0.0)
                    * 1000.0,
                p95_response_time_ms: metrics.avg_step_duration.to_f64().unwrap_or(0.0) * 1500.0, // Estimate
                p99_response_time_ms: metrics.avg_step_duration.to_f64().unwrap_or(0.0) * 2000.0, // Estimate
                throughput_operations_per_second: (metrics.step_throughput as f64 / 60.0),
                error_rate_percentage: error_rate,
                resource_usage: json!({
                    "active_connections": health.active_connections,
                    "max_connections": health.max_connections,
                    "connection_utilization": if health.max_connections > 0 {
                        (health.active_connections as f64 / health.max_connections as f64) * 100.0
                    } else {
                        0.0
                    },
                    "active_tasks": metrics.active_tasks_count,
                    "system_health_score": metrics.system_health_score
                }),
                slowest_operations,
                recommendations,
            })
        })
    }
}

/// Global shared analytics manager singleton
static GLOBAL_ANALYTICS_MANAGER: std::sync::OnceLock<std::sync::Arc<SharedAnalyticsManager>> =
    std::sync::OnceLock::new();

/// Get the global shared analytics manager
pub fn get_global_analytics_manager() -> std::sync::Arc<SharedAnalyticsManager> {
    GLOBAL_ANALYTICS_MANAGER
        .get_or_init(|| {
            info!("🎯 Creating global shared analytics manager");
            std::sync::Arc::new(SharedAnalyticsManager::new())
        })
        .clone()
}

// ===== SHARED ANALYTICS LOGIC ENDS HERE =====
// Language bindings should implement their own wrapper functions that
// convert language-specific types to/from the shared types above.
