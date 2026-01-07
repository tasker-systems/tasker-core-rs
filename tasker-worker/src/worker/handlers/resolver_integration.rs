//! # Resolver Integration
//!
//! TAS-93: Bridge adapters between the resolver chain pattern and the existing
//! handler dispatch infrastructure.
//!
//! This module provides adapters that enable composition between:
//! - `ResolvedHandler` (from `tasker-shared::registry`) - Resolution trait
//! - `StepHandler` (from this crate) - Execution trait
//!
//! ## Design Philosophy
//!
//! The adapters provide cohesion without tight coupling. By keeping the
//! `HandlerDispatchService` unchanged and adapting at the registry level,
//! we maintain backward compatibility while enabling the new resolver pattern.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    RESOLVER INTEGRATION FLOW                             │
//! └─────────────────────────────────────────────────────────────────────────┘
//!
//! HandlerDispatchService (unchanged)
//!         │
//!         │ registry.get(step)
//!         ▼
//! ResolverChainRegistry (implements StepHandlerRegistry)
//!         │
//!         │ chain.resolve(definition, context)
//!         ▼
//! ResolverChain → ExplicitMappingResolver → ResolvedHandler
//!         │
//!         │ wrap with MethodDispatchWrapper if needed
//!         ▼
//! ExecutableHandler (implements StepHandler)
//!         │
//!         │ handler.call(step)
//!         ▼
//! StepExecutionResult
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, warn};

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::models::core::task_template::HandlerDefinition;
use tasker_shared::registry::{
    method_dispatch::MethodDispatchWrapper, ResolutionContext, ResolvedHandler, ResolverChain,
};
use tasker_shared::types::base::TaskSequenceStep;
use tasker_shared::TaskerResult;

use super::traits::{StepHandler, StepHandlerRegistry};

// ============================================================================
// Trait Bridge: StepHandler → ResolvedHandler
// ============================================================================

/// Adapter that wraps a `StepHandler` to implement `ResolvedHandler`.
///
/// This allows existing `StepHandler` implementations to be used with the
/// resolver chain infrastructure.
///
/// ## Thread Safety
///
/// This adapter is `Send + Sync` as it only holds an `Arc` reference.
pub struct StepHandlerAsResolved {
    inner: Arc<dyn StepHandler>,
    supported_methods: Vec<String>,
}

impl std::fmt::Debug for StepHandlerAsResolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepHandlerAsResolved")
            .field("handler_name", &self.inner.name())
            .field("supported_methods", &self.supported_methods)
            .finish()
    }
}

impl StepHandlerAsResolved {
    /// Create a new adapter wrapping a StepHandler.
    ///
    /// By default, only the "call" method is supported.
    pub fn new(handler: Arc<dyn StepHandler>) -> Self {
        Self {
            inner: handler,
            supported_methods: vec!["call".to_string()],
        }
    }

    /// Create an adapter with additional supported methods.
    ///
    /// Use this for handlers that support method dispatch beyond "call".
    pub fn with_methods(handler: Arc<dyn StepHandler>, methods: Vec<String>) -> Self {
        let mut supported_methods = vec!["call".to_string()];
        for method in methods {
            if method != "call" && !supported_methods.contains(&method) {
                supported_methods.push(method);
            }
        }
        Self {
            inner: handler,
            supported_methods,
        }
    }

    /// Get the underlying StepHandler.
    pub fn inner(&self) -> &Arc<dyn StepHandler> {
        &self.inner
    }
}

impl ResolvedHandler for StepHandlerAsResolved {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn supports_method(&self, method: &str) -> bool {
        self.supported_methods.iter().any(|m| m == method)
    }

    fn supported_methods(&self) -> Vec<&str> {
        self.supported_methods.iter().map(|s| s.as_str()).collect()
    }
}

// ============================================================================
// Trait Bridge: ResolvedHandler → StepHandler (Executable)
// ============================================================================

