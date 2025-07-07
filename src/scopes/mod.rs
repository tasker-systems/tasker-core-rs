//! # Query Scopes Module
//!
//! This module provides Rails-like query scopes for all Tasker models.
//! Scopes enable chainable, composable queries with compile-time verification.
//!
//! ## Architecture
//!
//! The scope system is built on several key components:
//!
//! ### Common Infrastructure
//! - `ScopeBuilder` trait: Defines standard query execution methods (`all`, `first`, `count`, `exists`)
//! - `state_helpers`: Provides consistent state-based SQL condition generation
//! - `SpecialQuery` enum: Handles complex queries that need custom execution (like CTEs)
//!
//! ### Model-Specific Scopes
//! - **TaskScope**: 8 scopes for filtering tasks by state, time, and relationships
//! - **WorkflowStepScope**: 13 scopes for workflow step execution states and dependencies  
//! - **TaskTransitionScope**: 6 scopes for task state transition history
//! - **WorkflowStepTransitionScope**: 4 scopes for workflow step state transitions
//! - **WorkflowStepEdgeScope**: 5 scopes for DAG navigation and dependency analysis
//! - **NamedTaskScope**: 3 scopes for task template management and versioning
//! - **TaskNamespaceScope**: 1 scope for namespace organization
//!
//! ## Usage Patterns
//!
//! ### Basic Filtering
//! ```rust,no_run
//! use tasker_core::models::Task;
//! use tasker_core::scopes::ScopeBuilder;
//! use chrono::{Utc, Duration};
//! # async fn example(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
//! // Find all pending tasks created in the last hour
//! let recent_tasks = Task::scope()
//!     .created_since(Utc::now() - Duration::hours(1))
//!     .all(pool)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### State-Based Queries
//! ```rust,no_run
//! use tasker_core::models::WorkflowStep;
//! use tasker_core::scopes::ScopeBuilder;
//! # async fn example(pool: &sqlx::PgPool, task_id: i64) -> Result<(), sqlx::Error> {
//! // Find all active workflow steps for a specific task
//! let active_steps = WorkflowStep::scope()
//!     .for_task(task_id)
//!     .active()
//!     .all(pool)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Complex DAG Queries
//! ```rust,no_run
//! use tasker_core::models::WorkflowStepEdge;
//! use tasker_core::scopes::ScopeBuilder;
//! # async fn example(pool: &sqlx::PgPool, step_id: i64) -> Result<(), sqlx::Error> {
//! // Find all sibling workflow steps (steps with same dependencies)
//! let siblings = WorkflowStepEdge::scope()
//!     .siblings_of(step_id)
//!     .all(pool)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### JOIN Limitations
//!
//! **Important**: Due to SQLx QueryBuilder constraints, JOINs must be added before WHERE conditions.
//! Always call scopes that require JOINs first:
//!
//! ```rust,no_run
//! use tasker_core::models::Task;
//! use tasker_core::scopes::ScopeBuilder;
//! use chrono::{Utc, Duration};
//! # async fn example(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
//! # let timestamp = Utc::now() - Duration::hours(1);
//! // ✅ Correct order
//! Task::scope()
//!     .with_task_name("process_order".to_string())  // Adds JOIN
//!     .created_since(timestamp)         // Adds WHERE
//!     .all(pool).await?;
//!
//! // ❌ Incorrect order (JOIN will be ignored)
//! Task::scope()
//!     .created_since(timestamp)         // Adds WHERE
//!     .with_task_name("process_order".to_string())  // JOIN ignored!
//!     .all(pool).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Security
//!
//! All scopes use parameterized queries with proper SQLx binding to prevent SQL injection.
//! The `siblings_of` scope uses a sophisticated CTE with parameter binding for safe operation.

pub mod common;
pub mod edges;
pub mod named_task;
pub mod task;
pub mod transitions;
pub mod workflow_step;

// Re-export common functionality
pub use common::{ScopeBuilder, SpecialQuery};

// Re-export all scope builders
pub use edges::WorkflowStepEdgeScope;
pub use named_task::{NamedTaskScope, TaskNamespaceScope};
pub use task::TaskScope;
pub use transitions::{TaskTransitionScope, WorkflowStepTransitionScope};
pub use workflow_step::WorkflowStepScope;
