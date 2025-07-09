//! # Task Handler Registry
//!
//! Task handler registry with dual-path support for both Rust and FFI integration.
//!
//! ## Architecture
//!
//! The registry provides two paths for task handler management:
//! - **Direct handler references** for Rust consumers
//! - **Stringified references with metadata** for FFI consumers
//!
//! ## Key Features
//!
//! - **Thread-safe operations** using RwLock for concurrent access
//! - **Namespace/name/version** registration pattern
//! - **Event integration** for registration notifications
//! - **Comprehensive metadata** tracking and statistics
//! - **Validation** and error handling
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::orchestration::registry::TaskHandlerRegistry;
//! use tasker_core::orchestration::event_publisher::EventPublisher;
//!
//! # tokio_test::block_on(async {
//! let event_publisher = EventPublisher::new();
//! let registry = TaskHandlerRegistry::new(event_publisher);
//!
//! // Register an FFI handler (more common usage)
//! let result = registry.register_ffi_handler(
//!     "ecommerce",
//!     "order_processor",
//!     "1.0.0",
//!     "OrderProcessorHandler",
//!     None
//! ).await;
//! assert!(result.is_ok());
//!
//! // Register another FFI handler
//! let result = registry.register_ffi_handler(
//!     "payments",
//!     "payment_processor",
//!     "2.1.0",
//!     "PaymentProcessorHandler",
//!     None
//! ).await;
//! assert!(result.is_ok());
//!
//! // Check registry statistics
//! let stats = registry.stats().unwrap();
//! assert_eq!(stats.total_ffi_handlers, 2);
//! # });
//! ```

