//! # TAS-43 Task Readiness Event-Driven System
//!
//! Complete implementation of TAS-43 Event-Driven Task Readiness, providing <10ms
//! orchestration latency through PostgreSQL LISTEN/NOTIFY mechanisms combined with
//! hybrid reliability via fallback polling.
//!
//! ## Architecture Overview
//!
//! This system replaces TAS-37's polling-based task discovery with real-time event
//! notifications, while maintaining reliability through intelligent fallback mechanisms.
//! The design follows configuration-driven patterns and delegation-based architecture
//! established throughout the codebase.
//!
//! ## Core Components
//!
//! - **Database Triggers**: PostgreSQL functions that emit pg_notify events on task state changes
//! - **Event Classification**: Config-driven event parsing following ConfigDrivenMessageEvent patterns
//! - **LISTEN/NOTIFY Client**: Robust PostgreSQL notification listener with auto-reconnection
//! - **Task Readiness Coordinator**: Delegates to existing TaskClaimStepEnqueuer for atomic operations
//! - **Fallback Poller**: Safety net using existing tasker_ready_tasks view for missed events
//! - **Enhanced Coordinator**: Unified integration layer combining message-based and task readiness coordination
//!
//! ## Migration Strategy
//!
//! TAS-43 supports gradual migration through three operational modes:
//! - **PollingOnly**: Traditional TAS-37 behavior (safe fallback)
//! - **Hybrid**: Event-driven with polling fallback (recommended production rollout)
//! - **EventDrivenOnly**: Pure event-driven coordination (target state)
//!
//! ## Performance Targets
//!
//! - **<10ms**: Task readiness notification to step enqueueing latency
//! - **<1ms**: Event classification and delegation overhead
//! - **Zero missed tasks**: Hybrid approach ensures 100% task discovery reliability
//! - **Configurable rollback**: Automatic fallback on error thresholds

// Coordinators removed - functionality consolidated into TaskReadinessEventSystem with command pattern
pub mod events;
pub mod fallback_poller;
pub mod listener;

// Re-export key types for external usage
// Note: Coordinators were removed - use TaskReadinessEventSystem from event_systems module instead
pub use events::{
    NamespaceCreatedEvent, ReadinessEventClassifier, ReadinessTrigger, TaskReadinessEvent,
    TaskReadyEvent, TaskStateChangeEvent,
};
pub use fallback_poller::{FallbackPollerStats, ReadinessFallbackPoller};
pub use listener::{TaskReadinessListener, TaskReadinessListenerStats, TaskReadinessNotification};
