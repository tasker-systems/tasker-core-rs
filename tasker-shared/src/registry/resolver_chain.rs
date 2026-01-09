//! # Resolver Chain
//!
//! TAS-93: Priority-ordered chain of step handler resolvers.
//!
//! The resolver chain manages multiple resolvers and routes handler definitions
//! to the appropriate resolver based on priority and resolver hints.
//!
//! ## Resolution Flow
//!
//! ```text
//! HandlerDefinition ─────┐
//!                        │
//!                   ┌────▼────┐
//!            has    │ Check   │  no
//!   ┌───────hint?───┤ Hint    ├────────────┐
//!   │               └─────────┘            │
//!   │                                      │
//! ┌─▼─────────────┐               ┌────────▼──────────┐
//! │ Find resolver │               │ Iterate by        │
//! │ by name       │               │ priority          │
//! └───────┬───────┘               └────────┬──────────┘
//!         │                                │
//!         ▼                                ▼
//!   ┌─────────────┐               ┌─────────────────────┐
//!   │ Direct      │               │ Try each resolver   │
//!   │ resolution  │               │ that can_resolve()  │
//!   └─────────────┘               └─────────────────────┘
//! ```
//!
//! ## Priority System
//!
//! Resolvers are tried in priority order (lower = checked first):
//! - 10: ExplicitMappingResolver (direct key lookup)
//! - 20-99: Custom resolvers (domain-specific patterns)
//! - 100: ClassConstantResolver (fallback class path lookup)
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use tasker_shared::registry::{ResolverChain, StepHandlerResolver};
//! use std::sync::Arc;
//!
//! // Create chain with custom resolvers
//! let chain = ResolverChain::new()
//!     .with_resolver(Arc::new(ExplicitMappingResolver::new()))
//!     .with_resolver(Arc::new(PaymentHandlerResolver::new()))
//!     .with_resolver(Arc::new(ClassConstantResolver::new()));
//!
//! // Resolve a handler definition
//! let definition = HandlerDefinition::builder()
//!     .callable("payments:stripe:refund")
//!     .build();
//! let context = ResolutionContext::default();
//!
//! match chain.resolve(&definition, &context).await {
//!     Ok(handler) => println!("Resolved: {}", handler.name()),
//!     Err(error) => eprintln!("Failed: {}", error),
//! }
//! ```

use crate::models::core::task_template::HandlerDefinition;

use super::step_handler_resolver::{
    ResolutionContext, ResolutionError, ResolvedHandler, StepHandlerResolver,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, instrument, trace, warn};

/// A priority-ordered chain of step handler resolvers.
///
/// The resolver chain manages multiple resolvers and coordinates handler
/// resolution through either hint-based direct routing or priority-ordered
/// iteration.
///
/// ## Thread Safety
///
/// `ResolverChain` is `Send + Sync` since it only holds `Arc` references
/// to resolvers. Resolvers can be added via `with_resolver()` during
/// construction (builder pattern).
#[derive(Debug, Default)]
pub struct ResolverChain {
    /// Resolvers ordered by priority (lower priority = earlier in vec)
    resolvers: Vec<Arc<dyn StepHandlerResolver>>,

    /// Index by resolver name for hint-based bypass
    resolvers_by_name: HashMap<String, Arc<dyn StepHandlerResolver>>,
}

