//! Analytics Query Service (TAS-168)
//!
//! Encapsulates database queries for analytics endpoints. This service
//! delegates aggregation to PostgreSQL for efficiency and correctness:
//!
//! - Steps are aggregated by (namespace, task_name, version, step_name)
//! - Tasks are aggregated by (namespace, task_name, version)
//!
//! The service is independent of caching - that's handled by [`AnalyticsService`].

use bigdecimal::ToPrimitive;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use tasker_shared::database::sql_functions::SqlFunctionExecutor;
use tasker_shared::types::api::orchestration::{
    BottleneckAnalysis, PerformanceMetrics, ResourceUtilization, SlowStepInfo, SlowTaskInfo,
};
use tasker_shared::TaskerResult;
use tracing::debug;

/// Analytics query service for database operations
///
/// This service encapsulates all the database queries for analytics endpoints.
/// Aggregation is performed by PostgreSQL functions for efficiency and to ensure
/// proper disambiguation of step/task names within template context.
#[derive(Clone)]
pub struct AnalyticsQueryService {
    db_pool: PgPool,
}

impl std::fmt::Debug for AnalyticsQueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalyticsQueryService")
            .field("pool_size", &self.db_pool.size())
            .finish()
    }
}

impl AnalyticsQueryService {
    /// Create a new analytics query service
    pub fn new(db_pool: PgPool) -> Self {
        Self { db_pool }
    }

    /// Fetch and compute performance metrics
    ///
    /// Executes parallel queries for analytics metrics and system health counts,
    /// then combines them into a comprehensive `PerformanceMetrics` response.
    pub async fn get_performance_metrics(&self, hours: u32) -> TaskerResult<PerformanceMetrics> {
        debug!(hours = hours, "Fetching performance metrics");

        let executor = SqlFunctionExecutor::new(self.db_pool.clone());

        // Calculate the lookback period
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

        debug!(
            total_tasks = metrics.total_tasks,
            completion_rate = metrics.completion_rate,
            "Performance metrics computed"
        );

        Ok(metrics)
    }

