//! # Health Check Evaluator
//!
//! Pure function for evaluating system health from cached health status data.
//! Extracted from the command processor actor for testability.

use crate::health::HealthStatusCaches;
use crate::orchestration::commands::SystemHealth;
use tasker_shared::TaskerResult;

/// Evaluate system health from cached health status data (TAS-75)
///
/// Reads from `HealthStatusCaches` for non-blocking health checks.
/// Uses fail-open semantics: unevaluated status returns healthy.
pub async fn evaluate_health_status(
    health_caches: &HealthStatusCaches,
    actor_count: usize,
) -> TaskerResult<SystemHealth> {
    let db_status = health_caches.get_db_status().await;
    let channel_status = health_caches.get_channel_status().await;
    let queue_status = health_caches.get_queue_status().await;
    let backpressure = health_caches.get_backpressure().await;

    let health_evaluated =
        db_status.evaluated || channel_status.evaluated || queue_status.tier.is_evaluated();

    let database_connected = if db_status.evaluated {
        db_status.is_connected && !db_status.circuit_breaker_open
    } else {
        true // Fail-open during startup
    };

    let message_queues_healthy = !queue_status.tier.is_critical();

    let status = if !health_evaluated {
        "unknown".to_string()
    } else if !database_connected || !message_queues_healthy || backpressure.active {
        "unhealthy".to_string()
    } else if channel_status.is_saturated || queue_status.tier.is_warning() {
        "degraded".to_string()
    } else {
        "healthy".to_string()
    };

    Ok(SystemHealth {
        status,
        database_connected,
        message_queues_healthy,
        active_processors: actor_count as u32,
        circuit_breaker_open: db_status.circuit_breaker_open,
        circuit_breaker_failures: db_status.circuit_breaker_failures,
        command_channel_saturation_percent: channel_status.command_saturation_percent,
        backpressure_active: backpressure.active,
        queue_depth_tier: format!("{:?}", queue_status.tier),
        queue_depth_max: queue_status.max_depth,
        queue_depth_worst_queue: queue_status.worst_queue.clone(),
        health_evaluated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::types::{
        BackpressureStatus, ChannelHealthStatus, DatabaseHealthStatus, QueueDepthStatus,
        QueueDepthTier,
    };

    #[tokio::test]
    async fn test_unknown_when_not_evaluated() {
        let caches = HealthStatusCaches::new();
        let result = evaluate_health_status(&caches, 4).await.unwrap();
        assert_eq!(result.status, "unknown");
        assert!(result.database_connected); // fail-open
        assert!(result.message_queues_healthy);
        assert!(!result.health_evaluated);
        assert_eq!(result.active_processors, 4);
    }

    #[tokio::test]
    async fn test_healthy_when_all_good() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;
        caches
            .set_channel_status(ChannelHealthStatus {
                evaluated: true,
                is_saturated: false,
                is_critical: false,
                command_saturation_percent: 10.0,
                command_available_capacity: 90,
                command_messages_sent: 100,
                command_overflow_events: 0,
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "healthy");
        assert!(result.database_connected);
        assert!(result.health_evaluated);
    }

    #[tokio::test]
    async fn test_unhealthy_when_db_disconnected() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: false,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: Some("Connection refused".to_string()),
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "unhealthy");
        assert!(!result.database_connected);
    }

    #[tokio::test]
    async fn test_unhealthy_when_circuit_breaker_open() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: true,
                circuit_breaker_failures: 5,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "unhealthy");
        assert!(!result.database_connected);
        assert!(result.circuit_breaker_open);
        assert_eq!(result.circuit_breaker_failures, 5);
    }

    #[tokio::test]
    async fn test_degraded_when_channel_saturated() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;
        caches
            .set_channel_status(ChannelHealthStatus {
                evaluated: true,
                is_saturated: true,
                is_critical: false,
                command_saturation_percent: 85.0,
                command_available_capacity: 15,
                command_messages_sent: 500,
                command_overflow_events: 0,
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "degraded");
        assert_eq!(result.command_channel_saturation_percent, 85.0);
    }

    #[tokio::test]
    async fn test_degraded_when_queue_warning() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;
        caches
            .set_queue_status(QueueDepthStatus {
                tier: QueueDepthTier::Warning,
                max_depth: 1500,
                worst_queue: "worker_default_queue".to_string(),
                queue_depths: Default::default(),
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "degraded");
        assert_eq!(result.queue_depth_max, 1500);
    }

    #[tokio::test]
    async fn test_unhealthy_when_queue_critical() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;
        caches
            .set_queue_status(QueueDepthStatus {
                tier: QueueDepthTier::Critical,
                max_depth: 6000,
                worst_queue: "worker_default_queue".to_string(),
                queue_depths: Default::default(),
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "unhealthy");
        assert!(!result.message_queues_healthy);
    }

    #[tokio::test]
    async fn test_unhealthy_when_backpressure_active() {
        let caches = HealthStatusCaches::new();
        caches
            .set_db_status(DatabaseHealthStatus {
                evaluated: true,
                is_connected: true,
                circuit_breaker_open: false,
                circuit_breaker_failures: 0,
                last_check_duration_ms: 5,
                error_message: None,
            })
            .await;
        caches
            .set_backpressure(BackpressureStatus {
                active: true,
                reason: Some("Queue depth exceeded threshold".to_string()),
                retry_after_secs: Some(30),
                source: None,
            })
            .await;

        let result = evaluate_health_status(&caches, 2).await.unwrap();
        assert_eq!(result.status, "unhealthy");
        assert!(result.backpressure_active);
    }

    #[tokio::test]
    async fn test_actor_count_passed_through() {
        let caches = HealthStatusCaches::new();
        let result = evaluate_health_status(&caches, 7).await.unwrap();
        assert_eq!(result.active_processors, 7);
    }

    #[tokio::test]
    async fn test_fail_open_during_startup() {
        // When nothing has been evaluated yet, database_connected should be true (fail-open)
        let caches = HealthStatusCaches::new();
        let result = evaluate_health_status(&caches, 1).await.unwrap();
        assert!(result.database_connected);
        assert!(result.message_queues_healthy);
        // But health_evaluated should be false since nothing was evaluated
        assert!(!result.health_evaluated);
    }
}
