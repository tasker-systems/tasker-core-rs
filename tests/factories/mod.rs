//! # Factory System for Complex Workflow Testing
//!
//! This module provides a comprehensive factory system built on the factori crate
//! with SQLx integration for creating complex workflow test data.
//!
//! ## Key Features
//!
//! - **SQLx Integration**: All factories work directly with database persistence
//! - **Dependency Resolution**: Foundational objects are created first
//! - **State Machine Support**: Proper state transitions with audit trails
//! - **Complex Relationships**: DAG patterns, dependencies, and workflow edges
//! - **Ergonomic API**: Leverages factori's trait-based approach
//!
//! ## Factory Organization
//!
//! 1. **Foundation Factories**: Base objects that other factories depend on
//! 2. **Core Model Factories**: Main domain objects (Task, WorkflowStep, etc.)
//! 3. **Relationship Factories**: Dependencies and edges between objects
//! 4. **State Factories**: Transitions and state machine management
//! 5. **Pattern Factories**: Complex workflow patterns (diamond, parallel, etc.)
//!
//! ## Usage Example
//!
//! ```rust
//! use crate::factories::*;
//! use sqlx::PgPool;
//!
//! #[sqlx::test]
//! async fn test_complex_workflow(pool: PgPool) -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a complete workflow with dependencies
//!     let task = TaskFactory::default()
//!         .with_context(json!({"order_id": 12345}))
//!         .with_state_transitions()
//!         .create(&pool)
//!         .await?;
//!
//!     let diamond_workflow = DiamondWorkflowFactory::default()
//!         .for_task(task.task_id)
//!         .create(&pool)
//!         .await?;
//!
//!     assert_eq!(diamond_workflow.steps.len(), 4);
//!     assert_eq!(diamond_workflow.edges.len(), 4);
//!
//!     Ok(())
//! }
//! ```

pub mod base;
pub mod complex_workflows;
pub mod core;
pub mod foundation;
pub mod patterns;
pub mod relationships;
pub mod states;
// TODO: Implement additional composite workflow patterns
// pub mod api_integration_workflow;
// pub mod dummy_task_workflow;

// Note: Factories are imported directly in test files to prevent
// clippy from removing "unused" exports. See factory_tests.rs.
