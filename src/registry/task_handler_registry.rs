//! # Task Handler Registry
//!
//! Central registry for task handler resolution with support for both Rust and FFI handlers.
//!
//! ## Architecture
//!
//! The registry follows the Rails HandlerFactory pattern with hierarchical storage:
//! ```text
//! namespace -> name -> version -> handler
//! ```
//!
//! ## Key Features
//!
//! - **Dual-path support**: Direct handler references for Rust, metadata for FFI
//! - **Default handling**: Automatic defaults for namespace ("default") and version ("0.1.0")
//! - **Thread-safe operations**: Concurrent access using RwLock
//! - **Event integration**: Registration notifications via EventPublisher
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::TaskHandlerRegistry;
//! use tasker_core::models::core::task_request::TaskRequest;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = TaskHandlerRegistry::new();
//!
//! // Register an FFI handler
//! registry.register_ffi_handler(
//!     "payments",
//!     "order_processing",
//!     "1.0.0",
//!     "OrderProcessingHandler",
//!     None
//! ).await?;
//!
//! // Resolve from TaskRequest
//! let task_request = TaskRequest::new("order_processing".to_string(), "payments".to_string());
//! let handler_metadata = registry.resolve_handler(&task_request)?;
//! # Ok(())
//! # }
//! ```

use crate::error::{Result, TaskerError};
use crate::events::{Event, EventPublisher, OrchestrationEvent};
use crate::models::core::task_request::TaskRequest;
use crate::orchestration::types::{HandlerMetadata, TaskHandler};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Key for handler lookup in the registry
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerKey {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl HandlerKey {
    /// Create a new handler key with explicit values
    pub fn new(namespace: String, name: String, version: String) -> Self {
        Self {
            namespace,
            name,
            version,
        }
    }

    /// Create a handler key from a TaskRequest
    pub fn from_task_request(request: &TaskRequest) -> Self {
        Self {
            namespace: request.namespace.clone(),
            name: request.name.clone(),
            version: request.version.clone(),
        }
    }

    /// Convert to string key for storage
    pub fn key_string(&self) -> String {
        format!("{}/{}/{}", self.namespace, self.name, self.version)
    }
}

impl std::fmt::Display for HandlerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.namespace, self.name, self.version)
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_handlers: usize,
    pub total_ffi_handlers: usize,
    pub namespaces: Vec<String>,
}

/// Task handler registry with dual-path support
pub struct TaskHandlerRegistry {
    /// Direct handler references for Rust consumers
    handlers: Arc<RwLock<HashMap<String, Arc<dyn TaskHandler>>>>,
    /// Metadata for FFI consumers
    ffi_handlers: Arc<RwLock<HashMap<String, HandlerMetadata>>>,
    /// Event publisher for notifications
    event_publisher: Option<EventPublisher>,
}