impl ResolverChain {
    /// Create a new empty resolver chain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a resolver to the chain.
    ///
    /// Resolvers are automatically sorted by priority (lower = checked first).
    /// This method consumes self and returns it, enabling builder pattern usage.
    ///
    /// # Arguments
    ///
    /// * `resolver` - The resolver to add
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<dyn StepHandlerResolver>) -> Self {
        let name = resolver.resolver_name().to_string();
        self.resolvers_by_name.insert(name, Arc::clone(&resolver));
        self.resolvers.push(resolver);
        self.sort_by_priority();
        self
    }

    /// Add a resolver to an existing chain (mutable).
    ///
    /// This is useful when you need to add resolvers dynamically after
    /// initial construction.
    pub fn add_resolver(&mut self, resolver: Arc<dyn StepHandlerResolver>) {
        let name = resolver.resolver_name().to_string();
        self.resolvers_by_name.insert(name, Arc::clone(&resolver));
        self.resolvers.push(resolver);
        self.sort_by_priority();
    }

    /// Sort resolvers by priority (lower = earlier).
    fn sort_by_priority(&mut self) {
        self.resolvers.sort_by_key(|r| r.priority());
    }

    /// Get the number of resolvers in the chain.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resolvers.len()
    }

    /// Check if the chain has no resolvers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resolvers.is_empty()
    }

    /// Get resolver names in priority order.
    #[must_use]
    pub fn resolver_names(&self) -> Vec<&str> {
        self.resolvers.iter().map(|r| r.resolver_name()).collect()
    }

    /// Check if a resolver with the given name exists.
    #[must_use]
    pub fn has_resolver(&self, name: &str) -> bool {
        self.resolvers_by_name.contains_key(name)
    }

    /// Check if any resolver in the chain can resolve the given definition.
    ///
    /// This is a quick check without actually performing resolution.
    #[must_use]
    pub fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        // If hint is provided, check if that resolver can handle it
        if let Some(ref resolver_name) = definition.resolver {
            if let Some(resolver) = self.resolvers_by_name.get(resolver_name) {
                return resolver.can_resolve(definition);
            }
            return false;
        }

        // Otherwise, check if any resolver can handle it
        self.resolvers.iter().any(|r| r.can_resolve(definition))
    }

    /// Get all callable keys that are registered with any resolver.
    ///
    /// This aggregates registered callables from all resolvers that support
    /// introspection via the `registered_callables` method.
    #[must_use]
    pub fn registered_callables(&self) -> Vec<String> {
        let mut callables = Vec::new();
        for resolver in &self.resolvers {
            callables.extend(resolver.registered_callables());
        }
        callables.sort();
        callables.dedup();
        callables
    }

    /// Resolve a handler definition to a handler instance.
    ///
    /// Resolution flow:
    /// 1. If `definition.resolver` is set, use hint-based bypass
    /// 2. Otherwise, iterate through resolvers by priority
    /// 3. Return the first successful resolution, or an error if none succeed
    ///
    /// # Arguments
    ///
    /// * `definition` - The handler definition to resolve
    /// * `context` - Additional context for resolution
    ///
    /// # Returns
    ///
    /// A handler instance, or an error if resolution failed.
    #[instrument(
        skip(self, context),
        fields(
            callable = %definition.callable,
            resolver_hint = ?definition.resolver,
        )
    )]
    pub async fn resolve(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
    ) -> Result<Arc<dyn ResolvedHandler>, ResolutionError> {
        // Check for hint-based bypass
        if let Some(ref resolver_name) = definition.resolver {
            return self
                .resolve_with_hint(definition, context, resolver_name)
                .await;
        }

        // Iterate through resolvers by priority
        self.resolve_by_priority(definition, context).await
    }

    /// Resolve using a specific resolver (hint-based bypass).
    async fn resolve_with_hint(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
        resolver_name: &str,
    ) -> Result<Arc<dyn ResolvedHandler>, ResolutionError> {
        debug!(resolver = resolver_name, "Using hint-based resolver bypass");

        let resolver = self.resolvers_by_name.get(resolver_name).ok_or_else(|| {
            ResolutionError::new(
                &definition.callable,
                format!("Resolver '{}' not found in chain", resolver_name),
            )
            .with_hint(format!(
                "Available resolvers: {}",
                self.resolver_names().join(", ")
            ))
        })?;

        // Check if the resolver can handle this definition
        if !resolver.can_resolve(definition) {
            warn!(
                resolver = resolver_name,
                callable = %definition.callable,
                "Hinted resolver cannot handle this callable"
            );
            return Err(ResolutionError::new(
                &definition.callable,
                format!(
                    "Resolver '{}' cannot handle callable '{}'",
                    resolver_name, definition.callable
                ),
            )
            .with_tried_resolver(resolver_name.to_string())
            .with_hint("The resolver hint may be incorrect for this callable format"));
        }

        // Attempt resolution
        resolver.resolve(definition, context).await.ok_or_else(|| {
            ResolutionError::new(
                &definition.callable,
                format!(
                    "Resolver '{}' could not resolve callable '{}'",
                    resolver_name, definition.callable
                ),
            )
            .with_tried_resolver(resolver_name.to_string())
        })
    }

    /// Resolve by iterating through resolvers in priority order.
    async fn resolve_by_priority(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
    ) -> Result<Arc<dyn ResolvedHandler>, ResolutionError> {
        let mut tried_resolvers = Vec::new();

        for resolver in &self.resolvers {
            let resolver_name = resolver.resolver_name();

            // Quick check if resolver can handle this format
            if !resolver.can_resolve(definition) {
                trace!(
                    resolver = resolver_name,
                    callable = %definition.callable,
                    "Resolver cannot handle this callable format"
                );
                continue;
            }

            tried_resolvers.push(resolver_name.to_string());
            debug!(
                resolver = resolver_name,
                callable = %definition.callable,
                "Attempting resolution"
            );

            // Attempt resolution
            if let Some(handler) = resolver.resolve(definition, context).await {
                debug!(
                    resolver = resolver_name,
                    handler = handler.name(),
                    "Successfully resolved handler"
                );
                return Ok(handler);
            }

            trace!(
                resolver = resolver_name,
                "Resolution returned None, trying next resolver"
            );
        }

        // No resolver could handle it
        Err(ResolutionError::no_resolver_found(
            &definition.callable,
            tried_resolvers,
        ))
    }

    /// Get statistics about resolver usage.
    #[must_use]
    pub fn stats(&self) -> ResolverChainStats {
        ResolverChainStats {
            resolver_count: self.resolvers.len(),
            resolver_names: self
                .resolver_names()
                .into_iter()
                .map(String::from)
                .collect(),
            priorities: self.resolvers.iter().map(|r| r.priority()).collect(),
        }
    }
}

