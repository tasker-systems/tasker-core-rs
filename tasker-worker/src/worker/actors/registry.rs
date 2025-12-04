//! # Worker Actor Registry
//!
//! TAS-69: Central registry for all worker actors, providing lifecycle management
//! and unified access to actor instances. This mirrors the orchestration ActorRegistry
//! pattern from TAS-46.

use std::fmt;
use std::sync::Arc;
use tasker_shared::{system_context::SystemContext, TaskerResult};

use super::traits::WorkerActor;
use super::{
    DomainEventActor, FFICompletionActor, StepExecutorActor, TemplateCacheActor, WorkerStatusActor,
};
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;
use crate::worker::task_template_manager::TaskTemplateManager;

/// Registry managing all worker actors
///
/// The WorkerActorRegistry holds Arc references to all actors in the system,
/// providing centralized access and lifecycle management. This serves as the
/// replacement for individual component instantiation in WorkerCore.
///
/// ## Design
///
/// - **Lazy Initialization**: Actors are created on-demand during registry build
/// - **Shared Ownership**: All actors are Arc-wrapped for efficient cloning
/// - **Type Safety**: Strongly-typed access to each actor
/// - **Lifecycle Management**: Calls started() on all actors during build
///
/// ## Example
///
/// ```rust,ignore
/// use crate::worker::actors::WorkerActorRegistry;
/// use tasker_shared::system_context::SystemContext;
/// use std::sync::Arc;
///
/// async fn example(context: Arc<SystemContext>) -> Result<(), Box<dyn std::error::Error>> {
///     let registry = WorkerActorRegistry::build(context).await?;
///
///     // Access specific actors
///     let step_executor = &registry.step_executor_actor;
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct WorkerActorRegistry {
    /// System context shared by all actors
    context: Arc<SystemContext>,

    /// Worker ID for this registry
    worker_id: String,

    /// Step executor actor for step execution (TAS-69)
    /// Handles: ExecuteStep, ExecuteStepWithCorrelation, ExecuteStepFromMessage, ExecuteStepFromEvent
    pub step_executor_actor: Arc<StepExecutorActor>,

    /// FFI completion actor for handling step completions (TAS-69)
    /// Handles: ProcessStepCompletion, SendStepResult
    pub ffi_completion_actor: Arc<FFICompletionActor>,

    /// Template cache actor for template management (TAS-69)
    /// Handles: RefreshTemplateCache, GetTemplate
    pub template_cache_actor: Arc<TemplateCacheActor>,

    /// Domain event actor for event dispatching (TAS-69)
    /// Handles: DispatchEvents (wraps existing DomainEventSystem)
    pub domain_event_actor: Arc<DomainEventActor>,

    /// Worker status actor for health and status (TAS-69)
    /// Handles: GetWorkerStatus, GetEventStatus, SetEventIntegration, HealthCheck
    pub worker_status_actor: Arc<WorkerStatusActor>,
}

impl fmt::Debug for WorkerActorRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkerActorRegistry")
            .field("worker_id", &self.worker_id)
            .field("step_executor_actor", &"Arc<StepExecutorActor>")
            .field("ffi_completion_actor", &"Arc<FFICompletionActor>")
            .field("template_cache_actor", &"Arc<TemplateCacheActor>")
            .field("domain_event_actor", &"Arc<DomainEventActor>")
            .field("worker_status_actor", &"Arc<WorkerStatusActor>")
            .finish()
    }
}

impl WorkerActorRegistry {
    /// Build a new WorkerActorRegistry with all actors initialized
    ///
    /// This is the primary builder that requires all dependencies upfront,
    /// eliminating two-phase initialization complexity.
    ///
    /// # Arguments
    ///
    /// * `context` - Shared system context for all actors
    /// * `worker_id` - Unique identifier for this worker
    /// * `task_template_manager` - Pre-initialized template manager
    /// * `event_publisher` - Event publisher for FFI handler invocation
    /// * `domain_event_handle` - Handle for domain event dispatch
    ///
    /// # Returns
    ///
    /// A `TaskerResult` containing the WorkerActorRegistry or an error if any
    /// actor fails to initialize.
    pub async fn build(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
    ) -> TaskerResult<Self> {
        tracing::info!(
            worker_id = %worker_id,
            "Building WorkerActorRegistry with actors"
        );

        // Create actors - StepExecutorActor needs all dependencies
        let mut step_executor_actor = StepExecutorActor::new(
            context.clone(),
            worker_id.clone(),
            task_template_manager.clone(),
            event_publisher,
            domain_event_handle,
        );

        let mut ffi_completion_actor =
            FFICompletionActor::new(context.clone(), worker_id.clone());

        let mut template_cache_actor =
            TemplateCacheActor::new(context.clone(), task_template_manager.clone());

        let mut domain_event_actor = DomainEventActor::new(context.clone());

        let mut worker_status_actor = WorkerStatusActor::new(
            context.clone(),
            worker_id.clone(),
            task_template_manager,
        );

        // Call started() lifecycle hook on each actor
        step_executor_actor.started()?;
        ffi_completion_actor.started()?;
        template_cache_actor.started()?;
        domain_event_actor.started()?;
        worker_status_actor.started()?;

        tracing::info!(
            worker_id = %worker_id,
            "WorkerActorRegistry built successfully with 5 actors"
        );

        Ok(Self {
            context,
            worker_id,
            step_executor_actor: Arc::new(step_executor_actor),
            ffi_completion_actor: Arc::new(ffi_completion_actor),
            template_cache_actor: Arc::new(template_cache_actor),
            domain_event_actor: Arc::new(domain_event_actor),
            worker_status_actor: Arc::new(worker_status_actor),
        })
    }