impl TaskHandlerRegistry {
    /// Create a new task handler registry
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            ffi_handlers: Arc::new(RwLock::new(HashMap::new())),
            event_publisher: None,
        }
    }

    /// Create a new registry with event publisher
    pub fn with_event_publisher(event_publisher: EventPublisher) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            ffi_handlers: Arc::new(RwLock::new(HashMap::new())),
            event_publisher: Some(event_publisher),
        }
    }

    /// Register a Rust task handler
    pub async fn register_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler: Arc<dyn TaskHandler>,
    ) -> Result<()> {
        let key = HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());
        let key_string = key.key_string();

        info!(
            namespace = namespace,
            name = name,
            version = version,
            "Registering task handler"
        );

        // Validate handler
        self.validate_handler(&key, handler.as_ref())?;

        // Register direct handler reference
        {
            let mut handlers = self.handlers.write().map_err(|_| {
                TaskerError::OrchestrationError("Failed to acquire write lock".to_string())
            })?;

            if handlers.contains_key(&key_string) {
                warn!(key = key_string, "Handler already registered, replacing");
            }

            handlers.insert(key_string.clone(), handler.clone());
        }

        // Also register metadata for introspection
        let metadata = HandlerMetadata {
            namespace: namespace.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            handler_class: format!("{name}Handler"),
            config_schema: None,
            registered_at: Utc::now(),
        };

        {
            let mut ffi_handlers = self.ffi_handlers.write().map_err(|_| {
                TaskerError::OrchestrationError("Failed to acquire write lock".to_string())
            })?;

            ffi_handlers.insert(key_string.clone(), metadata.clone());
        }

        // Publish event if publisher is available
        if let Some(ref publisher) = self.event_publisher {
            let event = Event::orchestration(OrchestrationEvent::HandlerRegistered {
                handler_name: key_string.clone(),
                handler_type: "task_handler".to_string(),
                registered_at: Utc::now(),
            });
            publisher
                .publish_event(event)
                .await
                .map_err(|e| TaskerError::EventError(e.to_string()))?;
        }

        info!(key = key_string, "Task handler registered successfully");
        Ok(())
    }

    /// Register an FFI task handler (metadata only)
    pub async fn register_ffi_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        handler_class: &str,
        config_schema: Option<serde_json::Value>,
    ) -> Result<()> {
        let key = HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());
        let key_string = key.key_string();

        info!(
            namespace = namespace,
            name = name,
            version = version,
            handler_class = handler_class,
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
            let mut ffi_handlers = self.ffi_handlers.write().map_err(|_| {
                TaskerError::OrchestrationError("Failed to acquire write lock".to_string())
            })?;

            if ffi_handlers.contains_key(&key_string) {
                warn!(
                    key = key_string,
                    "FFI handler already registered, replacing"
                );
            }

            ffi_handlers.insert(key_string.clone(), metadata.clone());
        }

        // Publish event if publisher is available
        if let Some(ref publisher) = self.event_publisher {
            let event = Event::orchestration(OrchestrationEvent::HandlerRegistered {
                handler_name: key_string.clone(),
                handler_type: "task_handler".to_string(),
                registered_at: Utc::now(),
            });
            publisher
                .publish_event(event)
                .await
                .map_err(|e| TaskerError::EventError(e.to_string()))?;
        }

        info!(key = key_string, "FFI task handler registered successfully");
        Ok(())
    }

    /// Resolve a handler from a TaskRequest
    pub fn resolve_handler(&self, request: &TaskRequest) -> Result<HandlerMetadata> {
        // Build lookup key from request
        let key = HandlerKey::from_task_request(request);
        let key_string = key.key_string();

        debug!(
            namespace = &request.namespace,
            name = &request.name,
            version = &request.version,
            "Resolving handler for task request"
        );

        // Check FFI handlers (which includes metadata for all handlers)
        let ffi_handlers = self.ffi_handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        ffi_handlers.get(&key_string).cloned().ok_or_else(|| {
            TaskerError::ValidationError(format!(
                "Handler not found for {}/{}/{}",
                request.namespace, request.name, request.version
            ))
        })
    }

    /// Get a Rust handler directly (for in-process execution)
    pub fn get_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<Arc<dyn TaskHandler>> {
        let key = HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());
        let key_string = key.key_string();

        let handlers = self.handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        handlers
            .get(&key_string)
            .cloned()
            .ok_or_else(|| TaskerError::ValidationError(format!("Handler not found: {key_string}")))
    }

    /// Get handler metadata
    pub fn get_handler_metadata(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<HandlerMetadata> {
        let key = HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());
        let key_string = key.key_string();

        let ffi_handlers = self.ffi_handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        ffi_handlers.get(&key_string).cloned().ok_or_else(|| {
            TaskerError::ValidationError(format!("Handler metadata not found: {key_string}"))
        })
    }

    /// List all handlers in a namespace (or all if namespace is None)
    pub fn list_handlers(&self, namespace: Option<&str>) -> Result<Vec<HandlerMetadata>> {
        let ffi_handlers = self.ffi_handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        let filtered: Vec<_> = ffi_handlers
            .values()
            .filter(|metadata| {
                namespace.is_none() || Some(metadata.namespace.as_str()) == namespace
            })
            .cloned()
            .collect();

        Ok(filtered)
    }

    /// Check if a handler exists
    pub fn contains_handler(&self, namespace: &str, name: &str, version: &str) -> bool {
        let key = HandlerKey::new(namespace.to_string(), name.to_string(), version.to_string());
        let key_string = key.key_string();

        if let Ok(handlers) = self.ffi_handlers.read() {
            handlers.contains_key(&key_string)
        } else {
            false
        }
    }

    /// Get registry statistics
    pub fn stats(&self) -> Result<RegistryStats> {
        let handlers = self.handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        let ffi_handlers = self.ffi_handlers.read().map_err(|_| {
            TaskerError::OrchestrationError("Failed to acquire read lock".to_string())
        })?;

        let namespaces: std::collections::HashSet<_> =
            ffi_handlers.values().map(|m| m.namespace.clone()).collect();

        Ok(RegistryStats {
            total_handlers: handlers.len(),
            total_ffi_handlers: ffi_handlers.len(),
            namespaces: namespaces.into_iter().collect(),
        })
    }

    /// Clear all handlers (useful for testing)
    pub fn clear(&self) -> Result<()> {
        {
            let mut handlers = self.handlers.write().map_err(|_| {
                TaskerError::OrchestrationError("Failed to acquire write lock".to_string())
            })?;
            handlers.clear();
        }

        {
            let mut ffi_handlers = self.ffi_handlers.write().map_err(|_| {
                TaskerError::OrchestrationError("Failed to acquire write lock".to_string())
            })?;
            ffi_handlers.clear();
        }

        info!("Task handler registry cleared");
        Ok(())
    }

    /// Validate handler before registration
    fn validate_handler(&self, key: &HandlerKey, _handler: &dyn TaskHandler) -> Result<()> {
        // Validate namespace
        if key.namespace.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler namespace cannot be empty".to_string(),
            ));
        }

        // Validate name
        if key.name.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler name cannot be empty".to_string(),
            ));
        }

        // Validate version
        if key.version.is_empty() {
            return Err(TaskerError::ValidationError(
                "Handler version cannot be empty".to_string(),
            ));
        }

        // Validate semantic versioning
        if !self.is_valid_semver(&key.version) {
            return Err(TaskerError::ValidationError(format!(
                "Invalid version format: {}",
                key.version
            )));
        }

        Ok(())
    }

    /// Check if version follows semantic versioning
    fn is_valid_semver(&self, version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
    }
}

