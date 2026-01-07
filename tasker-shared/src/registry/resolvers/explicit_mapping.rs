//! # Explicit Mapping Resolver
//!
//! TAS-93: Direct key-to-handler mapping resolver.
//!
//! This resolver provides the highest-priority resolution strategy by mapping
//! explicit callable keys to handler instances or factories. It mirrors the
//! existing Rust registry pattern and is checked first in the resolver chain.
//!
//! ## Priority
//!
//! Priority: **10** (checked first in the resolver chain)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_shared::registry::resolvers::ExplicitMappingResolver;
//! use std::sync::Arc;
//!
//! let resolver = ExplicitMappingResolver::new();
//!
//! // Register a handler instance directly
//! resolver.register_instance("my_handler", Arc::new(MyHandler::new()));
//!
//! // Register a factory function
//! resolver.register("configurable_handler", |ctx| {
//!     let timeout = ctx.initialization.get("timeout_ms")
//!         .and_then(|v| v.as_i64())
//!         .unwrap_or(5000);
//!     Arc::new(ConfigurableHandler::with_timeout(timeout))
//! });
//! ```
//!
//! ## When to Use
//!
//! Use explicit mapping when:
//! - You want deterministic, fast resolution
//! - The callable is a simple key (not a class path)
//! - You need to control handler instantiation

use crate::models::core::task_template::HandlerDefinition;
use crate::registry::{ResolutionContext, ResolvedHandler, StepHandlerResolver};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// Type alias for handler factory functions.
///
/// Factories receive the resolution context and return a handler instance.
/// Use factories when handlers need initialization parameters.
pub type HandlerFactory = Box<dyn Fn(&ResolutionContext) -> Arc<dyn ResolvedHandler> + Send + Sync>;

/// Entry in the handler registry - either an instance or a factory.
enum RegistryEntry {
    /// Pre-instantiated handler (shared across all resolutions)
    Instance(Arc<dyn ResolvedHandler>),

    /// Factory that creates handlers on demand
    Factory(HandlerFactory),
}

impl fmt::Debug for RegistryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryEntry::Instance(h) => write!(f, "Instance({})", h.name()),
            RegistryEntry::Factory(_) => write!(f, "Factory(...)"),
        }
    }
}

/// Explicit mapping resolver for direct key-to-handler lookup.
///
/// This resolver maintains a thread-safe mapping of callable keys to handlers.
/// It supports both pre-instantiated handlers and factory functions for
/// deferred instantiation.
///
/// ## Thread Safety
///
/// Uses `RwLock` for safe concurrent access. Reads are lock-free when no
/// writes are in progress.
pub struct ExplicitMappingResolver {
    /// Handler mappings keyed by callable string
    mappings: RwLock<HashMap<String, RegistryEntry>>,

    /// Resolver name for logging
    name: String,
}

impl fmt::Debug for ExplicitMappingResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mappings = self.mappings.read().unwrap();
        f.debug_struct("ExplicitMappingResolver")
            .field("name", &self.name)
            .field("registered_handlers", &mappings.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for ExplicitMappingResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ExplicitMappingResolver {
    /// Create a new explicit mapping resolver.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mappings: RwLock::new(HashMap::new()),
            name: "ExplicitMappingResolver".to_string(),
        }
    }

    /// Create a resolver with a custom name.
    ///
    /// Useful when multiple explicit mapping resolvers are in the chain.
    #[must_use]
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            mappings: RwLock::new(HashMap::new()),
            name: name.into(),
        }
    }

    /// Register a pre-instantiated handler.
    ///
    /// The handler will be shared across all resolutions of this key.
    /// Use this for stateless handlers or when sharing state is desired.
    ///
    /// # Arguments
    ///
    /// * `key` - The callable string to register
    /// * `handler` - The handler instance to return for this key
    pub fn register_instance(&self, key: impl Into<String>, handler: Arc<dyn ResolvedHandler>) {
        self.mappings
            .write()
            .unwrap()
            .insert(key.into(), RegistryEntry::Instance(handler));
    }

    /// Register a handler factory.
    ///
    /// The factory is called each time this key is resolved, allowing
    /// for per-resolution configuration based on the context.
    ///
    /// # Arguments
    ///
    /// * `key` - The callable string to register
    /// * `factory` - Function that creates handler instances
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// resolver.register("timeout_handler", |ctx| {
    ///     let timeout = ctx.initialization.get("timeout_ms")
    ///         .and_then(|v| v.as_i64())
    ///         .unwrap_or(5000);
    ///     Arc::new(TimeoutHandler::new(timeout as u64))
    /// });
    /// ```
    pub fn register<F>(&self, key: impl Into<String>, factory: F)
    where
        F: Fn(&ResolutionContext) -> Arc<dyn ResolvedHandler> + Send + Sync + 'static,
    {
        self.mappings
            .write()
            .unwrap()
            .insert(key.into(), RegistryEntry::Factory(Box::new(factory)));
    }

    /// Unregister a handler.
    ///
    /// Returns `true` if a handler was removed, `false` if the key wasn't registered.
    pub fn unregister(&self, key: &str) -> bool {
        self.mappings.write().unwrap().remove(key).is_some()
    }

    /// Check if a key is registered.
    #[must_use]
    pub fn is_registered(&self, key: &str) -> bool {
        self.mappings.read().unwrap().contains_key(key)
    }

    /// Get the number of registered handlers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.mappings.read().unwrap().len()
    }

    /// Check if the resolver has no registered handlers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mappings.read().unwrap().is_empty()
    }

    /// Get all registered keys.
    #[must_use]
    pub fn registered_keys(&self) -> Vec<String> {
        self.mappings.read().unwrap().keys().cloned().collect()
    }

    /// Clear all registered handlers.
    pub fn clear(&self) {
        self.mappings.write().unwrap().clear();
    }
}

