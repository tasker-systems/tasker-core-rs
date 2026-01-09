//! # Step Handler Resolver Strategy Pattern
//!
//! TAS-93: Developer-facing strategy pattern for step handler resolution.
//!
//! This module provides the resolver abstraction layer that sits between
//! handler definitions (YAML templates) and handler instances (executable code).
//!
//! ## Architecture
//!
//! ```text
//! HandlerDefinition ─┐
//!   callable: "..."  │     ┌─────────────────┐     ┌─────────────────┐
//!   method: "..."    ├────►│  ResolverChain  ├────►│  Handler        │
//!   resolver: "..."  │     │  (priority-     │     │  Instance       │
//!                    │     │   ordered)      │     └─────────────────┘
//!                    └─────└─────────────────┘
//! ```
//!
//! ## The Three Concerns
//!
//! | Field | Purpose | Owner |
//! |-------|---------|-------|
//! | `callable` | Address/lookup key for finding the handler | Resolver interprets |
//! | `method` | Entry point method to invoke on the handler | Framework handles |
//! | `resolver` | Resolution strategy hint (bypass chain) | Framework routes |
//!
//! ## Resolver Responsibilities
//!
//! Resolvers have a single job: interpret `callable` as a lookup key and return
//! a handler instance. They do NOT handle method dispatch - the framework wraps
//! the returned handler if `method` is specified.
//!
//! ## Example: Custom Resolver
//!
//! ```rust,ignore
//! use tasker_shared::registry::{StepHandlerResolver, ResolutionContext};
//! use tasker_shared::models::core::task_template::HandlerDefinition;
//! use async_trait::async_trait;
//!
//! struct PaymentHandlerResolver {
//!     registry: HashMap<String, Arc<dyn StepHandler>>,
//! }
//!
//! #[async_trait]
//! impl StepHandlerResolver for PaymentHandlerResolver {
//!     fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
//!         // Match our custom format: "payments:provider:action"
//!         definition.callable.starts_with("payments:")
//!     }
//!
//!     async fn resolve(
//!         &self,
//!         definition: &HandlerDefinition,
//!         _context: &ResolutionContext,
//!     ) -> Option<Arc<dyn StepHandler>> {
//!         // Parse "payments:stripe:refund" and lookup handler
//!         let parts: Vec<&str> = definition.callable.split(':').collect();
//!         if parts.len() == 3 {
//!             let key = format!("{}_{}", parts[1], parts[2]);
//!             self.registry.get(&key).cloned()
//!         } else {
//!             None
//!         }
//!     }
//!
//!     fn resolver_name(&self) -> &str {
//!         "PaymentHandlerResolver"
//!     }
//!
//!     fn priority(&self) -> u32 {
//!         20 // After explicit mapping (10), before class constant (100)
//!     }
//! }
//! ```

use crate::models::core::task_template::HandlerDefinition;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Context provided to resolvers during resolution.
///
/// Contains additional information that may be useful for handler instantiation,
/// such as initialization parameters from the handler definition.
#[derive(Debug, Clone, Default)]
pub struct ResolutionContext {
    /// Initialization parameters from the handler definition
    pub initialization: HashMap<String, serde_json::Value>,

    /// Correlation ID for logging/tracing
    pub correlation_id: Option<String>,

    /// Namespace context (e.g., from task template)
    pub namespace: Option<String>,
}

impl ResolutionContext {
    /// Create a new resolution context from a handler definition.
    #[must_use]
    pub fn from_definition(definition: &HandlerDefinition) -> Self {
        Self {
            initialization: definition.initialization.clone(),
            correlation_id: None,
            namespace: None,
        }
    }

    /// Add correlation ID for tracing.
    #[must_use]
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Add namespace context.
    #[must_use]
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }
}

/// Error returned when handler resolution fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionError {
    /// The callable that could not be resolved
    pub callable: String,

    /// List of resolvers that were tried
    pub tried_resolvers: Vec<String>,

    /// Human-readable error message
    pub message: String,

    /// Optional hint about how to fix the error
    pub hint: Option<String>,
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Failed to resolve handler '{}': {}",
            self.callable, self.message
        )?;
        if !self.tried_resolvers.is_empty() {
            write!(f, " (tried: {})", self.tried_resolvers.join(", "))?;
        }
        if let Some(hint) = &self.hint {
            write!(f, " Hint: {}", hint)?;
        }
        Ok(())
    }
}

impl std::error::Error for ResolutionError {}

impl ResolutionError {
    /// Create a new resolution error.
    #[must_use]
    pub fn new(callable: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            callable: callable.into(),
            message: message.into(),
            tried_resolvers: Vec::new(),
            hint: None,
        }
    }

    /// Add a resolver to the list of tried resolvers.
    #[must_use]
    pub fn with_tried_resolver(mut self, resolver: impl Into<String>) -> Self {
        self.tried_resolvers.push(resolver.into());
        self
    }

    /// Add a hint about how to fix the error.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Create an error for when no resolver could handle the callable.
    #[must_use]
    pub fn no_resolver_found(callable: impl Into<String>, tried: Vec<String>) -> Self {
        let callable = callable.into();
        Self {
            callable: callable.clone(),
            message: format!("No resolver could handle callable '{}'", callable),
            tried_resolvers: tried,
            hint: Some(
                "Ensure the callable is registered with ExplicitMappingResolver, \
                 or matches a custom resolver's pattern, or is a valid class path."
                    .to_string(),
            ),
        }
    }
}

