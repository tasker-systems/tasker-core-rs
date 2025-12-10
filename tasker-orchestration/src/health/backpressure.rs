//! # Backpressure Checker
//!
//! TAS-75 Phase 5: Public API for checking backpressure status.
//!
//! This module provides the public `try_*` API for API handlers to check
//! backpressure conditions. All methods are synchronous and read from caches
//! updated by the background `StatusEvaluator` task.
//!
//! ## Design Principles
//!
//! 1. **Non-blocking**: All methods read from thread-safe caches
//! 2. **No database queries**: Cache-only reads in API hot path
//! 3. **Consistent interface**: Similar to the original `state.rs` methods
//! 4. **Detailed metrics**: Full backpressure metrics for health endpoints

use std::time::Duration;
use tasker_shared::types::web::ApiError;
use tracing::warn;

use super::caches::HealthStatusCaches;
use super::types::{
    BackpressureMetrics, BackpressureStatus, ChannelHealthStatus, QueueDepthStatus,
};

/// Public backpressure checking API
///
/// This struct provides synchronous access to cached health status data.
/// All methods read from caches that are updated by the background `StatusEvaluator`.
///
/// ## Usage
///
/// ```ignore
/// // In API handler
/// let checker = state.backpressure_checker();
///
/// // Check if backpressure is active (for gating API requests)
/// if let Some(error) = checker.try_check_backpressure() {
///     return Err(error);
/// }
///
/// // Get detailed metrics for health endpoint
/// let metrics = checker.get_backpressure_metrics().await;
/// ```
#[derive(Debug, Clone)]
pub struct BackpressureChecker {
    caches: HealthStatusCaches,
    stale_threshold: Duration,
}

impl BackpressureChecker {
    /// Create a new backpressure checker
    ///
    /// # Arguments
    /// * `caches` - Shared health status caches
    /// * `stale_threshold` - Duration after which cached data is considered stale
    pub fn new(caches: HealthStatusCaches, stale_threshold: Duration) -> Self {
        Self {
            caches,
            stale_threshold,
        }
    }

    /// Create a new backpressure checker with default stale threshold (30 seconds)
    pub fn with_default_threshold(caches: HealthStatusCaches) -> Self {
        Self::new(caches, Duration::from_secs(30))
    }

    // =========================================================================
    // Synchronous try_* API (for API hot path)
    // =========================================================================

    /// Check if backpressure is active (synchronous, cache-only)
    ///
    /// Returns `Some(ApiError)` if backpressure is active, `None` if healthy.
    /// This is the primary method for API handlers to check before accepting requests.
    ///
    /// # Returns
    /// - `Some(ApiError::Backpressure { ... })` if backpressure is active
    /// - `None` if the system is healthy and can accept requests
    ///
    /// # Example
    /// ```ignore
    /// if let Some(error) = checker.try_check_backpressure() {
    ///     // Return 503 with Retry-After header
    ///     return Err(error);
    /// }
    /// ```
    pub fn try_check_backpressure(&self) -> Option<ApiError> {
        // Try to get the backpressure status from cache
        // Note: This uses blocking_read since we want synchronous access in the hot path
        let status = match self.caches.backpressure().try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => {
                // Lock contention - return healthy to avoid false positives
                return None;
            }
        };

