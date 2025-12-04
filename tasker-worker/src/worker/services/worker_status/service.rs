//! # Worker Status Service
//!
//! TAS-69: Worker status and health check logic extracted from command_processor.rs.
//!
//! This service handles:
//! - Worker status reporting
//! - Health check processing
//! - Event integration status

use std::sync::Arc;

use tracing::debug;

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use crate::health::WorkerHealthStatus;
use crate::worker::command_processor::{EventIntegrationStatus, StepExecutionStats, WorkerStatus};
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_subscriber::WorkerEventSubscriber;
use crate::worker::task_template_manager::TaskTemplateManager;

/// Worker Status Service
///
/// TAS-69: Extracted from command_processor.rs to provide focused status
/// and health check handling.
///
/// This service encapsulates:
/// - Worker status reporting (steps executed, succeeded, failed)
/// - Health check processing with database and API connectivity
/// - Event integration status and statistics
pub struct WorkerStatusService {
    /// Worker identification
    worker_id: String,

    /// System context for dependencies
    #[expect(dead_code, reason = "TAS-69: Reserved for future health check enhancements")]
    context: Arc<SystemContext>,

    /// Task template manager for cache stats
    task_template_manager: Arc<TaskTemplateManager>,

    /// Worker start time for uptime calculation
    start_time: std::time::Instant,
}

impl std::fmt::Debug for WorkerStatusService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerStatusService")
            .field("worker_id", &self.worker_id)
            .field("uptime_seconds", &self.start_time.elapsed().as_secs())
            .finish()
    }
}

impl WorkerStatusService {
    /// Create a new WorkerStatusService
    pub fn new(
        worker_id: String,
        context: Arc<SystemContext>,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> Self {
        Self {
            worker_id,
            context,
            task_template_manager,
            start_time: std::time::Instant::now(),
        }
    }

    /// Get worker status
    pub fn get_worker_status(
        &self,
        stats: &StepExecutionStats,
        registered_handlers: Vec<String>,
    ) -> TaskerResult<WorkerStatus> {
        let uptime = self.start_time.elapsed().as_secs();

        let status = WorkerStatus {
            worker_id: self.worker_id.clone(),
            status: "healthy".to_string(),
            steps_executed: stats.total_executed,
            steps_succeeded: stats.total_succeeded,
            steps_failed: stats.total_failed,
            uptime_seconds: uptime,
            registered_handlers,
        };

        debug!(
            worker_id = %self.worker_id,
            status = ?status,
            "Returning worker status"
        );

        Ok(status)
    }

    /// Get health check status
    pub async fn get_health_status(&self, stats: &StepExecutionStats) -> TaskerResult<WorkerHealthStatus> {
        let health_status = WorkerHealthStatus {
            status: "healthy".to_string(),
            database_connected: true, // TODO: Add actual DB connectivity check
            orchestration_api_reachable: true, // TODO: Add actual API check
            supported_namespaces: self.task_template_manager.supported_namespaces().await,
            template_cache_stats: Some(self.task_template_manager.cache_stats().await),
            total_messages_processed: stats.total_executed,
            successful_executions: stats.total_succeeded,
            failed_executions: stats.total_failed,
        };

        debug!(
            worker_id = %self.worker_id,
            health_status = ?health_status,
            "Returning health status"
        );

        Ok(health_status)
    }

    /// Get event integration status
    pub fn get_event_status(
        &self,
        event_publisher: Option<&WorkerEventPublisher>,
        event_subscriber: Option<&WorkerEventSubscriber>,
    ) -> TaskerResult<EventIntegrationStatus> {
        let status = if let Some(publisher) = event_publisher {
            let publisher_stats = publisher.get_statistics();
            let subscriber_stats = event_subscriber
                .map(|s| s.get_statistics())
                .unwrap_or_default();

            EventIntegrationStatus {
                enabled: true,
                events_published: publisher_stats.events_published,
                events_received: subscriber_stats.completions_received,
                correlation_tracking_enabled: true,
                pending_correlations: 0,
                ffi_handlers_subscribed: publisher_stats.ffi_handlers_subscribed,
                completion_subscribers: publisher_stats.completion_subscribers,
            }
        } else {
            EventIntegrationStatus {
                enabled: false,
                events_published: 0,
                events_received: 0,
                correlation_tracking_enabled: false,
                pending_correlations: 0,
                ffi_handlers_subscribed: 0,
                completion_subscribers: 0,
            }
        };

        debug!(
            worker_id = %self.worker_id,
            event_status = ?status,
            "Returning event integration status"
        );

        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_worker_status_service_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let service = WorkerStatusService::new(
            "test_worker".to_string(),
            context,
            task_template_manager,
        );

        assert_eq!(service.worker_id, "test_worker");
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_worker_status(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let service = WorkerStatusService::new(
            "test_worker".to_string(),
            context,
            task_template_manager,
        );

        let stats = StepExecutionStats {
            total_executed: 10,
            total_succeeded: 8,
            total_failed: 2,
            average_execution_time_ms: 100.0,
        };

        let status = service.get_worker_status(&stats, vec!["handler1".to_string()]).unwrap();

        assert_eq!(status.worker_id, "test_worker");
        assert_eq!(status.status, "healthy");
        assert_eq!(status.steps_executed, 10);
        assert_eq!(status.steps_succeeded, 8);
        assert_eq!(status.steps_failed, 2);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_get_event_status_disabled(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let service = WorkerStatusService::new(
            "test_worker".to_string(),
            context,
            task_template_manager,
        );

        let status = service.get_event_status(None, None).unwrap();

        assert!(!status.enabled);
        assert_eq!(status.events_published, 0);
    }
}
