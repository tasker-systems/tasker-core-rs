//! Type Definitions for Step Enqueuer Service
//!
//! Defines result types, statistics, and metrics for step enqueueing operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a single orchestration cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEnqueuerServiceResult {
    /// Cycle execution timestamp
    pub cycle_started_at: DateTime<Utc>,
    /// Total time for the complete cycle
    pub cycle_duration_ms: u64,
    /// Number of tasks successfully processed
    pub tasks_processed: usize,
    /// Number of tasks that failed processing
    pub tasks_failed: usize,
    /// Priority distribution of claimed tasks
    pub priority_distribution: PriorityDistribution,
    /// Per-namespace enqueueing statistics
    pub namespace_stats: HashMap<String, NamespaceStats>,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Any warnings encountered during the cycle
    pub warnings: Vec<String>,
}

/// Priority distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PriorityDistribution {
    pub urgent_tasks: usize,
    pub high_tasks: usize,
    pub normal_tasks: usize,
    pub low_tasks: usize,
    pub invalid_tasks: usize,
    pub escalated_tasks: usize,
    pub avg_computed_priority: f64,
    pub avg_task_age_hours: f64,
}

/// Per-namespace statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceStats {
    pub tasks_processed: usize,
    pub steps_enqueued: usize,
    pub steps_failed: usize,
    pub queue_name: String,
    pub avg_processing_time_ms: u64,
}

/// Performance metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub claim_duration_ms: u64,
    pub discovery_duration_ms: u64,
    pub enqueueing_duration_ms: u64,
    pub release_duration_ms: u64,
    pub avg_task_processing_ms: u64,
    pub steps_per_second: f64,
    pub tasks_per_second: f64,
}

/// Aggregate summary for continuous orchestration to prevent memory bloat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousOrchestrationSummary {
    /// When continuous orchestration started
    pub started_at: DateTime<Utc>,
    /// When continuous orchestration ended (None if still running)
    pub ended_at: Option<DateTime<Utc>>,
    /// Total number of cycles completed
    pub total_cycles: u64,
    /// Total number of cycles that failed
    pub failed_cycles: u64,
    /// Total tasks processed across all cycles
    pub total_tasks_processed: u64,
    /// Total tasks that failed processing
    pub total_tasks_failed: u64,
    /// Total steps enqueued across all cycles
    pub total_steps_enqueued: u64,
    /// Total steps that failed to enqueue
    pub total_steps_failed: u64,
    /// Aggregate priority distribution across all cycles
    pub aggregate_priority_distribution: PriorityDistribution,
    /// Performance metrics aggregated across cycles
    pub aggregate_performance_metrics: AggregatePerformanceMetrics,
    /// Most active namespaces (top 10)
    pub top_namespaces: Vec<(String, u64)>, // (namespace, steps_enqueued)
    /// Total warnings collected
    pub total_warnings: u64,
    /// Sample of recent warnings (last 50)
    pub recent_warnings: Vec<String>,
}