        if status.active {
            let reason = status.reason.clone().unwrap_or_default();
            let retry_after_secs = status.retry_after_secs.unwrap_or(30);

            Some(ApiError::backpressure(reason, retry_after_secs))
        } else {
            None
        }
    }

    /// Get current queue depth status (synchronous, cache-only)
    ///
    /// Returns the cached queue depth status. If cache access fails,
    /// returns default (healthy) status.
    ///
    /// # Returns
    /// `QueueDepthStatus` with current queue depth information
    pub fn try_get_queue_depth_status(&self) -> QueueDepthStatus {
        match self.caches.queue_status().try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => {
                // Lock contention - return default
                QueueDepthStatus::default()
            }
        }
    }

    /// Get current channel status (synchronous, cache-only)
    ///
    /// Returns the cached channel health status. If cache access fails,
    /// returns default (healthy) status.
    ///
    /// # Returns
    /// `ChannelHealthStatus` with current channel saturation information
    pub fn try_get_channel_status(&self) -> ChannelHealthStatus {
        match self.caches.channel_status().try_read() {
            Ok(guard) => guard.clone(),
            Err(_) => {
                // Lock contention - return default
                ChannelHealthStatus::default()
            }
        }
    }

    /// Check if cached data is stale (synchronous)
    ///
    /// Returns true if the cache hasn't been updated recently.
    /// Useful for health endpoints to report degraded status.
    pub fn try_is_stale(&self) -> bool {
        match self.caches.last_evaluated().try_read() {
            Ok(guard) => match *guard {
                None => true, // Never evaluated = stale
                Some(instant) => instant.elapsed() > self.stale_threshold,
            },
            Err(_) => {
                // Lock contention - assume not stale
                false
            }
        }
    }

    // =========================================================================
    // Async API (for health endpoints and monitoring)
    // =========================================================================

    /// Get detailed backpressure metrics for health endpoint
    ///
    /// This method is async because it acquires read locks on multiple caches.
    /// It's designed for health endpoints, not for the API hot path.
    ///
    /// # Returns
    /// `BackpressureMetrics` with comprehensive health information
    pub async fn get_backpressure_metrics(&self) -> BackpressureMetrics {
        let db = self.caches.get_db_status().await;
        let channel = self.caches.get_channel_status().await;
        let queue = self.caches.get_queue_status().await;
        let bp = self.caches.get_backpressure().await;

        // Check for stale data
        if self.caches.is_stale(self.stale_threshold).await {
            warn!(
                threshold_ms = self.stale_threshold.as_millis(),
                "Health cache data is stale"
            );
        }

        BackpressureMetrics {
            circuit_breaker_open: db.circuit_breaker_open,
            circuit_breaker_failures: db.circuit_breaker_failures,
            command_channel_saturation_percent: channel.command_saturation_percent,
            command_channel_available_capacity: channel.command_available_capacity,
            command_channel_messages_sent: channel.command_messages_sent,
            command_channel_overflow_events: channel.command_overflow_events,
            backpressure_active: bp.active,
            queue_depth_tier: format!("{:?}", queue.tier),
            queue_depth_max: queue.max_depth,
            queue_depth_worst_queue: queue.worst_queue.clone(),
            queue_depths: queue.queue_depths.clone(),
        }
    }

    /// Get the current backpressure status (async)
    ///
    /// Returns the full backpressure status including reason and retry time.
    pub async fn get_backpressure_status(&self) -> BackpressureStatus {
        self.caches.get_backpressure().await
    }

    /// Check if the system is healthy (async)
    ///
    /// Returns true if no backpressure condition is active and data is not stale.
    pub async fn is_healthy(&self) -> bool {
        let bp = self.caches.get_backpressure().await;
        let stale = self.caches.is_stale(self.stale_threshold).await;

        !bp.active && !stale
    }

    /// Get reference to the underlying caches (for advanced use cases)
    pub fn caches(&self) -> &HealthStatusCaches {
        &self.caches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::types::QueueDepthTier;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_try_check_backpressure_healthy() {
        let caches = HealthStatusCaches::new();
        let checker = BackpressureChecker::with_default_threshold(caches);

        // Default status is healthy (no backpressure)
        let result = checker.try_check_backpressure();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_try_check_backpressure_active() {
        let caches = HealthStatusCaches::new();

        // Set backpressure active
        let bp_status = BackpressureStatus {
            active: true,
            reason: Some("Test backpressure".to_string()),
            retry_after_secs: Some(30),
            source: None,
        };
        caches.set_backpressure(bp_status).await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        let result = checker.try_check_backpressure();
        assert!(result.is_some());

        if let Some(ApiError::Backpressure { reason, .. }) = result {
            assert_eq!(reason, "Test backpressure");
        } else {
            panic!("Expected Backpressure error");
        }
    }

    #[tokio::test]
    async fn test_try_get_queue_depth_status() {
        let caches = HealthStatusCaches::new();

        let mut depths = HashMap::new();
        depths.insert("test_queue".to_string(), 2500);

        let queue_status = QueueDepthStatus {
            tier: QueueDepthTier::Warning,
            max_depth: 2500,
            worst_queue: "test_queue".to_string(),
            queue_depths: depths,
        };
        caches.set_queue_status(queue_status).await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        let result = checker.try_get_queue_depth_status();
        assert_eq!(result.tier, QueueDepthTier::Warning);
        assert_eq!(result.max_depth, 2500);
        assert_eq!(result.worst_queue, "test_queue");
    }

    #[tokio::test]
    async fn test_try_get_channel_status() {
        let caches = HealthStatusCaches::new();

        let channel_status = ChannelHealthStatus {
            evaluated: true,
            command_saturation_percent: 75.0,
            command_available_capacity: 250,
            command_messages_sent: 500,
            command_overflow_events: 0,
            is_saturated: false,
            is_critical: false,
        };
        caches.set_channel_status(channel_status).await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        let result = checker.try_get_channel_status();
        assert_eq!(result.command_saturation_percent, 75.0);
        assert_eq!(result.command_available_capacity, 250);
    }

    #[tokio::test]
    async fn test_try_is_stale_never_evaluated() {
        let caches = HealthStatusCaches::new();
        let checker = BackpressureChecker::with_default_threshold(caches);

        // Never evaluated = stale
        assert!(checker.try_is_stale());
    }

    #[tokio::test]
    async fn test_try_is_stale_recent_evaluation() {
        let caches = HealthStatusCaches::new();
        caches.mark_evaluated().await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        // Just evaluated = not stale
        assert!(!checker.try_is_stale());
    }

    #[tokio::test]
    async fn test_get_backpressure_metrics() {
        let caches = HealthStatusCaches::new();

        // Set up some status
        let channel_status = ChannelHealthStatus {
            evaluated: true,
            command_saturation_percent: 50.0,
            command_available_capacity: 500,
            command_messages_sent: 1000,
            command_overflow_events: 2,
            is_saturated: false,
            is_critical: false,
        };
        caches.set_channel_status(channel_status).await;
        caches.mark_evaluated().await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        let metrics = checker.get_backpressure_metrics().await;
        assert_eq!(metrics.command_channel_saturation_percent, 50.0);
        assert_eq!(metrics.command_channel_available_capacity, 500);
        assert_eq!(metrics.command_channel_messages_sent, 1000);
        assert!(!metrics.backpressure_active);
    }

    #[tokio::test]
    async fn test_is_healthy() {
        let caches = HealthStatusCaches::new();
        caches.mark_evaluated().await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        // Default status + recently evaluated = healthy
        assert!(checker.is_healthy().await);
    }

    #[tokio::test]
    async fn test_is_healthy_with_backpressure() {
        let caches = HealthStatusCaches::new();

        let bp_status = BackpressureStatus {
            active: true,
            reason: Some("Test".to_string()),
            retry_after_secs: Some(30),
            source: None,
        };
        caches.set_backpressure(bp_status).await;
        caches.mark_evaluated().await;

        let checker = BackpressureChecker::with_default_threshold(caches);

        // Backpressure active = not healthy
        assert!(!checker.is_healthy().await);
    }

    #[tokio::test]
    async fn test_is_healthy_stale_data() {
        let caches = HealthStatusCaches::new();
        // Don't mark evaluated - data is stale

        let checker = BackpressureChecker::new(caches, Duration::from_millis(1));

        // Stale data = not healthy
        assert!(!checker.is_healthy().await);
    }

    #[test]
    fn test_custom_stale_threshold() {
        let caches = HealthStatusCaches::new();
        let checker = BackpressureChecker::new(caches, Duration::from_secs(60));

        // Should use the custom threshold
        assert_eq!(checker.stale_threshold, Duration::from_secs(60));
    }
}
