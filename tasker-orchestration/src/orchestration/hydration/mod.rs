//! # Message Hydration Layer
//!
//! Transforms lightweight queue messages into rich domain objects through database hydration.
//!
//! ## Purpose
//!
//! Workers and external systems submit minimal message payloads to PGMQ queues to keep
//! message sizes small and queue operations fast. This module provides hydrators that
//! reconstruct full domain objects from these minimal messages by querying the database.
//!
//! ## Design Philosophy
//!
//! - **Plain Services**: Hydrators are simple services, not actors - they just transform data
//! - **Database-Driven**: Leverage existing database records as source of truth
//! - **Validation Included**: Ensure data integrity during hydration
//! - **Clear Error Handling**: Explicit error messages for each hydration step
//!
//! ## Available Hydrators
//!
//! - [`StepResultHydrator`]: StepMessage → StepExecutionResult (with database lookup)
//! - [`TaskRequestHydrator`]: PGMQ message → TaskRequestMessage
//! - [`FinalizationHydrator`]: PGMQ message → task_uuid
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::hydration::{
//!     StepResultHydrator, TaskRequestHydrator, FinalizationHydrator
//! };
//! use std::sync::Arc;
//!
//! # async fn example(context: Arc<tasker_shared::system_context::SystemContext>, message: pgmq::Message) -> tasker_shared::TaskerResult<()> {
//! // Step result hydration (with database lookup)
//! let step_hydrator = StepResultHydrator::new(context);
//! let result = step_hydrator.hydrate_from_message(&message).await?;
//!
//! // Task request hydration (message parsing only)
//! let task_hydrator = TaskRequestHydrator::new();
//! let task_request = task_hydrator.hydrate_from_message(&message).await?;
//!
//! // Finalization hydration (extract task_uuid)
//! let finalization_hydrator = FinalizationHydrator::new();
//! let task_uuid = finalization_hydrator.hydrate_from_message(&message).await?;
//!
//! # Ok(())
//! # }
//! ```

pub mod finalization_hydrator;
pub mod step_result_hydrator;
pub mod task_request_hydrator;

// Re-export hydrators for convenience
pub use finalization_hydrator::FinalizationHydrator;
pub use step_result_hydrator::StepResultHydrator;
pub use task_request_hydrator::TaskRequestHydrator;
