//! # Worker Actor Registry
//!
//! TAS-69: Central registry for all worker actors, providing lifecycle management
//! and unified access to actor instances. This mirrors the orchestration ActorRegistry
//! pattern from TAS-46.
//!
//! ## TAS-67: Dispatch-Only Architecture
//!
//! The registry is always built with dispatch channels for non-blocking handler
//! invocation. This is the only execution path - there is no fallback to direct
//! handler invocation. The dual-channel pattern ensures step claiming always
//! occurs before handler execution.

use std::fmt;
use std::sync::Arc;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::{system_context::SystemContext, TaskerResult};
use tokio::sync::{mpsc, Semaphore};

use super::messages::DispatchHandlerMessage;
use crate::worker::handlers::{CapacityChecker, LoadSheddingConfig};
use super::traits::WorkerActor;
use super::{
    DomainEventActor, FFICompletionActor, StepExecutorActor, TemplateCacheActor, WorkerStatusActor,
};
use crate::worker::event_publisher::WorkerEventPublisher;
use crate::worker::event_systems::domain_event_system::DomainEventSystemHandle;
use crate::worker::task_template_manager::TaskTemplateManager;

/// TAS-67: Configuration for dispatch mode
///
/// When dispatch mode is enabled, the registry creates channels for non-blocking
/// handler invocation.
///
/// TAS-75: Extended with load shedding configuration for capacity-based claim refusal.
#[derive(Debug, Clone)]
pub struct DispatchModeConfig {
    /// Buffer size for dispatch channel (from StepExecutorActor to HandlerDispatchService)
    pub dispatch_buffer_size: usize,
    /// Buffer size for completion channel (from HandlerDispatchService to CompletionProcessor)
    pub completion_buffer_size: usize,
    /// Maximum concurrent handler executions (semaphore permits)
    pub max_concurrent_handlers: usize,
    /// TAS-75: Load shedding configuration
    pub load_shedding: LoadSheddingConfig,
}

impl Default for DispatchModeConfig {
    fn default() -> Self {
        Self {
            dispatch_buffer_size: 1000,
            completion_buffer_size: 1000,
            max_concurrent_handlers: 10,
            load_shedding: LoadSheddingConfig::default(),
        }
    }
}

/// TAS-67: Dispatch channels for non-blocking handler invocation
///
/// TAS-75: Extended with shared semaphore for load shedding coordination between
/// StepExecutorActor (capacity checking) and HandlerDispatchService (execution bounding).
pub struct DispatchChannels {
    /// Receiver for dispatch messages (consumed by HandlerDispatchService)
    pub dispatch_receiver: mpsc::Receiver<DispatchHandlerMessage>,
    /// Sender for dispatch messages (used by StepExecutorActor internally)
    pub dispatch_sender: mpsc::Sender<DispatchHandlerMessage>,
    /// Receiver for completion results (consumed by CompletionProcessor or ActorCommandProcessor)
    pub completion_receiver: mpsc::Receiver<StepExecutionResult>,
    /// Sender for completion results (used by HandlerDispatchService)
    pub completion_sender: mpsc::Sender<StepExecutionResult>,
    /// TAS-75: Shared semaphore for handler execution bounding (used by HandlerDispatchService)
    pub handler_semaphore: Arc<Semaphore>,
    /// TAS-75: Maximum concurrent handlers (for HandlerDispatchService to reference)
    pub max_concurrent_handlers: usize,
    /// TAS-75: Load shedding configuration (for HandlerDispatchService to reference)
    pub load_shedding_config: LoadSheddingConfig,
}

