//! Continuous Orchestration Summary
//!
//! Manages aggregate statistics for long-running orchestration cycles.

use chrono::Utc;

use super::types::*;

impl Default for ContinuousOrchestrationSummary {
    fn default() -> Self {
        Self::new()
    }
}

impl ContinuousOrchestrationSummary {
    /// Create a new continuous orchestration summary
    pub fn new() -> Self {
        Self {
            started_at: Utc::now(),
            ended_at: None,
            total_cycles: 0,
            failed_cycles: 0,
            total_tasks_processed: 0,
            total_tasks_failed: 0,
            total_steps_enqueued: 0,
            total_steps_failed: 0,
            aggregate_priority_distribution: PriorityDistribution::default(),
            aggregate_performance_metrics: AggregatePerformanceMetrics::default(),
            top_namespaces: Vec::new(),
            total_warnings: 0,
            recent_warnings: Vec::new(),
        }
    }

    /// Accumulate results from a single cycle without storing the full result
    pub fn accumulate_cycle_result(&mut self, result: &StepEnqueuerServiceResult) {
        self.total_cycles += 1;
        self.total_tasks_processed += result.tasks_processed as u64;
        self.total_tasks_failed += result.tasks_failed as u64;

        // Aggregate priority distribution
        self.aggregate_priority_distribution.urgent_tasks +=
            result.priority_distribution.urgent_tasks;
        self.aggregate_priority_distribution.high_tasks += result.priority_distribution.high_tasks;
        self.aggregate_priority_distribution.normal_tasks +=
            result.priority_distribution.normal_tasks;
        self.aggregate_priority_distribution.low_tasks += result.priority_distribution.low_tasks;
        self.aggregate_priority_distribution.invalid_tasks +=
            result.priority_distribution.invalid_tasks;
        self.aggregate_priority_distribution.escalated_tasks +=
            result.priority_distribution.escalated_tasks;

        // Aggregate performance metrics
        self.aggregate_performance_metrics.total_cycle_duration_ms += result.cycle_duration_ms;
        self.aggregate_performance_metrics.total_claim_duration_ms +=
            result.performance_metrics.claim_duration_ms;
        self.aggregate_performance_metrics
            .total_discovery_duration_ms += result.performance_metrics.discovery_duration_ms;
        self.aggregate_performance_metrics
            .total_enqueueing_duration_ms += result.performance_metrics.enqueueing_duration_ms;
        self.aggregate_performance_metrics.total_release_duration_ms +=
            result.performance_metrics.release_duration_ms;

        // Track peak performance
        if result.performance_metrics.steps_per_second
            > self.aggregate_performance_metrics.peak_steps_per_second
        {
            self.aggregate_performance_metrics.peak_steps_per_second =
                result.performance_metrics.steps_per_second;
        }
        if result.performance_metrics.tasks_per_second
            > self.aggregate_performance_metrics.peak_tasks_per_second
        {
            self.aggregate_performance_metrics.peak_tasks_per_second =
                result.performance_metrics.tasks_per_second;
        }

        // Aggregate namespace statistics (keep top 10)
        for (namespace, stats) in &result.namespace_stats {
            if let Some(existing) = self
                .top_namespaces
                .iter_mut()
                .find(|(ns, _)| ns == namespace)
            {
                existing.1 += stats.steps_enqueued as u64;
            } else {
                self.top_namespaces
                    .push((namespace.clone(), stats.steps_enqueued as u64));
            }
        }
        // Keep only top 10 namespaces by activity
        self.top_namespaces.sort_by(|a, b| b.1.cmp(&a.1));
        self.top_namespaces.truncate(10);

        // Aggregate warnings (keep recent 50)
        self.total_warnings += result.warnings.len() as u64;
        for warning in &result.warnings {
            self.recent_warnings.push(warning.clone());
        }
        if self.recent_warnings.len() > 50 {
            self.recent_warnings
                .drain(0..self.recent_warnings.len() - 50);
        }
    }

    /// Increment error count for failed cycles
    pub fn increment_error_count(&mut self) {
        self.failed_cycles += 1;
    }

