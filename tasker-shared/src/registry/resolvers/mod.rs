//! # Built-in Step Handler Resolvers
//!
//! TAS-93: Standard resolvers for the step handler resolution chain.
//!
//! ## Resolver Priority Order
//!
//! Resolvers are tried in priority order (lower number = checked first):
//!
//! | Priority | Resolver | Description |
//! |----------|----------|-------------|
//! | 10 | `ExplicitMappingResolver` | Direct key-to-handler lookup |
//! | 20-99 | Custom resolvers | Domain-specific patterns |
//! | 100 | `ClassConstantResolver` | Class path fallback (no-op in Rust) |
//!
//! ## Building a Resolver Chain
//!
//! ```rust,ignore
//! use tasker_shared::registry::{ResolverChain, resolvers::*};
//! use std::sync::Arc;
//!
//! // Create resolvers
//! let explicit = Arc::new(ExplicitMappingResolver::new());
//! let class_constant = Arc::new(ClassConstantResolver::new());
//!
//! // Register handlers with explicit mapping
//! explicit.register_instance("my_handler", Arc::new(MyHandler::new()));
//!
//! // Build chain (automatically sorted by priority)
//! let chain = ResolverChain::new()
//!     .with_resolver(explicit)
//!     .with_resolver(class_constant);
//! ```
//!
//! ## Custom Resolvers
//!
//! To create a custom resolver, implement `StepHandlerResolver`:
//!
//! ```rust,ignore
//! use tasker_shared::registry::{StepHandlerResolver, ResolutionContext, ResolvedHandler};
//!
//! struct PaymentResolver { /* ... */ }
//!
//! #[async_trait]
//! impl StepHandlerResolver for PaymentResolver {
//!     fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
//!         definition.callable.starts_with("payments:")
//!     }
//!
//!     async fn resolve(
//!         &self,
//!         definition: &HandlerDefinition,
//!         context: &ResolutionContext,
//!     ) -> Option<Arc<dyn ResolvedHandler>> {
//!         // Parse "payments:stripe:refund" and lookup handler
//!         // ...
//!     }
//!
//!     fn resolver_name(&self) -> &str { "PaymentResolver" }
//!     fn priority(&self) -> u32 { 20 } // After explicit, before class constant
//! }
//! ```

mod class_constant;
mod explicit_mapping;

pub use class_constant::ClassConstantResolver;
pub use explicit_mapping::{ExplicitMappingResolver, HandlerFactory};
