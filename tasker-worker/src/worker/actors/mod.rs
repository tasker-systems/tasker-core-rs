//! # Worker Actor System
//!
//! TAS-69: Actor-based architecture for the worker system, mirroring the orchestration
//! actor pattern from TAS-46.
//!
//! ## Architecture
//!
//! The worker actor system provides:
//! - **WorkerActor trait**: Base trait with lifecycle hooks for all worker actors
//! - **`Handler<M>` trait**: Type-safe message handling for specific message types
//! - **Message trait**: Marker trait for command messages with associated response types
//! - **WorkerActorRegistry**: Central registry managing actor lifecycle and access
//!
//! ## Actors
//!
//! The system defines 5 worker actors:
//!
//! | Actor | Purpose |
//! |-------|---------|
//! | StepExecutorActor | Executes workflow steps via FFI handlers |
//! | FFICompletionActor | Processes step completion and sends results |
//! | TemplateCacheActor | Manages task template caching |
//! | DomainEventActor | Dispatches domain events (wraps DomainEventSystem) |
//! | WorkerStatusActor | Provides health and status information |
//!
//! ## Usage
//!
//! ```rust,ignore
//! // WorkerActorRegistry::build requires multiple dependencies from worker bootstrap
//! use tasker_worker::worker::actors::{WorkerActorRegistry, Handler};
//! use tasker_shared::system_context::SystemContext;
//! use std::sync::Arc;
//!
//! async fn example(context: Arc<SystemContext>) -> Result<(), Box<dyn std::error::Error>> {
//!     // Build registry with all actors (requires full worker bootstrap context)
//!     let (registry, dispatch_channels) = WorkerActorRegistry::build(
//!         context,
//!         worker_id,
//!         task_template_manager,
//!         event_publisher,
//!         domain_event_handle,
//!         dispatch_mode_config,
//!     ).await?;
//!
//!     // Access actors for message handling
//!     let step_executor = &registry.step_executor_actor;
//!
//!     // Registry manages lifecycle - shutdown when done
//!     // registry.shutdown().await;
//!     Ok(())
//! }
//! ```

mod domain_event_actor;
mod ffi_completion_actor;
mod messages;
mod registry;
mod step_executor_actor;
mod template_cache_actor;
mod traits;
mod worker_status_actor;

// Re-export core traits
pub use traits::{Handler, Message, WorkerActor};

// Re-export registry and dispatch mode types (TAS-67)
pub use registry::{DispatchChannels, DispatchModeConfig, WorkerActorRegistry};

// Re-export actors
pub use domain_event_actor::DomainEventActor;
pub use ffi_completion_actor::FFICompletionActor;
pub use step_executor_actor::StepExecutorActor;
pub use template_cache_actor::TemplateCacheActor;
pub use worker_status_actor::WorkerStatusActor;

// Re-export messages
pub use messages::{
    DispatchEventsMessage, DispatchHandlerMessage, ExecuteStepFromEventMessage,
    ExecuteStepFromPgmqMessage, ExecuteStepFromQueuedMessage, ExecuteStepMessage,
    ExecuteStepWithCorrelationMessage, GetEventStatusMessage, GetWorkerStatusMessage,
    HealthCheckMessage, ProcessStepCompletionMessage, RefreshTemplateCacheMessage,
    SendStepResultMessage, SetEventIntegrationMessage, TraceContext,
};