    /// Finalize the summary when continuous orchestration stops
    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());

        // Calculate final averages
        if self.total_cycles > 0 {
            let total_tasks = (self.aggregate_priority_distribution.urgent_tasks
                + self.aggregate_priority_distribution.high_tasks
                + self.aggregate_priority_distribution.normal_tasks
                + self.aggregate_priority_distribution.low_tasks
                + self.aggregate_priority_distribution.invalid_tasks)
                as f64;

            if total_tasks > 0.0 {
                self.aggregate_priority_distribution.avg_computed_priority =
                    (self.aggregate_priority_distribution.urgent_tasks as f64 * 4.0
                        + self.aggregate_priority_distribution.high_tasks as f64 * 3.0
                        + self.aggregate_priority_distribution.normal_tasks as f64 * 2.0
                        + self.aggregate_priority_distribution.low_tasks as f64 * 1.0)
                        / total_tasks;
            }

            // Calculate average performance metrics
            let total_duration_seconds =
                self.aggregate_performance_metrics.total_cycle_duration_ms as f64 / 1000.0;
            if total_duration_seconds > 0.0 {
                self.aggregate_performance_metrics.avg_steps_per_second =
                    self.total_steps_enqueued as f64 / total_duration_seconds;
                self.aggregate_performance_metrics.avg_tasks_per_second =
                    self.total_tasks_processed as f64 / total_duration_seconds;
            }
        }
    }

    /// Get average cycle duration in milliseconds
    pub fn avg_cycle_duration_ms(&self) -> f64 {
        if self.total_cycles > 0 {
            self.aggregate_performance_metrics.total_cycle_duration_ms as f64
                / self.total_cycles as f64
        } else {
            0.0
        }
    }

    /// Get success rate percentage
    pub fn success_rate_percentage(&self) -> f64 {
        if self.total_cycles > 0 {
            ((self.total_cycles - self.failed_cycles) as f64 / self.total_cycles as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get runtime duration
    pub fn runtime_duration(&self) -> chrono::Duration {
        let end_time = self.ended_at.unwrap_or_else(Utc::now);
        end_time - self.started_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_new_summary_creation() {
        let summary = ContinuousOrchestrationSummary::new();

        // Verify initial state
        assert_eq!(summary.total_cycles, 0);
        assert_eq!(summary.failed_cycles, 0);
        assert_eq!(summary.total_tasks_processed, 0);
        assert_eq!(summary.total_tasks_failed, 0);
        assert!(summary.ended_at.is_none());
    }

    #[test]
    fn test_default_creation() {
        let summary = ContinuousOrchestrationSummary::default();

        // Verify Default trait works the same as new()
        assert_eq!(summary.total_cycles, 0);
        assert_eq!(summary.failed_cycles, 0);
    }

    #[test]
    fn test_accumulate_cycle_result() {
        let mut summary = ContinuousOrchestrationSummary::new();

        let result = StepEnqueuerServiceResult {
            cycle_started_at: Utc::now(),
            cycle_duration_ms: 100,
            tasks_processed: 5,
            tasks_failed: 1,
            priority_distribution: PriorityDistribution {
                urgent_tasks: 2,
                high_tasks: 1,
                normal_tasks: 2,
                low_tasks: 0,
                invalid_tasks: 0,
                escalated_tasks: 0,
                avg_computed_priority: 0.0,
                avg_task_age_hours: 0.0,
            },
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: 10,
                discovery_duration_ms: 20,
                enqueueing_duration_ms: 30,
                release_duration_ms: 5,
                avg_task_processing_ms: 20,
                steps_per_second: 50.0,
                tasks_per_second: 10.0,
            },
            warnings: vec!["test warning".to_string()],
        };

        summary.accumulate_cycle_result(&result);

        // Verify accumulation
        assert_eq!(summary.total_cycles, 1);
        assert_eq!(summary.total_tasks_processed, 5);
        assert_eq!(summary.total_tasks_failed, 1);
        assert_eq!(summary.aggregate_priority_distribution.urgent_tasks, 2);
        assert_eq!(summary.aggregate_priority_distribution.high_tasks, 1);
        assert_eq!(
            summary
                .aggregate_performance_metrics
                .total_cycle_duration_ms,
            100
        );
        assert_eq!(summary.total_warnings, 1);
        assert_eq!(summary.recent_warnings.len(), 1);
    }

    #[test]
    fn test_increment_error_count() {
        let mut summary = ContinuousOrchestrationSummary::new();

        summary.increment_error_count();
        assert_eq!(summary.failed_cycles, 1);

        summary.increment_error_count();
        assert_eq!(summary.failed_cycles, 2);
    }

    #[test]
    fn test_finalize_sets_end_time() {
        let mut summary = ContinuousOrchestrationSummary::new();

        assert!(summary.ended_at.is_none());

        summary.finalize();

        assert!(summary.ended_at.is_some());
    }

    #[test]
    fn test_avg_cycle_duration_ms() {
        let mut summary = ContinuousOrchestrationSummary::new();

        // Empty summary should return 0.0
        assert_eq!(summary.avg_cycle_duration_ms(), 0.0);

        // Add cycle results
        let result = StepEnqueuerServiceResult {
            cycle_started_at: Utc::now(),
            cycle_duration_ms: 100,
            tasks_processed: 1,
            tasks_failed: 0,
            priority_distribution: PriorityDistribution::default(),
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: 10,
                discovery_duration_ms: 20,
                enqueueing_duration_ms: 30,
                release_duration_ms: 5,
                avg_task_processing_ms: 20,
                steps_per_second: 10.0,
                tasks_per_second: 2.0,
            },
            warnings: vec![],
        };

        summary.accumulate_cycle_result(&result);
        summary.accumulate_cycle_result(&result);

        // Should be 200ms total / 2 cycles = 100ms average
        assert_eq!(summary.avg_cycle_duration_ms(), 100.0);
    }

    #[test]
    fn test_success_rate_percentage() {
        let mut summary = ContinuousOrchestrationSummary::new();

        // Empty summary should return 0.0
        assert_eq!(summary.success_rate_percentage(), 0.0);

        // Add successful cycles
        let result = StepEnqueuerServiceResult {
            cycle_started_at: Utc::now(),
            cycle_duration_ms: 100,
            tasks_processed: 1,
            tasks_failed: 0,
            priority_distribution: PriorityDistribution::default(),
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: 10,
                discovery_duration_ms: 20,
                enqueueing_duration_ms: 30,
                release_duration_ms: 5,
                avg_task_processing_ms: 20,
                steps_per_second: 10.0,
                tasks_per_second: 2.0,
            },
            warnings: vec![],
        };

        summary.accumulate_cycle_result(&result);
        summary.accumulate_cycle_result(&result);
        summary.accumulate_cycle_result(&result);
        summary.increment_error_count(); // 1 failed out of 3 total

        // Should be 2 successful / 3 total = 66.67%
        let success_rate = summary.success_rate_percentage();
        assert!((success_rate - 66.666666).abs() < 0.01);
    }

    #[test]
    fn test_runtime_duration() {
        let summary = ContinuousOrchestrationSummary::new();

        // Runtime should be > 0 even for new summary
        let duration = summary.runtime_duration();
        assert!(duration.num_milliseconds() >= 0);
    }
}
