//! # Health Monitoring
//!
//! Provides health monitoring and metrics aggregation for the orchestration system.
//! Tracks executor health, system performance, and provides alerting capabilities.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::{Result, TaskerError};

use super::pool::HealthReport;

/// Health monitoring system for orchestration coordinators
#[derive(Debug, Clone)]
pub struct HealthMonitor {
    /// Unique identifier for this monitor
    id: Uuid,
    /// Health check interval in seconds
    health_check_interval_seconds: u64,
    /// Maximum database pool usage threshold (0.0-1.0)
    max_db_pool_usage: f64,
    /// History of health reports
    health_history: Arc<Mutex<Vec<TimestampedHealthReport>>>,
    /// Alert thresholds
    alert_thresholds: Arc<Mutex<AlertThresholds>>,
    /// Current system status
    system_status: Arc<Mutex<SystemStatus>>,
    /// Time when system became degraded (for persistent tracking)
    degraded_since: Arc<Mutex<Option<Instant>>>,
    /// Time when system became unhealthy (for persistent tracking)
    unhealthy_since: Arc<Mutex<Option<Instant>>>,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(
        coordinator_id: Uuid,
        health_check_interval_seconds: u64,
        max_db_pool_usage: f64,
    ) -> Self {
        info!(
            "🏥 HEALTH: Creating health monitor for coordinator {} (interval: {}s, max DB usage: {:.1}%)",
            coordinator_id,
            health_check_interval_seconds,
            max_db_pool_usage * 100.0
        );

        Self {
            id: coordinator_id,
            health_check_interval_seconds,
            max_db_pool_usage,
            health_history: Arc::new(Mutex::new(Vec::new())),
            alert_thresholds: Arc::new(Mutex::new(AlertThresholds::default())),
            system_status: Arc::new(Mutex::new(SystemStatus::Starting)),
            degraded_since: Arc::new(Mutex::new(None)),
            unhealthy_since: Arc::new(Mutex::new(None)),
        }
    }

    /// Record a health report from the pool manager
    pub async fn record_health_report(&self, report: HealthReport) -> Result<()> {
        let timestamp = Instant::now();

        debug!(
            "HEALTH[{}]: Recording health report - {} pools, {} executors ({} healthy, {} unhealthy)",
            self.id,
            report.total_pools,
            report.total_executors,
            report.healthy_executors,
            report.unhealthy_executors
        );

        // Calculate health metrics
        let health_percentage = if report.total_executors > 0 {
            (report.healthy_executors as f64 / report.total_executors as f64) * 100.0
        } else {
            0.0
        };

        // Update system status based on health
        let new_status = self.calculate_system_status(&report).await?;
        {
            let mut status = self.system_status.lock().await;
            *status = new_status.clone();
        }

        // Check for alerts
        self.check_alerts(&report, health_percentage).await?;

        // Store the timestamped report
        let timestamped_report = TimestampedHealthReport {
            timestamp,
            report,
            health_percentage,
            system_status: new_status,
        };

        {
            let mut history = self.health_history.lock().await;
            history.push(timestamped_report);

            // Keep only recent history (last 100 reports)
            if history.len() > 100 {
                history.remove(0);
            }
        }

        Ok(())
    }

    /// Calculate overall system status
    async fn calculate_system_status(&self, report: &HealthReport) -> Result<SystemStatus> {
        let health_percentage = if report.total_executors > 0 {
            (report.healthy_executors as f64 / report.total_executors as f64) * 100.0
        } else {
            return Ok(SystemStatus::Starting);
        };

        // System is unhealthy if more than 50% of executors are unhealthy
        if health_percentage < 50.0 {
            // Track when unhealthy state began
            let unhealthy_since = {
                let mut unhealthy_guard = self.unhealthy_since.lock().await;
                if unhealthy_guard.is_none() {
                    *unhealthy_guard = Some(Instant::now());
                    warn!(
                        "🚨 HEALTH: System becoming unhealthy - {:.1}% healthy",
                        health_percentage
                    );
                }
                unhealthy_guard.unwrap()
            };

            // Clear degraded time since we're now unhealthy
            *self.degraded_since.lock().await = None;

            return Ok(SystemStatus::Unhealthy {
                reason: format!("Only {health_percentage:.1}% of executors are healthy"),
                unhealthy_since,
            });
        }

        // System is degraded if 10-50% of executors are unhealthy
        if health_percentage < 90.0 {
            // Track when degraded state began
            let degraded_since = {
                let mut degraded_guard = self.degraded_since.lock().await;
                if degraded_guard.is_none() {
                    *degraded_guard = Some(Instant::now());
                    warn!(
                        "⚠️ HEALTH: System becoming degraded - {:.1}% healthy",
                        health_percentage
                    );
                }
                degraded_guard.unwrap()
            };

            // Clear unhealthy time since we're recovering
            *self.unhealthy_since.lock().await = None;

            return Ok(SystemStatus::Degraded {
                reason: format!("{health_percentage:.1}% of executors are healthy"),
                degraded_since,
            });
        }

        // Check for any completely failed pools
        for (executor_type, pool_status) in &report.pool_statuses {
            if pool_status.healthy_executors == 0 && pool_status.active_executors > 0 {
                return Ok(SystemStatus::Degraded {
                    reason: format!("All {} executors are unhealthy", executor_type.name()),
                    degraded_since: Instant::now(),
                });
            }
        }

        // System is healthy - clear any degraded/unhealthy timestamps
        *self.degraded_since.lock().await = None;
        *self.unhealthy_since.lock().await = None;

        Ok(SystemStatus::Healthy)
    }

