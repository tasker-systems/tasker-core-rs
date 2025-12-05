//! # Worker Status Actor
//!
//! TAS-69: Actor for worker status and health check operations.
//!
//! Handles status reporting, health checks, and event integration status
//! by delegating to WorkerStatusService.
//!
//! ## Lock-Free Statistics
//!
//! Step execution statistics use `AtomicU64` counters for lock-free updates.
//! This eliminates lock contention on the hot path (every step completion).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Lock-free step execution statistics using atomic counters
///
/// TAS-69: Eliminates lock contention on the hot path by using `AtomicU64`
/// for all counter updates. Average execution time is computed on read
/// from `total_execution_time_ms / total_executed`.
#[derive(Debug)]
pub struct AtomicStepExecutionStats {
    /// Total steps executed (success + failure)
    total_executed: AtomicU64,
    /// Total successful step executions
    total_succeeded: AtomicU64,
    /// Total failed step executions
    total_failed: AtomicU64,
    /// Sum of all execution times in milliseconds (for computing average)
    total_execution_time_ms: AtomicU64,
}

impl Default for AtomicStepExecutionStats {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicStepExecutionStats {
    /// Create new atomic stats with all counters at zero
    pub fn new() -> Self {
        Self {
            total_executed: AtomicU64::new(0),
            total_succeeded: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            total_execution_time_ms: AtomicU64::new(0),
        }
    }

    /// Record a successful step execution (lock-free)
    #[inline]
    pub fn record_success(&self, execution_time_ms: u64) {
        self.total_executed.fetch_add(1, Ordering::Relaxed);
        self.total_succeeded.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_ms
            .fetch_add(execution_time_ms, Ordering::Relaxed);
    }

    /// Record a failed step execution (lock-free)
    #[inline]
    pub fn record_failure(&self) {
        self.total_executed.fetch_add(1, Ordering::Relaxed);
        self.total_failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a snapshot of current statistics
    ///
    /// Computes average execution time from sum / count.
    /// Note: Individual reads are atomic but the snapshot is not
    /// transactionally consistent (acceptable for monitoring).
    pub fn snapshot(&self) -> StepExecutionStats {
        let total_executed = self.total_executed.load(Ordering::Relaxed);
        let total_succeeded = self.total_succeeded.load(Ordering::Relaxed);
        let total_failed = self.total_failed.load(Ordering::Relaxed);
        let total_time = self.total_execution_time_ms.load(Ordering::Relaxed);

        let average_execution_time_ms = if total_executed > 0 {
            total_time as f64 / total_executed as f64
        } else {
            0.0
        };

        StepExecutionStats {
            total_executed,
            total_succeeded,
            total_failed,
            average_execution_time_ms,
        }
    }
}

/// Worker Status Actor
///
/// TAS-69: Wraps WorkerStatusService with actor interface for message-based
/// status and health reporting.
///
/// ## Lock-Free Design
///
/// Step execution statistics use `AtomicStepExecutionStats` for lock-free
/// counter updates. This eliminates lock contention on the hot path
/// (every step completion calls `record_success` or `record_failure`).
pub struct WorkerStatusActor {
    context: Arc<SystemContext>,
    service: WorkerStatusService,
    /// Lock-free execution statistics using atomic counters
    stats: AtomicStepExecutionStats,
    /// Handler registry (rarely updated, RwLock is fine)
    handlers: RwLock<HashMap<String, String>>,
    /// Event publisher reference (rarely updated, RwLock is fine)
    event_publisher: RwLock<Option<WorkerEventPublisher>>,
    /// Event subscriber reference (rarely updated, RwLock is fine)
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
        let service = WorkerStatusService::new(worker_id, context.clone(), task_template_manager);

        Self {
            context,
            service,
            stats: AtomicStepExecutionStats::new(),
            handlers: RwLock::new(HashMap::new()),
            event_publisher: RwLock::new(None),
            event_subscriber: RwLock::new(None),
        }
    }

    /// Record a successful step execution (lock-free)
    ///
    /// This method is called on every step completion and uses atomic
    /// operations to avoid lock contention.
    #[inline]
    pub async fn record_success(&self, execution_time_ms: f64) {
        // Convert to u64 for atomic storage (sub-ms precision not needed for averages)
        self.stats.record_success(execution_time_ms as u64);
    }

    /// Record a failed step execution (lock-free)
    ///
    /// This method is called on every step failure and uses atomic
    /// operations to avoid lock contention.
    #[inline]
    pub async fn record_failure(&self) {
        self.stats.record_failure();
    }

    /// Get a snapshot of current statistics
    ///
    /// Computes average execution time from sum / count.
    pub fn stats_snapshot(&self) -> StepExecutionStats {
        self.stats.snapshot()
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
    async fn handle(
        &self,
        _msg: GetWorkerStatusMessage,
    ) -> TaskerResult<<GetWorkerStatusMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling GetWorkerStatusMessage");

        let stats = self.stats_snapshot();
        let handlers = self.handlers.read().await;
        let handler_list: Vec<String> = handlers.keys().cloned().collect();

        self.service.get_worker_status(&stats, handler_list)
    }
}

#[async_trait]
impl Handler<HealthCheckMessage> for WorkerStatusActor {
    async fn handle(
        &self,
        _msg: HealthCheckMessage,
    ) -> TaskerResult<<HealthCheckMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling HealthCheckMessage");

        let stats = self.stats_snapshot();
        self.service.get_health_status(&stats).await
    }
}

#[async_trait]
impl Handler<GetEventStatusMessage> for WorkerStatusActor {
    async fn handle(
        &self,
        _msg: GetEventStatusMessage,
    ) -> TaskerResult<<GetEventStatusMessage as Message>::Response> {
        debug!(actor = self.name(), "Handling GetEventStatusMessage");

        let ep = self.event_publisher.read().await;
        let es = self.event_subscriber.read().await;

        self.service.get_event_status(ep.as_ref(), es.as_ref())
    }
}

#[async_trait]
impl Handler<SetEventIntegrationMessage> for WorkerStatusActor {
    async fn handle(
        &self,
        msg: SetEventIntegrationMessage,
    ) -> TaskerResult<<SetEventIntegrationMessage as Message>::Response> {
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

        let stats = actor.stats_snapshot();
        assert_eq!(stats.total_executed, 2);
        assert_eq!(stats.total_succeeded, 2);
        assert_eq!(stats.total_failed, 0);
        // Average is now computed from sum/count: (100+200)/2 = 150
        assert!((stats.average_execution_time_ms - 150.0).abs() < 1.0);
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_record_failure(pool: sqlx::PgPool) {
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

        actor.record_failure().await;
        actor.record_failure().await;
        actor.record_success(100.0).await;

        let stats = actor.stats_snapshot();
        assert_eq!(stats.total_executed, 3);
        assert_eq!(stats.total_succeeded, 1);
        assert_eq!(stats.total_failed, 2);
    }

    #[test]
    fn test_atomic_stats_lock_free() {
        // Verify AtomicStepExecutionStats can be used without locks
        let stats = AtomicStepExecutionStats::new();

        stats.record_success(100);
        stats.record_success(200);
        stats.record_failure();

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_executed, 3);
        assert_eq!(snapshot.total_succeeded, 2);
        assert_eq!(snapshot.total_failed, 1);
        assert!((snapshot.average_execution_time_ms - 100.0).abs() < 1.0); // 300/3 = 100
    }

    #[test]
    fn test_worker_status_actor_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorkerStatusActor>();
    }
}