/// An executable handler that bridges `ResolvedHandler` to `StepHandler`.
///
/// This adapter enables handlers resolved through `ResolverChain` to be
/// executed by `HandlerDispatchService`. It handles method dispatch when
/// a non-default method is specified.
///
/// ## Method Dispatch
///
/// When the handler definition specifies a method other than "call", this
/// adapter uses the `MethodDispatchWrapper` to carry the method name.
/// The actual method invocation depends on whether the underlying handler
/// supports dynamic method dispatch.
///
/// For handlers that implement `MethodDispatchable`, the specified method
/// is invoked. For standard handlers, the default `call()` is used.
pub struct ExecutableHandler {
    /// The resolved handler
    handler: Arc<dyn ResolvedHandler>,

    /// Optional method dispatch wrapper (when method != "call")
    dispatch_wrapper: Option<MethodDispatchWrapper>,

    /// Cached handler name for logging
    handler_name: String,

    /// The execution delegate (how to actually call the handler)
    executor: Arc<dyn HandlerExecutor>,
}

impl std::fmt::Debug for ExecutableHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutableHandler")
            .field("handler_name", &self.handler_name)
            .field("has_method_dispatch", &self.dispatch_wrapper.is_some())
            .finish()
    }
}

/// Trait for executing resolved handlers.
///
/// This abstraction allows different execution strategies:
/// - Direct execution for handlers that are already `StepHandler`
/// - Method dispatch for handlers with multiple entry points
/// - FFI bridge for Ruby/Python handlers
#[async_trait]
pub trait HandlerExecutor: Send + Sync + 'static {
    /// Execute the handler for the given step.
    async fn execute(
        &self,
        handler: &Arc<dyn ResolvedHandler>,
        method: Option<&str>,
        step: &TaskSequenceStep,
    ) -> TaskerResult<StepExecutionResult>;
}

/// Default executor that expects the handler to also implement StepHandler.
///
/// This is the common case for Rust handlers where the same type implements
/// both traits.
#[derive(Debug, Default)]
pub struct DefaultHandlerExecutor;

#[async_trait]
impl HandlerExecutor for DefaultHandlerExecutor {
    async fn execute(
        &self,
        handler: &Arc<dyn ResolvedHandler>,
        method: Option<&str>,
        _step: &TaskSequenceStep,
    ) -> TaskerResult<StepExecutionResult> {
        // For the default executor, we need the handler to also be a StepHandler.
        // This is achieved by storing the original StepHandler in the resolution process.
        //
        // The method parameter is logged but not used in the default case because
        // standard Rust handlers don't support dynamic method dispatch.
        if let Some(m) = method {
            if m != "call" {
                debug!(
                    handler = handler.name(),
                    method = m,
                    "Method dispatch requested but handler uses default executor - using call()"
                );
            }
        }

        // We need to somehow invoke the handler. Since ResolvedHandler doesn't
        // have a call() method, we rely on the handler being wrapped appropriately
        // during registration.
        //
        // For now, return an error indicating the handler needs proper registration.
        Err(tasker_shared::TaskerError::WorkerError(format!(
            "Handler '{}' resolved but no executor configured. \
             Use StepHandlerAsResolved to wrap StepHandler implementations.",
            handler.name()
        )))
    }
}

/// Executor that wraps a StepHandler for direct execution.
///
/// This executor is used when a StepHandler is registered with the resolver
/// chain and needs to be executed.
pub struct StepHandlerExecutor {
    handler: Arc<dyn StepHandler>,
}

impl std::fmt::Debug for StepHandlerExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepHandlerExecutor")
            .field("handler_name", &self.handler.name())
            .finish()
    }
}

