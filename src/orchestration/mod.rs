//! # Orchestration Engine
//!
//! High-performance orchestration core for workflow coordination and step execution management.
//!
//! ## Architecture
//!
//! The orchestration engine follows a **delegation-based architecture** where:
//! - **Rust provides the orchestration core**: High-performance decision-making, dependency resolution, and state management
//! - **Frameworks handle execution**: Queue management and business logic execution
//! - **SQL functions provide intelligence**: Existing database functions handle complex orchestration logic
//!
//! ## Core Components
//!
//! - **OrchestrationCoordinator**: Main orchestration engine that coordinates task execution lifecycle
//! - **StepExecutor**: Individual step execution and lifecycle management within orchestration core
//! - **ViableStepDiscovery**: Uses SQL functions to determine which steps are ready for execution
//! - **StateManager**: Manages state transitions using SQL functions for evaluation
//! - **EventPublisher**: Publishes orchestration events across FFI boundaries
//! - **TaskHandlerRegistry**: Dual-path registry for both Rust and FFI task handler management
//! - **ConfigurationManager**: YAML-driven configuration with environment overrides
//! - **BaseStepHandler**: Configuration-driven step execution framework with hooks for business logic implementation
//!
//! ## Integration with SQL Functions
//!
//! The orchestration engine leverages existing SQL functions from the Rails Tasker system:
//! - `get_step_readiness_status()`: Determines step execution readiness including circuit breaker logic
//! - `calculate_dependency_levels()`: Provides dependency graph analysis
//! - `get_task_execution_context()`: Comprehensive task state evaluation
//! - `get_system_health_counts()`: System-wide health monitoring
//!
//! For complete implementation details, see `docs/ORCHESTRATION_ANALYSIS.md`.

pub mod backoff_calculator;
pub mod config;
pub mod coordinator;
pub mod errors;
pub mod event_publisher;
pub mod registry;
pub mod state_manager;
pub mod step_executor;
pub mod step_handler;
pub mod system_events;
pub mod task_finalizer;
pub mod types;
pub mod viable_step_discovery;
pub mod workflow_coordinator;

// Re-export core types and components for easy access
pub use coordinator::{
    OrchestrationCoordinator, StepExecutionDelegate, StepExecutionResult, StepExecutionStatus,
    TaskOrchestrationResult,
};
pub use step_executor::{
    ExecutionPriority, ExecutionStats, RetryConfig, StepExecutionConfig, StepExecutionMetrics,
    StepExecutionRequest, StepExecutor,
};
pub use viable_step_discovery::ViableStepDiscovery;
pub use workflow_coordinator::{
    WorkflowCoordinator, WorkflowCoordinatorConfig, WorkflowExecutionMetrics,
};
// pub use task_finalizer::TaskFinalizer;
// pub use backoff_calculator::BackoffCalculator;

// Re-export new components (to be implemented)
pub use config::{ConfigurationManager, StepTemplate, TaskTemplate, TaskerConfig};
pub use errors::*;
pub use event_publisher::EventPublisher;
pub use registry::TaskHandlerRegistry;
pub use state_manager::StateManager;
pub use step_handler::{
    BaseStepHandler, ExecutionStatus, StepExecutionContext, StepExecutionEvent, StepHandler,
    StepHandlerFactory, StepResult,
};
pub use system_events::{
    constants, EventMetadata, StateTransition, SystemEventsConfig, SystemEventsManager,
};
pub use types::ViableStep;
pub use types::*;
