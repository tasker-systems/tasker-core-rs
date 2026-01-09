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
//! - **StepHandlerResolver**: Strategy pattern for step handler resolution (TAS-93)
//! - **ResolverChain**: Priority-ordered chain of step handler resolvers (TAS-93)
//! - **MethodDispatchWrapper**: Framework-level method dispatch for handlers (TAS-93)
//! - **resolvers**: Built-in resolver implementations (TAS-93)
//! - **PluginRegistry**: Dynamic plugin discovery and lifecycle management
//! - **SubscriberRegistry**: Event subscriber management with pattern matching
//!
//! ## Architecture
//!
//! ```text
//! Registry Infrastructure
//! ├── TaskHandlerRegistry       (Orchestration task handlers)
//! ├── StepHandlerResolver       (Step handler resolution strategy)
//! ├── ResolverChain             (Priority-ordered resolver coordination)
//! ├── MethodDispatchWrapper     (Method dispatch for resolved handlers)
//! └── resolvers/
//!     ├── ExplicitMappingResolver  (Priority 10: direct key lookup)
//!     └── ClassConstantResolver    (Priority 100: class path fallback)
//! ```
//!
//! ## TAS-93 Resolution Flow
//!
//! ```text
//! HandlerDefinition ─────►  ResolverChain  ─────►  MethodDispatchWrapper
//!   callable: "..."           (resolves)             (wraps handler with
//!   method: "..."                                     target method)
//!   resolver: "..."
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

pub mod method_dispatch;
pub mod resolver_chain;
pub mod resolvers;
pub mod step_handler_resolver;
pub mod task_handler_registry;

pub use method_dispatch::{
    validate_method_support, wrap_with_method, MethodDispatchError, MethodDispatchInfo,
    MethodDispatchWrapper,
};
pub use resolver_chain::{ResolverChain, ResolverChainStats};
pub use step_handler_resolver::{
    ResolutionContext, ResolutionError, ResolvedHandler, StepHandlerResolver,
};
pub use task_handler_registry::{
    HandlerKey, RegistryStats as TaskHandlerRegistryStats, TaskHandlerRegistry,
    TaskTemplateDiscoveryResult,
};
