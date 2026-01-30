//! # SQL Function Integration Tests
//!
//! This module contains comprehensive tests for PostgreSQL functions that power
//! Tasker's workflow orchestration system.

#[path = "sql_functions/production_workflow_validation.rs"]
pub mod production_workflow_validation;
#[path = "sql_functions/sql_function_executor_test.rs"]
pub mod sql_function_executor_test;
#[path = "sql_functions/step_readiness_test.rs"]
pub mod step_readiness_test;
#[path = "sql_functions/system_health_counts_test.rs"]
pub mod system_health_counts_test;
#[path = "sql_functions/task_execution_context_test.rs"]
pub mod task_execution_context_test;