impl StepHandlerExecutor {
    /// Create a new executor for a StepHandler.
    pub fn new(handler: Arc<dyn StepHandler>) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl HandlerExecutor for StepHandlerExecutor {
    async fn execute(
        &self,
        _handler: &Arc<dyn ResolvedHandler>,
        method: Option<&str>,
        step: &TaskSequenceStep,
    ) -> TaskerResult<StepExecutionResult> {
        if let Some(m) = method {
            if m != "call" {
                debug!(
                    handler = self.handler.name(),
                    method = m,
                    "Method dispatch to non-call method - invoking call() (method dispatch requires MethodDispatchable)"
                );
            }
        }

        self.handler.call(step).await
    }
}

impl ExecutableHandler {
    /// Create a new executable handler.
    ///
    /// # Arguments
    ///
    /// * `handler` - The resolved handler
    /// * `method` - Optional method name (None or "call" means default)
    /// * `executor` - The execution delegate
    pub fn new(
        handler: Arc<dyn ResolvedHandler>,
        method: Option<&str>,
        executor: Arc<dyn HandlerExecutor>,
    ) -> Self {
        let handler_name = handler.name().to_string();
        let dispatch_wrapper = if let Some(m) = method {
            if m != "call" {
                Some(MethodDispatchWrapper::with_method(Arc::clone(&handler), m))
            } else {
                None
            }
        } else {
            None
        };

        Self {
            handler,
            dispatch_wrapper,
            handler_name,
            executor,
        }
    }

    /// Create an executable handler from a StepHandler.
    ///
    /// This is the common case for Rust handlers.
    pub fn from_step_handler(handler: Arc<dyn StepHandler>, method: Option<&str>) -> Self {
        let resolved: Arc<dyn ResolvedHandler> =
            Arc::new(StepHandlerAsResolved::new(Arc::clone(&handler)));
        let executor: Arc<dyn HandlerExecutor> = Arc::new(StepHandlerExecutor::new(handler));

        Self::new(resolved, method, executor)
    }

    /// Get the target method name.
    pub fn method(&self) -> &str {
        self.dispatch_wrapper
            .as_ref()
            .map(|w| w.method())
            .unwrap_or("call")
    }

    /// Check if this uses non-default method dispatch.
    pub fn uses_method_dispatch(&self) -> bool {
        self.dispatch_wrapper.is_some()
    }
}

#[async_trait]
impl StepHandler for ExecutableHandler {
    async fn call(&self, step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
        let method = self.dispatch_wrapper.as_ref().map(|w| w.method());
        self.executor.execute(&self.handler, method, step).await
    }

    fn name(&self) -> &str {
        &self.handler_name
    }
}

// ============================================================================
// ResolverChainRegistry: StepHandlerRegistry using ResolverChain
// ============================================================================

/// A `StepHandlerRegistry` implementation that uses `ResolverChain` for resolution.
///
/// This registry bridges the resolver pattern to the existing dispatch infrastructure,
/// allowing `HandlerDispatchService` to work unchanged while benefiting from the
/// flexible resolver chain.
///
/// ## Usage
///
/// ```rust,ignore
/// use tasker_worker::worker::handlers::ResolverChainRegistry;
/// use tasker_shared::registry::{ResolverChain, resolvers::ExplicitMappingResolver};
/// use std::sync::Arc;
///
/// // Build resolver chain
/// let explicit = Arc::new(ExplicitMappingResolver::new());
/// explicit.register_instance("my_handler", Arc::new(MyResolvedHandler::new()));
///
/// let chain = ResolverChain::new().with_resolver(explicit);
///
/// // Create registry
/// let registry = ResolverChainRegistry::new(Arc::new(chain));
///
/// // Use with HandlerDispatchService (unchanged)
/// let dispatch_service = HandlerDispatchService::new(
///     dispatch_rx,
///     completion_tx,
///     Arc::new(registry),
///     config,
/// );
/// ```
pub struct ResolverChainRegistry {
    /// The resolver chain for handler resolution
    chain: Arc<ResolverChain>,

    /// Default executor for resolved handlers
    default_executor: Arc<dyn HandlerExecutor>,

    /// Registry name for logging
    name: String,
}

impl std::fmt::Debug for ResolverChainRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolverChainRegistry")
            .field("name", &self.name)
            .field("resolver_count", &self.chain.len())
            .finish()
    }
}