    /// Check for alert conditions
    async fn check_alerts(&self, report: &HealthReport, health_percentage: f64) -> Result<()> {
        let alert_thresholds = self.alert_thresholds.lock().await;

        // Low health percentage alert
        if health_percentage < alert_thresholds.min_health_percentage {
            warn!(
                "🚨 HEALTH ALERT: System health is {:.1}% (threshold: {:.1}%)",
                health_percentage, alert_thresholds.min_health_percentage
            );
        }

        // Check for pools with no healthy executors
        for (executor_type, pool_status) in &report.pool_statuses {
            if pool_status.healthy_executors == 0 && pool_status.active_executors > 0 {
                error!(
                    "🚨 HEALTH ALERT: {} pool has no healthy executors ({} total)",
                    executor_type.name(),
                    pool_status.active_executors
                );
            }

            // Check for high error rates (calculated from pool metrics)
            let error_rate = if pool_status.total_items_processed > 0 {
                pool_status.total_items_failed as f64 / pool_status.total_items_processed as f64
            } else {
                0.0
            };

            if error_rate > alert_thresholds.max_error_rate {
                warn!(
                    "🚨 HEALTH ALERT: {} pool has high error rate {:.1}% (threshold: {:.1}%)",
                    executor_type.name(),
                    error_rate * 100.0,
                    alert_thresholds.max_error_rate * 100.0
                );
            }
        }

        Ok(())
    }

    /// Get current system status
    pub async fn get_system_status(&self) -> SystemStatus {
        self.system_status.lock().await.clone()
    }

    /// Get health history
    pub async fn get_health_history(&self) -> Vec<TimestampedHealthReport> {
        self.health_history.lock().await.clone()
    }

    /// Get health summary for the last N reports
    pub async fn get_health_summary(&self, last_n_reports: usize) -> HealthSummary {
        let history = self.health_history.lock().await;
        let recent_reports: Vec<_> = history.iter().rev().take(last_n_reports).collect();

        if recent_reports.is_empty() {
            return HealthSummary {
                report_count: 0,
                average_health_percentage: 0.0,
                min_health_percentage: 0.0,
                max_health_percentage: 0.0,
                healthy_reports: 0,
                degraded_reports: 0,
                unhealthy_reports: 0,
                time_span_seconds: 0.0,
            };
        }

        let total_health: f64 = recent_reports.iter().map(|r| r.health_percentage).sum();
        let average_health = total_health / recent_reports.len() as f64;

        let min_health = recent_reports
            .iter()
            .map(|r| r.health_percentage)
            .fold(f64::INFINITY, f64::min);

        let max_health = recent_reports
            .iter()
            .map(|r| r.health_percentage)
            .fold(f64::NEG_INFINITY, f64::max);

        let mut healthy_reports = 0;
        let mut degraded_reports = 0;
        let mut unhealthy_reports = 0;

        for report in &recent_reports {
            match report.system_status {
                SystemStatus::Healthy => healthy_reports += 1,
                SystemStatus::Degraded { .. } => degraded_reports += 1,
                SystemStatus::Unhealthy { .. } => unhealthy_reports += 1,
                SystemStatus::Starting => {} // Don't count starting reports
            }
        }

        let time_span = if recent_reports.len() > 1 {
            let oldest = recent_reports.last().unwrap().timestamp;
            let newest = recent_reports.first().unwrap().timestamp;
            newest.duration_since(oldest).as_secs_f64()
        } else {
            0.0
        };

        HealthSummary {
            report_count: recent_reports.len(),
            average_health_percentage: average_health,
            min_health_percentage: min_health,
            max_health_percentage: max_health,
            healthy_reports,
            degraded_reports,
            unhealthy_reports,
            time_span_seconds: time_span,
        }
    }

