//! # Health Monitoring
//!
//! Provides health monitoring and metrics aggregation for the orchestration system.
//! Tracks executor health, system performance, and provides alerting capabilities.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use tasker_shared::{TaskerError, TaskerResult};

use super::operational_state::{OperationalStateManager, SystemOperationalState};
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

    /// Record a health report from the pool manager (TAS-37 Supplemental: enhanced for operational state awareness)
    pub async fn record_health_report(&self, report: HealthReport) -> TaskerResult<()> {
        self.record_health_report_with_operational_state(report, None)
            .await
    }

    /// Record a health report with operational state context (TAS-37 Supplemental)
    ///
    /// This method provides operational state awareness for context-sensitive health monitoring.
    /// During graceful shutdown, health degradation is expected and alerts can be suppressed.
    pub async fn record_health_report_with_operational_state(
        &self,
        report: HealthReport,
        operational_state: Option<&OperationalStateManager>,
    ) -> TaskerResult<()> {
        self.record_health_report_with_config(report, operational_state, None)
            .await
    }

    /// Record a health report with operational state and configuration context (TAS-37 Supplemental)
    ///
    /// This method provides full configuration-aware health monitoring by using actual
    /// configuration values for threshold multipliers instead of hardcoded defaults.
    pub async fn record_health_report_with_config(
        &self,
        report: HealthReport,
        operational_state: Option<&OperationalStateManager>,
        config: Option<&tasker_shared::config::OperationalStateConfig>,
    ) -> TaskerResult<()> {
        let timestamp = Instant::now();

        // TAS-37 Supplemental: Get operational state context for context-aware monitoring
        let (current_state, should_suppress_alerts, health_threshold_multiplier) =
            if let Some(op_state) = operational_state {
                let state = op_state.current_state().await;
                let suppress = op_state.should_suppress_alerts().await;

                // Use configuration-aware threshold multiplier if config is provided
                let multiplier = if let Some(operational_config) = config {
                    op_state
                        .health_threshold_multiplier_with_config(operational_config)
                        .await
                } else {
                    op_state.health_threshold_multiplier().await
                };

                (Some(state), suppress, multiplier)
            } else {
                (None, false, 1.0)
            };

        debug!(
            "HEALTH[{}]: Recording health report - {} pools, {} executors ({} healthy, {} unhealthy) | operational_state={:?}, suppress_alerts={}, threshold_multiplier={}",
            self.id,
            report.total_pools,
            report.total_executors,
            report.healthy_executors,
            report.unhealthy_executors,
            current_state,
            should_suppress_alerts,
            health_threshold_multiplier
        );

        // Calculate health metrics
        let health_percentage = if report.total_executors > 0 {
            (report.healthy_executors as f64 / report.total_executors as f64) * 100.0
        } else {
            0.0
        };

        // Update system status based on health with operational context
        let new_status = self
            .calculate_system_status_with_context(
                &report,
                current_state.as_ref(),
                health_threshold_multiplier,
            )
            .await?;
        {
            let mut status = self.system_status.lock().await;
            *status = new_status.clone();
        }

        // Check for alerts with operational state awareness
        if !should_suppress_alerts {
            self.check_alerts_with_context(
                &report,
                health_percentage,
                current_state.as_ref(),
                health_threshold_multiplier,
            )
            .await?;
        } else {
            debug!(
                "HEALTH[{}]: Alerts suppressed due to operational state: {:?}",
                self.id, current_state
            );
        }

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

    /// Calculate overall system status (backward compatibility)
    #[allow(dead_code)]
    async fn calculate_system_status(&self, report: &HealthReport) -> TaskerResult<SystemStatus> {
        self.calculate_system_status_with_context(report, None, 1.0)
            .await
    }

    /// Calculate overall system status with operational state context (TAS-37 Supplemental)
    async fn calculate_system_status_with_context(
        &self,
        report: &HealthReport,
        operational_state: Option<&SystemOperationalState>,
        health_threshold_multiplier: f64,
    ) -> TaskerResult<SystemStatus> {
        let health_percentage = if report.total_executors > 0 {
            (report.healthy_executors as f64 / report.total_executors as f64) * 100.0
        } else {
            return Ok(SystemStatus::Starting);
        };

        // TAS-37 Supplemental: Adjust thresholds based on operational state
        // During startup or graceful shutdown, use relaxed thresholds
        let unhealthy_threshold = 50.0 * health_threshold_multiplier;
        let degraded_threshold = 90.0 * health_threshold_multiplier;

        // Log context when using adjusted thresholds
        if let Some(state) = operational_state {
            if health_threshold_multiplier != 1.0 {
                debug!(
                    "HEALTH[{}]: Using adjusted thresholds for {:?} - unhealthy: {:.1}%, degraded: {:.1}%",
                    self.id,
                    state,
                    unhealthy_threshold,
                    degraded_threshold
                );
            }
        }

        // System is unhealthy if below the adjusted unhealthy threshold
        if health_percentage < unhealthy_threshold {
            // Track when unhealthy state began
            let unhealthy_since = {
                let mut unhealthy_guard = self.unhealthy_since.lock().await;
                if unhealthy_guard.is_none() {
                    *unhealthy_guard = Some(Instant::now());
                    // TAS-37 Supplemental: Context-aware logging based on operational state
                    match operational_state {
                        Some(SystemOperationalState::GracefulShutdown) => {
                            info!(
                                "ℹ️ HEALTH: System health degrading during graceful shutdown - {:.1}% healthy (expected during shutdown)",
                                health_percentage
                            );
                        }
                        Some(SystemOperationalState::Startup) => {
                            info!(
                                "ℹ️ HEALTH: System health low during startup - {:.1}% healthy (expected during initialization)",
                                health_percentage
                            );
                        }
                        _ => {
                            warn!(
                                "🚨 HEALTH: System becoming unhealthy - {:.1}% healthy",
                                health_percentage
                            );
                        }
                    }
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

        // System is degraded if below the adjusted degraded threshold
        if health_percentage < degraded_threshold {
            // Track when degraded state began
            let degraded_since = {
                let mut degraded_guard = self.degraded_since.lock().await;
                if degraded_guard.is_none() {
                    *degraded_guard = Some(Instant::now());
                    // TAS-37 Supplemental: Context-aware logging for degraded state
                    match operational_state {
                        Some(SystemOperationalState::GracefulShutdown) => {
                            debug!(
                                "ℹ️ HEALTH: System performance degrading during graceful shutdown - {:.1}% healthy (expected)",
                                health_percentage
                            );
                        }
                        Some(SystemOperationalState::Startup) => {
                            debug!(
                                "ℹ️ HEALTH: System performance degraded during startup - {:.1}% healthy (expected during initialization)",
                                health_percentage
                            );
                        }
                        _ => {
                            warn!(
                                "⚠️ HEALTH: System becoming degraded - {:.1}% healthy",
                                health_percentage
                            );
                        }
                    }
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

    /// Check for alert conditions (backward compatibility)
    #[allow(dead_code)]
    async fn check_alerts(&self, report: &HealthReport, health_percentage: f64) -> TaskerResult<()> {
        self.check_alerts_with_context(report, health_percentage, None, 1.0)
            .await
    }

    /// Check for alert conditions with operational state context (TAS-37 Supplemental)
    async fn check_alerts_with_context(
        &self,
        report: &HealthReport,
        health_percentage: f64,
        operational_state: Option<&SystemOperationalState>,
        health_threshold_multiplier: f64,
    ) -> TaskerResult<()> {
        let alert_thresholds = self.alert_thresholds.lock().await;

        // TAS-37 Supplemental: Adjust alert thresholds based on operational state
        let adjusted_min_health =
            alert_thresholds.min_health_percentage * health_threshold_multiplier;
        let adjusted_max_error_rate =
            alert_thresholds.max_error_rate / health_threshold_multiplier.max(0.1); // Prevent division by zero

        // Low health percentage alert with context-aware messaging
        if health_percentage < adjusted_min_health {
            match operational_state {
                Some(SystemOperationalState::GracefulShutdown) => {
                    debug!(
                        "ℹ️ HEALTH: Low health during graceful shutdown - {:.1}% (adjusted threshold: {:.1}%) - expected during shutdown",
                        health_percentage, adjusted_min_health
                    );
                }
                Some(SystemOperationalState::Startup) => {
                    debug!(
                        "ℹ️ HEALTH: Low health during startup - {:.1}% (adjusted threshold: {:.1}%) - expected during initialization",
                        health_percentage, adjusted_min_health
                    );
                }
                _ => {
                    warn!(
                        "🚨 HEALTH ALERT: System health is {:.1}% (threshold: {:.1}%)",
                        health_percentage, adjusted_min_health
                    );
                }
            }
        }

        // Check for pools with no healthy executors with context-aware messaging
        for (executor_type, pool_status) in &report.pool_statuses {
            if pool_status.healthy_executors == 0 && pool_status.active_executors > 0 {
                match operational_state {
                    Some(SystemOperationalState::GracefulShutdown) => {
                        info!(
                            "ℹ️ HEALTH: {} pool has no healthy executors during graceful shutdown ({} total) - expected during shutdown",
                            executor_type.name(),
                            pool_status.active_executors
                        );
                    }
                    Some(SystemOperationalState::Emergency) => {
                        error!(
                            "🚨 HEALTH ALERT: {} pool has no healthy executors during emergency shutdown ({} total)",
                            executor_type.name(),
                            pool_status.active_executors
                        );
                    }
                    _ => {
                        error!(
                            "🚨 HEALTH ALERT: {} pool has no healthy executors ({} total)",
                            executor_type.name(),
                            pool_status.active_executors
                        );
                    }
                }
            }

            // Check for high error rates with context-aware thresholds and messaging
            let error_rate = if pool_status.total_items_processed > 0 {
                pool_status.total_items_failed as f64 / pool_status.total_items_processed as f64
            } else {
                0.0
            };

            if error_rate > adjusted_max_error_rate {
                match operational_state {
                    Some(SystemOperationalState::GracefulShutdown) => {
                        debug!(
                            "ℹ️ HEALTH: {} pool has elevated error rate during graceful shutdown {:.1}% (adjusted threshold: {:.1}%) - may be expected",
                            executor_type.name(),
                            error_rate * 100.0,
                            adjusted_max_error_rate * 100.0
                        );
                    }
                    Some(SystemOperationalState::Startup) => {
                        debug!(
                            "ℹ️ HEALTH: {} pool has elevated error rate during startup {:.1}% (adjusted threshold: {:.1}%) - may be expected during initialization",
                            executor_type.name(),
                            error_rate * 100.0,
                            adjusted_max_error_rate * 100.0
                        );
                    }
                    _ => {
                        warn!(
                            "🚨 HEALTH ALERT: {} pool has high error rate {:.1}% (threshold: {:.1}%)",
                            executor_type.name(),
                            error_rate * 100.0,
                            adjusted_max_error_rate * 100.0
                        );
                    }
                }
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
    pub async fn update_alert_thresholds(&self, thresholds: AlertThresholds) -> TaskerResult<()> {
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

    /// Check web API database pool usage with configurable thresholds (TAS-37 Web Integration)
    ///
    /// This method provides web API pool monitoring with custom thresholds from web configuration.
    /// It checks against both warning and critical thresholds for graduated alerting.
    pub fn check_web_database_pool_usage(
        &self,
        active_connections: u32,
        max_connections: u32,
        warning_threshold: f64,
        critical_threshold: f64,
    ) -> WebPoolHealthStatus {
        if max_connections == 0 {
            return WebPoolHealthStatus::Healthy {
                usage_ratio: 0.0,
                active_connections: 0,
                max_connections: 0,
            };
        }

        let current_usage = active_connections as f64 / max_connections as f64;

        if current_usage >= critical_threshold {
            warn!(
                "HEALTH[{}]: Web API database pool CRITICAL usage {:.1}% (threshold: {:.1}%) - {} of {} connections",
                self.id,
                current_usage * 100.0,
                critical_threshold * 100.0,
                active_connections,
                max_connections
            );
            WebPoolHealthStatus::Critical {
                usage_ratio: current_usage,
                active_connections,
                max_connections,
                threshold_exceeded: critical_threshold,
            }
        } else if current_usage >= warning_threshold {
            warn!(
                "HEALTH[{}]: Web API database pool WARNING usage {:.1}% (threshold: {:.1}%) - {} of {} connections",
                self.id,
                current_usage * 100.0,
                warning_threshold * 100.0,
                active_connections,
                max_connections
            );
            WebPoolHealthStatus::Warning {
                usage_ratio: current_usage,
                active_connections,
                max_connections,
                threshold_exceeded: warning_threshold,
            }
        } else {
            debug!(
                "HEALTH[{}]: Web API database pool healthy usage {:.1}% - {} of {} connections",
                self.id,
                current_usage * 100.0,
                active_connections,
                max_connections
            );
            WebPoolHealthStatus::Healthy {
                usage_ratio: current_usage,
                active_connections,
                max_connections,
            }
        }
    }

    /// Record web API pool usage metrics for health monitoring (TAS-37 Web Integration)
    ///
    /// This method integrates web API pool monitoring into the broader health monitoring system.
    /// It can be called periodically by the web API to report pool usage statistics.
    pub async fn record_web_pool_usage(
        &self,
        pool_usage: WebPoolUsageReport,
        operational_state: Option<&OperationalStateManager>,
    ) -> TaskerResult<()> {
        let _timestamp = Instant::now();

        // Get operational state context for monitoring decisions
        let (current_state, should_suppress_alerts) = if let Some(op_state) = operational_state {
            let state = op_state.current_state().await;
            let suppress = op_state.should_suppress_alerts().await;
            (Some(state), suppress)
        } else {
            (None, false)
        };

        debug!(
            "HEALTH[{}]: Recording web pool usage - {:.1}% utilization ({} active, {} max) | operational_state={:?}, suppress_alerts={}",
            self.id,
            pool_usage.usage_ratio * 100.0,
            pool_usage.active_connections,
            pool_usage.max_connections,
            current_state,
            should_suppress_alerts
        );

        // Check web pool health status with custom thresholds
        let health_status = self.check_web_database_pool_usage(
            pool_usage.active_connections,
            pool_usage.max_connections,
            pool_usage.warning_threshold,
            pool_usage.critical_threshold,
        );

        // Generate alerts if not suppressed and pool is unhealthy
        if !should_suppress_alerts {
            match &health_status {
                WebPoolHealthStatus::Warning {
                    usage_ratio,
                    threshold_exceeded,
                    ..
                } => match current_state.as_ref() {
                    Some(SystemOperationalState::GracefulShutdown) => {
                        debug!(
                                "ℹ️ HEALTH: Web API pool elevated usage during graceful shutdown - {:.1}% (warning threshold: {:.1}%) - expected during shutdown",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                    Some(SystemOperationalState::Startup) => {
                        debug!(
                                "ℹ️ HEALTH: Web API pool elevated usage during startup - {:.1}% (warning threshold: {:.1}%) - expected during initialization",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                    _ => {
                        warn!(
                                "⚠️ HEALTH ALERT: Web API database pool usage HIGH - {:.1}% (warning threshold: {:.1}%)",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                },
                WebPoolHealthStatus::Critical {
                    usage_ratio,
                    threshold_exceeded,
                    ..
                } => match current_state.as_ref() {
                    Some(SystemOperationalState::GracefulShutdown) => {
                        info!(
                                "ℹ️ HEALTH: Web API pool critical usage during graceful shutdown - {:.1}% (critical threshold: {:.1}%) - may be expected during shutdown",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                    Some(SystemOperationalState::Emergency) => {
                        error!(
                                "🚨 HEALTH ALERT: Web API pool CRITICAL usage during emergency shutdown - {:.1}% (critical threshold: {:.1}%)",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                    _ => {
                        error!(
                                "🚨 HEALTH ALERT: Web API database pool usage CRITICAL - {:.1}% (critical threshold: {:.1}%)",
                                usage_ratio * 100.0,
                                threshold_exceeded * 100.0
                            );
                    }
                },
                WebPoolHealthStatus::Healthy { .. } => {
                    // No alert needed for healthy status
                }
            }
        } else {
            debug!(
                "HEALTH[{}]: Web pool usage alerts suppressed due to operational state: {:?}",
                self.id, current_state
            );
        }

        // TODO: Store web pool usage history for trend analysis
        // This could be implemented similarly to the main health_history Vec
        // For now, we just log and alert

        Ok(())
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

/// Web API database pool health status (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub enum WebPoolHealthStatus {
    /// Pool usage is within healthy limits
    Healthy {
        usage_ratio: f64,
        active_connections: u32,
        max_connections: u32,
    },
    /// Pool usage exceeds warning threshold but below critical
    Warning {
        usage_ratio: f64,
        active_connections: u32,
        max_connections: u32,
        threshold_exceeded: f64,
    },
    /// Pool usage exceeds critical threshold
    Critical {
        usage_ratio: f64,
        active_connections: u32,
        max_connections: u32,
        threshold_exceeded: f64,
    },
}

/// Web API pool usage report for health monitoring (TAS-37 Web Integration)
#[derive(Debug, Clone)]
pub struct WebPoolUsageReport {
    /// Current connection usage ratio (0.0-1.0)
    pub usage_ratio: f64,
    /// Number of active connections
    pub active_connections: u32,
    /// Maximum allowed connections
    pub max_connections: u32,
    /// Warning threshold from web configuration (0.0-1.0)
    pub warning_threshold: f64,
    /// Critical threshold from web configuration (0.0-1.0)
    pub critical_threshold: f64,
    /// Pool name for identification
    pub pool_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::coordinator::pool::PoolStatus;
    use tasker_shared::config::orchestration::executor::ExecutorType;
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

    #[test]
    fn test_web_database_pool_usage_monitoring() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);

        // Test healthy usage (below warning threshold)
        let status = monitor.check_web_database_pool_usage(3, 10, 0.5, 0.8);
        assert!(matches!(status, WebPoolHealthStatus::Healthy { .. }));
        if let WebPoolHealthStatus::Healthy {
            usage_ratio,
            active_connections,
            max_connections,
        } = status
        {
            assert_eq!(usage_ratio, 0.3);
            assert_eq!(active_connections, 3);
            assert_eq!(max_connections, 10);
        }

        // Test warning usage (above warning, below critical threshold)
        let status = monitor.check_web_database_pool_usage(6, 10, 0.5, 0.8);
        assert!(matches!(status, WebPoolHealthStatus::Warning { .. }));
        if let WebPoolHealthStatus::Warning {
            usage_ratio,
            threshold_exceeded,
            ..
        } = status
        {
            assert_eq!(usage_ratio, 0.6);
            assert_eq!(threshold_exceeded, 0.5);
        }

        // Test critical usage (above critical threshold)
        let status = monitor.check_web_database_pool_usage(9, 10, 0.5, 0.8);
        assert!(matches!(status, WebPoolHealthStatus::Critical { .. }));
        if let WebPoolHealthStatus::Critical {
            usage_ratio,
            threshold_exceeded,
            ..
        } = status
        {
            assert_eq!(usage_ratio, 0.9);
            assert_eq!(threshold_exceeded, 0.8);
        }

        // Test edge case - no pool configured
        let status = monitor.check_web_database_pool_usage(5, 0, 0.5, 0.8);
        assert!(matches!(status, WebPoolHealthStatus::Healthy { .. }));
        if let WebPoolHealthStatus::Healthy {
            usage_ratio,
            active_connections,
            max_connections,
        } = status
        {
            assert_eq!(usage_ratio, 0.0);
            assert_eq!(active_connections, 0);
            assert_eq!(max_connections, 0);
        }
    }

    #[tokio::test]
    async fn test_web_pool_usage_recording() {
        let monitor = HealthMonitor::new(Uuid::new_v4(), 30, 0.8);

        // Create a test web pool usage report
        let pool_report = WebPoolUsageReport {
            usage_ratio: 0.6,
            active_connections: 6,
            max_connections: 10,
            warning_threshold: 0.75,
            critical_threshold: 0.90,
            pool_name: "web_api_pool".to_string(),
        };

        // Test recording without operational state (should succeed)
        let result = monitor
            .record_web_pool_usage(pool_report.clone(), None)
            .await;
        assert!(result.is_ok());

        // Test recording with different usage levels
        let warning_report = WebPoolUsageReport {
            usage_ratio: 0.8,
            active_connections: 8,
            max_connections: 10,
            warning_threshold: 0.75,
            critical_threshold: 0.90,
            pool_name: "web_api_pool".to_string(),
        };

        let result = monitor.record_web_pool_usage(warning_report, None).await;
        assert!(result.is_ok());

        let critical_report = WebPoolUsageReport {
            usage_ratio: 0.95,
            active_connections: 19,
            max_connections: 20,
            warning_threshold: 0.75,
            critical_threshold: 0.90,
            pool_name: "web_api_pool".to_string(),
        };

        let result = monitor.record_web_pool_usage(critical_report, None).await;
        assert!(result.is_ok());
    }
}
