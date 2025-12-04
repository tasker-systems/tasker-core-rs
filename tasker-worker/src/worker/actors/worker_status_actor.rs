//! # Worker Status Actor
//!
//! TAS-69: Actor for worker status and health check operations.
//!
//! Handles status reporting, health checks, and event integration status
//! by delegating to WorkerStatusService.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use async_trait::async_trait;
use tracing::{debug, info};

use tasker_shared::system_context::SystemContext;
use tasker_shared::TaskerResult;

use super::messages::{
    GetEventStatusMessage, GetWorkerStatusMessage, HealthCheckMessage, SetEventIntegrationMessage,
};
use super::traits::{Handler, Message, WorkerActor};
use crate::worker::command_processor::StepExecutionStats;
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_subscriber::WorkerEventSubscriber;
use crate::worker::services::WorkerStatusService;
use crate::worker::task_template_manager::TaskTemplateManager;

/// Worker Status Actor
///
/// TAS-69: Wraps WorkerStatusService with actor interface for message-based
/// status and health reporting.
pub struct WorkerStatusActor {
    context: Arc<SystemContext>,
    service: WorkerStatusService,
    /// Execution statistics tracked by the actor
    stats: RwLock<StepExecutionStats>,
    /// Handler registry
    handlers: RwLock<HashMap<String, String>>,
    /// Event publisher reference
    event_publisher: RwLock<Option<WorkerEventPublisher>>,
    /// Event subscriber reference
    event_subscriber: RwLock<Option<WorkerEventSubscriber>>,
}

impl std::fmt::Debug for WorkerStatusActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerStatusActor")
            .field("context", &"Arc<SystemContext>")
            .field("service", &self.service)
            .finish()
    }
}

impl WorkerStatusActor {
    /// Create a new WorkerStatusActor
    pub fn new(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
    ) -> Self {
        let service = WorkerStatusService::new(
            worker_id,
            context.clone(),
            task_template_manager,
        );

        Self {
            context,
            service,
            stats: RwLock::new(StepExecutionStats {
                total_executed: 0,
                total_succeeded: 0,
                total_failed: 0,
                average_execution_time_ms: 0.0,
            }),
            handlers: RwLock::new(HashMap::new()),
            event_publisher: RwLock::new(None),
            event_subscriber: RwLock::new(None),
        }
    }

    /// Update execution statistics
    pub async fn update_stats(&self, executed: u64, succeeded: u64, failed: u64, avg_time: f64) {
        let mut stats = self.stats.write().await;
        stats.total_executed = executed;
        stats.total_succeeded = succeeded;
        stats.total_failed = failed;
        stats.average_execution_time_ms = avg_time;
    }

    /// Record a successful step execution
    pub async fn record_success(&self, execution_time_ms: f64) {
        let mut stats = self.stats.write().await;
        stats.total_executed += 1;
        stats.total_succeeded += 1;

        // Update average
        let total = stats.total_executed as f64;
        stats.average_execution_time_ms =
            (stats.average_execution_time_ms * (total - 1.0) + execution_time_ms) / total;
    }

    /// Record a failed step execution
    pub async fn record_failure(&self) {
        let mut stats = self.stats.write().await;
        stats.total_executed += 1;
        stats.total_failed += 1;
    }

    /// Register a handler
    pub async fn register_handler(&self, name: String, class: String) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(name, class);
    }

    /// Set event publisher
    pub async fn set_event_publisher(&self, publisher: WorkerEventPublisher) {
        let mut ep = self.event_publisher.write().await;
        *ep = Some(publisher);
    }

    /// Set event subscriber
    pub async fn set_event_subscriber(&self, subscriber: WorkerEventSubscriber) {
        let mut es = self.event_subscriber.write().await;
        *es = Some(subscriber);
    }

    /// Clear event integration
    pub async fn clear_event_integration(&self) {
        let mut ep = self.event_publisher.write().await;
        *ep = None;
        let mut es = self.event_subscriber.write().await;
        *es = None;
    }
}

impl WorkerActor for WorkerStatusActor {
    fn name(&self) -> &'static str {
        "WorkerStatusActor"
    }

    fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    fn started(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "WorkerStatusActor started");
        Ok(())
    }

    fn stopped(&mut self) -> TaskerResult<()> {
        info!(actor = self.name(), "WorkerStatusActor stopped");
        Ok(())
    }
}

#[async_trait]
impl Handler<GetWorkerStatusMessage> for WorkerStatusActor {
    async fn handle(&self, _msg: GetWorkerStatusMessage) -> TaskerResult<<GetWorkerStatusMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling GetWorkerStatusMessage");

        let stats = self.stats.read().await;
        let handlers = self.handlers.read().await;
        let handler_list: Vec<String> = handlers.keys().cloned().collect();

        self.service.get_worker_status(&stats, handler_list)
    }
}

#[async_trait]
impl Handler<HealthCheckMessage> for WorkerStatusActor {
    async fn handle(&self, _msg: HealthCheckMessage) -> TaskerResult<<HealthCheckMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling HealthCheckMessage");

        let stats = self.stats.read().await;
        self.service.get_health_status(&stats).await
    }
}

#[async_trait]
impl Handler<GetEventStatusMessage> for WorkerStatusActor {
    async fn handle(&self, _msg: GetEventStatusMessage) -> TaskerResult<<GetEventStatusMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling GetEventStatusMessage");

        let ep = self.event_publisher.read().await;
        let es = self.event_subscriber.read().await;

        self.service.get_event_status(ep.as_ref(), es.as_ref())
    }
}

#[async_trait]
impl Handler<SetEventIntegrationMessage> for WorkerStatusActor {
    async fn handle(&self, msg: SetEventIntegrationMessage) -> TaskerResult<<SetEventIntegrationMessage as Message>::Response> {
        debug!(
            actor = self.name(),
            enabled = msg.enabled,
            "Handling SetEventIntegrationMessage"
        );

        if !msg.enabled {
            self.clear_event_integration().await;
        }
        // Note: Enabling event integration requires external setup of publisher/subscriber

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_worker_status_actor_creation(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let actor = WorkerStatusActor::new(
            context.clone(),
            "test_worker".to_string(),
            task_template_manager,
        );

        assert_eq!(actor.name(), "WorkerStatusActor");
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_record_success(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));

        let actor = WorkerStatusActor::new(
            context.clone(),
            "test_worker".to_string(),
            task_template_manager,
        );

        actor.record_success(100.0).await;
        actor.record_success(200.0).await;

        let stats = actor.stats.read().await;
        assert_eq!(stats.total_executed, 2);
        assert_eq!(stats.total_succeeded, 2);
        assert_eq!(stats.total_failed, 0);
        assert!((stats.average_execution_time_ms - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_worker_status_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorkerStatusActor>();
    }
}
