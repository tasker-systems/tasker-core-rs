//! Model Tests Module
//!
//! This module contains all model tests using SQLx native testing
//! for automatic database isolation.

// SQLx native tests - each test gets its own database!
pub mod annotation_type;
pub mod dependent_system;
pub mod dependent_system_object_map;
pub mod task_transition;

// Orchestration model tests
pub mod step_dag_relationship;
pub mod step_readiness_status;
pub mod task_execution_context;
