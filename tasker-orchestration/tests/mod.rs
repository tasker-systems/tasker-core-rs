//! Orchestration Tests Module
//!
//! External integration tests for the tasker-orchestration crate.
//! These tests verify orchestration-specific functionality including:
//!
//! - Coordinator management and scaling
//! - Circuit breaker behavior
//! - Messaging and queue integration
//! - End-to-end workflow execution
//! - Web API endpoints
//!

pub mod circuit_breaker;
pub mod complex_workflow_integration_test;
pub mod configuration_integration_test;
pub mod coordinator;
pub mod messaging;
pub mod state_manager;
pub mod task_initializer_test;
