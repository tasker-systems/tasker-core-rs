//! # Auto-Scaling Engine
//!
//! Implements the auto-scaling algorithms and policies for executor pools.
//! Provides intelligent scaling decisions based on utilization, throughput,
//! and health metrics.

use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::error::{Result, TaskerError};
use crate::orchestration::executor::traits::ExecutorType;

use super::pool::PoolMetrics;

/// Scaling action recommendations
#[derive(Debug, Clone, PartialEq)]
pub enum ScalingAction {
    /// Scale up by the specified number of executors
    ScaleUp { count: usize },
    /// Scale down by the specified number of executors
    ScaleDown { count: usize },
    /// No scaling action needed
    NoChange,
}

/// Auto-scaling engine for executor pools
#[derive(Debug)]
pub struct ScalingEngine {
    /// Whether auto-scaling is enabled
    enabled: bool,
    /// Target utilization (0.0-1.0)
    target_utilization: f64,
    /// Minimum interval between scaling actions (seconds)
    scaling_interval_seconds: u64,
    /// Cooldown period after scaling actions (seconds)
    scaling_cooldown_seconds: u64,
    /// Last scaling action time per executor type
    last_scaling_actions: std::collections::HashMap<ExecutorType, Instant>,
}

impl ScalingEngine {
    /// Create a new scaling engine
    pub fn new(
        enabled: bool,
        target_utilization: f64,
        scaling_interval_seconds: u64,
        scaling_cooldown_seconds: u64,
    ) -> Self {
        info!(
            "🎛️ SCALING: Creating scaling engine (enabled: {}, target: {:.1}%)",
            enabled,
            target_utilization * 100.0
        );

        Self {
            enabled,
            target_utilization,
            scaling_interval_seconds,
            scaling_cooldown_seconds,
            last_scaling_actions: std::collections::HashMap::new(),
        }
    }

    /// Evaluate scaling action for a pool based on its metrics
    pub async fn evaluate_scaling_action(&self, metrics: &PoolMetrics) -> Result<ScalingAction> {
        if !self.enabled {
            return Ok(ScalingAction::NoChange);
        }

        // Check if we're in cooldown period
        if let Some(last_action) = self.last_scaling_actions.get(&metrics.executor_type) {
            let cooldown = Duration::from_secs(self.scaling_cooldown_seconds);
            if last_action.elapsed() < cooldown {
                debug!(
                    "SCALING: {} in cooldown period ({:.1}s remaining)",
                    metrics.executor_type.name(),
                    (cooldown - last_action.elapsed()).as_secs_f64()
                );
                return Ok(ScalingAction::NoChange);
            }
        }

        let action = self.calculate_scaling_action(metrics).await?;

        // Record this evaluation (not just when we take action)
        // This helps prevent rapid re-evaluation
        if !matches!(action, ScalingAction::NoChange) {
            debug!(
                "SCALING: Recommended action for {}: {:?}",
                metrics.executor_type.name(),
                action
            );
        }

        Ok(action)
    }

    /// Calculate the appropriate scaling action based on metrics
    async fn calculate_scaling_action(&self, metrics: &PoolMetrics) -> Result<ScalingAction> {
        let utilization = metrics.average_utilization;
        let current_count = metrics.current_executors;
        let min_count = metrics.min_executors;
        let max_count = metrics.max_executors;

        debug!(
            "SCALING: Evaluating {} - utilization: {:.1}%, executors: {} (min: {}, max: {})",
            metrics.executor_type.name(),
            utilization * 100.0,
            current_count,
            min_count,
            max_count
        );

        // High error rate - scale up to distribute load
        if metrics.average_error_rate > 0.05 {
            // 5% error rate threshold
            if current_count < max_count {
                let scale_up_count = 1; // Conservative scaling when errors are high
                info!(
                    "SCALING: High error rate ({:.1}%) detected for {}, scaling up by {}",
                    metrics.average_error_rate * 100.0,
                    metrics.executor_type.name(),
                    scale_up_count
                );
                return Ok(ScalingAction::ScaleUp {
                    count: scale_up_count,
                });
            }
        }

        // High queue depth - scale up to handle backlog
        if metrics.max_queue_depth > 100 {
            // Configurable threshold
            if current_count < max_count {
                let scale_up_count = (metrics.max_queue_depth / 50)
                    .max(1)
                    .min(max_count - current_count);
                info!(
                    "SCALING: High queue depth ({}) detected for {}, scaling up by {}",
                    metrics.max_queue_depth,
                    metrics.executor_type.name(),
                    scale_up_count
                );
                return Ok(ScalingAction::ScaleUp {
                    count: scale_up_count,
                });
            }
        }

        // Utilization-based scaling
        if utilization > self.target_utilization * 1.2 {
            // 20% above target
            // Scale up conditions
            if current_count < max_count {
                let scale_factor = utilization / self.target_utilization;
                let target_count = ((current_count as f64) * scale_factor).ceil() as usize;
                let scale_up_count = (target_count - current_count).min(max_count - current_count);

                if scale_up_count > 0 {
                    info!(
                        "SCALING: High utilization ({:.1}%) for {}, scaling up by {}",
                        utilization * 100.0,
                        metrics.executor_type.name(),
                        scale_up_count
                    );
                    return Ok(ScalingAction::ScaleUp {
                        count: scale_up_count,
                    });
                }
            } else {
                warn!(
                    "SCALING: {} at max capacity ({}) with high utilization ({:.1}%)",
                    metrics.executor_type.name(),
                    max_count,
                    utilization * 100.0
                );
            }
        } else if utilization < self.target_utilization * 0.5 {
            // 50% below target
            // Scale down conditions
            if current_count > min_count {
                let scale_down_count = ((current_count - min_count) / 2).max(1);
                info!(
                    "SCALING: Low utilization ({:.1}%) for {}, scaling down by {}",
                    utilization * 100.0,
                    metrics.executor_type.name(),
                    scale_down_count
                );
                return Ok(ScalingAction::ScaleDown {
                    count: scale_down_count,
                });
            }
        }

        Ok(ScalingAction::NoChange)
    }

