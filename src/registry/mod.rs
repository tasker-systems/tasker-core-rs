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
//! ├── PluginRegistry        (Plugin lifecycle management)
//! └── SubscriberRegistry    (Event subscription management)
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::{TaskHandlerRegistry, PluginRegistry, SubscriberRegistry};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create task handler registry
//! let pool = sqlx::PgPool::connect("postgresql://localhost/test").await?;
//! let task_handler_registry = TaskHandlerRegistry::new(pool);
//!
//! // Create plugin registry
//! let mut plugin_registry = PluginRegistry::new();
//! plugin_registry.register_plugin("analytics", "1.0.0", "Analytics plugin").await?;
//!
//! // Create subscriber registry
//! let subscriber_registry = SubscriberRegistry::new();
//! // Register subscribers for events...
//! # Ok(())
//! # }
//! ```

pub mod plugin_registry;
pub mod subscriber_registry;
pub mod task_handler_registry;

// Re-export main types for easy access
pub use plugin_registry::{Plugin, PluginMetadata, PluginRegistry, PluginState, PluginStats};
pub use subscriber_registry::{
    EventSubscriber, SubscriberDetail, SubscriberRegistry, SubscriberStats,
};
pub use task_handler_registry::{
    HandlerKey, RegistryStats as TaskHandlerRegistryStats, TaskHandlerRegistry,
};
