//! # SQL Function Integration Tests
//!
//! This module contains comprehensive tests for PostgreSQL functions that power
//! Tasker's workflow orchestration system.

pub mod production_workflow_validation;
pub mod step_readiness_test;
pub mod system_health_counts_test;
pub mod task_execution_context_test;