/// Statistics about a resolver chain.
#[derive(Debug, Clone)]
pub struct ResolverChainStats {
    /// Number of resolvers in the chain
    pub resolver_count: usize,

    /// Names of resolvers in priority order
    pub resolver_names: Vec<String>,

    /// Priorities of resolvers in order
    pub priorities: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::RwLock;

    // Mock handler for testing
    #[derive(Debug)]
    struct MockHandler {
        name: String,
    }

    impl MockHandler {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl ResolvedHandler for MockHandler {
        fn name(&self) -> &str {
            &self.name
        }
    }

    // Mock resolver for testing
    #[derive(Debug)]
    struct MockResolver {
        name: String,
        priority: u32,
        prefix: String,
        handlers: RwLock<HashMap<String, Arc<dyn ResolvedHandler>>>,
    }

    impl MockResolver {
        fn new(name: &str, prefix: &str, priority: u32) -> Self {
            Self {
                name: name.to_string(),
                priority,
                prefix: prefix.to_string(),
                handlers: RwLock::new(HashMap::new()),
            }
        }

        fn register(&self, key: &str, handler: Arc<dyn ResolvedHandler>) {
            self.handlers
                .write()
                .unwrap()
                .insert(key.to_string(), handler);
        }
    }

    #[async_trait]
    impl StepHandlerResolver for MockResolver {
        fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
            definition.callable.starts_with(&self.prefix)
        }

        async fn resolve(
            &self,
            definition: &HandlerDefinition,
            _context: &ResolutionContext,
        ) -> Option<Arc<dyn ResolvedHandler>> {
            self.handlers
                .read()
                .unwrap()
                .get(&definition.callable)
                .cloned()
        }

        fn resolver_name(&self) -> &str {
            &self.name
        }

        fn priority(&self) -> u32 {
            self.priority
        }
    }

