//! # Event Systems - Unified Event-Driven Coordination
//!
//! This module consolidates event-driven coordination systems for orchestration,
//! using shared abstractions from `tasker-shared` for consistent patterns across
//! different event contexts.
//!
//! ## Architecture
//!
//! - **orchestration_event_system**: EventDrivenSystem implementation for orchestration coordination with orchestration_queues components
//! - **task_readiness_event_system**: EventDrivenSystem implementation for task readiness (database events)
//! - **unified_event_coordinator**: Demonstration of unified pattern management across event types
//!
//! ## TAS-43 Refactoring
//!
//! The previous `orchestration_coordinator` has been decomposed into the `orchestration_queues`
//! module with focused components (listener, fallback_poller, events) following the same
//! architectural patterns as task_readiness components.
//!
//! ## Shared Patterns
//!
//! All systems use shared abstractions from `tasker_shared`:
//! - `DeploymentMode` for PollingOnly/Hybrid/EventDrivenOnly configuration
//! - `EventDrivenSystem` trait for unified lifecycle management
//! - `EventSystemStatistics` for consistent monitoring

pub mod orchestration_event_system;
pub mod task_readiness_event_system;
pub mod unified_event_coordinator;

// Re-export key types for external usage
pub use orchestration_event_system::{
    OrchestrationComponentStatistics, OrchestrationEventSystem, OrchestrationEventSystemConfig,
    OrchestrationStatistics,
};
pub use task_readiness_event_system::{
    TaskReadinessEventSystem, TaskReadinessEventSystemConfig, TaskReadinessStatistics,
};
pub use unified_event_coordinator::{
    UnifiedCoordinatorConfig, UnifiedEventCoordinator, UnifiedHealthReport,
};
