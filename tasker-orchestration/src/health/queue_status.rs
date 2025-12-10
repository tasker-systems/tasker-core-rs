//! # Queue Depth Evaluator
//!
//! TAS-75 Phase 5: Evaluates PGMQ queue depths for backpressure detection.
//!
//! This module provides functions to check queue health without blocking the
//! API hot path. The evaluator is called by the background `StatusEvaluator` task.

use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{debug, warn};

use super::types::{QueueDepthStatus, QueueDepthTier, QueueHealthConfig};

/// Evaluate queue depth status for all specified queues
///
/// Queries PGMQ metrics_all in a single query and filters to requested queues.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `queue_names` - List of queue names to check
/// * `config` - Health check configuration with thresholds
///
/// # Returns
/// `QueueDepthStatus` with current queue depth information
pub async fn evaluate_queue_status(
    pool: &PgPool,
    queue_names: &[String],
    config: &QueueHealthConfig,
) -> QueueDepthStatus {
    let depths = get_queue_depths(pool, queue_names).await;

    let mut max_depth = 0i64;
    let mut worst_queue = String::new();
    // Start with Normal (not Unknown) since we are actively evaluating
    let mut worst_tier = QueueDepthTier::Normal;

    for (queue_name, depth) in &depths {
        if *depth > max_depth {
            max_depth = *depth;
            worst_queue = queue_name.clone();
        }

        let tier = classify_queue_depth(*depth, config);

        if tier > worst_tier {
            worst_tier = tier;
        }
    }

    debug!(
        tier = ?worst_tier,
        max_depth = max_depth,
        worst_queue = %worst_queue,
        queue_count = queue_names.len(),
        "Queue depth status evaluated"
    );

    QueueDepthStatus {
        tier: worst_tier,
        max_depth,
        worst_queue,
        queue_depths: depths,
    }
}

/// Classify a single queue depth into a tier
///
/// # Arguments
/// * `depth` - Current queue depth
/// * `config` - Configuration with thresholds
///
/// # Returns
/// The appropriate `QueueDepthTier` for this depth
fn classify_queue_depth(depth: i64, config: &QueueHealthConfig) -> QueueDepthTier {
    if depth >= config.overflow_threshold {
        QueueDepthTier::Overflow
    } else if depth >= config.critical_threshold {
        QueueDepthTier::Critical
    } else if depth >= config.warning_threshold {
        QueueDepthTier::Warning
    } else {
        QueueDepthTier::Normal
    }
}

/// Queue metrics row from pgmq.metrics_all()
///
/// Note: queue_name is Option<String> because pgmq.metrics_all() returns it as nullable.
/// We filter out None values when processing.
#[derive(Debug)]
struct QueueMetricsRow {
    queue_name: Option<String>,
    queue_length: i64,
}

/// Get queue depths for multiple queues using a single query
///
/// Uses `pgmq.metrics_all()` to fetch all queue metrics in one database call,
/// then filters in memory to only include the requested queues.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `queue_names` - List of queue names to check
///
/// # Returns
/// HashMap of queue_name -> depth
async fn get_queue_depths(pool: &PgPool, queue_names: &[String]) -> HashMap<String, i64> {
    // Fetch all queue metrics in a single query
    let all_metrics = get_all_queue_metrics(pool).await;

    // Filter to only requested queues and build the result map
    let mut depths = HashMap::new();
    for queue_name in queue_names {
        // Use the fetched metric or default to 0 if queue doesn't exist
        let depth = all_metrics.get(queue_name).copied().unwrap_or(0);
        depths.insert(queue_name.clone(), depth);
    }

    depths
}

