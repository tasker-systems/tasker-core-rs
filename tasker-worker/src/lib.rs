//! # tasker-worker: TAS-40 Simple Worker Command Pattern
//!
//! This crate provides a simple command pattern worker system that replaces
//! complex executor pools, auto-scaling coordinators, and polling systems
//! with pure tokio command processing and declarative FFI event integration.
//!
//! ## Key Features
//!
//! - **Simple Command Pattern**: ~100 lines vs 1000+ lines of complex worker system
//! - **No Polling**: Pure command-driven architecture with tokio channels
//! - **Declarative FFI Integration**: Event-driven communication with Ruby/Python/WASM
//! - **Database-as-API**: Workers hydrate context from SimpleStepMessage UUIDs
//! - **Bidirectional Events**: Worker → FFI execution requests, FFI → Worker completions
//! - **Ruby Integration**: Uses Ruby-aligned StepHandlerCallResult types
//! - **Orchestration Integration**: Simple command-based communication
//!
//! ## Architecture
//!
//! ```text
//! WorkerProcessor -> tokio::mpsc -> Command Handlers -> Database Hydration
//!     ↓                                                        ↓
//! WorkerEventPublisher -> WorkerEventSystem -> FFI Handler (Ruby/Python/WASM)
//!     ↑                                           ↓
//! WorkerEventSubscriber <- WorkerEventSystem <- FFI Handler Completion
//!     ↓                                           
//! StepExecutionResult -> OrchestrationCore
//! ```
//!
//! ## Event-Driven Integration Example
//!
//! ```rust
//! use tasker_worker::{WorkerProcessor, WorkerEventPublisher, WorkerEventSubscriber};
//! use tasker_shared::system_context::SystemContext;
//!
//! // Create worker with event integration
//! let context = SystemContext::new().await?;
//! let (mut processor, sender) = WorkerProcessor::new("namespace".to_string(), context, 100);
//!
//! // Event publisher fires events to FFI handlers
//! let event_publisher = WorkerEventPublisher::new(worker_id, namespace);
//!
//! // Event subscriber receives completions from FFI handlers  
//! let event_subscriber = WorkerEventSubscriber::new(worker_id, namespace);
//! let completion_receiver = event_subscriber.start_completion_listener();
//!
//! // Processor integrates both command and event channels
//! processor.start_with_events(completion_receiver).await?;
//! ```

pub mod api_clients;
pub mod bootstrap;
pub mod command_processor;
pub mod config;
pub mod error;
pub mod event_publisher;
pub mod event_subscriber;
pub mod health;
pub mod task_template_manager;
pub mod web;
pub mod worker;

// Testing infrastructure (only available in test builds or with test feature)
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use api_clients::OrchestrationApiClient;
pub use bootstrap::{
    WorkerBootstrap, WorkerBootstrapConfig, WorkerSystemHandle, WorkerSystemStatus,
};
pub use command_processor::{
    EventIntegrationStatus, StepExecutionStats, WorkerCommand, WorkerProcessor, WorkerStatus,
};
pub use error::{WorkerError, Result};
pub use event_publisher::{WorkerEventError, WorkerEventPublisher, WorkerEventPublisherStats};
pub use event_subscriber::{
    CorrelatedCompletionListener, CorrelatedStepResult, WorkerEventSubscriber,
    WorkerEventSubscriberError, WorkerEventSubscriberStats,
};
pub use health::WorkerHealthStatus;
pub use task_template_manager::{
    CacheStats, CachedTemplate, TaskTemplateManager, TaskTemplateManagerConfig,
    WorkerTaskTemplateOperations,
};
pub use worker::{WorkerCore, WorkerCoreStatus};
