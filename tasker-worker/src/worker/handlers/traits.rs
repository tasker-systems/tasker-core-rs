//! # Step Handler Traits
//!
//! TAS-67: Language-agnostic handler abstractions for step execution.
//!
//! These traits define the contract for step handlers across all supported
//! languages (Rust, Ruby, Python). They enable a unified dispatch architecture
//! where the HandlerDispatchService can invoke handlers without knowing their
//! implementation language.
//!
//! ## Architecture
//!
//! ```text
//! StepExecutorActor ────→ HandlerDispatchService ────→ StepHandlerRegistry
//!                              │                              │
//!                              │                              ├─→ RustHandler (native)
//!                              │                              │
//!                              │                              └─→ FfiHandler (Ruby/Python)
//!                              │                                       │
//!                              └──── CompletionChannel ←───────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tasker_worker::worker::handlers::{StepHandlerRegistry, StepHandler};
//!
//! // Implement for Rust handlers
//! struct RustHandlerRegistry {
//!     handlers: HashMap<String, Arc<dyn StepHandler>>,
//! }
//!
//! #[async_trait]
//! impl StepHandlerRegistry for RustHandlerRegistry {
//!     async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
//!         self.handlers.get(&step.step_definition.handler_class).cloned()
//!     }
//!
//!     fn register(&self, name: &str, handler: Arc<dyn StepHandler>) {
//!         // ...
//!     }
//! }
//! ```

use async_trait::async_trait;
use std::sync::Arc;

use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::base::TaskSequenceStep;
use tasker_shared::TaskerResult;

/// Registry for step handlers
///
/// Provides handler lookup by step metadata. Implementations are language-specific:
/// - Rust: HashMap-based with direct handler references
/// - Ruby: Bridges to Ruby's dry-container registry via FFI
/// - Python: Bridges to Python's handler registry via PyO3
///
/// ## Thread Safety
///
/// All implementations must be `Send + Sync` to support concurrent handler
/// registration and lookup from multiple worker threads.
///
/// ## Handler Resolution
///
/// The `get` method resolves handlers using step metadata:
/// - `step.step_definition.handler_class`: Primary lookup key
/// - `step.workflow_step.name`: Fallback for dynamic handlers
/// - `step.task.task.namespace`: Namespace scoping
#[async_trait]
pub trait StepHandlerRegistry: Send + Sync + 'static {
    /// Resolve a handler for the given step
    ///
    /// Returns `None` if no handler is registered for this step type.
    /// The implementation should use `step.step_definition.handler_class`
    /// as the primary lookup key.
    ///
    /// # Arguments
    ///
    /// * `step` - The step data containing handler resolution metadata
    ///
    /// # Returns
    ///
    /// An Arc-wrapped handler if found, None otherwise.
    async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>>;

    /// Register a handler by name
    ///
    /// # Arguments
    ///
    /// * `name` - The handler name (typically matches handler_class in step definitions)
    /// * `handler` - The handler implementation
    fn register(&self, name: &str, handler: Arc<dyn StepHandler>);

    /// Check if a handler is available for the given name
    ///
    /// # Arguments
    ///
    /// * `name` - The handler name to check
    ///
    /// # Returns
    ///
    /// True if a handler is registered with this name
    fn handler_available(&self, name: &str) -> bool;

    /// List all registered handler names
    ///
    /// Useful for debugging and introspection.
    fn registered_handlers(&self) -> Vec<String>;
}

/// Step handler execution trait
///
/// Defines the execution contract for step handlers. Implementations perform
/// the actual step work and return results.
///
/// ## Async Execution
///
/// The `call` method is async to support:
/// - Non-blocking I/O operations
/// - Concurrent handler execution with bounded parallelism
/// - Timeout handling at the dispatch layer
///
/// ## Error Handling
///
/// Handlers should return `TaskerError` for recoverable errors that can be
/// retried by the orchestration system. Panics are caught by the dispatch
/// service and converted to failure results.
#[async_trait]
pub trait StepHandler: Send + Sync + 'static {
    /// Execute the step handler
    ///
    /// # Arguments
    ///
    /// * `step` - The step data with all execution context
    ///
    /// # Returns
    ///
    /// A `StepExecutionResult` indicating success or failure with result data.
    ///
    /// # Errors
    ///
    /// Returns `TaskerError` for execution failures that should be recorded
    /// in the step result.
    async fn call(&self, step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult>;

    /// Handler name for logging and metrics
    ///
    /// Should match the registered name in the registry.
    fn name(&self) -> &str;

    /// Handler version (optional, for compatibility tracking)
    fn version(&self) -> &str {
        "1.0.0"
    }

    /// Namespace this handler belongs to (optional)
    fn namespace(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    // Test handler implementation
    struct TestHandler {
        name: String,
    }

    #[async_trait]
    impl StepHandler for TestHandler {
        async fn call(&self, _step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
            Ok(StepExecutionResult::success(
                uuid::Uuid::new_v4(),
                serde_json::json!({"handled_by": self.name}),
                0,    // execution_time_ms
                None, // custom_metadata
            ))
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    // Test registry implementation
    struct TestRegistry {
        handlers: RwLock<HashMap<String, Arc<dyn StepHandler>>>,
    }

    impl TestRegistry {
        fn new() -> Self {
            Self {
                handlers: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StepHandlerRegistry for TestRegistry {
        async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
            let handlers = self.handlers.read().unwrap();
            handlers
                .get(&step.step_definition.handler.callable)
                .cloned()
        }

        fn register(&self, name: &str, handler: Arc<dyn StepHandler>) {
            let mut handlers = self.handlers.write().unwrap();
            handlers.insert(name.to_string(), handler);
        }

        fn handler_available(&self, name: &str) -> bool {
            let handlers = self.handlers.read().unwrap();
            handlers.contains_key(name)
        }

        fn registered_handlers(&self) -> Vec<String> {
            let handlers = self.handlers.read().unwrap();
            handlers.keys().cloned().collect()
        }
    }

    #[test]
    fn test_handler_trait_bounds() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<TestHandler>();
    }

    #[test]
    fn test_registry_trait_bounds() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<TestRegistry>();
    }

    #[test]
    fn test_registry_operations() {
        let registry = TestRegistry::new();
        let handler: Arc<dyn StepHandler> = Arc::new(TestHandler {
            name: "test_handler".to_string(),
        });

        registry.register("test_handler", handler);
        assert!(registry.handler_available("test_handler"));
        assert!(!registry.handler_available("nonexistent"));
        assert_eq!(registry.registered_handlers(), vec!["test_handler"]);
    }
}
