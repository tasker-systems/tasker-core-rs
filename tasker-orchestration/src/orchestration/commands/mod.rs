//! # Orchestration Commands Module
//!
//! Command types, result structures, and the command processing service for orchestration.
//!
//! ## Structure
//!
//! - `types`: Command enum and result types
//! - `service`: Business logic service implementing three lifecycle flows
//! - `pgmq_message_resolver`: PGMQ-specific signal resolution infrastructure

pub mod types;
pub(crate) mod pgmq_message_resolver;
pub mod service;

pub use types::{
    AtomicProcessingStats, CommandResponder, OrchestrationCommand, OrchestrationProcessingStats,
    StepProcessResult, SystemHealth, TaskFinalizationResult, TaskInitializeResult,
    TaskReadinessResult,
};
pub use service::CommandProcessingService;
