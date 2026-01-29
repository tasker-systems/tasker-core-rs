//! # Messaging Module
//!
//! Provider-agnostic messaging for workflow orchestration.
//! Supports PGMQ, RabbitMQ, and in-memory backends through the `MessagingProvider` abstraction.
//!
//! ## Module Structure
//!
//! - `client` - Domain-level `MessageClient` struct (TAS-133c)
//! - `service` - Provider-agnostic messaging abstraction (TAS-133)
//! - `errors` - Messaging error types
//! - `execution_types` - Step execution request/response types
//! - `message` - Step and task message types
//! - `orchestration_messages` - Orchestration-specific message types
//!
//! ## TAS-133 Migration (Complete)
//!
//! The legacy `clients` module has been removed. Use:
//! - `MessageClient` struct from `client` module for domain operations
//! - `MessagingProvider` enum from `service` module for provider abstraction
//! - `PgmqMessagingService` from `service::providers` for PGMQ-specific features

pub mod client;
pub mod errors;
pub mod execution_types;
pub mod message;
pub mod orchestration_messages;
pub mod service;

// Re-export PgmqClient directly from tasker-pgmq for code that needs low-level access
pub use tasker_pgmq::PgmqClient;

// Re-export error types
pub use errors::{MessagingError, MessagingResult};

// Re-export execution types
pub use execution_types::{
    BatchProcessingOutcome, CursorConfig, DecisionPointOutcome, StepBatchRequest,
    StepBatchResponse, StepExecutionError, StepExecutionMetadata, StepExecutionRequest,
    StepExecutionResult, StepRequestMetadata,
};

// Re-export message types
pub use message::StepMessage;

// Re-export orchestration messages
pub use orchestration_messages::*;
