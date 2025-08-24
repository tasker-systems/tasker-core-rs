//! # Orchestration Engine
//!
//! High-performance orchestration core for workflow coordination and step execution management.
//!

pub mod backoff_calculator;
pub mod bootstrap;
pub mod config;
pub mod error_classifier;
pub mod errors;
pub mod executor_processors;
pub mod state_manager;
pub mod system_events;
pub mod task_claim;
pub mod task_config_finder;
pub mod types;
pub mod viable_step_discovery;
pub mod workflow_coordinator;

pub use tasker_shared::config::orchestration::{
    OrchestrationSystemConfig, StepEnqueuerConfig, StepResultProcessorConfig,
    TaskClaimStepEnqueuerConfig, TaskClaimerConfig,
};

// Re-export core types and components for easy access
pub use backoff_calculator::{
    BackoffCalculator, BackoffCalculatorConfig, BackoffContext, BackoffError, BackoffResult,
    BackoffType,
};
pub use bootstrap::{
    BootstrapConfig, OrchestrationBootstrap, OrchestrationSystemHandle, SystemStatus,
};

pub use executor_processors::step_enqueuer::{
    NamespaceEnqueueStats, StepEnqueueResult, StepEnqueuer,
};

pub use executor_processors::task_enqueuer::{
    DirectEnqueueHandler, EnqueueError, EnqueueHandler, EnqueueOperation, EnqueuePriority,
    EnqueueRequest, EnqueueResult, EventBasedEnqueueHandler, TaskEnqueuer,
};
pub use executor_processors::task_finalizer::{
    FinalizationAction, FinalizationError, FinalizationResult, TaskFinalizer,
};
pub use executor_processors::task_initializer::{
    TaskInitializationConfig, TaskInitializationError, TaskInitializationResult, TaskInitializer,
};
pub use viable_step_discovery::ViableStepDiscovery;

// Re-export new components (to be implemented)
pub use config::{
    BackoffConfig, ConfigurationManager, DatabaseConfig, EventConfig, ExecutionConfig,
    ReenqueueDelays, TaskerConfig, TelemetryConfig,
};
pub use error_classifier::{
    ErrorCategory, ErrorClassification, ErrorClassifier, ErrorClassifierConfig, ErrorContext,
    RetryStrategy, StandardErrorClassifier,
};

// Use unified event publisher from events module
pub use executor_processors::result_processor::{OrchestrationResultProcessor, StepError};
pub use state_manager::StateManager;
pub use system_events::{
    constants, EventMetadata, StateTransition, SystemEventsConfig, SystemEventsManager,
};
pub use task_claim::finalization_claimer::{
    ClaimGuard, FinalizationClaimResult, FinalizationClaimer, FinalizationClaimerConfig,
};
pub use task_claim::task_claimer::{ClaimedTask, TaskClaimer};
pub use tasker_shared::events::EventPublisher;
pub use types::*;
