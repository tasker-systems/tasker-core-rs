//! # Task Finalization Service
//!
//! Manages task finalization by delegating to TaskFinalizerActor.
//!
//! ## Purpose
//!
//! This service encapsulates the actor delegation pattern for task finalization,
//! providing a clean interface for completing task lifecycle management.
//!
//! ## Responsibilities
//!
//! - Task completion state validation
//! - Final status calculation and persistence
//! - Cleanup operations and resource release
//! - Finalization result reporting

use chrono::{DateTime, Utc};
use std::sync::Arc;
use tasker_shared::TaskerResult;
use tracing::{debug, info};
use uuid::Uuid;

use crate::actors::{ActorRegistry, FinalizeTaskMessage, Handler};

/// Result of task finalization
#[derive(Debug)]
pub enum FinalizationResult {
    /// Finalization succeeded
    Success {
        task_uuid: Uuid,
        final_status: String,
        completion_time: Option<DateTime<Utc>>,
    },
    /// Finalization failed
    Failed { error: String },
}

/// Service for managing task finalization lifecycle
///
/// This service wraps the TaskFinalizerActor delegation pattern, providing
/// a clean interface for task finalization operations.
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::orchestration::lifecycle_services::TaskFinalizationService;
/// use std::sync::Arc;
/// use uuid::Uuid;
///
/// # async fn example(
/// #     actors: Arc<crate::actors::ActorRegistry>,
/// #     task_uuid: Uuid
/// # ) -> tasker_shared::TaskerResult<()> {
/// let service = TaskFinalizationService::new(actors);
/// let result = service.finalize_task(task_uuid).await?;
/// # Ok(())
/// # }
/// ```
pub struct TaskFinalizationService {
    actors: Arc<ActorRegistry>,
}

impl TaskFinalizationService {
    /// Create a new TaskFinalizationService
    ///
    /// # Arguments
    ///
    /// * `actors` - Actor registry providing access to TaskFinalizerActor
    pub fn new(actors: Arc<ActorRegistry>) -> Self {
        Self { actors }
    }

    /// Finalize a task
    ///
    /// Delegates to TaskFinalizerActor which handles:
    /// - Task completion state validation
    /// - Final status calculation and persistence
    /// - Cleanup operations and resource release
    /// - Finalization result reporting
    ///
    /// # Arguments
    ///
    /// * `task_uuid` - UUID of the task to finalize
    ///
    /// # Returns
    ///
    /// Finalization result with task completion details
    ///
    /// # Errors
    ///
    /// - `DatabaseError`: Database operation failure
    /// - `OrchestrationError`: Actor processing failure
    pub async fn finalize_task(&self, task_uuid: Uuid) -> TaskerResult<FinalizationResult> {
        debug!(
            task_uuid = %task_uuid,
            "TASK_FINALIZATION_SERVICE: Finalizing task via TaskFinalizerActor"
        );

        // TAS-46 Phase 3: Use TaskFinalizerActor for task finalization
        let msg = FinalizeTaskMessage { task_uuid };

        match self.actors.task_finalizer_actor.handle(msg).await {
            Ok(result) => {
                info!(
                    task_uuid = %task_uuid,
                    action = ?result.action,
                    "TASK_FINALIZATION_SERVICE: Task finalized successfully"
                );

                Ok(FinalizationResult::Success {
                    task_uuid,
                    final_status: format!("{:?}", result.action),
                    completion_time: Some(Utc::now()),
                })
            }
            Err(e) => {
                debug!(
                    task_uuid = %task_uuid,
                    error = %e,
                    "TASK_FINALIZATION_SERVICE: Task finalization failed"
                );

                Ok(FinalizationResult::Failed {
                    error: format!("Task finalization failed: {e}"),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_finalization_service_construction() {
        // Verify the service can be constructed
        // Full integration tests require actor registry
    }
}
