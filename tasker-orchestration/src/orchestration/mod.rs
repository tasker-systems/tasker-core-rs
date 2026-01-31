//! # Orchestration Engine
//!
//! High-performance orchestration core for workflow coordination and step execution management.
//!
//! ## Architecture Overview
//!
//! This module implements a service-oriented architecture with clear separation of concerns:
//!
//! ### Core Components
//!
//! - **Command Processor**: Pure command routing without business logic
//! - **Orchestration Core**: System-level orchestration coordination
//! - **Health Check Evaluator**: Pure-function health status evaluation
//!
//! ### Service Layer (TAS-46 Refactoring)
//!
//! **Message Hydration Services**
//! - Transform lightweight queue messages into rich domain objects
//! - Database-driven hydration for complete data reconstruction
//! - Clean separation of parsing from business logic
//!
//! ### Infrastructure
//!
//! - **Event Systems**: Real-time coordination via LISTEN/NOTIFY
//! - **Queue Management**: PGMQ-based message queues
//! - **Bootstrap**: System initialization and configuration
//!
//! ### Business Logic
//!
//! - **Lifecycle Components**: Task initializers, finalizers, result processors
//! - **Backoff Calculator**: Intelligent retry coordination
//! - **Viable Step Discovery**: Dependency-based step readiness
//! - **Error Handling**: Classification and retry strategy management
//!
//! ## Design Principles
//!
//! 1. **Services over inline logic**: All business logic lives in focused services
//! 2. **Clear dependencies**: Infrastructure → Services → Command Processor
//! 3. **Single responsibility**: Each service has one clear purpose
//! 4. **Actor delegation**: Services wrap actors, preserving sophisticated business logic

// ============================================================================
// Module Declarations
// ============================================================================

// Core Components
pub mod bootstrap;
pub mod channels; // TAS-133: Semantic NewType channel wrappers
pub mod commands; // TAS-148: Command types (extracted from legacy command_processor)
pub mod core;
pub mod health_check_evaluator;

// Service Layer (TAS-46 Refactoring)
pub mod hydration; // Phase 4: Message hydration services

// Infrastructure
pub mod event_systems;
pub mod orchestration_queues;
pub mod system_events;
pub mod task_readiness;

// Business Logic
pub mod backoff_calculator;
pub mod error_classifier;
pub mod error_handling_service;
pub mod lifecycle;
pub mod viable_step_discovery;

// Background Services (TAS-49 Phase 2)
pub mod staleness_detector;

// Configuration and Errors
pub mod config;
pub mod errors;

// ============================================================================
// Re-exports: Core Components
// ============================================================================

pub use bootstrap::{
    BootstrapConfig, OrchestrationBootstrap, OrchestrationSystemHandle, SystemStatus,
};

// TAS-133: Semantic channel wrappers for type-safe channel usage
pub use channels::{
    ChannelFactory, OrchestrationCommandReceiver, OrchestrationCommandSender,
    OrchestrationNotificationReceiver, OrchestrationNotificationSender,
};

// TAS-148: Re-export command types from the commands module
pub use commands::{
    CommandResponder, OrchestrationCommand, OrchestrationProcessingStats, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult, TaskReadinessResult,
};

pub use core::{OrchestrationCore, OrchestrationCoreStatus};

// ============================================================================
// Re-exports: Service Layer (TAS-46 Refactoring)
// ============================================================================

// Command Processing Service (TAS-46: actor/service separation)
pub use commands::CommandProcessingService;

// Phase 4: Message Hydration Services
pub use hydration::{FinalizationHydrator, StepResultHydrator, TaskRequestHydrator};

// ============================================================================
// Re-exports: Infrastructure
// ============================================================================

// Event Systems (TAS-43 Unified Event System)
pub use event_systems::{
    OrchestrationComponentStatistics, OrchestrationEventSystem, OrchestrationEventSystemConfig,
    OrchestrationStatistics, TaskReadinessEventSystem, UnifiedCoordinatorConfig,
    UnifiedEventCoordinator, UnifiedHealthReport,
};

// Queue Management (TAS-43 Orchestration Queues)
pub use orchestration_queues::{
    OrchestrationFallbackPoller, OrchestrationListenerConfig, OrchestrationListenerStats,
    OrchestrationPollerConfig, OrchestrationPollerStats, OrchestrationQueueEvent,
    OrchestrationQueueListener,
};

// Task Readiness (TAS-43 components)
pub use task_readiness::{FallbackPoller, FallbackPollerConfig};

// System Events
pub use system_events::{
    constants, EventMetadata, StateTransition, SystemEventsConfig, SystemEventsManager,
};

// Event Publisher (from tasker-shared)
pub use tasker_shared::events::EventPublisher;

// Event-Driven Patterns (from tasker-shared)
pub use tasker_shared::{
    DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus, EventDrivenSystem,
    EventSystemStatistics, SystemStatistics,
};

// ============================================================================
// Re-exports: Business Logic
// ============================================================================

// Lifecycle Components
pub use lifecycle::result_processing::{OrchestrationResultProcessor, StepError};
pub use lifecycle::step_enqueuer::{NamespaceEnqueueStats, StepEnqueueResult, StepEnqueuer};
pub use lifecycle::task_finalization::{
    FinalizationAction, FinalizationError, FinalizationResult, TaskFinalizer,
};
pub use lifecycle::task_initialization::{
    TaskInitializationError, TaskInitializationResult, TaskInitializer,
};

// Backoff and Retry Logic
pub use backoff_calculator::{
    BackoffCalculator, BackoffCalculatorConfig, BackoffContext, BackoffError, BackoffResult,
    BackoffType,
};

// Step Discovery
pub use viable_step_discovery::ViableStepDiscovery;

// Error Handling
pub use error_classifier::{
    ErrorCategory, ErrorClassification, ErrorClassifier, ErrorClassifierConfig, ErrorContext,
    RetryStrategy, StandardErrorClassifier,
};
pub use error_handling_service::{
    ErrorHandlingAction, ErrorHandlingConfig, ErrorHandlingResult, ErrorHandlingService,
};

// Background Services (TAS-49 Phase 2)
pub use staleness_detector::{StalenessDetector, StalenessResult};

// ============================================================================
// Re-exports: Configuration
// ============================================================================

// Orchestration Configuration
pub use config::{
    BackoffConfig, DatabaseConfig, ExecutionConfig, ReenqueueDelaysConfig, TaskerConfig,
    TelemetryConfig,
};

// Shared Configuration (from tasker-shared)
pub use tasker_shared::config::orchestration::{
    OrchestrationSystemConfig, StepEnqueuerConfig, StepResultProcessorConfig,
    TaskClaimStepEnqueuerConfig,
};
// TAS-61: ReadinessFallbackConfig and TaskReadinessNotificationConfig removed (part of deleted TaskReadinessConfig)
