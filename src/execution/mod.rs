//! Execution Module - Legacy Message Protocols
//!
//! This module contains legacy message protocol definitions that are being
//! phased out in favor of the pgmq-based messaging system. These types are
//! maintained for backward compatibility during the transition.

pub mod message_protocols;

pub use message_protocols::{
    StepBatchRequest, StepBatchResponse, StepExecutionRequest, StepExecutionResult,
};