/// Strategy trait for resolving callable addresses to handler instances.
///
/// Resolvers interpret the `callable` field in a `HandlerDefinition` as a lookup
/// key and return handler instances. The format of `callable` depends on which
/// resolver interprets it - like URL schemes, different resolvers expect
/// different formats.
///
/// ## Resolver Responsibilities
///
/// - **DO**: Interpret `callable` according to your format convention
/// - **DO**: Return `None` if you cannot resolve the callable
/// - **DO**: Use `ResolutionContext` for handler instantiation
/// - **DON'T**: Handle method dispatch (framework does this)
/// - **DON'T**: Wrap the handler (framework does this if needed)
///
/// ## Priority System
///
/// Resolvers are tried in priority order (lower = checked first):
/// - 10: ExplicitMappingResolver (registered key lookup)
/// - 20-99: Custom resolvers (domain-specific patterns)
/// - 100: ClassConstantResolver (fallback class path lookup)
///
/// ## Thread Safety
///
/// All resolvers must be `Send + Sync` since they may be called from
/// multiple worker threads concurrently.
#[async_trait]
pub trait StepHandlerResolver: Send + Sync + fmt::Debug {
    /// Check if this resolver can handle the given callable format.
    ///
    /// This is a quick check before attempting resolution. Return `true`
    /// if the callable format matches what this resolver expects.
    ///
    /// # Arguments
    ///
    /// * `definition` - The handler definition containing the callable
    ///
    /// # Returns
    ///
    /// `true` if this resolver should attempt to resolve the callable.
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool;