    /// Update alert thresholds
    pub async fn update_alert_thresholds(&self, thresholds: AlertThresholds) -> Result<()> {
        // Validate thresholds
        if thresholds.min_health_percentage < 0.0 || thresholds.min_health_percentage > 100.0 {
            return Err(TaskerError::InvalidParameter(
                "Min health percentage must be between 0 and 100".to_string(),
            ));
        }

        if thresholds.max_error_rate < 0.0 || thresholds.max_error_rate > 1.0 {
            return Err(TaskerError::InvalidParameter(
                "Max error rate must be between 0 and 1".to_string(),
            ));
        }

        *self.alert_thresholds.lock().await = thresholds;
        info!("HEALTH: Updated alert thresholds");
        Ok(())
    }

    /// Get current alert thresholds
    pub async fn get_alert_thresholds(&self) -> AlertThresholds {
        self.alert_thresholds.lock().await.clone()
    }

    /// Get the monitor ID for external logging and tracking
    pub fn monitor_id(&self) -> Uuid {
        self.id
    }

    /// Get the configured health check interval in seconds
    pub fn health_check_interval_seconds(&self) -> u64 {
        self.health_check_interval_seconds
    }

    /// Check if enough time has passed since last health check to perform another
    pub fn should_check_health(&self, last_check_time: std::time::Instant) -> bool {
        let elapsed = last_check_time.elapsed().as_secs();
        elapsed >= self.health_check_interval_seconds
    }

    /// Check if the database pool usage is within acceptable limits
    /// Returns (is_healthy, current_usage_ratio) where usage_ratio is 0.0-1.0
    pub fn check_database_pool_usage(
        &self,
        active_connections: u32,
        max_connections: u32,
    ) -> (bool, f64) {
        if max_connections == 0 {
            return (true, 0.0); // No pool configured, assume healthy
        }

        let current_usage = active_connections as f64 / max_connections as f64;
        let is_healthy = current_usage <= self.max_db_pool_usage;

        if !is_healthy {
            warn!(
                "HEALTH[{}]: Database pool usage {:.1}% exceeds threshold {:.1}%",
                self.id,
                current_usage * 100.0,
                self.max_db_pool_usage * 100.0
            );
        }

        (is_healthy, current_usage)
    }

    /// Get the maximum database pool usage threshold
    pub fn max_db_pool_usage_threshold(&self) -> f64 {
        self.max_db_pool_usage
    }
}

/// System status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum SystemStatus {
    /// System is starting up
    Starting,
    /// System is healthy (>90% executors healthy)
    Healthy,
    /// System is degraded (50-90% executors healthy)
    Degraded {
        reason: String,
        degraded_since: Instant,
    },
    /// System is unhealthy (<50% executors healthy)
    Unhealthy {
        reason: String,
        unhealthy_since: Instant,
    },
}

/// Health report with timestamp
#[derive(Debug, Clone)]
pub struct TimestampedHealthReport {
    pub timestamp: Instant,
    pub report: HealthReport,
    pub health_percentage: f64,
    pub system_status: SystemStatus,
}

/// Alert thresholds configuration
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    /// Minimum acceptable health percentage (0-100)
    pub min_health_percentage: f64,
    /// Maximum acceptable error rate (0.0-1.0)
    pub max_error_rate: f64,
    /// Maximum acceptable response time (milliseconds)
    pub max_response_time_ms: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            min_health_percentage: 80.0, // Alert if less than 80% healthy
            max_error_rate: 0.05,        // Alert if more than 5% error rate
            max_response_time_ms: 5000,  // Alert if response time > 5 seconds
        }
    }
}

/// Health summary statistics
#[derive(Debug, Clone)]
pub struct HealthSummary {
    pub report_count: usize,
    pub average_health_percentage: f64,
    pub min_health_percentage: f64,
    pub max_health_percentage: f64,
    pub healthy_reports: usize,
    pub degraded_reports: usize,
    pub unhealthy_reports: usize,
    pub time_span_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::coordinator::pool::PoolStatus;
    use crate::orchestration::executor::traits::ExecutorType;
    use std::collections::HashMap;

