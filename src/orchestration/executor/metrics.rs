//! # Executor Metrics
//!
//! This module provides comprehensive metrics collection and aggregation for orchestration
//! executors. It tracks performance indicators, resource usage, and operational statistics
//! that are used for monitoring, scaling decisions, and health evaluation.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::error::{Result, TaskerError};

/// Comprehensive metrics for an orchestration executor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorMetrics {
    /// Unique identifier for the executor
    pub executor_id: Uuid,
    /// Type of executor
    pub executor_type: String,
    /// When these metrics were collected (Unix timestamp)
    pub collected_at: u64,
    /// Uptime in seconds
    pub uptime_seconds: u64,

    // Processing metrics
    /// Total items processed since startup
    pub total_items_processed: u64,
    /// Total items failed since startup
    pub total_items_failed: u64,
    /// Total items skipped since startup
    pub total_items_skipped: u64,
    /// Current items per second rate (recent average)
    pub items_per_second: f64,
    /// Average processing time per item in milliseconds
    pub avg_processing_time_ms: f64,
    /// 95th percentile processing time in milliseconds
    pub p95_processing_time_ms: f64,
    /// 99th percentile processing time in milliseconds
    pub p99_processing_time_ms: f64,

    // Error and retry metrics
    /// Current error rate (0.0 - 1.0)
    pub error_rate: f64,
    /// Total number of retries attempted
    pub total_retries: u64,
    /// Number of errors by type
    pub error_counts: HashMap<String, u64>,

    // Resource utilization metrics
    /// Current utilization rate (0.0 - 1.0)
    pub utilization: f64,
    /// Average batch size actually processed
    pub avg_batch_size: f64,
    /// Current backpressure factor (0.0 - 1.0)
    pub backpressure_factor: f64,
    /// Number of active database connections
    pub active_connections: u32,

    // Queue and processing metrics
    /// Current queue depth (items waiting)
    pub queue_depth: u64,
    /// Number of processing batches completed
    pub batches_completed: u64,
    /// Number of empty polling cycles
    pub empty_polls: u64,
    /// Current polling interval in milliseconds
    pub polling_interval_ms: u64,

    // Circuit breaker metrics
    /// Circuit breaker state
    pub circuit_breaker_state: String,
    /// Number of circuit breaker trips
    pub circuit_breaker_trips: u64,
    /// Time until circuit breaker retry (seconds, if applicable)
    pub circuit_breaker_retry_seconds: Option<u64>,

    // Time window metrics (last 5 minutes)
    /// Recent performance window
    pub recent_metrics: RecentMetrics,
}

/// Recent performance metrics over a time window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentMetrics {
    /// Window duration in seconds
    pub window_seconds: u64,
    /// Items processed in this window
    pub items_processed: u64,
    /// Items failed in this window
    pub items_failed: u64,
    /// Average processing time in this window
    pub avg_processing_time_ms: f64,
    /// Maximum processing time in this window
    pub max_processing_time_ms: f64,
    /// Number of batches in this window
    pub batches_completed: u64,
    /// Utilization rate in this window
    pub utilization: f64,
}

impl Default for RecentMetrics {
    fn default() -> Self {
        Self {
            window_seconds: 300, // 5 minutes
            items_processed: 0,
            items_failed: 0,
            avg_processing_time_ms: 0.0,
            max_processing_time_ms: 0.0,
            batches_completed: 0,
            utilization: 0.0,
        }
    }
}

/// Individual metric data point for time series analysis
#[derive(Debug, Clone)]
struct MetricDataPoint {
    timestamp: Instant,
    items_processed: u64,
    items_failed: u64,
    processing_time_ms: u64,
    batch_size: usize,
    utilization: f64,
}

/// Metrics collector that aggregates performance data over time
#[derive(Debug)]
pub struct MetricsCollector {
    executor_id: Uuid,
    executor_type: String,
    started_at: Instant,

    // Cumulative counters
    total_items_processed: Arc<RwLock<u64>>,
    total_items_failed: Arc<RwLock<u64>>,
    total_items_skipped: Arc<RwLock<u64>>,
    total_retries: Arc<RwLock<u64>>,
    batches_completed: Arc<RwLock<u64>>,
    empty_polls: Arc<RwLock<u64>>,
    circuit_breaker_trips: Arc<RwLock<u64>>,

