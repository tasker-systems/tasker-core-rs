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
//! - **State Manager**: Task and workflow state management
//!
//! ### Service Layer (TAS-46 Refactoring)
//!
//! **Message Hydration Services** (Phase 4)
//! - Transform lightweight queue messages into rich domain objects
//! - Database-driven hydration for complete data reconstruction
//! - Clean separation of parsing from business logic
//!
//! **Lifecycle Management Services** (Phase 5)
//! - Encapsulate actor delegation patterns
//! - Task initialization, step processing, and finalization
//! - Single-purpose interfaces for command processors
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
pub mod command_processor;
pub mod core;
pub mod state_manager;

// Service Layer (TAS-46 Refactoring)
pub mod hydration;          // Phase 4: Message hydration services
pub mod lifecycle_services; // Phase 5: Lifecycle management services

// Infrastructure
pub mod event_systems;
pub mod orchestration_queues;
pub mod task_readiness;
pub mod system_events;

// Business Logic
pub mod lifecycle;
pub mod backoff_calculator;
pub mod viable_step_discovery;
pub mod error_classifier;
pub mod error_handling_service;

// Configuration and Errors
pub mod config;
pub mod errors;

// ============================================================================
// Re-exports: Core Components
// ============================================================================

pub use bootstrap::{
    BootstrapConfig, OrchestrationBootstrap, OrchestrationSystemHandle, SystemStatus,
};

pub use command_processor::{
    OrchestrationCommand, OrchestrationProcessingStats, OrchestrationProcessor, StepProcessResult,
    SystemHealth, TaskFinalizationResult, TaskInitializeResult,
};

pub use core::{OrchestrationCore, OrchestrationCoreStatus};

pub use state_manager::StateManager;

// ============================================================================
// Re-exports: Service Layer (TAS-46 Refactoring)
// ============================================================================

// Phase 4: Message Hydration Services
pub use hydration::{FinalizationHydrator, StepResultHydrator, TaskRequestHydrator};

// Phase 5: Lifecycle Management Services
pub use lifecycle_services::{
    StepProcessingResult, StepResultProcessingService, TaskFinalizationService,
    TaskInitializationService, TaskInitializationSuccess,
};
// Renamed to avoid conflict with lifecycle::task_finalizer::FinalizationResult
pub use lifecycle_services::FinalizationResult as ServiceFinalizationResult;

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
pub use lifecycle::result_processor::{OrchestrationResultProcessor, StepError};
pub use lifecycle::step_enqueuer::{NamespaceEnqueueStats, StepEnqueueResult, StepEnqueuer};
pub use lifecycle::task_finalizer::{
    FinalizationAction, FinalizationError, FinalizationResult, TaskFinalizer,
};
pub use lifecycle::task_initializer::{
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

// ============================================================================
// Re-exports: Configuration
// ============================================================================

// Orchestration Configuration
pub use config::{
    BackoffConfig, DatabaseConfig, ExecutionConfig, ReenqueueDelays, TaskerConfig, TelemetryConfig,
};

// Shared Configuration (from tasker-shared)
pub use tasker_shared::config::orchestration::{
    OrchestrationSystemConfig, StepEnqueuerConfig, StepResultProcessorConfig,
    TaskClaimStepEnqueuerConfig,
};
pub use tasker_shared::config::{ReadinessFallbackConfig, TaskReadinessNotificationConfig};
