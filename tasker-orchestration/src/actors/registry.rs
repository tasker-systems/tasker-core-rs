//! # Actor Registry
//!
//! Central registry for all orchestration actors, providing lifecycle management
//! and unified access to actor instances.

use crate::actors::result_processor_actor::ResultProcessorActor;
use crate::actors::step_enqueuer_actor::StepEnqueuerActor;
use crate::actors::task_finalizer_actor::TaskFinalizerActor;
use crate::actors::task_request_actor::TaskRequestActor;
use crate::actors::traits::OrchestrationActor;
use crate::orchestration::lifecycle::result_processor::OrchestrationResultProcessor;
use crate::orchestration::lifecycle::step_enqueuer_service::StepEnqueuerService;
use crate::orchestration::lifecycle::task_finalizer::TaskFinalizer;
use crate::orchestration::lifecycle::task_initialization::TaskInitializer;
use crate::orchestration::lifecycle::task_request_processor::{
    TaskRequestProcessor, TaskRequestProcessorConfig,
};
use std::sync::Arc;
use tasker_shared::{system_context::SystemContext, TaskerResult};

/// Registry managing all orchestration actors
///
/// The ActorRegistry holds Arc references to all actors in the system,
/// providing centralized access and lifecycle management. This serves as
/// the replacement for the individual component fields in OrchestrationProcessor.
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
/// ```rust,no_run
/// use tasker_orchestration::actors::ActorRegistry;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let context = Arc::new(SystemContext::from_config(config).await?);
/// let registry = ActorRegistry::build(context).await?;
///
/// // Access specific actors
/// let task_initializer = &registry.task_initializer_actor;
/// let task_finalizer = &registry.task_finalizer_actor;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct ActorRegistry {
    /// System context shared by all actors
    context: Arc<SystemContext>,

    /// Task request actor for processing task initialization requests (TAS-46 Phase 2)
    pub task_request_actor: Arc<TaskRequestActor>,

    /// Result processor actor for processing step execution results (TAS-46 Phase 2)
    pub result_processor_actor: Arc<ResultProcessorActor>,

    /// Step enqueuer actor for batch processing ready tasks (TAS-46 Phase 3)
    pub step_enqueuer_actor: Arc<StepEnqueuerActor>,

    /// Task finalizer actor for task finalization with atomic claiming (TAS-46 Phase 3)
    pub task_finalizer_actor: Arc<TaskFinalizerActor>,
    // Future actors (TAS-46 Phase 3):
    // pub task_initializer_actor: Arc<TaskInitializerActor>,
}

impl ActorRegistry {
    /// Build a new ActorRegistry with all actors initialized
    ///
    /// This performs the following steps:
    /// 1. Creates all actor instances
    /// 2. Calls started() lifecycle hook on each actor
    /// 3. Returns the registry if all actors initialize successfully
    ///
    /// # Arguments
    ///
    /// * `context` - Shared system context for all actors
    ///
    /// # Returns
    ///
    /// A `TaskerResult` containing the ActorRegistry or an error if any
    /// actor fails to initialize.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any actor fails to construct
    /// - Any actor's started() hook returns an error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_orchestration::actors::ActorRegistry;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let context = Arc::new(SystemContext::from_config(config).await?);
    /// let registry = ActorRegistry::build(context).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build(context: Arc<SystemContext>) -> TaskerResult<Self> {
        tracing::info!("Building ActorRegistry with actors");

        // Create shared StepEnqueuerService (used by multiple actors)
        let task_claim_step_enqueuer = StepEnqueuerService::new(context.clone()).await?;
        let task_claim_step_enqueuer = Arc::new(task_claim_step_enqueuer);

        // Create TaskRequestActor and its dependencies
        let task_initializer = Arc::new(TaskInitializer::new(
            context.clone(),
            task_claim_step_enqueuer.clone(),
        ));

        let task_request_processor = Arc::new(TaskRequestProcessor::new(
            context.message_client.clone(),
            context.task_handler_registry.clone(),
            task_initializer,
            TaskRequestProcessorConfig::default(),
        ));

        let mut task_request_actor = TaskRequestActor::new(context.clone(), task_request_processor);
        task_request_actor.started()?;
        let task_request_actor = Arc::new(task_request_actor);

        // Create ResultProcessorActor and its dependencies
        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer.clone());
        let result_processor = Arc::new(OrchestrationResultProcessor::new(
            task_finalizer,
            context.clone(),
        ));

        let mut result_processor_actor =
            ResultProcessorActor::new(context.clone(), result_processor);
        result_processor_actor.started()?;
        let result_processor_actor = Arc::new(result_processor_actor);

        // Create StepEnqueuerActor using shared StepEnqueuerService
        let mut step_enqueuer_actor =
            StepEnqueuerActor::new(context.clone(), task_claim_step_enqueuer.clone());
        step_enqueuer_actor.started()?;
        let step_enqueuer_actor = Arc::new(step_enqueuer_actor);

        // Create TaskFinalizerActor using shared StepEnqueuerService
        let task_finalizer = TaskFinalizer::new(context.clone(), task_claim_step_enqueuer.clone());
        let mut task_finalizer_actor = TaskFinalizerActor::new(context.clone(), task_finalizer);
        task_finalizer_actor.started()?;
        let task_finalizer_actor = Arc::new(task_finalizer_actor);

        tracing::info!("✅ ActorRegistry built successfully with 4 actors");

        Ok(Self {
            context,
            task_request_actor,
            result_processor_actor,
            step_enqueuer_actor,
            task_finalizer_actor,
        })
    }

    /// Get the system context shared by all actors
    ///
    /// Provides access to the underlying SystemContext for testing
    /// and introspection purposes.
    pub fn context(&self) -> &Arc<SystemContext> {
        &self.context
    }

    /// Shutdown all actors gracefully
    ///
    /// Calls the stopped() lifecycle hook on each actor in reverse
    /// initialization order. Errors are logged but don't prevent
    /// other actors from shutting down.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # async fn example(registry: ActorRegistry) {
    /// registry.shutdown().await;
    /// # }
    /// ```
    pub async fn shutdown(&mut self) {
        tracing::info!("Shutting down ActorRegistry");

        // Call stopped() on all actors in reverse initialization order
        if let Some(actor) = Arc::get_mut(&mut self.task_finalizer_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop TaskFinalizerActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.step_enqueuer_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop StepEnqueuerActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.result_processor_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop ResultProcessorActor");
            }
        }

        if let Some(actor) = Arc::get_mut(&mut self.task_request_actor) {
            if let Err(e) = actor.stopped() {
                tracing::error!(error = %e, "Failed to stop TaskRequestActor");
            }
        }

        // Future actors will be stopped here in reverse order

        tracing::info!("✅ ActorRegistry shutdown complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_actor_registry_builds_successfully() {
        // This test will expand as we add actors
        // For now, just verify the registry structure compiles
    }

    #[test]
    fn test_actor_registry_is_cloneable() {
        // Verify that ActorRegistry can be cloned (important for sharing)
        fn assert_clone<T: Clone>() {}
        assert_clone::<ActorRegistry>();
    }

    #[test]
    fn test_actor_registry_is_send_sync() {
        // Verify that ActorRegistry can be sent across threads
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ActorRegistry>();
    }
}