/// Get all queue metrics in a single database call
///
/// Uses `pgmq.metrics_all()` to efficiently fetch metrics for all queues.
///
/// # Arguments
/// * `pool` - Database connection pool
///
/// # Returns
/// HashMap of queue_name -> depth for all queues in PGMQ
async fn get_all_queue_metrics(pool: &PgPool) -> HashMap<String, i64> {
    match sqlx::query_as!(
        QueueMetricsRow,
        r#"SELECT queue_name, COALESCE(queue_length, 0) as "queue_length!" FROM pgmq.metrics_all()"#
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let mut metrics = HashMap::new();
            for row in rows {
                // Filter out rows with None queue_name (should not happen in practice)
                if let Some(name) = row.queue_name {
                    metrics.insert(name, row.queue_length);
                }
            }
            metrics
        }
        Err(e) => {
            warn!(
                error = %e,
                "Failed to get queue metrics from pgmq.metrics_all()"
            );
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = QueueHealthConfig::default();
        assert_eq!(config.warning_threshold, 1000);
        assert_eq!(config.critical_threshold, 5000);
        assert_eq!(config.overflow_threshold, 10000);
    }

    #[test]
    fn test_classify_queue_depth_normal() {
        let config = QueueHealthConfig::default();

        assert_eq!(classify_queue_depth(0, &config), QueueDepthTier::Normal);
        assert_eq!(classify_queue_depth(500, &config), QueueDepthTier::Normal);
        assert_eq!(classify_queue_depth(999, &config), QueueDepthTier::Normal);
    }

    #[test]
    fn test_classify_queue_depth_warning() {
        let config = QueueHealthConfig::default();

        assert_eq!(classify_queue_depth(1000, &config), QueueDepthTier::Warning);
        assert_eq!(classify_queue_depth(2500, &config), QueueDepthTier::Warning);
        assert_eq!(classify_queue_depth(4999, &config), QueueDepthTier::Warning);
    }

    #[test]
    fn test_classify_queue_depth_critical() {
        let config = QueueHealthConfig::default();

        assert_eq!(
            classify_queue_depth(5000, &config),
            QueueDepthTier::Critical
        );
        assert_eq!(
            classify_queue_depth(7500, &config),
            QueueDepthTier::Critical
        );
        assert_eq!(
            classify_queue_depth(9999, &config),
            QueueDepthTier::Critical
        );
    }

    #[test]
    fn test_classify_queue_depth_overflow() {
        let config = QueueHealthConfig::default();

        assert_eq!(
            classify_queue_depth(10000, &config),
            QueueDepthTier::Overflow
        );
        assert_eq!(
            classify_queue_depth(15000, &config),
            QueueDepthTier::Overflow
        );
        assert_eq!(
            classify_queue_depth(100000, &config),
            QueueDepthTier::Overflow
        );
    }

    #[test]
    fn test_classify_queue_depth_custom_thresholds() {
        let config = QueueHealthConfig {
            warning_threshold: 100,
            critical_threshold: 500,
            overflow_threshold: 1000,
        };

        assert_eq!(classify_queue_depth(50, &config), QueueDepthTier::Normal);
        assert_eq!(classify_queue_depth(100, &config), QueueDepthTier::Warning);
        assert_eq!(classify_queue_depth(500, &config), QueueDepthTier::Critical);
        assert_eq!(
            classify_queue_depth(1000, &config),
            QueueDepthTier::Overflow
        );
    }

    #[test]
    fn test_queue_depth_tier_ordering() {
        // Verify tier comparison works correctly
        assert!(QueueDepthTier::Normal < QueueDepthTier::Warning);
        assert!(QueueDepthTier::Warning < QueueDepthTier::Critical);
        assert!(QueueDepthTier::Critical < QueueDepthTier::Overflow);

        // Verify that worst_tier selection logic works
        let mut worst = QueueDepthTier::Normal;

        if QueueDepthTier::Warning > worst {
            worst = QueueDepthTier::Warning;
        }
        assert_eq!(worst, QueueDepthTier::Warning);

        if QueueDepthTier::Critical > worst {
            worst = QueueDepthTier::Critical;
        }
        assert_eq!(worst, QueueDepthTier::Critical);
    }
}