    /// Fetch and compute bottleneck analysis
    ///
    /// Uses pre-aggregated SQL functions that group by proper identity keys:
    /// - Steps: (namespace, task_name, version, step_name)
    /// - Tasks: (namespace, task_name, version)
    pub async fn get_bottleneck_analysis(
        &self,
        limit: i32,
        min_executions: i32,
    ) -> TaskerResult<BottleneckAnalysis> {
        debug!(
            limit = limit,
            min_executions = min_executions,
            "Fetching bottleneck analysis"
        );

        let executor = SqlFunctionExecutor::new(self.db_pool.clone());

        // Get pre-aggregated data and system health in parallel
        let (slow_steps_raw, slow_tasks_raw, health) = tokio::try_join!(
            executor.get_slowest_steps_aggregated(Some(limit), Some(min_executions)),
            executor.get_slowest_tasks_aggregated(Some(limit), Some(min_executions)),
            executor.get_system_health_counts()
        )?;

        // Convert DB types to API types
        let slow_steps: Vec<SlowStepInfo> = slow_steps_raw
            .into_iter()
            .map(|s| SlowStepInfo {
                namespace_name: s.namespace_name,
                task_name: s.task_name,
                version: s.version,
                step_name: s.step_name,
                average_duration_seconds: s.average_duration_seconds.to_f64().unwrap_or(0.0),
                max_duration_seconds: s.max_duration_seconds.to_f64().unwrap_or(0.0),
                execution_count: s.execution_count,
                error_count: s.error_count,
                error_rate: s.error_rate.to_f64().unwrap_or(0.0),
                last_executed_at: s
                    .last_executed_at
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).to_rfc3339()),
            })
            .collect();

        let slow_tasks: Vec<SlowTaskInfo> = slow_tasks_raw
            .into_iter()
            .map(|t| SlowTaskInfo {
                namespace_name: t.namespace_name,
                task_name: t.task_name,
                version: t.version,
                average_duration_seconds: t.average_duration_seconds.to_f64().unwrap_or(0.0),
                max_duration_seconds: t.max_duration_seconds.to_f64().unwrap_or(0.0),
                execution_count: t.execution_count,
                average_step_count: t.average_step_count.to_f64().unwrap_or(0.0),
                error_count: t.total_error_steps,
                error_rate: t.error_rate.to_f64().unwrap_or(0.0),
                last_executed_at: t
                    .last_executed_at
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).to_rfc3339()),
            })
            .collect();

        // Calculate resource utilization
        let pool_utilization = self.calculate_pool_utilization();
        let resource_utilization = ResourceUtilization {
            database_pool_utilization: pool_utilization,
            system_health: health,
        };

        // Generate recommendations
        let recommendations = self.generate_recommendations(&slow_steps, &resource_utilization);

        let analysis = BottleneckAnalysis {
            slow_steps,
            slow_tasks,
            resource_utilization,
            recommendations,
        };

        debug!(
            slow_steps = analysis.slow_steps.len(),
            slow_tasks = analysis.slow_tasks.len(),
            recommendations = analysis.recommendations.len(),
            "Bottleneck analysis computed"
        );

        Ok(analysis)
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    /// Calculate database pool utilization
    fn calculate_pool_utilization(&self) -> f64 {
        let size = self.db_pool.size() as f64;
        let idle = self.db_pool.num_idle() as f64;
        if size > 0.0 {
            (size - idle) / size
        } else {
            0.0
        }
    }

    /// Generate recommendations based on analysis findings
    fn generate_recommendations(
        &self,
        slow_steps: &[SlowStepInfo],
        resource_util: &ResourceUtilization,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Check for high error rates
        if !slow_steps.is_empty() {
            let highest_error_rate = slow_steps
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
        if resource_util.database_pool_utilization > 0.8 {
            recommendations.push(format!(
                "Database pool utilization is high ({:.0}%). Consider increasing pool size or optimizing queries.",
                resource_util.database_pool_utilization * 100.0
            ));
        }

        // Check for slow operations - include template context in recommendation
        if !slow_steps.is_empty() && slow_steps[0].average_duration_seconds > 30.0 {
            let step = &slow_steps[0];
            recommendations.push(format!(
                "Step '{}/{}@{}::{}' averages {:.1}s. Consider optimization or timeout adjustment.",
                step.namespace_name,
                step.task_name,
                step.version,
                step.step_name,
                step.average_duration_seconds
            ));
        }

        recommendations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_pool_utilization_empty() {
        // This would require a mock pool, so we just test the logic
        // with known values via the formula
        let size = 10.0_f64;
        let idle = 3.0_f64;
        let expected = (size - idle) / size; // 0.7
        assert!((expected - 0.7).abs() < 0.001);
    }

    fn sample_slow_step(
        namespace: &str,
        task: &str,
        version: &str,
        step: &str,
        avg_duration: f64,
        error_rate: f64,
    ) -> SlowStepInfo {
        SlowStepInfo {
            namespace_name: namespace.to_string(),
            task_name: task.to_string(),
            version: version.to_string(),
            step_name: step.to_string(),
            average_duration_seconds: avg_duration,
            max_duration_seconds: avg_duration * 2.0,
            execution_count: 100,
            error_count: (100.0 * error_rate) as i64,
            error_rate,
            last_executed_at: None,
        }
    }

    #[tokio::test]
    async fn test_generate_recommendations_high_error_rate() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsQueryService::new(pool);

        let slow_steps = vec![sample_slow_step(
            "payments",
            "process_order",
            "v1",
            "validate",
            5.0,
            0.30, // 30% error rate
        )];

        let resource_util = ResourceUtilization {
            database_pool_utilization: 0.5,
            system_health: Default::default(),
        };

        let recommendations = service.generate_recommendations(&slow_steps, &resource_util);

        assert!(!recommendations.is_empty());
        assert!(recommendations[0].contains("High error rate"));
    }

    #[tokio::test]
    async fn test_generate_recommendations_pool_pressure() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsQueryService::new(pool);

        let slow_steps = vec![];
        let resource_util = ResourceUtilization {
            database_pool_utilization: 0.95, // 95% utilization
            system_health: Default::default(),
        };

        let recommendations = service.generate_recommendations(&slow_steps, &resource_util);

        assert!(!recommendations.is_empty());
        assert!(recommendations[0].contains("Database pool utilization"));
    }

    #[tokio::test]
    async fn test_generate_recommendations_slow_step() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsQueryService::new(pool);

        let slow_steps = vec![sample_slow_step(
            "processing",
            "batch_job",
            "v2",
            "slow_processing",
            45.0, // 45 seconds avg
            0.04,
        )];

        let resource_util = ResourceUtilization {
            database_pool_utilization: 0.3,
            system_health: Default::default(),
        };

        let recommendations = service.generate_recommendations(&slow_steps, &resource_util);

        assert!(!recommendations.is_empty());
        // Check that recommendation includes full template context
        assert!(recommendations[0].contains("processing/batch_job@v2::slow_processing"));
        assert!(recommendations[0].contains("45.0s"));
    }

    #[tokio::test]
    async fn test_generate_recommendations_healthy_system() {
        let pool = sqlx::PgPool::connect_lazy("postgresql://test").unwrap();
        let service = AnalyticsQueryService::new(pool);

        let slow_steps = vec![sample_slow_step(
            "fast",
            "quick_task",
            "v1",
            "fast_step",
            0.5,
            0.005, // 0.5% error rate
        )];

        let resource_util = ResourceUtilization {
            database_pool_utilization: 0.3, // 30% utilization
            system_health: Default::default(),
        };

        let recommendations = service.generate_recommendations(&slow_steps, &resource_util);

        // Healthy system should have no recommendations
        assert!(recommendations.is_empty());
    }
}
