//! # Actor-Based Architecture for Lifecycle Components
//!
//! TAS-46: This module implements a lightweight Actor pattern that formalizes
//! the relationship between Commands and Lifecycle Components, providing clear
//! boundaries and better testability without the overhead of a full actor framework.
//!
//! ## Overview
//!
//! The actor pattern here provides:
//! - **Clear Boundaries**: Each lifecycle component becomes an Actor with explicit message handling
//! - **Better Testability**: Actors can be tested in isolation with message-based interfaces
//! - **Supervision Hooks**: Optional lifecycle methods (started/stopped) for resource management
//! - **Consistent Patterns**: All actors follow the same construction and interaction patterns
//!
//! ## Architecture
//!
//! ```text
//! OrchestrationCommand ────→ ActorRegistry ────→ Specific Actor
//!                                 │                      │
//!                                 │                      ├─→ Handler<M>
//!                                 │                      │
//!                                 └──────────────────────┴─→ Lifecycle Component
//! ```
//!
//! ## Key Components
//!
//! - [`OrchestrationActor`]: Base trait for all actors with lifecycle hooks
//! - [`Handler<M>`]: Message handling trait for specific message types
//! - [`Message`]: Marker trait for command messages
//! - [`ActorRegistry`]: Registry managing all orchestration actors
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use tasker_orchestration::actors::{OrchestrationActor, Handler, Message};
//! use std::sync::Arc;
//! use async_trait::async_trait;
//!
//! // Define a message
//! pub struct InitializeTaskMessage {
//!     pub request: TaskRequest,
//! }
//!
//! impl Message for InitializeTaskMessage {
//!     type Response = TaskInitializationResult;
//! }
//!
//! // Define an actor
//! pub struct TaskRequestActor {
//!     context: Arc<SystemContext>,
//!     processor: TaskRequestProcessor,
//! }
//!
//! impl OrchestrationActor for TaskRequestActor {
//!     fn name(&self) -> &'static str { "TaskRequestActor" }
//!     fn context(&self) -> &Arc<SystemContext> { &self.context }
//! }
//!
//! #[async_trait]
//! impl Handler<InitializeTaskMessage> for TaskRequestActor {
//!     type Response = TaskInitializationResult;
//!
//!     async fn handle(&self, msg: InitializeTaskMessage)
//!         -> TaskerResult<TaskInitializationResult> {
//!         // Delegate to existing processor
//!         self.processor.process_task_request(&msg.request).await
//!     }
//! }
//! ```

pub mod registry;
pub mod result_processor_actor;
pub mod step_enqueuer_actor;
pub mod task_finalizer_actor;
pub mod task_request_actor;
pub mod traits;

// Re-export core traits for convenience
pub use registry::ActorRegistry;
pub use result_processor_actor::{ProcessStepResultMessage, ResultProcessorActor};
pub use step_enqueuer_actor::{ProcessBatchMessage, StepEnqueuerActor};
pub use task_finalizer_actor::{FinalizeTaskMessage, TaskFinalizerActor};
pub use task_request_actor::{ProcessTaskRequestMessage, TaskRequestActor};
pub use traits::{Handler, Message, OrchestrationActor};
