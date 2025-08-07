//! # Messaging Module
//!
//! PostgreSQL message queue (pgmq) based messaging for workflow orchestration.
//! Provides queue-based task and step processing, replacing the TCP command architecture.

pub mod execution_types;
pub mod message;
pub mod orchestration_messages;
pub mod pgmq_client;

pub use execution_types::{
    StepBatchRequest, StepBatchResponse, StepExecutionError, StepExecutionRequest,
    StepExecutionResult, StepRequestMetadata, StepResultMetadata,
};
pub use message::{StepMessage, StepMessageMetadata};
pub use orchestration_messages::*;
pub use pgmq_client::*;
