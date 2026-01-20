//! Integration tests for cross-crate workflows.
//!
//! Requires: Database + messaging backend (PGMQ or RabbitMQ)
//! Enable with: --features test-messaging

#![cfg(feature = "test-messaging")]

mod common;
mod integration;
