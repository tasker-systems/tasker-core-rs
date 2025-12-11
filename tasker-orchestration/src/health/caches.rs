//! # Health Status Caches
//!
//! TAS-75 Phase 5: Thread-safe caches for health status data.
//!
//! This module provides the central cache infrastructure for the health module.
//! All health status data is stored in these caches and updated by the background
//! `StatusEvaluator` task. Web handlers and API endpoints read from these caches
//! synchronously without database queries.
//!
//! ## Design Principles
//!
//! 1. **Thread-safe**: All caches use `Arc<RwLock<>>` for concurrent access
//! 2. **Non-blocking reads**: API handlers use `try_read()` or `read()` without blocking
//! 3. **Background updates**: Only `StatusEvaluator` writes to caches
//! 4. **Stale detection**: `last_evaluated` timestamp enables staleness warnings

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use super::types::{
    BackpressureStatus, ChannelHealthStatus, DatabaseHealthStatus, QueueDepthStatus,
};

// =============================================================================
// Health Status Caches
// =============================================================================

/// Thread-safe health status caches
///
/// This struct holds all cached health status data that is read by API handlers
/// and updated by the background `StatusEvaluator` task.
///
/// ## Usage
///
/// ```ignore
/// // Create caches (typically in bootstrap)
/// let caches = HealthStatusCaches::new();
///
/// // Clone for sharing across components
/// let evaluator_caches = caches.clone();
/// let api_caches = caches.clone();
///
/// // Read from cache (non-blocking in async context)
/// let queue_status = caches.queue_status().read().await.clone();
///
/// // Write to cache (only from StatusEvaluator)
/// *caches.queue_status().write().await = new_status;
/// ```
#[derive(Debug, Clone)]
pub struct HealthStatusCaches {
    /// Database pool health status
    db_status: Arc<RwLock<DatabaseHealthStatus>>,

    /// Channel saturation status
    channel_status: Arc<RwLock<ChannelHealthStatus>>,

    /// Queue depth status
    queue_status: Arc<RwLock<QueueDepthStatus>>,

    /// Aggregate backpressure decision (pre-computed)
    backpressure: Arc<RwLock<BackpressureStatus>>,

    /// Last evaluation timestamp for staleness detection
    last_evaluated: Arc<RwLock<Option<Instant>>>,
}

impl Default for HealthStatusCaches {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthStatusCaches {
    /// Create new health status caches with default values
    ///
    /// All status fields are initialized to their defaults:
    /// - Database: not connected
    /// - Channels: 0% saturation
    /// - Queues: Normal tier, 0 depth
    /// - Backpressure: not active
    /// - Last evaluated: None (never evaluated)
    #[must_use]
    pub fn new() -> Self {
        Self {
            db_status: Arc::new(RwLock::new(DatabaseHealthStatus::default())),
            channel_status: Arc::new(RwLock::new(ChannelHealthStatus::default())),
            queue_status: Arc::new(RwLock::new(QueueDepthStatus::default())),
            backpressure: Arc::new(RwLock::new(BackpressureStatus::default())),
            last_evaluated: Arc::new(RwLock::new(None)),
        }
    }

    // =========================================================================
    // Cache Accessors
    // =========================================================================

    /// Get reference to database health status cache
    #[must_use]
    pub fn db_status(&self) -> &Arc<RwLock<DatabaseHealthStatus>> {
        &self.db_status
    }

    /// Get reference to channel health status cache
    #[must_use]
    pub fn channel_status(&self) -> &Arc<RwLock<ChannelHealthStatus>> {
        &self.channel_status
    }

    /// Get reference to queue depth status cache
    #[must_use]
    pub fn queue_status(&self) -> &Arc<RwLock<QueueDepthStatus>> {
        &self.queue_status
    }

    /// Get reference to backpressure status cache
    #[must_use]
    pub fn backpressure(&self) -> &Arc<RwLock<BackpressureStatus>> {
        &self.backpressure
    }

    /// Get reference to last evaluated timestamp cache
    #[must_use]
    pub fn last_evaluated(&self) -> &Arc<RwLock<Option<Instant>>> {
        &self.last_evaluated
    }

    // =========================================================================
    // Convenience Methods for Reading
    // =========================================================================

    /// Get a snapshot of the current database health status
    ///
    /// This is an async read that clones the current value.
    pub async fn get_db_status(&self) -> DatabaseHealthStatus {
        self.db_status.read().await.clone()
    }

    /// Get a snapshot of the current channel health status
    ///
    /// This is an async read that clones the current value.
    pub async fn get_channel_status(&self) -> ChannelHealthStatus {
        self.channel_status.read().await.clone()
    }

    /// Get a snapshot of the current queue depth status
    ///
    /// This is an async read that clones the current value.
    pub async fn get_queue_status(&self) -> QueueDepthStatus {
        self.queue_status.read().await.clone()
    }

    /// Get a snapshot of the current backpressure status
    ///
    /// This is an async read that clones the current value.
    pub async fn get_backpressure(&self) -> BackpressureStatus {
        self.backpressure.read().await.clone()
    }

    /// Get the time since last evaluation
    ///
    /// Returns `None` if never evaluated, otherwise returns the duration
    /// since the last successful evaluation cycle.
    pub async fn time_since_evaluation(&self) -> Option<std::time::Duration> {
        self.last_evaluated
            .read()
            .await
            .map(|instant| instant.elapsed())
    }

    /// Check if cached data is stale
    ///
    /// Data is considered stale if:
    /// - Never evaluated (returns true)
    /// - Last evaluation was longer ago than `stale_threshold`
    pub async fn is_stale(&self, stale_threshold: std::time::Duration) -> bool {
        match *self.last_evaluated.read().await {
            None => true, // Never evaluated = stale
            Some(instant) => instant.elapsed() > stale_threshold,
        }
    }

