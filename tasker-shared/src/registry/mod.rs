//! # Registry Infrastructure
//!
//! General-purpose registries for system components, plugins, and subscribers.
//!
//! ## Overview
//!
//! The registry module provides infrastructure for managing different types of
//! registries across the system. This is separate from the orchestration-specific
//! TaskHandlerRegistry to maintain separation of concerns.
//!
//! ## Available Registries
//!
//! - **TaskHandlerRegistry**: Task handler registration and resolution (orchestration-specific)
//! - **PluginRegistry**: Dynamic plugin discovery and lifecycle management
//! - **SubscriberRegistry**: Event subscriber management with pattern matching
//!
//! ## Architecture
//!
//! ```text
//! Registry Infrastructure
//! ├── TaskHandlerRegistry   (Orchestration task handlers)
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::registry::TaskHandlerRegistry;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create task handler registry
//! let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! let task_handler_registry = TaskHandlerRegistry::new(pool);
//!     Ok(())
//! }
//!
//! ```

pub mod task_handler_registry;

pub use task_handler_registry::{
    HandlerKey, RegistryStats as TaskHandlerRegistryStats, TaskHandlerRegistry,
};