    // Error tracking
    error_counts: Arc<RwLock<HashMap<String, u64>>>,

    // Time series data
    metric_history: Arc<RwLock<VecDeque<MetricDataPoint>>>,
    max_history_size: usize,

    // Current state
    current_backpressure_factor: Arc<RwLock<f64>>,
    current_polling_interval_ms: Arc<RwLock<u64>>,
    circuit_breaker_state: Arc<RwLock<String>>,
    circuit_breaker_retry_time: Arc<RwLock<Option<Instant>>>,
    active_connections: Arc<RwLock<u32>>,
    queue_depth: Arc<RwLock<u32>>,

    // Configuration
    metrics_window_seconds: u64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(executor_id: Uuid, executor_type: String) -> Self {
        Self {
            executor_id,
            executor_type,
            started_at: Instant::now(),
            total_items_processed: Arc::new(RwLock::new(0)),
            total_items_failed: Arc::new(RwLock::new(0)),
            total_items_skipped: Arc::new(RwLock::new(0)),
            total_retries: Arc::new(RwLock::new(0)),
            batches_completed: Arc::new(RwLock::new(0)),
            empty_polls: Arc::new(RwLock::new(0)),
            circuit_breaker_trips: Arc::new(RwLock::new(0)),
            error_counts: Arc::new(RwLock::new(HashMap::new())),
            metric_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history_size: 2000, // Keep ~6 hours of data at 10-second intervals
            current_backpressure_factor: Arc::new(RwLock::new(1.0)),
            current_polling_interval_ms: Arc::new(RwLock::new(100)),
            circuit_breaker_state: Arc::new(RwLock::new("closed".to_string())),
            circuit_breaker_retry_time: Arc::new(RwLock::new(None)),
            active_connections: Arc::new(RwLock::new(0)),
            queue_depth: Arc::new(RwLock::new(0)),
            metrics_window_seconds: 300, // 5 minutes
        }
    }

    /// Record a processing batch result
    pub fn record_batch(
        &self,
        items_processed: u64,
        items_failed: u64,
        processing_time_ms: u64,
        batch_size: usize,
    ) -> Result<()> {
        // Update cumulative counters
        self.increment_counter(&self.total_items_processed, items_processed)?;
        self.increment_counter(&self.total_items_failed, items_failed)?;
        self.increment_counter(&self.batches_completed, 1)?;

        // Calculate utilization (items processed / theoretical maximum)
        let theoretical_max = batch_size as f64;
        let utilization = if theoretical_max > 0.0 {
            (items_processed + items_failed) as f64 / theoretical_max
        } else {
            0.0
        };

        // Record time series data point
        let data_point = MetricDataPoint {
            timestamp: Instant::now(),
            items_processed,
            items_failed,
            processing_time_ms,
            batch_size,
            utilization,
        };

        self.add_to_history(data_point)?;

        Ok(())
    }

    /// Record an empty polling cycle
    pub fn record_empty_poll(&self) -> Result<()> {
        self.increment_counter(&self.empty_polls, 1)
    }

