//! # Executor Health Management
//!
//! This module provides health monitoring and state management for orchestration executors.
//! It tracks executor health states, performance indicators, and provides mechanisms for
//! health evaluation and reporting.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;

/// Health state of an orchestration executor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutorHealth {
    /// Executor is functioning normally
    Healthy {
        /// Last heartbeat timestamp (Unix timestamp)
        last_heartbeat: u64,
        /// Number of items processed recently
        items_processed: u64,
        /// Average processing time in milliseconds
        avg_processing_time_ms: f64,
        /// Uptime in seconds
        uptime_seconds: u64,
    },
    /// Executor is experiencing performance issues but still functional
    Degraded {
        /// Last heartbeat timestamp (Unix timestamp)
        last_heartbeat: u64,
        /// Reason for degraded performance
        reason: String,
        /// Error rate (0.0 - 1.0)
        error_rate: f64,
        /// Duration in degraded state (seconds)
        degraded_for_seconds: u64,
    },
    /// Executor is not responding or has critical issues
    Unhealthy {
        /// Last time executor was seen alive (Unix timestamp)
        last_seen: u64,
        /// Reason for unhealthy state
        reason: String,
        /// Duration in unhealthy state (seconds)
        unhealthy_for_seconds: u64,
    },
    /// Executor is in the process of starting up
    Starting {
        /// When the startup process began (Unix timestamp)
        started_at: u64,
        /// Current startup phase
        phase: String,
    },
    /// Executor is in the process of stopping
    Stopping {
        /// When the shutdown process was initiated (Unix timestamp)
        initiated_at: u64,
        /// Whether this is a graceful shutdown
        graceful: bool,
        /// Current shutdown phase
        phase: String,
    },
}

impl ExecutorHealth {
    /// Check if the health state indicates the executor is operational
    pub fn is_operational(&self) -> bool {
        matches!(
            self,
            ExecutorHealth::Healthy { .. } | ExecutorHealth::Degraded { .. }
        )
    }

    /// Check if the health state indicates the executor needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(
            self,
            ExecutorHealth::Degraded { .. } | ExecutorHealth::Unhealthy { .. }
        )
    }

    /// Get a human-readable description of the health state
    pub fn description(&self) -> String {
        match self {
            ExecutorHealth::Healthy {
                items_processed,
                avg_processing_time_ms,
                ..
            } => {
                format!(
                    "Healthy - {items_processed} items processed, {avg_processing_time_ms:.1}ms avg"
                )
            }
            ExecutorHealth::Degraded {
                reason, error_rate, ..
            } => {
                format!(
                    "Degraded - {} (error rate: {:.1}%)",
                    reason,
                    error_rate * 100.0
                )
            }
            ExecutorHealth::Unhealthy { reason, .. } => {
                format!("Unhealthy - {reason}")
            }
            ExecutorHealth::Starting { phase, .. } => {
                format!("Starting - {phase}")
            }
            ExecutorHealth::Stopping {
                graceful, phase, ..
            } => {
                let shutdown_type = if *graceful { "graceful" } else { "forced" };
                format!("Stopping ({shutdown_type}) - {phase}")
            }
        }
    }

    /// Get the severity level of the health state (0 = healthy, 10 = critical)
    pub fn severity_level(&self) -> u8 {
        match self {
            ExecutorHealth::Healthy { .. } => 0,
            ExecutorHealth::Starting { .. } => 2,
            ExecutorHealth::Degraded { error_rate, .. } => {
                // Severity increases with error rate
                (3.0 + error_rate * 4.0) as u8
            }
            ExecutorHealth::Stopping { graceful, .. } => {
                if *graceful {
                    5
                } else {
                    7
                }
            }
            ExecutorHealth::Unhealthy { .. } => 10,
        }
    }
}

/// Performance metrics for health evaluation
#[derive(Debug, Clone)]
struct PerformanceMetric {
    timestamp: Instant,
    processing_time_ms: u64,
    success: bool,
    items_processed: usize,
}