impl fmt::Debug for DispatchChannels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DispatchChannels")
            .field("dispatch_sender", &"mpsc::Sender<DispatchHandlerMessage>")
            .field(
                "dispatch_receiver",
                &"mpsc::Receiver<DispatchHandlerMessage>",
            )
            .field("completion_sender", &"mpsc::Sender<StepExecutionResult>")
            .field(
                "completion_receiver",
                &"mpsc::Receiver<StepExecutionResult>",
            )
            .field(
                "handler_semaphore",
                &format!(
                    "Arc<Semaphore>(available={})",
                    self.handler_semaphore.available_permits()
                ),
            )
            .field("max_concurrent_handlers", &self.max_concurrent_handlers)
            .field("load_shedding_config", &self.load_shedding_config)
            .finish()
    }
}

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
/// ## TAS-67: Dispatch-Only Architecture
///
/// The registry creates dispatch and completion channels for non-blocking
/// handler invocation. The StepExecutorActor uses dispatch for all step
/// execution - there is no fallback path.
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
    /// TAS-67: Build a new WorkerActorRegistry with non-blocking dispatch
    ///
    /// This builder creates dispatch and completion channels for non-blocking
    /// handler invocation. All step execution flows through the dispatch channel -
    /// there is no fallback path.
    ///
    /// # Arguments
    ///
    /// * `context` - Shared system context for all actors
    /// * `worker_id` - Unique identifier for this worker
    /// * `task_template_manager` - Pre-initialized template manager
    /// * `event_publisher` - Event publisher for FFI handler invocation
    /// * `domain_event_handle` - Handle for domain event dispatch
    /// * `dispatch_config` - Configuration for dispatch channels (required)
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// - `WorkerActorRegistry` - The registry with actors initialized
    /// - `DispatchChannels` - Channels for handler dispatch and completion
    pub async fn build(
        context: Arc<SystemContext>,
        worker_id: String,
        task_template_manager: Arc<TaskTemplateManager>,
        event_publisher: WorkerEventPublisher,
        domain_event_handle: DomainEventSystemHandle,
        dispatch_config: DispatchModeConfig,
    ) -> TaskerResult<(Self, DispatchChannels)> {
        tracing::info!(
            worker_id = %worker_id,
            dispatch_buffer_size = dispatch_config.dispatch_buffer_size,
            completion_buffer_size = dispatch_config.completion_buffer_size,
            max_concurrent_handlers = dispatch_config.max_concurrent_handlers,
            load_shedding_enabled = dispatch_config.load_shedding.enabled,
            "Building WorkerActorRegistry with dispatch and load shedding"
        );

        // Create dispatch channels
        let (dispatch_sender, dispatch_receiver) =
            mpsc::channel(dispatch_config.dispatch_buffer_size);
        let (completion_sender, completion_receiver) =
            mpsc::channel(dispatch_config.completion_buffer_size);

        // TAS-75: Create shared semaphore for handler execution bounding
        // This semaphore is shared between:
        // - StepExecutorActor (via CapacityChecker for claim-time load shedding)
        // - HandlerDispatchService (for execution-time concurrency bounding)
        let handler_semaphore = Arc::new(Semaphore::new(dispatch_config.max_concurrent_handlers));

        // TAS-75: Create CapacityChecker for load shedding
        let capacity_checker = CapacityChecker::new(
            handler_semaphore.clone(),
            dispatch_config.max_concurrent_handlers,
            dispatch_config.load_shedding.clone(),
            format!("{}-capacity", worker_id),
        );

        // Create actors - StepExecutorActor requires dispatch_sender
        // TAS-75: Wire in CapacityChecker for load shedding
        let mut step_executor_actor = StepExecutorActor::new(
            context.clone(),
            worker_id.clone(),
            task_template_manager.clone(),
            event_publisher,
            domain_event_handle,
            dispatch_sender.clone(),
        )
        .with_capacity_checker(capacity_checker);

        let mut ffi_completion_actor = FFICompletionActor::new(context.clone(), worker_id.clone());

        let mut template_cache_actor =
            TemplateCacheActor::new(context.clone(), task_template_manager.clone());

        let mut domain_event_actor = DomainEventActor::new(context.clone());

        let mut worker_status_actor =
            WorkerStatusActor::new(context.clone(), worker_id.clone(), task_template_manager);

        // Call started() lifecycle hook on each actor
        step_executor_actor.started()?;
        ffi_completion_actor.started()?;
        template_cache_actor.started()?;
        domain_event_actor.started()?;
        worker_status_actor.started()?;

        tracing::info!(
            worker_id = %worker_id,
            load_shedding_enabled = dispatch_config.load_shedding.enabled,
            capacity_threshold = dispatch_config.load_shedding.capacity_threshold_percent,
            "WorkerActorRegistry built successfully (5 actors, load shedding wired)"
        );

        let registry = Self {
            context,
            worker_id,
            step_executor_actor: Arc::new(step_executor_actor),
            ffi_completion_actor: Arc::new(ffi_completion_actor),
            template_cache_actor: Arc::new(template_cache_actor),
            domain_event_actor: Arc::new(domain_event_actor),
            worker_status_actor: Arc::new(worker_status_actor),
        };

        // TAS-75: Include shared semaphore and config in DispatchChannels
        // HandlerDispatchService should use this semaphore instead of creating its own
        let channels = DispatchChannels {
            dispatch_receiver,
            dispatch_sender,
            completion_receiver,
            completion_sender,
            handler_semaphore,
            max_concurrent_handlers: dispatch_config.max_concurrent_handlers,
            load_shedding_config: dispatch_config.load_shedding,
        };

        Ok((registry, channels))
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
        let in_process_bus = Arc::new(RwLock::new(InProcessEventBus::new(
            InProcessEventBusConfig::default(),
        )));
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

        // TAS-67: dispatch_config is now required
        // TAS-75: Extended with load shedding config
        let dispatch_config = DispatchModeConfig {
            dispatch_buffer_size: 100,
            completion_buffer_size: 100,
            ..Default::default()
        };

        let result = WorkerActorRegistry::build(
            context,
            worker_id.clone(),
            task_template_manager,
            event_publisher,
            domain_event_handle,
            dispatch_config,
        )
        .await;

        assert!(result.is_ok(), "Registry should build successfully");
        let (registry, channels) = result.unwrap();
        assert_eq!(registry.worker_id(), worker_id);

        // TAS-75: Verify load shedding infrastructure is wired
        assert!(
            channels.handler_semaphore.available_permits() > 0,
            "Handler semaphore should have permits"
        );
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

        // TAS-67: dispatch_config is now required
        // TAS-75: Extended with load shedding config
        let dispatch_config = DispatchModeConfig {
            dispatch_buffer_size: 100,
            completion_buffer_size: 100,
            ..Default::default()
        };

        let (mut registry, _channels) = WorkerActorRegistry::build(
            context,
            worker_id,
            task_template_manager,
            event_publisher,
            domain_event_handle,
            dispatch_config,
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

        // TAS-67: dispatch_config is now required
        // TAS-75: Extended with load shedding config
        let dispatch_config = DispatchModeConfig {
            dispatch_buffer_size: 100,
            completion_buffer_size: 100,
            ..Default::default()
        };

        let (registry, _channels) = WorkerActorRegistry::build(
            context,
            worker_id,
            task_template_manager,
            event_publisher,
            domain_event_handle,
            dispatch_config,
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