use crate::orchestration::errors::{OrchestrationResult, RegistryError};
use crate::orchestration::event_publisher::EventPublisher;
use crate::orchestration::types::{
    HandlerMetadata, OrchestrationEvent, RegistryStats, TaskHandler,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

/// Task handler registry with dual-path support for Rust and FFI integration
pub struct TaskHandlerRegistry {
    /// Direct handler references for Rust consumers
    handlers: Arc<RwLock<HashMap<String, Arc<dyn TaskHandler>>>>,
    /// Stringified references for FFI consumers
    ffi_handlers: Arc<RwLock<HashMap<String, HandlerMetadata>>>,
    /// Event publisher for registration notifications
    event_publisher: EventPublisher,
}

impl TaskHandlerRegistry {
    /// Create a new task handler registry
    pub fn new(event_publisher: EventPublisher) -> Self {
        info!("Creating new TaskHandlerRegistry");

        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            ffi_handlers: Arc::new(RwLock::new(HashMap::new())),
            event_publisher,
        }
    }

    /// Register a task handler with namespace/name/version (Rust direct reference)
    pub async fn register_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler: Arc<dyn TaskHandler>,
    ) -> OrchestrationResult<()> {
        let key = format!("{namespace}/{name}/{version}");

        info!(
            namespace = namespace,
            name = name,
            version = version,
            key = key,
            "Registering task handler"
        );

        // Validate handler
        self.validate_handler(handler.as_ref())?;

        // Register direct handler reference for Rust
        {
            let mut handlers =
                self.handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_handlers".to_string(),
                        reason: e.to_string(),
                    })?;

            if handlers.contains_key(&key) {
                warn!(key = key, "Handler already registered, replacing");
            }

            handlers.insert(key.clone(), handler.clone());
        }

        // Register metadata for introspection
        let metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            handler_class: format!("{name}Handler"),
            config_schema: None,
            registered_at: Utc::now(),
        };

        {
            let mut ffi_handlers =
                self.ffi_handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_ffi_handlers".to_string(),
                        reason: e.to_string(),
                    })?;

            ffi_handlers.insert(key.clone(), metadata.clone());
        }

        // Publish registration event
        self.event_publisher
            .publish_event(OrchestrationEvent::HandlerRegistered {
                key: key.clone(),
                metadata,
                registered_at: Utc::now(),
            })
            .await?;

        info!(key = key, "Task handler registered successfully");
        Ok(())
    }

    /// Register a task handler for FFI (stringified reference)
    pub async fn register_ffi_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler_class: &str,
        config_schema: Option<serde_json::Value>,
    ) -> OrchestrationResult<()> {
        let key = format!("{namespace}/{name}/{version}");

        info!(
            namespace = namespace,
            name = name,
            version = version,
            handler_class = handler_class,
            key = key,
            "Registering FFI task handler"
        );

        let metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            handler_class: handler_class.to_string(),
            config_schema,
            registered_at: Utc::now(),
        };

        {
            let mut ffi_handlers =
                self.ffi_handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_ffi_handlers".to_string(),
                        reason: e.to_string(),
                    })?;

            if ffi_handlers.contains_key(&key) {
                warn!(key = key, "FFI handler already registered, replacing");
            }

            ffi_handlers.insert(key.clone(), metadata.clone());
        }

        // Publish registration event
        self.event_publisher
            .publish_event(OrchestrationEvent::HandlerRegistered {
                key: key.clone(),
                metadata,
                registered_at: Utc::now(),
            })
            .await?;

        info!(key = key, "FFI task handler registered successfully");
        Ok(())
    }

    /// Get task handler by namespace/name/version (Rust direct reference)
    pub fn get_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<Arc<dyn TaskHandler>> {
        let key = format!("{namespace}/{name}/{version}");

        let handlers = self
            .handlers
            .read()
            .map_err(|e| RegistryError::ThreadSafetyError {
                operation: "read_lock_handlers".to_string(),
                reason: e.to_string(),
            })?;

        handlers
            .get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key).into())
    }

    /// Get task handler metadata (for both Rust and FFI)
    pub fn get_handler_metadata(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<HandlerMetadata> {
        let key = format!("{namespace}/{name}/{version}");

        let ffi_handlers =
            self.ffi_handlers
                .read()
                .map_err(|e| RegistryError::ThreadSafetyError {
                    operation: "read_lock_ffi_handlers".to_string(),
                    reason: e.to_string(),
                })?;

        ffi_handlers
            .get(&key)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound(key).into())
    }

    /// List all handlers in a namespace
    pub fn list_handlers(
        &self,
        namespace: Option<&str>,
    ) -> OrchestrationResult<Vec<HandlerMetadata>> {
        let ffi_handlers =
            self.ffi_handlers
                .read()
                .map_err(|e| RegistryError::ThreadSafetyError {
                    operation: "read_lock_ffi_handlers".to_string(),
                    reason: e.to_string(),
                })?;

        let filtered: Vec<_> = ffi_handlers
            .values()
            .filter(|metadata| namespace.is_none_or(|ns| metadata.namespace == ns))
            .cloned()
            .collect();

        Ok(filtered)
    }

    /// Get registry statistics
    pub fn stats(&self) -> OrchestrationResult<RegistryStats> {
        let handlers = self
            .handlers
            .read()
            .map_err(|e| RegistryError::ThreadSafetyError {
                operation: "read_lock_handlers".to_string(),
                reason: e.to_string(),
            })?;

        let ffi_handlers =
            self.ffi_handlers
                .read()
                .map_err(|e| RegistryError::ThreadSafetyError {
                    operation: "read_lock_ffi_handlers".to_string(),
                    reason: e.to_string(),
                })?;

        let namespaces: std::collections::HashSet<_> =
            ffi_handlers.values().map(|m| m.namespace.clone()).collect();

        Ok(RegistryStats {
            total_handlers: handlers.len(),
            total_ffi_handlers: ffi_handlers.len(),
            namespaces: namespaces.into_iter().collect(),
            thread_safe: true,
        })
    }

    /// Clear all handlers (useful for testing)
    pub fn clear(&self) -> OrchestrationResult<()> {
        {
            let mut handlers =
                self.handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_handlers".to_string(),
                        reason: e.to_string(),
                    })?;
            handlers.clear();
        }

        {
            let mut ffi_handlers =
                self.ffi_handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_ffi_handlers".to_string(),
                        reason: e.to_string(),
                    })?;
            ffi_handlers.clear();
        }

        info!("Task handler registry cleared");
        Ok(())
    }

    /// Check if a handler exists
    pub fn contains_handler(&self, namespace: &str, name: &str, version: &str) -> bool {
        let key = format!("{namespace}/{name}/{version}");

        if let Ok(handlers) = self.handlers.read() {
            handlers.contains_key(&key)
        } else {
            false
        }
    }

    /// Remove a handler from the registry
    pub fn remove_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<bool> {
        let key = format!("{namespace}/{name}/{version}");

        let removed_from_handlers = {
            let mut handlers =
                self.handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_handlers".to_string(),
                        reason: e.to_string(),
                    })?;
            handlers.remove(&key).is_some()
        };

        let removed_from_ffi = {
            let mut ffi_handlers =
                self.ffi_handlers
                    .write()
                    .map_err(|e| RegistryError::ThreadSafetyError {
                        operation: "write_lock_ffi_handlers".to_string(),
                        reason: e.to_string(),
                    })?;
            ffi_handlers.remove(&key).is_some()
        };

        let removed = removed_from_handlers || removed_from_ffi;

        if removed {
            info!(key = key, "Handler removed from registry");
        } else {
            warn!(key = key, "Handler not found for removal");
        }

        Ok(removed)
    }

    /// Validate handler implementation
    fn validate_handler(&self, handler: &dyn TaskHandler) -> OrchestrationResult<()> {
        // Get metadata to validate basic implementation
        let metadata = handler.metadata();

        // Validate required fields
        if metadata.namespace.is_empty() {
            return Err(RegistryError::ValidationError {
                handler_class: metadata.handler_class.clone(),
                reason: "Namespace cannot be empty".to_string(),
            }
            .into());
        }

        if metadata.name.is_empty() {
            return Err(RegistryError::ValidationError {
                handler_class: metadata.handler_class.clone(),
                reason: "Name cannot be empty".to_string(),
            }
            .into());
        }

        if metadata.version.is_empty() {
            return Err(RegistryError::ValidationError {
                handler_class: metadata.handler_class.clone(),
                reason: "Version cannot be empty".to_string(),
            }
            .into());
        }

        // Validate version format (semantic versioning)
        if !self.is_valid_semver(&metadata.version) {
            return Err(RegistryError::ValidationError {
                handler_class: metadata.handler_class.clone(),
                reason: format!("Invalid version format: {}", metadata.version),
            }
            .into());
        }

        Ok(())
    }

    /// Check if version follows semantic versioning pattern
    fn is_valid_semver(&self, version: &str) -> bool {
        // Simple regex check for semantic versioning: major.minor.patch
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
    }
}

