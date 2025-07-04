//! # Query Builder System
//!
//! High-performance query building with Rails-style scopes and type safety.
//!
//! ## Overview
//!
//! This module provides a comprehensive query building system that offers Rails ActiveRecord
//! scope functionality with compile-time safety and PostgreSQL-optimized performance.
//!
//! ## Key Components
//!
//! - [`builder`] - Core query builder with SQL generation
//! - [`conditions`] - WHERE clause building with type safety
//! - [`joins`] - JOIN clause management (INNER, LEFT, CROSS, etc.)
//! - [`pagination`] - Efficient pagination with LIMIT/OFFSET
//! - [`scopes`] - Rails-equivalent scopes for complex queries
//!
//! ## Scope Categories
//!
//! ### Task Scopes ([`TaskScopes`])
//! - `by_current_state()` - Find tasks by current state using efficient transition lookup
//! - `active()` - Active tasks with non-terminal workflow steps
//! - `by_annotation()` - Filter by JSONB annotation data
//! - `created_since()` / `completed_since()` / `failed_since()` - Time-based filtering
//! - `in_namespace()` / `with_task_name()` / `with_version()` - Template-based filtering
//!
//! ### WorkflowStep Scopes ([`WorkflowStepScopes`])
//! - `by_current_state()` - Step state filtering with transition tracking
//! - `completed()` / `failed()` / `pending()` - State-specific queries
//! - `completed_since()` / `failed_since()` - Time-based step analysis
//! - `for_tasks_since()` - Task-based step filtering
//!
//! ### Advanced Scope Helpers ([`ScopeHelpers`])
//! - `task_completion_stats()` - Comprehensive task statistics
//! - `ready_steps_for_task()` - Complex DAG analysis for executable steps
//!
//! ## Performance Features
//!
//! - **DISTINCT ON Optimization**: PostgreSQL-specific syntax for "first per group" queries
//! - **EXISTS Clauses**: Efficient subquery patterns vs. IN clauses
//! - **Index-Aware Queries**: Optimized for existing database indexes
//! - **Compile-time Safety**: Type-safe parameter binding prevents SQL injection
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use tasker_core::query_builder::{TaskScopes, QueryBuilder};
//!
//! // Find active tasks in processing state
//! let query = TaskScopes::by_current_state(Some("processing"))
//!     .and(TaskScopes::active())
//!     .limit(100);
//! let sql = query.build_sql();
//!
//! // Complex dependency analysis
//! let ready_steps = ScopeHelpers::ready_steps_for_task(task_id)
//!     .execute(&pool).await?;
//! ```
//!
//! ## Rails Heritage
//!
//! All scopes are direct migrations from Rails ActiveRecord scopes with equivalent
//! functionality and improved performance through PostgreSQL-specific optimizations.

pub mod builder;
pub mod conditions;
pub mod joins;
pub mod pagination;
pub mod scopes;

pub use builder::QueryBuilder;
pub use conditions::{Condition, WhereClause};
pub use joins::{Join, JoinType};
pub use pagination::Pagination;
pub use scopes::*;