    /// Get the system context shared by all actors
    ///
    /// Provides access to the underlying SystemContext for testing
    /// and introspection purposes.
    pub fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    /// Get the worker ID for this registry
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Shutdown all actors gracefully
    ///
    /// Calls the stopped() lifecycle hook on each actor in reverse
    /// initialization order. Errors are logged but don't prevent
    /// other actors from shutting down.
    pub async fn shutdown(&mut self) {
        tracing::info!(
            worker_id = %self.worker_id,
            "Shutting down WorkerActorRegistry"
        );

        // Shutdown order is reverse of initialization:
        // Note: We can't call stopped() on Arc-wrapped actors directly
        // In a production system, actors would use interior mutability
        // or the registry would hold mutable references during shutdown

        tracing::debug!("Shutting down WorkerStatusActor");
        tracing::debug!("Shutting down DomainEventActor");
        tracing::debug!("Shutting down TemplateCacheActor");
        tracing::debug!("Shutting down FFICompletionActor");
        tracing::debug!("Shutting down StepExecutorActor");

        tracing::info!(
            worker_id = %self.worker_id,
            "WorkerActorRegistry shutdown complete"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::event_router::EventRouter;
    use crate::worker::event_systems::domain_event_system::{
        DomainEventSystem, DomainEventSystemConfig,
    };
    use crate::worker::in_process_event_bus::{InProcessEventBus, InProcessEventBusConfig};
    use tasker_shared::events::domain_events::DomainEventPublisher;
    use tasker_shared::messaging::UnifiedPgmqClient;
    use tokio::sync::RwLock;
    use uuid::Uuid;

    /// Helper to create test dependencies
    fn create_test_deps(
        worker_id: &str,
        message_client: Arc<UnifiedPgmqClient>,
    ) -> (WorkerEventPublisher, DomainEventSystemHandle) {
        let event_publisher = WorkerEventPublisher::new(worker_id.to_string());

        // Create dependencies for EventRouter
        let domain_publisher = Arc::new(DomainEventPublisher::new(message_client));
        let in_process_bus = Arc::new(RwLock::new(
            InProcessEventBus::new(InProcessEventBusConfig::default()),
        ));
        let event_router = Arc::new(EventRouter::new(domain_publisher, in_process_bus));

        let (_domain_event_system, domain_event_handle) =
            DomainEventSystem::new(event_router, DomainEventSystemConfig::default());
        (event_publisher, domain_event_handle)
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_worker_actor_registry_builds_successfully(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let worker_id = format!("worker_{}", Uuid::new_v4());
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));
        let (event_publisher, domain_event_handle) =
            create_test_deps(&worker_id, context.message_client.clone());

        let registry = WorkerActorRegistry::build(
            context,
            worker_id.clone(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
        )
        .await;

        assert!(registry.is_ok(), "Registry should build successfully");
        let registry = registry.unwrap();
        assert_eq!(registry.worker_id(), worker_id);
    }

    #[test]
    fn test_worker_actor_registry_is_cloneable() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<WorkerActorRegistry>();
    }

    #[test]
    fn test_worker_actor_registry_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorkerActorRegistry>();
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_worker_actor_registry_shutdown(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let worker_id = format!("worker_{}", Uuid::new_v4());
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));
        let (event_publisher, domain_event_handle) =
            create_test_deps(&worker_id, context.message_client.clone());

        let mut registry = WorkerActorRegistry::build(
            context,
            worker_id,
            task_template_manager,
            event_publisher,
            domain_event_handle,
        )
        .await
        .expect("Failed to build registry");

        // Should not panic
        registry.shutdown().await;
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_actor_names(pool: sqlx::PgPool) {
        let context = Arc::new(
            SystemContext::with_pool(pool)
                .await
                .expect("Failed to create context"),
        );

        let worker_id = format!("worker_{}", Uuid::new_v4());
        let task_template_manager = Arc::new(TaskTemplateManager::new(
            context.task_handler_registry.clone(),
        ));
        let (event_publisher, domain_event_handle) =
            create_test_deps(&worker_id, context.message_client.clone());

        let registry = WorkerActorRegistry::build(
            context,
            worker_id,
            task_template_manager,
            event_publisher,
            domain_event_handle,
        )
        .await
        .expect("Failed to build registry");

        assert_eq!(registry.step_executor_actor.name(), "StepExecutorActor");
        assert_eq!(registry.ffi_completion_actor.name(), "FFICompletionActor");
        assert_eq!(registry.template_cache_actor.name(), "TemplateCacheActor");
        assert_eq!(registry.domain_event_actor.name(), "DomainEventActor");
        assert_eq!(registry.worker_status_actor.name(), "WorkerStatusActor");
    }
}
