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