    #[test]
    fn test_empty_chain() {
        let chain = ResolverChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.resolver_names().is_empty());
    }

    #[test]
    fn test_add_resolvers() {
        let resolver1: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("First", "first:", 10));
        let resolver2: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("Second", "second:", 20));

        let chain = ResolverChain::new()
            .with_resolver(resolver1)
            .with_resolver(resolver2);

        assert_eq!(chain.len(), 2);
        assert!(!chain.is_empty());
        assert!(chain.has_resolver("First"));
        assert!(chain.has_resolver("Second"));
        assert!(!chain.has_resolver("Third"));
    }

    #[test]
    fn test_priority_ordering() {
        let low_priority: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("Low", "low:", 100));
        let high_priority: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("High", "high:", 10));
        let mid_priority: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("Mid", "mid:", 50));

        // Add in non-priority order
        let chain = ResolverChain::new()
            .with_resolver(low_priority)
            .with_resolver(high_priority)
            .with_resolver(mid_priority);

        // Should be sorted by priority
        let names = chain.resolver_names();
        assert_eq!(names, vec!["High", "Mid", "Low"]);

        let stats = chain.stats();
        assert_eq!(stats.priorities, vec![10, 50, 100]);
    }

    #[test]
    fn test_mutable_add() {
        let mut chain = ResolverChain::new();

        let resolver: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("Dynamic", "dyn:", 50));
        chain.add_resolver(resolver);

        assert_eq!(chain.len(), 1);
        assert!(chain.has_resolver("Dynamic"));
    }

    #[tokio::test]
    async fn test_resolve_by_priority() {
        let resolver1 = Arc::new(MockResolver::new("First", "first:", 10));
        resolver1.register(
            "first:handler",
            Arc::new(MockHandler::new("handler_from_first")),
        );

        let resolver2 = Arc::new(MockResolver::new("Second", "second:", 20));
        resolver2.register(
            "second:handler",
            Arc::new(MockHandler::new("handler_from_second")),
        );

        let chain = ResolverChain::new()
            .with_resolver(resolver1 as Arc<dyn StepHandlerResolver>)
            .with_resolver(resolver2 as Arc<dyn StepHandlerResolver>);

        // Test resolving from first resolver
        let definition = HandlerDefinition::builder()
            .callable("first:handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "handler_from_first");

        // Test resolving from second resolver
        let definition = HandlerDefinition::builder()
            .callable("second:handler".to_string())
            .build();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "handler_from_second");
    }

    #[tokio::test]
    async fn test_resolve_with_hint() {
        let resolver1 = Arc::new(MockResolver::new("First", "first:", 10));
        resolver1.register(
            "first:handler",
            Arc::new(MockHandler::new("handler_from_first")),
        );

        let resolver2 = Arc::new(MockResolver::new("Second", "second:", 20));
        resolver2.register(
            "second:handler",
            Arc::new(MockHandler::new("handler_from_second")),
        );

        let chain = ResolverChain::new()
            .with_resolver(resolver1 as Arc<dyn StepHandlerResolver>)
            .with_resolver(resolver2 as Arc<dyn StepHandlerResolver>);

        // Use hint to directly route to Second resolver
        let definition = HandlerDefinition::builder()
            .callable("second:handler".to_string())
            .resolver("Second".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "handler_from_second");
    }

    #[tokio::test]
    async fn test_hint_resolver_not_found() {
        let resolver: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("First", "first:", 10));
        let chain = ResolverChain::new().with_resolver(resolver);

        let definition = HandlerDefinition::builder()
            .callable("some:handler".to_string())
            .resolver("NonExistent".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.message.contains("not found"));
        assert!(error.hint.is_some());
    }

    #[tokio::test]
    async fn test_hint_resolver_cannot_handle() {
        let resolver = Arc::new(MockResolver::new("First", "first:", 10));
        let chain = ResolverChain::new().with_resolver(resolver as Arc<dyn StepHandlerResolver>);

        // Hint points to First, but callable doesn't match First's prefix
        let definition = HandlerDefinition::builder()
            .callable("other:handler".to_string())
            .resolver("First".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.message.contains("cannot handle"));
    }

    #[tokio::test]
    async fn test_no_resolver_found() {
        let resolver = Arc::new(MockResolver::new("First", "first:", 10));
        // Register a handler but try to resolve a different one
        resolver.register(
            "first:registered",
            Arc::new(MockHandler::new("registered_handler")),
        );

        let chain = ResolverChain::new().with_resolver(resolver as Arc<dyn StepHandlerResolver>);

        // Try to resolve an unregistered handler
        let definition = HandlerDefinition::builder()
            .callable("first:unregistered".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.tried_resolvers.contains(&"First".to_string()));
    }

    #[tokio::test]
    async fn test_empty_chain_resolution() {
        let chain = ResolverChain::new();

        let definition = HandlerDefinition::builder()
            .callable("any:handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.tried_resolvers.is_empty());
    }

    #[tokio::test]
    async fn test_fallback_to_next_resolver() {
        // First resolver matches but doesn't have the handler
        let resolver1 = Arc::new(MockResolver::new("First", "shared:", 10));

        // Second resolver also matches and has the handler
        let resolver2 = Arc::new(MockResolver::new("Second", "shared:", 20));
        resolver2.register(
            "shared:handler",
            Arc::new(MockHandler::new("found_in_second")),
        );

        let chain = ResolverChain::new()
            .with_resolver(resolver1 as Arc<dyn StepHandlerResolver>)
            .with_resolver(resolver2 as Arc<dyn StepHandlerResolver>);

        let definition = HandlerDefinition::builder()
            .callable("shared:handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let result = chain.resolve(&definition, &context).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "found_in_second");
    }

    #[test]
    fn test_stats() {
        let resolver1: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("First", "first:", 10));
        let resolver2: Arc<dyn StepHandlerResolver> =
            Arc::new(MockResolver::new("Second", "second:", 50));

        let chain = ResolverChain::new()
            .with_resolver(resolver1)
            .with_resolver(resolver2);

        let stats = chain.stats();
        assert_eq!(stats.resolver_count, 2);
        assert_eq!(stats.resolver_names, vec!["First", "Second"]);
        assert_eq!(stats.priorities, vec![10, 50]);
    }

    #[test]
    fn test_chain_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ResolverChain>();
    }
}
