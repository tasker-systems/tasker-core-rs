//! # Task Initialization Service
//!
//! Manages task initialization by delegating to TaskRequestActor.
//!
//! ## Purpose
//!
//! This service encapsulates the actor delegation pattern for task initialization,
//! providing a clean interface for command processors while maintaining the
//! sophisticated business logic in the actor layer.
//!
//! ## Responsibilities
//!
//! - Request validation and conversion
//! - Task creation with proper database transactions
//! - Namespace queue initialization
//! - Handler registration verification
//! - Complete workflow setup with proper error handling

use std::sync::Arc;
use tasker_shared::messaging::TaskRequestMessage;
use tasker_shared::TaskerResult;
use tracing::{debug, info};
use uuid::Uuid;

use crate::actors::{ActorRegistry, Handler, ProcessTaskRequestMessage};

/// Result of task initialization
#[derive(Debug)]
pub struct TaskInitializationSuccess {
    pub task_uuid: Uuid,
    pub message: String,
}

/// Service for managing task initialization lifecycle
///
/// This service wraps the TaskRequestActor delegation pattern, providing
/// a clean interface for task initialization operations.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::lifecycle_services::TaskInitializationService;
/// use std::sync::Arc;
///
/// # async fn example(
/// #     actors: Arc<crate::actors::ActorRegistry>,
/// #     request: tasker_shared::messaging::TaskRequestMessage
/// # ) -> tasker_shared::TaskerResult<()> {
/// let service = TaskInitializationService::new(actors);
/// let result = service.initialize_task(request).await?;
/// println!("Task initialized: {}", result.task_uuid);
/// # Ok(())
/// # }
/// ```
pub struct TaskInitializationService {
    actors: Arc<ActorRegistry>,
}

impl TaskInitializationService {
    /// Create a new TaskInitializationService
    ///
    /// # Arguments
    ///
    /// * `actors` - Actor registry providing access to TaskRequestActor
    pub fn new(actors: Arc<ActorRegistry>) -> Self {
        Self { actors }
    }

    /// Initialize a new task
    ///
    /// Delegates to TaskRequestActor which handles:
    /// - Request validation and conversion
    /// - Task creation with proper database transactions
    /// - Namespace queue initialization
    /// - Handler registration verification
    /// - Complete workflow setup with proper error handling
    ///
    /// # Arguments
    ///
    /// * `request` - Task request message with all initialization parameters
    ///
    /// # Returns
    ///
    /// Task UUID and success message on successful initialization
    ///
    /// # Errors
    ///
    /// - `ValidationError`: Invalid request parameters
    /// - `DatabaseError`: Database operation failure
    /// - `OrchestrationError`: Actor processing failure
    pub async fn initialize_task(
        &self,
        request: TaskRequestMessage,
    ) -> TaskerResult<TaskInitializationSuccess> {
        debug!(
            request_id = %request.request_id,
            namespace = %request.task_request.namespace,
            handler = %request.task_request.name,
            "TASK_INIT_SERVICE: Initializing task via TaskRequestActor"
        );

        // TAS-46: Actor-based task initialization
        let msg = ProcessTaskRequestMessage { request };
        let task_uuid = self.actors.task_request_actor.handle(msg).await?;

        info!(
            task_uuid = %task_uuid,
            "TASK_INIT_SERVICE: Task initialized successfully"
        );

        Ok(TaskInitializationSuccess {
            task_uuid,
            message:
                "Task initialized successfully via TaskRequestActor - ready for SQL-based discovery"
                    .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_initialization_service_construction() {
        // Verify the service can be constructed
        // Full integration tests require actor registry
    }
}