    /// Record an error by type
    pub fn record_error(&self, error_type: &str) -> Result<()> {
        if let Ok(mut errors) = self.error_counts.write() {
            let count = errors.entry(error_type.to_string()).or_insert(0);
            *count += 1;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire error counts lock".to_string(),
            ))
        }
    }

    /// Record a retry attempt
    pub fn record_retry(&self) -> Result<()> {
        self.increment_counter(&self.total_retries, 1)
    }

    /// Record skipped items
    pub fn record_skipped_items(&self, count: u64) -> Result<()> {
        self.increment_counter(&self.total_items_skipped, count)
    }

    /// Update backpressure factor
    pub fn set_backpressure_factor(&self, factor: f64) -> Result<()> {
        if let Ok(mut current) = self.current_backpressure_factor.write() {
            *current = factor;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire backpressure factor lock".to_string(),
            ))
        }
    }

    /// Update polling interval
    pub fn set_polling_interval_ms(&self, interval_ms: u64) -> Result<()> {
        if let Ok(mut current) = self.current_polling_interval_ms.write() {
            *current = interval_ms;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire polling interval lock".to_string(),
            ))
        }
    }

    /// Update circuit breaker state
    pub fn set_circuit_breaker_state(
        &self,
        state: &str,
        retry_time: Option<Instant>,
    ) -> Result<()> {
        // Update state
        if let Ok(mut current_state) = self.circuit_breaker_state.write() {
            *current_state = state.to_string();
        } else {
            return Err(TaskerError::Internal(
                "Failed to acquire circuit breaker state lock".to_string(),
            ));
        }

        // Update retry time
        if let Ok(mut current_retry) = self.circuit_breaker_retry_time.write() {
            *current_retry = retry_time;
        } else {
            return Err(TaskerError::Internal(
                "Failed to acquire circuit breaker retry time lock".to_string(),
            ));
        }

        // Increment trip counter if opening
        if state == "open" {
            self.increment_counter(&self.circuit_breaker_trips, 1)?;
        }

        Ok(())
    }

    /// Set active connections count
    pub fn set_active_connections(&self, count: u32) -> Result<()> {
        if let Ok(mut connections) = self.active_connections.write() {
            *connections = count;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire active connections lock".to_string(),
            ))
        }
    }

    /// Set queue depth
    pub fn set_queue_depth(&self, depth: u32) -> Result<()> {
        if let Ok(mut queue_depth) = self.queue_depth.write() {
            *queue_depth = depth;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire queue depth lock".to_string(),
            ))
        }
    }

    /// Generate current metrics snapshot
    pub fn current_metrics(&self) -> Result<ExecutorMetrics> {
        let now = Instant::now();
        let uptime = now.duration_since(self.started_at);

        // Get cumulative counters
        let total_processed = self.read_counter(&self.total_items_processed)?;
        let total_failed = self.read_counter(&self.total_items_failed)?;
        let total_skipped = self.read_counter(&self.total_items_skipped)?;
        let total_retries = self.read_counter(&self.total_retries)?;
        let batches_completed = self.read_counter(&self.batches_completed)?;
        let empty_polls = self.read_counter(&self.empty_polls)?;
        let circuit_breaker_trips = self.read_counter(&self.circuit_breaker_trips)?;

        // Get current state
        let backpressure_factor = self.read_value(&self.current_backpressure_factor)?;
        let polling_interval_ms = self.read_value(&self.current_polling_interval_ms)?;
        let circuit_breaker_state = self.read_string(&self.circuit_breaker_state)?;

        // Calculate circuit breaker retry seconds
        let circuit_breaker_retry_seconds =
            if let Ok(retry_time) = self.circuit_breaker_retry_time.read() {
                retry_time.map(|time| {
                    if time > now {
                        time.duration_since(now).as_secs()
                    } else {
                        0
                    }
                })
            } else {
                None
            };

        // Get error counts
        let error_counts = if let Ok(errors) = self.error_counts.read() {
            errors.clone()
        } else {
            HashMap::new()
        };

        // Calculate error rate
        let total_items = total_processed + total_failed;
        let error_rate = if total_items > 0 {
            total_failed as f64 / total_items as f64
        } else {
            0.0
        };

        // Calculate performance metrics from recent history
        let recent_metrics = self.calculate_recent_metrics()?;
        let processing_stats = self.calculate_processing_statistics()?;

        // Calculate items per second from uptime
        let items_per_second = if uptime.as_secs() > 0 {
            total_processed as f64 / uptime.as_secs() as f64
        } else {
            0.0
        };

        // Calculate utilization from recent data
        let utilization = recent_metrics.utilization;

        // Calculate average batch size from actual batch size data points
        let avg_batch_size = self.calculate_average_batch_size()?;

        Ok(ExecutorMetrics {
            executor_id: self.executor_id,
            executor_type: self.executor_type.clone(),
            collected_at: current_timestamp(),
            uptime_seconds: uptime.as_secs(),
            total_items_processed: total_processed,
            total_items_failed: total_failed,
            total_items_skipped: total_skipped,
            items_per_second,
            avg_processing_time_ms: processing_stats.avg_time,
            p95_processing_time_ms: processing_stats.p95_time,
            p99_processing_time_ms: processing_stats.p99_time,
            error_rate,
            total_retries,
            error_counts,
            utilization,
            avg_batch_size,
            backpressure_factor,
            active_connections: *self.active_connections.read().unwrap(),
            queue_depth: *self.queue_depth.read().unwrap() as u64,
            batches_completed,
            empty_polls,
            polling_interval_ms,
            circuit_breaker_state,
            circuit_breaker_trips,
            circuit_breaker_retry_seconds,
            recent_metrics,
        })
    }

    /// Reset all metrics (for testing or maintenance)
    pub fn reset(&self) -> Result<()> {
        self.set_counter(&self.total_items_processed, 0)?;
        self.set_counter(&self.total_items_failed, 0)?;
        self.set_counter(&self.total_items_skipped, 0)?;
        self.set_counter(&self.total_retries, 0)?;
        self.set_counter(&self.batches_completed, 0)?;
        self.set_counter(&self.empty_polls, 0)?;
        self.set_counter(&self.circuit_breaker_trips, 0)?;

        if let Ok(mut errors) = self.error_counts.write() {
            errors.clear();
        }

        if let Ok(mut history) = self.metric_history.write() {
            history.clear();
        }

        Ok(())
    }

    // Helper methods

    fn increment_counter(&self, counter: &Arc<RwLock<u64>>, value: u64) -> Result<()> {
        if let Ok(mut count) = counter.write() {
            *count += value;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire counter lock".to_string(),
            ))
        }
    }

    fn set_counter(&self, counter: &Arc<RwLock<u64>>, value: u64) -> Result<()> {
        if let Ok(mut count) = counter.write() {
            *count = value;
            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire counter lock".to_string(),
            ))
        }
    }

    fn read_counter(&self, counter: &Arc<RwLock<u64>>) -> Result<u64> {
        if let Ok(count) = counter.read() {
            Ok(*count)
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire counter lock".to_string(),
            ))
        }
    }

    fn read_value<T: Copy>(&self, value: &Arc<RwLock<T>>) -> Result<T> {
        if let Ok(val) = value.read() {
            Ok(*val)
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire value lock".to_string(),
            ))
        }
    }

    fn read_string(&self, value: &Arc<RwLock<String>>) -> Result<String> {
        if let Ok(val) = value.read() {
            Ok(val.clone())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire string lock".to_string(),
            ))
        }
    }

    fn add_to_history(&self, data_point: MetricDataPoint) -> Result<()> {
        if let Ok(mut history) = self.metric_history.write() {
            history.push_back(data_point);

            // Keep history size bounded
            while history.len() > self.max_history_size {
                history.pop_front();
            }

            Ok(())
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire history lock".to_string(),
            ))
        }
    }

    fn calculate_recent_metrics(&self) -> Result<RecentMetrics> {
        let now = Instant::now();
        let window_duration = Duration::from_secs(self.metrics_window_seconds);
        let cutoff_time = now - window_duration;

        if let Ok(history) = self.metric_history.read() {
            let recent_points: Vec<&MetricDataPoint> = history
                .iter()
                .filter(|point| point.timestamp >= cutoff_time)
                .collect();

            if recent_points.is_empty() {
                return Ok(RecentMetrics::default());
            }

            let items_processed = recent_points.iter().map(|p| p.items_processed).sum();
            let items_failed = recent_points.iter().map(|p| p.items_failed).sum();
            let batches_completed = recent_points.len() as u64;

            let avg_processing_time_ms = recent_points
                .iter()
                .map(|p| p.processing_time_ms as f64)
                .sum::<f64>()
                / recent_points.len() as f64;

            let max_processing_time_ms = recent_points
                .iter()
                .map(|p| p.processing_time_ms as f64)
                .fold(0.0, f64::max);

            let avg_utilization = recent_points.iter().map(|p| p.utilization).sum::<f64>()
                / recent_points.len() as f64;

            Ok(RecentMetrics {
                window_seconds: self.metrics_window_seconds,
                items_processed,
                items_failed,
                avg_processing_time_ms,
                max_processing_time_ms,
                batches_completed,
                utilization: avg_utilization,
            })
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire history lock".to_string(),
            ))
        }
    }

    fn calculate_average_batch_size(&self) -> Result<f64> {
        if let Ok(history) = self.metric_history.read() {
            if history.is_empty() {
                return Ok(0.0);
            }

            // Calculate average batch size from actual batch_size values stored in data points
            let total_batch_sizes: usize = history.iter().map(|p| p.batch_size).sum();
            let avg_batch_size = if !history.is_empty() {
                total_batch_sizes as f64 / history.len() as f64
            } else {
                0.0
            };

            Ok(avg_batch_size)
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire history lock".to_string(),
            ))
        }
    }

    fn calculate_processing_statistics(&self) -> Result<ProcessingStatistics> {
        if let Ok(history) = self.metric_history.read() {
            if history.is_empty() {
                return Ok(ProcessingStatistics {
                    avg_time: 0.0,
                    p95_time: 0.0,
                    p99_time: 0.0,
                });
            }

            let mut times: Vec<f64> = history
                .iter()
                .map(|p| p.processing_time_ms as f64)
                .collect();

            times.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let avg_time = times.iter().sum::<f64>() / times.len() as f64;

            let p95_index = ((times.len() as f64) * 0.95) as usize;
            let p99_index = ((times.len() as f64) * 0.99) as usize;

            let p95_time = times
                .get(p95_index.min(times.len() - 1))
                .copied()
                .unwrap_or(0.0);
            let p99_time = times
                .get(p99_index.min(times.len() - 1))
                .copied()
                .unwrap_or(0.0);

            Ok(ProcessingStatistics {
                avg_time,
                p95_time,
                p99_time,
            })
        } else {
            Err(TaskerError::Internal(
                "Failed to acquire history lock".to_string(),
            ))
        }
    }
}

