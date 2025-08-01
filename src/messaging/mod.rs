//! # Messaging Module
//!
//! PostgreSQL message queue (pgmq) based messaging for workflow orchestration.
//! Provides queue-based task and step processing, replacing the TCP command architecture.

pub mod pgmq_client;
pub mod message;

pub use pgmq_client::*;
pub use message::*;