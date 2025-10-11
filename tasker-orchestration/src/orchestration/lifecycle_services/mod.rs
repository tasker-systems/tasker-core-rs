//! # Lifecycle Management Services
//!
//! Services that manage task and step lifecycle operations by delegating to actors.
//!
//! ## Purpose
//!
//! This module provides clean service interfaces for task lifecycle management,
//! encapsulating the actor delegation patterns and allowing command processors
//! to focus on routing rather than business logic orchestration.
//!
//! ## Design Philosophy
//!
//! - **Plain Services**: Simple wrappers around actor delegation
//! - **Clean Interfaces**: Focused single-purpose methods
//! - **Error Handling**: Comprehensive error transformation and logging
//! - **Actor Delegation**: All business logic stays in actors
//!
//! ## Available Services
//!
//! - [`TaskInitializationService`]: Manages task initialization via TaskRequestActor
//! - [`StepResultProcessingService`]: Manages step result processing via ResultProcessorActor
//! - [`TaskFinalizationService`]: Manages task finalization via TaskFinalizerActor
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_orchestration::orchestration::lifecycle_services::{
//!     TaskInitializationService, StepResultProcessingService, TaskFinalizationService
//! };
//! use std::sync::Arc;
//!
//! # async fn example(actors: Arc<crate::actors::ActorRegistry>) -> tasker_shared::TaskerResult<()> {
//! // Task initialization
//! let init_service = TaskInitializationService::new(actors.clone());
//! // let result = init_service.initialize_task(request).await?;
//!
//! // Step result processing
//! let processing_service = StepResultProcessingService::new(actors.clone());
//! // let result = processing_service.process_step_result(step_result).await?;
//!
//! // Task finalization
//! let finalization_service = TaskFinalizationService::new(actors.clone());
//! // let result = finalization_service.finalize_task(task_uuid).await?;
//!
//! # Ok(())
//! # }
//! ```

pub mod step_result_processing_service;
pub mod task_finalization_service;
pub mod task_initialization_service;

// Re-export services for convenience
pub use step_result_processing_service::{StepProcessingResult, StepResultProcessingService};
pub use task_finalization_service::{FinalizationResult, TaskFinalizationService};
pub use task_initialization_service::{TaskInitializationService, TaskInitializationSuccess};