    fn create_test_health_report(healthy_executors: usize, total_executors: usize) -> HealthReport {
        let mut pool_statuses = HashMap::new();

        // Create a mock pool status
        pool_statuses.insert(
            ExecutorType::TaskRequestProcessor,
            PoolStatus {
                executor_type: ExecutorType::TaskRequestProcessor,
                active_executors: total_executors,
                healthy_executors,
                unhealthy_executors: total_executors - healthy_executors,
                min_executors: 1,
                max_executors: 10,
                total_items_processed: 1000,
                total_items_failed: 10,
                average_utilization: 0.75,
                uptime_seconds: 3600,
            },
        );

        HealthReport {
            total_pools: 1,
            total_executors,
            healthy_executors,
            unhealthy_executors: total_executors - healthy_executors,
            pool_statuses,
        }
    }

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);
        assert!(matches!(
            monitor.get_system_status().await,
            SystemStatus::Starting
        ));
    }

    #[tokio::test]
    async fn test_healthy_system_status() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);
        let report = create_test_health_report(9, 10); // 90% healthy

        monitor.record_health_report(report).await.unwrap();
        assert!(matches!(
            monitor.get_system_status().await,
            SystemStatus::Healthy
        ));
    }

    #[tokio::test]
    async fn test_degraded_system_status() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);
        let report = create_test_health_report(7, 10); // 70% healthy

        monitor.record_health_report(report).await.unwrap();
        assert!(matches!(
            monitor.get_system_status().await,
            SystemStatus::Degraded { .. }
        ));
    }

    #[tokio::test]
    async fn test_unhealthy_system_status() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);
        let report = create_test_health_report(3, 10); // 30% healthy

        monitor.record_health_report(report).await.unwrap();
        assert!(matches!(
            monitor.get_system_status().await,
            SystemStatus::Unhealthy { .. }
        ));
    }

    #[tokio::test]
    async fn test_health_history() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);

        // Record multiple reports
        for i in 1..=5 {
            let report = create_test_health_report(i * 2, 10);
            monitor.record_health_report(report).await.unwrap();
        }

        assert_eq!(monitor.get_health_history().await.len(), 5);
    }

    #[tokio::test]
    async fn test_health_summary() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);

        // Record reports with varying health
        let reports = vec![
            create_test_health_report(10, 10), // 100% healthy
            create_test_health_report(8, 10),  // 80% healthy
            create_test_health_report(6, 10),  // 60% healthy
        ];

        for report in reports {
            monitor.record_health_report(report).await.unwrap();
        }

        let summary = monitor.get_health_summary(3).await;
        assert_eq!(summary.report_count, 3);
        assert_eq!(summary.average_health_percentage, 80.0); // (100 + 80 + 60) / 3
        assert_eq!(summary.min_health_percentage, 60.0);
        assert_eq!(summary.max_health_percentage, 100.0);
    }

    #[tokio::test]
    async fn test_health_check_interval_methods() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);

        // Verify getter returns correct interval
        assert_eq!(monitor.health_check_interval_seconds(), 30);

        // Test should_check_health logic
        let now = std::time::Instant::now();

        // Should not check immediately
        assert!(!monitor.should_check_health(now));

        // Simulate time passage (in a real scenario this would be actual time)
        let old_time = now - std::time::Duration::from_secs(31);
        assert!(monitor.should_check_health(old_time));

        let recent_time = now - std::time::Duration::from_secs(15);
        assert!(!monitor.should_check_health(recent_time));
    }

    #[tokio::test]
    async fn test_database_pool_usage_monitoring() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8); // 80% threshold

        // Test healthy usage (within threshold)
        let (is_healthy, usage) = monitor.check_database_pool_usage(4, 10);
        assert!(is_healthy);
        assert_eq!(usage, 0.4);

        // Test usage exactly at threshold
        let (is_healthy, usage) = monitor.check_database_pool_usage(8, 10);
        assert!(is_healthy);
        assert_eq!(usage, 0.8);

        // Test unhealthy usage (above threshold)
        let (is_healthy, usage) = monitor.check_database_pool_usage(9, 10);
        assert!(!is_healthy);
        assert_eq!(usage, 0.9);

        // Test edge case - no pool configured
        let (is_healthy, usage) = monitor.check_database_pool_usage(5, 0);
        assert!(is_healthy);
        assert_eq!(usage, 0.0);
    }

    #[test]
    fn test_monitor_id_and_thresholds() {
        let monitor_id = Uuid::new_v4();
        let monitor = HealthMonitor::new(monitor_id, 45, 0.75);

        // Verify ID is accessible
        assert_eq!(monitor.monitor_id(), monitor_id);

        // Verify thresholds are accessible
        assert_eq!(monitor.health_check_interval_seconds(), 45);
        assert_eq!(monitor.max_db_pool_usage_threshold(), 0.75);
    }
}