    /// Record that a scaling action was taken
    pub fn record_scaling_action(&mut self, executor_type: ExecutorType) {
        self.last_scaling_actions
            .insert(executor_type, Instant::now());
    }

    /// Get scaling status for all executor types
    pub fn get_scaling_status(&self) -> ScalingStatus {
        ScalingStatus {
            enabled: self.enabled,
            target_utilization: self.target_utilization,
            scaling_interval_seconds: self.scaling_interval_seconds,
            scaling_cooldown_seconds: self.scaling_cooldown_seconds,
            last_scaling_actions: self.last_scaling_actions.clone(),
        }
    }

    /// Update scaling configuration
    pub fn update_config(
        &mut self,
        enabled: Option<bool>,
        target_utilization: Option<f64>,
        scaling_interval_seconds: Option<u64>,
        scaling_cooldown_seconds: Option<u64>,
    ) -> Result<()> {
        if let Some(enabled) = enabled {
            self.enabled = enabled;
            info!(
                "SCALING: Auto-scaling {}",
                if enabled { "enabled" } else { "disabled" }
            );
        }

        if let Some(target) = target_utilization {
            if !(0.1..=1.0).contains(&target) {
                return Err(TaskerError::InvalidParameter(
                    "Target utilization must be between 0.1 and 1.0".to_string(),
                ));
            }
            self.target_utilization = target;
            info!(
                "SCALING: Target utilization updated to {:.1}%",
                target * 100.0
            );
        }

        if let Some(interval) = scaling_interval_seconds {
            if interval < 5 {
                return Err(TaskerError::InvalidParameter(
                    "Scaling interval must be at least 5 seconds".to_string(),
                ));
            }
            self.scaling_interval_seconds = interval;
            info!("SCALING: Scaling interval updated to {}s", interval);
        }

        if let Some(cooldown) = scaling_cooldown_seconds {
            if cooldown < 10 {
                return Err(TaskerError::InvalidParameter(
                    "Scaling cooldown must be at least 10 seconds".to_string(),
                ));
            }
            self.scaling_cooldown_seconds = cooldown;
            info!("SCALING: Scaling cooldown updated to {}s", cooldown);
        }

        Ok(())
    }

    /// Check if scaling is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get target utilization
    pub fn target_utilization(&self) -> f64 {
        self.target_utilization
    }
}

/// Status information for the scaling engine
#[derive(Debug, Clone)]
pub struct ScalingStatus {
    pub enabled: bool,
    pub target_utilization: f64,
    pub scaling_interval_seconds: u64,
    pub scaling_cooldown_seconds: u64,
    pub last_scaling_actions: std::collections::HashMap<ExecutorType, Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metrics(
        executor_type: ExecutorType,
        utilization: f64,
        error_rate: f64,
        queue_depth: usize,
    ) -> PoolMetrics {
        PoolMetrics {
            executor_type,
            current_executors: 3,
            min_executors: 1,
            max_executors: 10,
            total_throughput: 100.0,
            average_utilization: utilization,
            max_queue_depth: queue_depth,
            average_error_rate: error_rate,
        }
    }

    #[tokio::test]
    async fn test_scaling_engine_creation() {
        let engine = ScalingEngine::new(true, 0.75, 30, 60);
        assert!(engine.is_enabled());
        assert_eq!(engine.target_utilization(), 0.75);
    }

    #[tokio::test]
    async fn test_scale_up_on_high_utilization() {
        let engine = ScalingEngine::new(true, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.95, 0.01, 50);

        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::ScaleUp { .. }));
    }

    #[tokio::test]
    async fn test_scale_down_on_low_utilization() {
        let engine = ScalingEngine::new(true, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.20, 0.01, 10);

        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::ScaleDown { .. }));
    }

    #[tokio::test]
    async fn test_scale_up_on_high_error_rate() {
        let engine = ScalingEngine::new(true, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.60, 0.10, 20);

        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::ScaleUp { .. }));
    }

    #[tokio::test]
    async fn test_scale_up_on_high_queue_depth() {
        let engine = ScalingEngine::new(true, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.60, 0.01, 150);

        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::ScaleUp { .. }));
    }

    #[tokio::test]
    async fn test_no_action_when_disabled() {
        let engine = ScalingEngine::new(false, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.95, 0.01, 150);

        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::NoChange));
    }

    #[tokio::test]
    async fn test_cooldown_period() {
        let mut engine = ScalingEngine::new(true, 0.75, 30, 60);
        let metrics = create_test_metrics(ExecutorType::TaskRequestProcessor, 0.95, 0.01, 50);

        // Record a scaling action
        engine.record_scaling_action(ExecutorType::TaskRequestProcessor);

        // Immediately try to scale again - should be in cooldown
        let action = engine.evaluate_scaling_action(&metrics).await.unwrap();
        assert!(matches!(action, ScalingAction::NoChange));
    }
}
