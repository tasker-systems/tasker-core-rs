//! Tasker-specific messaging types and compatibility layer
//!
//! This module re-exports types from both pgmq-notify (for generic functionality) and
//! the tasker_pgmq_client module (for tasker-specific types and methods) to provide
//! a unified API surface for messaging operations.

/// Re-export generic types from pgmq-notify for API compatibility
pub use pgmq_notify::{ClientStatus, QueueMetrics};

/// Re-export tasker-specific types and trait from the extension module
pub use super::tasker_pgmq_client::{
    PgmqStepMessage, PgmqStepMessageMetadata, TaskerPgmqClientExt,
};