    /// Resolve a callable address to a handler instance.
    ///
    /// The `definition.callable` is YOUR lookup key - interpret it however
    /// makes sense for your resolver. Return the handler instance, or `None`
    /// if this resolver cannot resolve the given callable.
    ///
    /// You do NOT need to handle method dispatch - the framework wraps
    /// the returned handler if `definition.method` is set.
    ///
    /// # Arguments
    ///
    /// * `definition` - The handler definition with callable and initialization
    /// * `context` - Additional context for resolution (correlation ID, namespace)
    ///
    /// # Returns
    ///
    /// A handler instance wrapped in `Arc`, or `None` if resolution failed.
    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        context: &ResolutionContext,
    ) -> Option<Arc<dyn ResolvedHandler>>;

    /// Resolver name for logging and metrics.
    ///
    /// Should be unique and descriptive, e.g., "ExplicitMappingResolver".
    fn resolver_name(&self) -> &str;

    /// Priority in the resolver chain (lower = checked first).
    ///
    /// Default priority is 100, which is checked last.
    /// Use priority 10 for explicit mapping, 20-99 for custom resolvers.
    fn priority(&self) -> u32 {
        100
    }

    /// Return the list of callable keys registered with this resolver.
    ///
    /// This is used for introspection and debugging. Resolvers that maintain
    /// a registry of handlers should return the known keys. Resolvers that
    /// match patterns dynamically (e.g., class constant resolvers) may return
    /// an empty list.
    ///
    /// Returns owned Strings to avoid lifetime issues with internal locks.
    ///
    /// Default implementation returns an empty list.
    fn registered_callables(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Trait for resolved handler instances.
///
/// This is a minimal trait that represents any handler that can be invoked.
/// The actual execution contract may vary by language (Rust, Ruby, Python).
///
/// ## Why a Separate Trait?
///
/// This trait is defined in `tasker-shared` to avoid circular dependencies
/// between `tasker-shared` and `tasker-worker`. The `StepHandler` trait
/// in `tasker-worker` provides the full execution contract.
///
/// Implementations will typically wrap the language-specific handler types.
pub trait ResolvedHandler: Send + Sync + fmt::Debug {
    /// Handler name for logging.
    fn name(&self) -> &str;

    /// Check if this handler supports a specific method.
    ///
    /// Returns `true` if the handler can be invoked with the given method name.
    /// Default implementation returns `true` for "call" only.
    fn supports_method(&self, method: &str) -> bool {
        method == "call"
    }

    /// Get the list of supported methods.
    ///
    /// Default implementation returns just "call".
    fn supported_methods(&self) -> Vec<&str> {
        vec!["call"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test implementation of ResolvedHandler
    #[derive(Debug)]
    struct MockHandler {
        name: String,
        methods: Vec<String>,
    }

    impl MockHandler {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                methods: vec!["call".to_string()],
            }
        }

        fn with_methods(name: &str, methods: Vec<&str>) -> Self {
            Self {
                name: name.to_string(),
                methods: methods.into_iter().map(String::from).collect(),
            }
        }
    }

    impl ResolvedHandler for MockHandler {
        fn name(&self) -> &str {
            &self.name
        }

        fn supports_method(&self, method: &str) -> bool {
            self.methods.iter().any(|m| m == method)
        }

        fn supported_methods(&self) -> Vec<&str> {
            self.methods.iter().map(|s| s.as_str()).collect()
        }
    }

    // Test implementation of StepHandlerResolver
    #[derive(Debug)]
    struct MockResolver {
        name: String,
        priority: u32,
        prefix: String,
        handlers: std::sync::RwLock<HashMap<String, Arc<dyn ResolvedHandler>>>,
    }

    impl MockResolver {
        fn new(name: &str, prefix: &str, priority: u32) -> Self {
            Self {
                name: name.to_string(),
                priority,
                prefix: prefix.to_string(),
                handlers: std::sync::RwLock::new(HashMap::new()),
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
    fn test_resolution_context_from_definition() {
        let definition = HandlerDefinition::builder()
            .callable("TestHandler".to_string())
            .initialization(HashMap::from([(
                "timeout".to_string(),
                serde_json::json!(5000),
            )]))
            .build();

        let context = ResolutionContext::from_definition(&definition);
        assert_eq!(
            context.initialization.get("timeout"),
            Some(&serde_json::json!(5000))
        );
        assert!(context.correlation_id.is_none());
        assert!(context.namespace.is_none());
    }

    #[test]
    fn test_resolution_context_builder() {
        let context = ResolutionContext::default()
            .with_correlation_id("test-123")
            .with_namespace("payments");

        assert_eq!(context.correlation_id.as_deref(), Some("test-123"));
        assert_eq!(context.namespace.as_deref(), Some("payments"));
    }

    #[test]
    fn test_resolution_error_display() {
        let error = ResolutionError::no_resolver_found(
            "unknown:handler",
            vec![
                "ExplicitMappingResolver".to_string(),
                "ClassConstantResolver".to_string(),
            ],
        );

        let display = error.to_string();
        assert!(display.contains("unknown:handler"));
        assert!(display.contains("ExplicitMappingResolver"));
        assert!(display.contains("ClassConstantResolver"));
    }

    #[test]
    fn test_resolution_error_builder() {
        let error = ResolutionError::new("my_handler", "Handler not found")
            .with_tried_resolver("CustomResolver")
            .with_hint("Register the handler first");

        assert_eq!(error.callable, "my_handler");
        assert_eq!(error.message, "Handler not found");
        assert_eq!(error.tried_resolvers, vec!["CustomResolver"]);
        assert_eq!(error.hint.as_deref(), Some("Register the handler first"));
    }

    #[test]
    fn test_mock_handler_supports_method() {
        let handler = MockHandler::new("test");
        assert!(handler.supports_method("call"));
        assert!(!handler.supports_method("validate"));

        let handler_with_methods =
            MockHandler::with_methods("test", vec!["call", "validate", "process"]);
        assert!(handler_with_methods.supports_method("call"));
        assert!(handler_with_methods.supports_method("validate"));
        assert!(handler_with_methods.supports_method("process"));
        assert!(!handler_with_methods.supports_method("unknown"));
    }

    #[tokio::test]
    async fn test_mock_resolver_can_resolve() {
        let resolver = MockResolver::new("TestResolver", "test:", 50);

        let matching_def = HandlerDefinition::builder()
            .callable("test:handler".to_string())
            .build();
        assert!(resolver.can_resolve(&matching_def));

        let non_matching_def = HandlerDefinition::builder()
            .callable("other:handler".to_string())
            .build();
        assert!(!resolver.can_resolve(&non_matching_def));
    }

    #[tokio::test]
    async fn test_mock_resolver_resolve() {
        let resolver = MockResolver::new("TestResolver", "test:", 50);
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("test_handler"));
        resolver.register("test:my_handler", handler);

        let definition = HandlerDefinition::builder()
            .callable("test:my_handler".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved = resolver.resolve(&definition, &context).await;
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().name(), "test_handler");

        // Test unregistered handler
        let unknown_def = HandlerDefinition::builder()
            .callable("test:unknown".to_string())
            .build();
        let resolved = resolver.resolve(&unknown_def, &context).await;
        assert!(resolved.is_none());
    }

    #[test]
    fn test_resolver_priority() {
        let high_priority = MockResolver::new("HighPriority", "high:", 10);
        let default_priority = MockResolver::new("DefaultPriority", "default:", 100);
        let custom_priority = MockResolver::new("CustomPriority", "custom:", 50);

        assert_eq!(high_priority.priority(), 10);
        assert_eq!(default_priority.priority(), 100);
        assert_eq!(custom_priority.priority(), 50);
    }

    #[test]
    fn test_resolver_trait_bounds() {
        fn assert_send_sync_debug<T: Send + Sync + std::fmt::Debug>() {}
        assert_send_sync_debug::<MockResolver>();
    }

    #[test]
    fn test_handler_trait_bounds() {
        fn assert_send_sync_debug<T: Send + Sync + std::fmt::Debug>() {}
        assert_send_sync_debug::<MockHandler>();
    }
}
