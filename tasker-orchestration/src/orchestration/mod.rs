//! # Orchestration Engine
//!
//! High-performance orchestration core for workflow coordination and step execution management.
//!

pub mod backoff_calculator;
pub mod bootstrap;
pub mod command_processor;
pub mod config;
pub mod core;
pub mod error_classifier;
pub mod errors;
pub mod event_systems;
pub mod lifecycle;
pub mod orchestration_queues;
pub mod state_manager;
pub mod system_events;
pub mod task_claim;
pub mod task_readiness;
pub mod types;
pub mod viable_step_discovery;

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

pub use lifecycle::step_enqueuer::{NamespaceEnqueueStats, StepEnqueueResult, StepEnqueuer};

pub use lifecycle::task_enqueuer::{
    DirectEnqueueHandler, EnqueueError, EnqueueHandler, EnqueueOperation, EnqueuePriority,
    EnqueueRequest, EnqueueResult, EventBasedEnqueueHandler, TaskEnqueuer,
};
pub use lifecycle::task_finalizer::{
    FinalizationAction, FinalizationError, FinalizationResult, TaskFinalizer,
};
pub use lifecycle::task_initializer::{
    TaskInitializationConfig, TaskInitializationError, TaskInitializationResult, TaskInitializer,
};
pub use viable_step_discovery::ViableStepDiscovery;

// Re-export command pattern components (TAS-40)
pub use command_processor::{
    OrchestrationCommand, OrchestrationProcessingStats, OrchestrationProcessor, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult,
};
pub use core::{OrchestrationCore, OrchestrationCoreStatus};
// Re-export TAS-43 Unified Event System components from event_systems namespace
pub use event_systems::{
    OrchestrationComponentStatistics, OrchestrationEventSystem, OrchestrationEventSystemConfig,
    OrchestrationStatistics, TaskReadinessEventSystem, TaskReadinessEventSystemConfig,
    TaskReadinessStatistics, UnifiedCoordinatorConfig, UnifiedEventCoordinator,
    UnifiedHealthReport,
};

// Re-export TAS-43 Task Readiness components
pub use task_readiness::{
    FallbackPollerStats,
    NamespaceCreatedEvent,
    // Core task readiness functionality (database event specific)
    // Note: Coordinators removed - use TaskReadinessEventSystem from event_systems module instead
    ReadinessEventClassifier,
    ReadinessFallbackPoller,
    ReadinessTrigger,
    TaskReadinessEvent,
    TaskReadinessListener,
    TaskReadinessListenerStats,
    TaskReadinessNotification,
    TaskReadyEvent,
    TaskStateChangeEvent,
};

// Re-export TAS-43 Orchestration Queue components
pub use orchestration_queues::{
    OrchestrationFallbackPoller,
    OrchestrationListenerConfig,
    OrchestrationListenerStats,
    OrchestrationPollerConfig,
    OrchestrationPollerStats,
    // Core orchestration queue functionality (queue event specific)
    // Focused components that replace EventDrivenOrchestrationCoordinator
    OrchestrationQueueEvent,
    OrchestrationQueueListener,
};

// Re-export unified event-driven patterns from tasker-shared
pub use tasker_shared::{
    DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus, EventDrivenSystem,
    EventSystemStatistics, SystemStatistics,
};

// Re-export configuration types from tasker-shared for convenience
pub use tasker_shared::config::{ReadinessFallbackConfig, TaskReadinessNotificationConfig};

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
pub use lifecycle::result_processor::{OrchestrationResultProcessor, StepError};
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