/// Aggregate performance metrics to track long-running orchestration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregatePerformanceMetrics {
    pub total_cycle_duration_ms: u64,
    pub total_claim_duration_ms: u64,
    pub total_discovery_duration_ms: u64,
    pub total_enqueueing_duration_ms: u64,
    pub total_release_duration_ms: u64,
    pub peak_steps_per_second: f64,
    pub peak_tasks_per_second: f64,
    pub avg_steps_per_second: f64,
    pub avg_tasks_per_second: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_distribution_default() {
        let dist = PriorityDistribution::default();
        assert_eq!(dist.urgent_tasks, 0);
        assert_eq!(dist.high_tasks, 0);
        assert_eq!(dist.normal_tasks, 0);
        assert_eq!(dist.low_tasks, 0);
        assert_eq!(dist.invalid_tasks, 0);
        assert_eq!(dist.escalated_tasks, 0);
        assert_eq!(dist.avg_computed_priority, 0.0);
        assert_eq!(dist.avg_task_age_hours, 0.0);
    }

    #[test]
    fn test_priority_distribution_serialization() {
        let dist = PriorityDistribution {
            urgent_tasks: 5,
            high_tasks: 3,
            normal_tasks: 10,
            low_tasks: 2,
            invalid_tasks: 1,
            escalated_tasks: 2,
            avg_computed_priority: 7.5,
            avg_task_age_hours: 2.3,
        };

        let json = serde_json::to_string(&dist).unwrap();
        let deserialized: PriorityDistribution = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.urgent_tasks, 5);
        assert_eq!(deserialized.escalated_tasks, 2);
        assert_eq!(deserialized.avg_computed_priority, 7.5);
    }

    #[test]
    fn test_namespace_stats_structure() {
        let stats = NamespaceStats {
            tasks_processed: 10,
            steps_enqueued: 50,
            steps_failed: 2,
            queue_name: "test_queue".to_string(),
            avg_processing_time_ms: 150,
        };

        assert_eq!(stats.tasks_processed, 10);
        assert_eq!(stats.steps_enqueued, 50);
        assert_eq!(stats.queue_name, "test_queue");
    }

    #[test]
    fn test_performance_metrics_structure() {
        let metrics = PerformanceMetrics {
            claim_duration_ms: 100,
            discovery_duration_ms: 200,
            enqueueing_duration_ms: 300,
            release_duration_ms: 50,
            avg_task_processing_ms: 150,
            steps_per_second: 10.5,
            tasks_per_second: 2.5,
        };

        assert_eq!(metrics.claim_duration_ms, 100);
        assert_eq!(metrics.steps_per_second, 10.5);
        assert_eq!(metrics.tasks_per_second, 2.5);
    }

    #[test]
    fn test_step_enqueuer_service_result_structure() {
        let now = Utc::now();
        let result = StepEnqueuerServiceResult {
            cycle_started_at: now,
            cycle_duration_ms: 1000,
            tasks_processed: 10,
            tasks_failed: 1,
            priority_distribution: PriorityDistribution::default(),
            namespace_stats: HashMap::new(),
            performance_metrics: PerformanceMetrics {
                claim_duration_ms: 100,
                discovery_duration_ms: 200,
                enqueueing_duration_ms: 300,
                release_duration_ms: 50,
                avg_task_processing_ms: 150,
                steps_per_second: 10.0,
                tasks_per_second: 2.0,
            },
            warnings: vec!["test warning".to_string()],
        };

        assert_eq!(result.tasks_processed, 10);
        assert_eq!(result.tasks_failed, 1);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_continuous_orchestration_summary_structure() {
        let now = Utc::now();
        let summary = ContinuousOrchestrationSummary {
            started_at: now,
            ended_at: None,
            total_cycles: 100,
            failed_cycles: 5,
            total_tasks_processed: 1000,
            total_tasks_failed: 50,
            total_steps_enqueued: 5000,
            total_steps_failed: 100,
            aggregate_priority_distribution: PriorityDistribution::default(),
            aggregate_performance_metrics: AggregatePerformanceMetrics::default(),
            top_namespaces: vec![
                ("namespace1".to_string(), 1000),
                ("namespace2".to_string(), 500),
            ],
            total_warnings: 10,
            recent_warnings: vec!["warning1".to_string(), "warning2".to_string()],
        };

        assert_eq!(summary.total_cycles, 100);
        assert_eq!(summary.failed_cycles, 5);
        assert_eq!(summary.top_namespaces.len(), 2);
        assert!(summary.ended_at.is_none());
    }

    #[test]
    fn test_aggregate_performance_metrics_default() {
        let metrics = AggregatePerformanceMetrics::default();
        assert_eq!(metrics.total_cycle_duration_ms, 0);
        assert_eq!(metrics.peak_steps_per_second, 0.0);
        assert_eq!(metrics.avg_tasks_per_second, 0.0);
    }
}