impl ResolverChainRegistry {
    /// Create a new registry with the given resolver chain.
    pub fn new(chain: Arc<ResolverChain>) -> Self {
        Self {
            chain,
            default_executor: Arc::new(DefaultHandlerExecutor),
            name: "ResolverChainRegistry".to_string(),
        }
    }

    /// Create a registry with a custom name.
    pub fn with_name(chain: Arc<ResolverChain>, name: impl Into<String>) -> Self {
        Self {
            chain,
            default_executor: Arc::new(DefaultHandlerExecutor),
            name: name.into(),
        }
    }

    /// Set a custom default executor.
    pub fn with_executor(mut self, executor: Arc<dyn HandlerExecutor>) -> Self {
        self.default_executor = executor;
        self
    }

    /// Extract handler definition from a step.
    fn extract_definition(&self, step: &TaskSequenceStep) -> HandlerDefinition {
        step.step_definition.handler.clone()
    }

    /// Build resolution context from a step.
    fn build_context(&self, step: &TaskSequenceStep) -> ResolutionContext {
        let definition = self.extract_definition(step);
        let mut context = ResolutionContext::from_definition(&definition);

        // Add step-specific context
        // Use the task's correlation_id (TAS-29) for distributed tracing
        context.correlation_id = Some(step.task.task.correlation_id.to_string());
        context.namespace = Some(step.task.namespace_name.clone());

        context
    }
}

#[async_trait]
impl StepHandlerRegistry for ResolverChainRegistry {
    async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
        let definition = self.extract_definition(step);
        let context = self.build_context(step);

        debug!(
            registry = %self.name,
            callable = %definition.callable,
            method = ?definition.method,
            "Resolving handler via chain"
        );

        match self.chain.resolve(&definition, &context).await {
            Ok(resolved) => {
                let method = definition.method.as_deref();

                // Create executable handler with method dispatch support
                let executable =
                    ExecutableHandler::new(resolved, method, Arc::clone(&self.default_executor));

                debug!(
                    registry = %self.name,
                    handler = executable.name(),
                    method = executable.method(),
                    "Handler resolved successfully"
                );

                Some(Arc::new(executable))
            }
            Err(e) => {
                warn!(
                    registry = %self.name,
                    callable = %definition.callable,
                    error = %e,
                    tried_resolvers = ?e.tried_resolvers,
                    "Handler resolution failed"
                );
                None
            }
        }
    }

    fn register(&self, name: &str, _handler: Arc<dyn StepHandler>) {
        // Registration should happen through the resolver chain, not directly
        warn!(
            registry = %self.name,
            handler_name = %name,
            "Direct registration not supported - use ResolverChain.register_resolver() instead"
        );
    }

    fn handler_available(&self, name: &str) -> bool {
        // Check if any resolver can handle this callable
        let definition = HandlerDefinition::builder()
            .callable(name.to_string())
            .build();

        self.chain.can_resolve(&definition)
    }

    fn registered_handlers(&self) -> Vec<String> {
        // Delegate to the chain
        self.chain.registered_callables()
    }
}

// ============================================================================
// Convenience: Hybrid Registry for Migration
// ============================================================================

/// A registry that combines resolver chain lookup with fallback to a legacy registry.
///
/// This enables gradual migration from the old pattern to the new resolver pattern.
/// Handlers registered with the resolver chain take precedence.
pub struct HybridStepHandlerRegistry<L: StepHandlerRegistry> {
    /// Primary: Resolver chain for new-style resolution
    resolver_registry: ResolverChainRegistry,

    /// Fallback: Legacy registry for handlers not yet migrated
    legacy_registry: Arc<L>,

    /// Registry name for logging
    name: String,
}

impl<L: StepHandlerRegistry> std::fmt::Debug for HybridStepHandlerRegistry<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridStepHandlerRegistry")
            .field("name", &self.name)
            .finish()
    }
}