/// Health monitor for tracking executor health over time
#[derive(Debug)]
pub struct HealthMonitor {
    executor_id: Uuid,
    started_at: Instant,
    last_heartbeat: Arc<RwLock<Instant>>,
    current_health: Arc<RwLock<ExecutorHealth>>,
    performance_history: Arc<RwLock<VecDeque<PerformanceMetric>>>,
    max_history_size: usize,
    health_check_config: HealthCheckConfig,
    /// Time when degraded state began (for tracking degraded duration)
    degraded_since: Arc<RwLock<Option<Instant>>>,
    /// Time when unhealthy state began (for tracking unhealthy duration)
    unhealthy_since: Arc<RwLock<Option<Instant>>>,
}

/// Configuration for health checking
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Maximum time without heartbeat before marking unhealthy (seconds)
    pub heartbeat_timeout_seconds: u64,
    /// Maximum error rate before marking degraded (0.0 - 1.0)
    pub max_error_rate: f64,
    /// Maximum average processing time before marking degraded (milliseconds)
    pub max_avg_processing_time_ms: f64,
    /// Minimum number of metrics needed for health evaluation
    pub min_metrics_for_evaluation: usize,
    /// Time window for evaluating recent performance (seconds)
    pub evaluation_window_seconds: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout_seconds: 30,
            max_error_rate: 0.1,                // 10%
            max_avg_processing_time_ms: 5000.0, // 5 seconds
            min_metrics_for_evaluation: 5,
            evaluation_window_seconds: 300, // 5 minutes
        }
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(executor_id: Uuid) -> Self {
        let now = Instant::now();
        Self {
            executor_id,
            started_at: now,
            last_heartbeat: Arc::new(RwLock::new(now)),
            current_health: Arc::new(RwLock::new(ExecutorHealth::Starting {
                started_at: current_timestamp(),
                phase: "initializing".to_string(),
            })),
            performance_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history_size: 1000,
            health_check_config: HealthCheckConfig::default(),
            degraded_since: Arc::new(RwLock::new(None)),
            unhealthy_since: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new health monitor with custom configuration
    pub fn with_config(executor_id: Uuid, config: HealthCheckConfig) -> Self {
        let mut monitor = Self::new(executor_id);
        monitor.health_check_config = config;
        monitor
    }

    /// Record a heartbeat
    pub async fn heartbeat(&self) -> Result<()> {
        let now = Instant::now();

        // Update last heartbeat
        {
            let mut last_heartbeat = self.last_heartbeat.write().await;
            *last_heartbeat = now;
        }

        // Update health state if necessary
        self.evaluate_health().await?;

        Ok(())
    }

    /// Record performance metrics
    pub async fn record_performance(
        &self,
        processing_time_ms: u64,
        success: bool,
        items_processed: usize,
    ) -> Result<()> {
        let metric = PerformanceMetric {
            timestamp: Instant::now(),
            processing_time_ms,
            success,
            items_processed,
        };

        // Add to performance history
        {
            let mut history = self.performance_history.write().await;
            history.push_back(metric);

            // Keep history size bounded
            while history.len() > self.max_history_size {
                history.pop_front();
            }
        }

        // Re-evaluate health after recording performance
        self.evaluate_health().await?;

        Ok(())
    }

    /// Get current health state
    pub async fn current_health(&self) -> Result<ExecutorHealth> {
        let health = self.current_health.read().await;
        Ok(health.clone())
    }

    /// Mark executor as starting with specific phase
    pub async fn mark_starting(&self, phase: &str) -> Result<()> {
        let mut health = self.current_health.write().await;
        *health = ExecutorHealth::Starting {
            started_at: current_timestamp(),
            phase: phase.to_string(),
        };
        Ok(())
    }

    /// Mark executor as stopping
    pub async fn mark_stopping(&self, graceful: bool, phase: &str) -> Result<()> {
        let mut health = self.current_health.write().await;
        *health = ExecutorHealth::Stopping {
            initiated_at: current_timestamp(),
            graceful,
            phase: phase.to_string(),
        };
        Ok(())
    }

    /// Force set health state (for testing or emergency situations)
    pub async fn force_set_health(&self, health: ExecutorHealth) -> Result<()> {
        let mut current_health = self.current_health.write().await;
        *current_health = health;
        Ok(())
    }

    /// Evaluate current health based on metrics and heartbeat
    async fn evaluate_health(&self) -> Result<()> {
        let now = Instant::now();

        // Check heartbeat timeout
        let last_heartbeat = {
            let heartbeat = self.last_heartbeat.read().await;
            *heartbeat
        };

        let time_since_heartbeat = now.duration_since(last_heartbeat);
        if time_since_heartbeat.as_secs() > self.health_check_config.heartbeat_timeout_seconds {
            // Mark as unhealthy due to heartbeat timeout
            let new_health = ExecutorHealth::Unhealthy {
                last_seen: timestamp_from_instant(last_heartbeat),
                reason: format!(
                    "No heartbeat for {} seconds",
                    time_since_heartbeat.as_secs()
                ),
                unhealthy_for_seconds: time_since_heartbeat.as_secs(),
            };

            let mut health = self.current_health.write().await;
            *health = new_health;
            return Ok(());
        }

        // Get recent performance metrics
        let recent_metrics = self.get_recent_metrics().await?;

        // If we don't have enough metrics, keep current state or mark as starting
        if recent_metrics.len() < self.health_check_config.min_metrics_for_evaluation {
            let current = self.current_health().await?;
            if let ExecutorHealth::Unhealthy { .. } = current {
                // If we were unhealthy but now have heartbeat, move to starting
                let mut health = self.current_health.write().await;
                *health = ExecutorHealth::Starting {
                    started_at: current_timestamp(),
                    phase: "recovering".to_string(),
                };
            }
            return Ok(());
        }

        // Calculate performance metrics
        let total_items: usize = recent_metrics.iter().map(|m| m.items_processed).sum();
        let successful_items: usize = recent_metrics
            .iter()
            .filter(|m| m.success)
            .map(|m| m.items_processed)
            .sum();

        let error_rate = if total_items > 0 {
            1.0 - (successful_items as f64 / total_items as f64)
        } else {
            0.0
        };

        let avg_processing_time: f64 = recent_metrics
            .iter()
            .map(|m| m.processing_time_ms as f64)
            .sum::<f64>()
            / recent_metrics.len() as f64;

        let uptime_seconds = now.duration_since(self.started_at).as_secs();

        // Determine health state based on metrics
        let new_health = if error_rate > self.health_check_config.max_error_rate {
            // Track when degraded state began
            let degraded_for_seconds = {
                let mut degraded_since = self.degraded_since.write().await;
                if degraded_since.is_none() {
                    *degraded_since = Some(now);
                    0
                } else {
                    now.duration_since(degraded_since.unwrap()).as_secs()
                }
            };

            // Clear unhealthy since we're degraded (better state)
            *self.unhealthy_since.write().await = None;

            ExecutorHealth::Degraded {
                last_heartbeat: current_timestamp(),
                reason: format!("High error rate: {:.1}%", error_rate * 100.0),
                error_rate,
                degraded_for_seconds,
            }
        } else if avg_processing_time > self.health_check_config.max_avg_processing_time_ms {
            // Track when degraded state began
            let degraded_for_seconds = {
                let mut degraded_since = self.degraded_since.write().await;
                if degraded_since.is_none() {
                    *degraded_since = Some(now);
                    0
                } else {
                    now.duration_since(degraded_since.unwrap()).as_secs()
                }
            };

            // Clear unhealthy since we're degraded (better state)
            *self.unhealthy_since.write().await = None;

            ExecutorHealth::Degraded {
                last_heartbeat: current_timestamp(),
                reason: format!("Slow processing: {avg_processing_time:.1}ms avg"),
                error_rate,
                degraded_for_seconds,
            }
        } else {
            // System is healthy - clear any degraded/unhealthy timestamps
            *self.degraded_since.write().await = None;
            *self.unhealthy_since.write().await = None;

            ExecutorHealth::Healthy {
                last_heartbeat: current_timestamp(),
                items_processed: total_items as u64,
                avg_processing_time_ms: avg_processing_time,
                uptime_seconds,
            }
        };

        // Update health state
        let mut health = self.current_health.write().await;
        *health = new_health;

        Ok(())
    }

    /// Get recent performance metrics within the evaluation window
    async fn get_recent_metrics(&self) -> Result<Vec<PerformanceMetric>> {
        let now = Instant::now();
        let window_duration =
            Duration::from_secs(self.health_check_config.evaluation_window_seconds);
        let cutoff_time = now - window_duration;

        let history = self.performance_history.read().await;
        let recent_metrics: Vec<PerformanceMetric> = history
            .iter()
            .filter(|metric| metric.timestamp >= cutoff_time)
            .cloned()
            .collect();
        Ok(recent_metrics)
    }

    /// Get executor ID
    pub fn executor_id(&self) -> Uuid {
        self.executor_id
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        Instant::now().duration_since(self.started_at).as_secs()
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Convert Instant to Unix timestamp
fn timestamp_from_instant(_instant: Instant) -> u64 {
    // This is approximate since Instant doesn't have a fixed epoch
    // In practice, this would need to be tracked differently
    current_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_health_states() {
        let healthy = ExecutorHealth::Healthy {
            last_heartbeat: current_timestamp(),
            items_processed: 100,
            avg_processing_time_ms: 50.0,
            uptime_seconds: 3600,
        };

        assert!(healthy.is_operational());
        assert!(!healthy.needs_attention());
        assert_eq!(healthy.severity_level(), 0);

        let degraded = ExecutorHealth::Degraded {
            last_heartbeat: current_timestamp(),
            reason: "High error rate".to_string(),
            error_rate: 0.15,
            degraded_for_seconds: 60,
        };

        assert!(degraded.is_operational());
        assert!(degraded.needs_attention());
        assert!(degraded.severity_level() > 0);
    }

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let executor_id = Uuid::new_v4();
        let monitor = HealthMonitor::new(executor_id);

        assert_eq!(monitor.executor_id(), executor_id);

        let health = monitor.current_health().await.unwrap();
        match health {
            ExecutorHealth::Starting { .. } => (),
            _ => panic!("Expected Starting health state"),
        }
    }

    #[tokio::test]
    async fn test_heartbeat_recording() {
        let executor_id = Uuid::new_v4();
        let monitor = HealthMonitor::new(executor_id);

        // Record a heartbeat
        assert!(monitor.heartbeat().await.is_ok());

        // Record some performance metrics
        assert!(monitor.record_performance(100, true, 5).await.is_ok());
        assert!(monitor.record_performance(150, true, 3).await.is_ok());
        assert!(monitor.record_performance(200, false, 2).await.is_ok());
    }

    #[tokio::test]
    async fn test_phase_transitions() {
        let executor_id = Uuid::new_v4();
        let monitor = HealthMonitor::new(executor_id);

        // Mark as starting
        assert!(monitor.mark_starting("connecting").await.is_ok());
        let health = monitor.current_health().await.unwrap();
        match health {
            ExecutorHealth::Starting { phase, .. } => {
                assert_eq!(phase, "connecting");
            }
            _ => panic!("Expected Starting health state"),
        }

        // Mark as stopping
        assert!(monitor.mark_stopping(true, "shutdown").await.is_ok());
        let health = monitor.current_health().await.unwrap();
        match health {
            ExecutorHealth::Stopping {
                graceful, phase, ..
            } => {
                assert!(graceful);
                assert_eq!(phase, "shutdown");
            }
            _ => panic!("Expected Stopping health state"),
        }
    }

    #[test]
    fn test_health_descriptions() {
        let healthy = ExecutorHealth::Healthy {
            last_heartbeat: current_timestamp(),
            items_processed: 42,
            avg_processing_time_ms: 123.5,
            uptime_seconds: 3600,
        };

        let description = healthy.description();
        assert!(description.contains("42 items"));
        assert!(description.contains("123.5ms"));
    }
}