#[async_trait]
impl StepHandlerResolver for ExplicitMappingResolver {
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        self.mappings
            .read()
            .unwrap()
            .contains_key(&definition.callable)
    }

    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
    ) -> Option<Arc<dyn ResolvedHandler>> {
        let mappings = self.mappings.read().unwrap();
        let entry = mappings.get(&definition.callable)?;

        Some(match entry {
            RegistryEntry::Instance(handler) => Arc::clone(handler),
            RegistryEntry::Factory(factory) => factory(context),
        })
    }

    fn resolver_name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> u32 {
        10 // Highest priority - checked first
    }

    fn registered_callables(&self) -> Vec<String> {
        self.registered_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock handler for testing
    #[derive(Debug)]
    struct MockHandler {
        name: String,
        #[expect(dead_code, reason = "Field used for testing factory configuration")]
        config_value: Option<i64>,
    }

    impl MockHandler {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                config_value: None,
            }
        }

        fn with_config(name: &str, value: i64) -> Self {
            Self {
                name: name.to_string(),
                config_value: Some(value),
            }
        }
    }

    impl ResolvedHandler for MockHandler {
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_new_resolver() {
        let resolver = ExplicitMappingResolver::new();
        assert!(resolver.is_empty());
        assert_eq!(resolver.len(), 0);
        assert_eq!(resolver.resolver_name(), "ExplicitMappingResolver");
    }

    #[test]
    fn test_custom_name() {
        let resolver = ExplicitMappingResolver::with_name("PaymentHandlers");
        assert_eq!(resolver.resolver_name(), "PaymentHandlers");
    }

    #[test]
    fn test_register_instance() {
        let resolver = ExplicitMappingResolver::new();
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("test_handler"));

        resolver.register_instance("my_handler", handler);

        assert!(resolver.is_registered("my_handler"));
        assert!(!resolver.is_registered("other_handler"));
        assert_eq!(resolver.len(), 1);
    }

    #[test]
    fn test_register_factory() {
        let resolver = ExplicitMappingResolver::new();

        resolver.register("factory_handler", |_ctx| {
            Arc::new(MockHandler::new("from_factory"))
        });

        assert!(resolver.is_registered("factory_handler"));
        assert_eq!(resolver.len(), 1);
    }

    #[test]
    fn test_unregister() {
        let resolver = ExplicitMappingResolver::new();
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("test_handler"));

        resolver.register_instance("my_handler", handler);
        assert!(resolver.is_registered("my_handler"));

        let removed = resolver.unregister("my_handler");
        assert!(removed);
        assert!(!resolver.is_registered("my_handler"));

        let not_removed = resolver.unregister("nonexistent");
        assert!(!not_removed);
    }

    #[test]
    fn test_clear() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("h1", Arc::new(MockHandler::new("h1")));
        resolver.register_instance("h2", Arc::new(MockHandler::new("h2")));

        assert_eq!(resolver.len(), 2);

        resolver.clear();
        assert!(resolver.is_empty());
    }

    #[test]
    fn test_registered_keys() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("handler_a", Arc::new(MockHandler::new("a")));
        resolver.register_instance("handler_b", Arc::new(MockHandler::new("b")));

        let mut keys = resolver.registered_keys();
        keys.sort();
        assert_eq!(keys, vec!["handler_a", "handler_b"]);
    }

    #[test]
    fn test_can_resolve() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("my_handler", Arc::new(MockHandler::new("test")));

        let registered_def = HandlerDefinition::builder()
            .callable("my_handler".to_string())
            .build();
        assert!(resolver.can_resolve(&registered_def));

        let unregistered_def = HandlerDefinition::builder()
            .callable("unknown_handler".to_string())
            .build();
        assert!(!resolver.can_resolve(&unregistered_def));
    }

    #[tokio::test]
    async fn test_resolve_instance() {
        let resolver = ExplicitMappingResolver::new();
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("test_handler"));
        resolver.register_instance("my_handler", handler);

        let definition = HandlerDefinition::builder()
            .callable("my_handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved = resolver.resolve(&definition, &context).await;
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name(), "test_handler");
    }

    #[tokio::test]
    async fn test_resolve_factory() {
        let resolver = ExplicitMappingResolver::new();

        resolver.register("configurable", |ctx| {
            let value = ctx
                .initialization
                .get("timeout_ms")
                .and_then(|v| v.as_i64())
                .unwrap_or(1000);
            Arc::new(MockHandler::with_config("configured", value))
        });

        let mut definition = HandlerDefinition::builder()
            .callable("configurable".to_string())
            .build();
        definition
            .initialization
            .insert("timeout_ms".to_string(), serde_json::json!(5000));

        let context = ResolutionContext::from_definition(&definition);

        let resolved = resolver.resolve(&definition, &context).await;
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name(), "configured");
    }

    #[tokio::test]
    async fn test_resolve_unregistered_returns_none() {
        let resolver = ExplicitMappingResolver::new();

        let definition = HandlerDefinition::builder()
            .callable("unregistered".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved = resolver.resolve(&definition, &context).await;
        assert!(resolved.is_none());
    }

    #[test]
    fn test_priority_is_10() {
        let resolver = ExplicitMappingResolver::new();
        assert_eq!(resolver.priority(), 10);
    }

    #[test]
    fn test_register_overwrites_existing() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("handler", Arc::new(MockHandler::new("first")));
        resolver.register_instance("handler", Arc::new(MockHandler::new("second")));

        assert_eq!(resolver.len(), 1);
    }

    #[tokio::test]
    async fn test_overwritten_handler_resolves_to_new() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("handler", Arc::new(MockHandler::new("first")));
        resolver.register_instance("handler", Arc::new(MockHandler::new("second")));

        let definition = HandlerDefinition::builder()
            .callable("handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved = resolver.resolve(&definition, &context).await;
        assert_eq!(resolved.unwrap().name(), "second");
    }

    #[test]
    fn test_debug_formatting() {
        let resolver = ExplicitMappingResolver::new();
        resolver.register_instance("my_handler", Arc::new(MockHandler::new("test")));

        let debug_str = format!("{:?}", resolver);
        assert!(debug_str.contains("ExplicitMappingResolver"));
        assert!(debug_str.contains("my_handler"));
    }

    #[test]
    fn test_resolver_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ExplicitMappingResolver>();
    }

    #[tokio::test]
    async fn test_shared_instance_returns_same_arc() {
        let resolver = ExplicitMappingResolver::new();
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("shared"));
        resolver.register_instance("shared_handler", Arc::clone(&handler));

        let definition = HandlerDefinition::builder()
            .callable("shared_handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved1 = resolver.resolve(&definition, &context).await.unwrap();
        let resolved2 = resolver.resolve(&definition, &context).await.unwrap();

        // Both resolutions should return the same Arc
        assert!(Arc::ptr_eq(&resolved1, &resolved2));
    }

    #[tokio::test]
    async fn test_factory_creates_new_instances() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let resolver = ExplicitMappingResolver::new();
        resolver.register("counting", move |_ctx| {
            let count = counter_clone.fetch_add(1, Ordering::SeqCst);
            Arc::new(MockHandler::new(&format!("instance_{}", count)))
        });

        let definition = HandlerDefinition::builder()
            .callable("counting".to_string())
            .build();
        let context = ResolutionContext::default();

        let r1 = resolver.resolve(&definition, &context).await.unwrap();
        let r2 = resolver.resolve(&definition, &context).await.unwrap();

        assert_eq!(r1.name(), "instance_0");
        assert_eq!(r2.name(), "instance_1");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