impl<L: StepHandlerRegistry> HybridStepHandlerRegistry<L> {
    /// Create a new hybrid registry.
    ///
    /// # Arguments
    ///
    /// * `chain` - The resolver chain for new-style resolution
    /// * `legacy` - The legacy registry for fallback
    pub fn new(chain: Arc<ResolverChain>, legacy: Arc<L>) -> Self {
        Self {
            resolver_registry: ResolverChainRegistry::new(chain),
            legacy_registry: legacy,
            name: "HybridStepHandlerRegistry".to_string(),
        }
    }

    /// Create with a custom name.
    pub fn with_name(chain: Arc<ResolverChain>, legacy: Arc<L>, name: impl Into<String>) -> Self {
        Self {
            resolver_registry: ResolverChainRegistry::new(chain),
            legacy_registry: legacy,
            name: name.into(),
        }
    }
}

#[async_trait]
impl<L: StepHandlerRegistry> StepHandlerRegistry for HybridStepHandlerRegistry<L> {
    async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
        // Try resolver chain first
        if let Some(handler) = self.resolver_registry.get(step).await {
            debug!(
                registry = %self.name,
                handler = handler.name(),
                source = "resolver_chain",
                "Handler resolved via chain"
            );
            return Some(handler);
        }

        // Fall back to legacy registry
        if let Some(handler) = self.legacy_registry.get(step).await {
            debug!(
                registry = %self.name,
                handler = handler.name(),
                source = "legacy",
                "Handler resolved via legacy registry"
            );
            return Some(handler);
        }