/// Processing time statistics
#[derive(Debug)]
struct ProcessingStatistics {
    avg_time: f64,
    p95_time: f64,
    p99_time: f64,
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn test_metrics_collector_creation() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        let metrics = collector.current_metrics().unwrap();
        assert_eq!(metrics.executor_id, executor_id);
        assert_eq!(metrics.executor_type, "TestExecutor");
        assert_eq!(metrics.total_items_processed, 0);
    }

    #[test]
    fn test_batch_recording() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        // Record some batches
        collector.record_batch(10, 1, 1000, 20).unwrap();
        collector.record_batch(15, 0, 800, 20).unwrap();

        let metrics = collector.current_metrics().unwrap();
        assert_eq!(metrics.total_items_processed, 25);
        assert_eq!(metrics.total_items_failed, 1);
        assert_eq!(metrics.batches_completed, 2);
        assert!(metrics.avg_processing_time_ms > 0.0);
    }

    #[test]
    fn test_error_recording() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        collector.record_error("DatabaseError").unwrap();
        collector.record_error("TimeoutError").unwrap();
        collector.record_error("DatabaseError").unwrap();

        let metrics = collector.current_metrics().unwrap();
        assert_eq!(metrics.error_counts.get("DatabaseError"), Some(&2));
        assert_eq!(metrics.error_counts.get("TimeoutError"), Some(&1));
    }

    #[test]
    fn test_backpressure_updates() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        collector.set_backpressure_factor(0.5).unwrap();
        collector.set_polling_interval_ms(200).unwrap();

        let metrics = collector.current_metrics().unwrap();
        assert_eq!(metrics.backpressure_factor, 0.5);
        assert_eq!(metrics.polling_interval_ms, 200);
    }

    #[test]
    fn test_circuit_breaker_state() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        let retry_time = Instant::now() + Duration::from_secs(30);
        collector
            .set_circuit_breaker_state("open", Some(retry_time))
            .unwrap();

        let metrics = collector.current_metrics().unwrap();
        assert_eq!(metrics.circuit_breaker_state, "open");
        assert_eq!(metrics.circuit_breaker_trips, 1);
        assert!(metrics.circuit_breaker_retry_seconds.is_some());
    }

    #[test]
    fn test_metrics_reset() {
        let executor_id = Uuid::new_v4();
        let collector = MetricsCollector::new(executor_id, "TestExecutor".to_string());

        // Record some data
        collector.record_batch(10, 1, 1000, 20).unwrap();
        collector.record_error("TestError").unwrap();

        // Verify data is there
        let metrics_before = collector.current_metrics().unwrap();
        assert!(metrics_before.total_items_processed > 0);
        assert!(!metrics_before.error_counts.is_empty());

        // Reset and verify
        collector.reset().unwrap();
        let metrics_after = collector.current_metrics().unwrap();
        assert_eq!(metrics_after.total_items_processed, 0);
        assert!(metrics_after.error_counts.is_empty());
    }
}
