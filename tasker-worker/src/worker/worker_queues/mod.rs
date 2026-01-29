//! # Worker Queues - Queue-Level Event Coordination for Workers
//!
//! This module provides focused components for worker queue event handling,
//! creating architectural consistency with the orchestration queue components.
//!
//! ## Architecture
//!
//! - **listener**: tasker-pgmq listener for real-time worker namespace queue events
//! - **fallback_poller**: Polling fallback for namespace queue messages when events fail
//! - **events**: Event type definitions for worker namespace queue messages
//!
//! ## Deployment Mode Support
//!
//! - **PollingOnly**: Only fallback_poller active
//! - **Hybrid**: listener (primary) + fallback_poller (backup)
//! - **EventDrivenOnly**: Only listener active
//!
//! ## Integration
//!
//! These components are used directly by WorkerEventSystem, following the same
//! architectural patterns as orchestration_queues for consistent modularity.

pub mod events;
pub mod fallback_poller;
pub mod listener;

// Re-export key types for internal usage
pub use events::{WorkerNotification, WorkerQueueEvent};