impl Default for TaskHandlerRegistry {
    fn default() -> Self {
        Self::new()
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
    use crate::orchestration::errors::OrchestrationError;
    use crate::orchestration::types::{TaskCompletionInfo, TaskContext, TaskResult};
    use async_trait::async_trait;

    // Mock handler for testing
    struct MockTaskHandler {
        namespace: String,
        name: String,
        version: String,
    }

    #[async_trait]
    impl TaskHandler for MockTaskHandler {
        async fn handle_task(
            &self,
            _context: &TaskContext,
        ) -> std::result::Result<TaskResult, OrchestrationError> {
            Ok(TaskResult::Complete(TaskCompletionInfo {
                task_id: 1,
                steps_executed: 0,
                total_execution_time_ms: 0,
                completed_at: Utc::now(),
                step_results: Vec::new(),
            }))
        }

        fn metadata(&self) -> HandlerMetadata {
            HandlerMetadata {
                namespace: self.namespace.clone(),
                name: self.name.clone(),
                version: self.version.clone(),
                handler_class: format!("{}Handler", self.name),
                config_schema: None,
                registered_at: Utc::now(),
            }
        }
    }

    #[tokio::test]
    async fn test_register_and_resolve() {
        let registry = TaskHandlerRegistry::new();

        // Register FFI handler
        registry
            .register_ffi_handler(
                "payments",
                "order_processing",
                "1.0.0",
                "OrderProcessingHandler",
                None,
            )
            .await
            .unwrap();

        // Create task request
        let request = TaskRequest::new("order_processing".to_string(), "payments".to_string())
            .with_version("1.0.0".to_string());

        // Resolve handler
        let metadata = registry.resolve_handler(&request).unwrap();
        assert_eq!(metadata.namespace, "payments");
        assert_eq!(metadata.name, "order_processing");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.handler_class, "OrderProcessingHandler");
    }

    #[tokio::test]
    async fn test_default_values() {
        let registry = TaskHandlerRegistry::new();

        // Register with defaults
        registry
            .register_ffi_handler("default", "simple_task", "0.1.0", "SimpleTaskHandler", None)
            .await
            .unwrap();

        // Create request with defaults
        let request = TaskRequest::new("simple_task".to_string(), "default".to_string());

        // Should resolve with defaults
        let metadata = registry.resolve_handler(&request).unwrap();
        assert_eq!(metadata.namespace, "default");
        assert_eq!(metadata.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_rust_handler_registration() {
        let registry = TaskHandlerRegistry::new();

        let handler = Arc::new(MockTaskHandler {
            namespace: "test".to_string(),
            name: "mock_handler".to_string(),
            version: "1.0.0".to_string(),
        });

        // Register Rust handler
        registry
            .register_handler("test", "mock_handler", "1.0.0", handler.clone())
            .await
            .unwrap();

        // Get handler back
        let retrieved = registry
            .get_handler("test", "mock_handler", "1.0.0")
            .unwrap();
        let metadata = retrieved.metadata();
        assert_eq!(metadata.namespace, "test");
        assert_eq!(metadata.name, "mock_handler");
    }

    #[tokio::test]
    async fn test_list_handlers_by_namespace() {
        let registry = TaskHandlerRegistry::new();

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

    #[test]
    fn test_semver_validation() {
        let registry = TaskHandlerRegistry::new();

        assert!(registry.is_valid_semver("1.0.0"));
        assert!(registry.is_valid_semver("2.10.3"));
        assert!(registry.is_valid_semver("0.1.0"));

        assert!(!registry.is_valid_semver("1.0"));
        assert!(!registry.is_valid_semver("1.0.0.0"));
        assert!(!registry.is_valid_semver("v1.0.0"));
        assert!(!registry.is_valid_semver("1.a.0"));
    }
}