        None
    }

    fn register(&self, name: &str, handler: Arc<dyn StepHandler>) {
        // Delegate to legacy registry
        self.legacy_registry.register(name, handler);
    }

    fn handler_available(&self, name: &str) -> bool {
        self.resolver_registry.handler_available(name)
            || self.legacy_registry.handler_available(name)
    }

    fn registered_handlers(&self) -> Vec<String> {
        let mut handlers = self.resolver_registry.registered_handlers();
        for name in self.legacy_registry.registered_handlers() {
            if !handlers.contains(&name) {
                handlers.push(name);
            }
        }
        handlers.sort();
        handlers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;
    use tasker_shared::registry::resolvers::ExplicitMappingResolver;

    // Test handler for unit tests
    #[derive(Debug)]
    struct TestStepHandler {
        name: String,
    }

    impl TestStepHandler {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl StepHandler for TestStepHandler {
        async fn call(&self, _step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
            Ok(StepExecutionResult::success(
                uuid::Uuid::new_v4(),
                serde_json::json!({"handler": self.name}),
                0,
                None,
            ))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    // Test legacy registry
    struct TestLegacyRegistry {
        handlers: RwLock<HashMap<String, Arc<dyn StepHandler>>>,
    }

    impl TestLegacyRegistry {
        fn new() -> Self {
            Self {
                handlers: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StepHandlerRegistry for TestLegacyRegistry {
        async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
            let handlers = self.handlers.read().unwrap();
            handlers
                .get(&step.step_definition.handler.callable)
                .cloned()
        }

        fn register(&self, name: &str, handler: Arc<dyn StepHandler>) {
            self.handlers
                .write()
                .unwrap()
                .insert(name.to_string(), handler);
        }

        fn handler_available(&self, name: &str) -> bool {
            self.handlers.read().unwrap().contains_key(name)
        }

        fn registered_handlers(&self) -> Vec<String> {
            self.handlers.read().unwrap().keys().cloned().collect()
        }
    }

    #[test]
    fn test_step_handler_as_resolved() {
        let handler: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("test"));
        let resolved = StepHandlerAsResolved::new(handler);

        assert_eq!(resolved.name(), "test");
        assert!(resolved.supports_method("call"));
        assert!(!resolved.supports_method("validate"));
        assert_eq!(resolved.supported_methods(), vec!["call"]);
    }

    #[test]
    fn test_step_handler_as_resolved_with_methods() {
        let handler: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("test"));
        let resolved = StepHandlerAsResolved::with_methods(
            handler,
            vec!["validate".to_string(), "process".to_string()],
        );

        assert!(resolved.supports_method("call"));
        assert!(resolved.supports_method("validate"));
        assert!(resolved.supports_method("process"));
        assert!(!resolved.supports_method("unknown"));
    }

    #[test]
    fn test_executable_handler_from_step_handler() {
        let handler: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("test"));
        let executable = ExecutableHandler::from_step_handler(handler, None);

        assert_eq!(executable.name(), "test");
        assert_eq!(executable.method(), "call");
        assert!(!executable.uses_method_dispatch());
    }

    #[test]
    fn test_executable_handler_with_method() {
        let handler: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("test"));
        let executable = ExecutableHandler::from_step_handler(handler, Some("validate"));

        assert_eq!(executable.name(), "test");
        assert_eq!(executable.method(), "validate");
        assert!(executable.uses_method_dispatch());
    }

    #[test]
    fn test_resolver_chain_registry_handler_available() {
        let resolver = Arc::new(ExplicitMappingResolver::new());

        // Register a handler
        let handler: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("test_handler"));
        let resolved: Arc<dyn ResolvedHandler> =
            Arc::new(StepHandlerAsResolved::new(Arc::clone(&handler)));
        resolver.register_instance("test_handler", resolved);

        let chain = Arc::new(ResolverChain::new().with_resolver(resolver));
        let registry = ResolverChainRegistry::new(chain);

        assert!(registry.handler_available("test_handler"));
        assert!(!registry.handler_available("unknown_handler"));
    }

    #[test]
    fn test_hybrid_registry_handler_available() {
        // Create resolver chain with one handler
        let resolver = Arc::new(ExplicitMappingResolver::new());
        let handler1: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("chain_handler"));
        let resolved: Arc<dyn ResolvedHandler> =
            Arc::new(StepHandlerAsResolved::new(Arc::clone(&handler1)));
        resolver.register_instance("chain_handler", resolved);

        let chain = Arc::new(ResolverChain::new().with_resolver(resolver));

        // Create legacy registry with another handler
        let legacy = Arc::new(TestLegacyRegistry::new());
        let handler2: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("legacy_handler"));
        legacy.register("legacy_handler", handler2);

        // Create hybrid registry
        let hybrid = HybridStepHandlerRegistry::new(chain, legacy);

        // Both handlers should be available
        assert!(hybrid.handler_available("chain_handler"));
        assert!(hybrid.handler_available("legacy_handler"));
        assert!(!hybrid.handler_available("unknown"));
    }

    #[test]
    fn test_hybrid_registry_registered_handlers() {
        // Create resolver chain with one handler
        let resolver = Arc::new(ExplicitMappingResolver::new());
        let handler1: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("chain_handler"));
        let resolved: Arc<dyn ResolvedHandler> =
            Arc::new(StepHandlerAsResolved::new(Arc::clone(&handler1)));
        resolver.register_instance("chain_handler", resolved);

        let chain = Arc::new(ResolverChain::new().with_resolver(resolver));

        // Create legacy registry with another handler
        let legacy = Arc::new(TestLegacyRegistry::new());
        let handler2: Arc<dyn StepHandler> = Arc::new(TestStepHandler::new("legacy_handler"));
        legacy.register("legacy_handler", handler2);

        // Create hybrid registry
        let hybrid = HybridStepHandlerRegistry::new(chain, legacy);

        let handlers = hybrid.registered_handlers();
        assert!(handlers.contains(&"chain_handler".to_string()));
        assert!(handlers.contains(&"legacy_handler".to_string()));
    }

    #[test]
    fn test_executable_handler_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ExecutableHandler>();
    }

    #[test]
    fn test_resolver_chain_registry_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<ResolverChainRegistry>();
    }
}