    // =========================================================================
    // Convenience Methods for Writing (used by StatusEvaluator)
    // =========================================================================

    /// Update the database health status
    ///
    /// Should only be called by `StatusEvaluator` during evaluation cycles.
    pub async fn set_db_status(&self, status: DatabaseHealthStatus) {
        *self.db_status.write().await = status;
    }

    /// Update the channel health status
    ///
    /// Should only be called by `StatusEvaluator` during evaluation cycles.
    pub async fn set_channel_status(&self, status: ChannelHealthStatus) {
        *self.channel_status.write().await = status;
    }

    /// Update the queue depth status
    ///
    /// Should only be called by `StatusEvaluator` during evaluation cycles.
    pub async fn set_queue_status(&self, status: QueueDepthStatus) {
        *self.queue_status.write().await = status;
    }

    /// Update the backpressure status
    ///
    /// Should only be called by `StatusEvaluator` during evaluation cycles.
    pub async fn set_backpressure(&self, status: BackpressureStatus) {
        *self.backpressure.write().await = status;
    }

    /// Update the last evaluated timestamp to now
    ///
    /// Should be called at the end of each evaluation cycle by `StatusEvaluator`.
    pub async fn mark_evaluated(&self) {
        *self.last_evaluated.write().await = Some(Instant::now());
    }

    /// Update all caches atomically (within a single async context)
    ///
    /// This is the preferred method for `StatusEvaluator` to update all caches
    /// at once after an evaluation cycle.
    pub async fn update_all(
        &self,
        db_status: DatabaseHealthStatus,
        channel_status: ChannelHealthStatus,
        queue_status: QueueDepthStatus,
        backpressure: BackpressureStatus,
    ) {
        // Note: These writes happen sequentially within the same async context.
        // For true atomicity across all fields, we would need a single outer lock,
        // but the current design prioritizes read concurrency over write atomicity.
        *self.db_status.write().await = db_status;
        *self.channel_status.write().await = channel_status;
        *self.queue_status.write().await = queue_status;
        *self.backpressure.write().await = backpressure;
        *self.last_evaluated.write().await = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::types::QueueDepthTier;

    #[tokio::test]
    async fn test_new_caches_have_defaults() {
        let caches = HealthStatusCaches::new();

        // Verify all defaults
        let db = caches.get_db_status().await;
        assert!(!db.is_connected);

        let channel = caches.get_channel_status().await;
        assert_eq!(channel.command_saturation_percent, 0.0);

        let queue = caches.get_queue_status().await;
        // Default tier is Unknown until evaluation runs
        assert_eq!(queue.tier, QueueDepthTier::Unknown);
        assert_eq!(queue.max_depth, 0);

        let bp = caches.get_backpressure().await;
        assert!(!bp.active);

        // Never evaluated
        assert!(caches.time_since_evaluation().await.is_none());
    }

    #[tokio::test]
    async fn test_update_and_read_db_status() {
        let caches = HealthStatusCaches::new();

        let new_status = DatabaseHealthStatus {
            evaluated: true,
            is_connected: true,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            last_check_duration_ms: 5,
            error_message: None,
        };

        caches.set_db_status(new_status.clone()).await;

        let read_status = caches.get_db_status().await;
        assert!(read_status.is_connected);
        assert_eq!(read_status.last_check_duration_ms, 5);
    }

    #[tokio::test]
    async fn test_update_and_read_queue_status() {
        let caches = HealthStatusCaches::new();

        let mut queue_depths = std::collections::HashMap::new();
        queue_depths.insert("test_queue".to_string(), 500);

        let new_status = QueueDepthStatus {
            tier: QueueDepthTier::Warning,
            max_depth: 500,
            worst_queue: "test_queue".to_string(),
            queue_depths,
        };

        caches.set_queue_status(new_status.clone()).await;

        let read_status = caches.get_queue_status().await;
        assert_eq!(read_status.tier, QueueDepthTier::Warning);
        assert_eq!(read_status.max_depth, 500);
        assert_eq!(read_status.worst_queue, "test_queue");
    }

    #[tokio::test]
    async fn test_stale_detection_never_evaluated() {
        let caches = HealthStatusCaches::new();

        // Never evaluated should be considered stale
        assert!(caches.is_stale(std::time::Duration::from_secs(10)).await);
    }

    #[tokio::test]
    async fn test_stale_detection_recent_evaluation() {
        let caches = HealthStatusCaches::new();

        caches.mark_evaluated().await;

        // Just evaluated should not be stale
        assert!(!caches.is_stale(std::time::Duration::from_secs(10)).await);
    }

    #[tokio::test]
    async fn test_update_all_sets_timestamp() {
        let caches = HealthStatusCaches::new();

        assert!(caches.time_since_evaluation().await.is_none());

        caches
            .update_all(
                DatabaseHealthStatus::default(),
                ChannelHealthStatus::default(),
                QueueDepthStatus::default(),
                BackpressureStatus::default(),
            )
            .await;

        // Should now have a timestamp
        assert!(caches.time_since_evaluation().await.is_some());
    }

    #[tokio::test]
    async fn test_caches_are_cloneable_and_shared() {
        let caches = HealthStatusCaches::new();
        let caches_clone = caches.clone();

        // Update via original
        let new_status = BackpressureStatus {
            active: true,
            reason: Some("Test backpressure".to_string()),
            retry_after_secs: Some(30),
            source: None,
        };
        caches.set_backpressure(new_status).await;

        // Read via clone - should see the update
        let read_status = caches_clone.get_backpressure().await;
        assert!(read_status.active);
        assert_eq!(read_status.reason, Some("Test backpressure".to_string()));
    }
}
