//! # Orchestration Queues - Queue-Level Event Coordination
//!
//! This module provides focused components for orchestration queue event handling,
//! creating architectural consistency with the database-level task_readiness components.
//!
//! ## Architecture
//!
//! - **listener**: tasker-pgmq listener for real-time orchestration queue events
//! - **fallback_poller**: Polling fallback for queue messages when events fail
//! - **events**: Event type definitions for orchestration queue messages
//!
//! ## Deployment Mode Support
//!
//! - **PollingOnly**: Only fallback_poller active
//! - **Hybrid**: listener (primary) + fallback_poller (backup)
//! - **EventDrivenOnly**: Only listener active
//!
//! ## Integration
//!
//! These components are used directly by OrchestrationEventSystem, following the same
//! architectural patterns as task_readiness components for consistent modularity.

pub mod events;
pub mod fallback_poller;
pub mod listener;

// Re-export key types for external usage
pub use events::OrchestrationQueueEvent;
pub use fallback_poller::{
    OrchestrationFallbackPoller, OrchestrationPollerConfig, OrchestrationPollerStats,
};
pub use listener::{
    OrchestrationListenerConfig, OrchestrationListenerStats, OrchestrationNotification,
    OrchestrationQueueListener,
};