/// Example usage for integration testing
impl TaskHandlerRegistry {
    /// Setup test handlers for integration testing
    pub async fn setup_test_handlers(&self) -> OrchestrationResult<()> {
        // Register test task handlers (not step handlers)
        self.register_ffi_handler(
            "ecommerce",
            "order_processor",
            "1.0.0",
            "OrderProcessorHandler",
            None,
        )
        .await?;

        self.register_ffi_handler(
            "payments",
            "payment_processor",
            "2.1.0",
            "PaymentProcessorHandler",
            None,
        )
        .await?;

        self.register_ffi_handler(
            "inventory",
            "inventory_manager",
            "1.5.0",
            "InventoryManagerHandler",
            None,
        )
        .await?;

        self.register_ffi_handler(
            "notifications",
            "email_sender",
            "1.2.0",
            "EmailSenderHandler",
            None,
        )
        .await?;

        info!("Test handlers setup completed");
        Ok(())
    }
}

impl Clone for TaskHandlerRegistry {
    fn clone(&self) -> Self {
        Self {
            handlers: Arc::clone(&self.handlers),
            ffi_handlers: Arc::clone(&self.ffi_handlers),
            event_publisher: self.event_publisher.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::types::TaskContext;
    use async_trait::async_trait;

    // Mock TaskHandler for testing
    struct MockTaskHandler {
        metadata: HandlerMetadata,
    }

    #[async_trait]
    impl TaskHandler for MockTaskHandler {
        async fn handle_task(
            &self,
            _task_context: &TaskContext,
        ) -> OrchestrationResult<crate::orchestration::types::TaskResult> {
            Ok(crate::orchestration::types::TaskResult::ReenqueueImmediate)
        }

        fn metadata(&self) -> HandlerMetadata {
            self.metadata.clone()
        }
    }

    #[tokio::test]
    async fn test_register_and_get_handler() {
        let event_publisher = EventPublisher::new();
        let registry = TaskHandlerRegistry::new(event_publisher);

        let handler = Arc::new(MockTaskHandler {
            metadata: HandlerMetadata {
                namespace: "test".to_string(),
                name: "mock_handler".to_string(),
                version: "1.0.0".to_string(),
                handler_class: "MockHandler".to_string(),
                config_schema: None,
                registered_at: Utc::now(),
            },
        });

        // Register handler
        registry
            .register_handler("test", "mock_handler", "1.0.0", handler.clone())
            .await
            .unwrap();

        // Get handler back
        let retrieved = registry
            .get_handler("test", "mock_handler", "1.0.0")
            .unwrap();
        assert_eq!(retrieved.metadata().namespace, "test");
        assert_eq!(retrieved.metadata().name, "mock_handler");
        assert_eq!(retrieved.metadata().version, "1.0.0");
    }

    #[tokio::test]
    async fn test_register_ffi_handler() {
        let event_publisher = EventPublisher::new();
        let registry = TaskHandlerRegistry::new(event_publisher);

        // Register FFI handler
        registry
            .register_ffi_handler(
                "payments",
                "payment_processor",
                "2.1.0",
                "PaymentProcessorHandler",
                None,
            )
            .await
            .unwrap();

        // Get metadata
        let metadata = registry
            .get_handler_metadata("payments", "payment_processor", "2.1.0")
            .unwrap();
        assert_eq!(metadata.namespace, "payments");
        assert_eq!(metadata.name, "payment_processor");
        assert_eq!(metadata.version, "2.1.0");
        assert_eq!(metadata.handler_class, "PaymentProcessorHandler");
    }

    #[tokio::test]
    async fn test_list_handlers_by_namespace() {
        let event_publisher = EventPublisher::new();
        let registry = TaskHandlerRegistry::new(event_publisher);

        // Register handlers in different namespaces
        registry
            .register_ffi_handler("ecommerce", "order_handler", "1.0.0", "OrderHandler", None)
            .await
            .unwrap();
        registry
            .register_ffi_handler("ecommerce", "cart_handler", "1.0.0", "CartHandler", None)
            .await
            .unwrap();
        registry
            .register_ffi_handler(
                "payments",
                "payment_handler",
                "1.0.0",
                "PaymentHandler",
                None,
            )
            .await
            .unwrap();

        // List handlers in ecommerce namespace
        let ecommerce_handlers = registry.list_handlers(Some("ecommerce")).unwrap();
        assert_eq!(ecommerce_handlers.len(), 2);

        // List all handlers
        let all_handlers = registry.list_handlers(None).unwrap();
        assert_eq!(all_handlers.len(), 3);
    }

    #[tokio::test]
    async fn test_registry_stats() {
        let event_publisher = EventPublisher::new();
        let registry = TaskHandlerRegistry::new(event_publisher);

        // Register some handlers
        registry
            .register_ffi_handler("test", "handler1", "1.0.0", "Handler1", None)
            .await
            .unwrap();
        registry
            .register_ffi_handler("test", "handler2", "1.0.0", "Handler2", None)
            .await
            .unwrap();

        let stats = registry.stats().unwrap();
        assert_eq!(stats.total_ffi_handlers, 2);
        assert_eq!(stats.namespaces.len(), 1);
        assert!(stats.namespaces.contains(&"test".to_string()));
        assert!(stats.thread_safe);
    }
}
