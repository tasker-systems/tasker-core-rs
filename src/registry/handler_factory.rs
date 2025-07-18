//! # Handler Factory
//!
//! Factory pattern for creating and managing task handlers with thread-safe registration.
//!
//! ## Overview
//!
//! The HandlerFactory provides a centralized way to create task handlers and manage
//! their lifecycle. It works in conjunction with the TaskHandlerRegistry to provide
//! a complete handler management system.
//!
//! ## Key Features
//!
//! - **Thread-safe handler creation** using Arc for shared ownership
//! - **Configuration-driven instantiation** from templates
//! - **Lifecycle management** for handler instances
//! - **Dependency injection** for handler dependencies
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::handler_factory::HandlerFactory;
//! use tasker_core::orchestration::config::ConfigurationManager;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config_manager = Arc::new(ConfigurationManager::new());
//! let factory = HandlerFactory::new(config_manager);
//!
//! // Create a handler from configuration
//! let handler = factory.create_handler("payments", "payment_processor", "1.0.0").await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::orchestration::task_handler::BaseTaskHandler;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Factory for creating task handlers with configuration management
pub struct HandlerFactory {
    /// Configuration manager for loading handler templates
    config_manager: Arc<ConfigurationManager>,
    /// Database pool for handler creation
    pool: Option<PgPool>,
    /// Cache of created handlers for reuse
    handler_cache: tokio::sync::RwLock<HashMap<String, Arc<BaseTaskHandler>>>,
}

impl HandlerFactory {
    /// Create a new handler factory
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self {
            config_manager,
            pool: None,
            handler_cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Create a new handler factory with database pool
    pub fn with_pool(config_manager: Arc<ConfigurationManager>, pool: PgPool) -> Self {
        Self {
            config_manager,
            pool: Some(pool),
            handler_cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Create a handler from configuration
    pub async fn create_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<Arc<BaseTaskHandler>> {
        let handler_key = format!("{namespace}:{name}:{version}");

        // Check cache first
        {
            let cache = self.handler_cache.read().await;
            if let Some(handler) = cache.get(&handler_key) {
                debug!("Returning cached handler for {}", handler_key);
                return Ok(handler.clone());
            }
        }

        // Create new handler
        let handler = self.create_new_handler(namespace, name, version).await?;
        let handler_arc = Arc::new(handler);

        // Cache the handler
        {
            let mut cache = self.handler_cache.write().await;
            cache.insert(handler_key.clone(), handler_arc.clone());
        }

        info!("Created and cached new handler for {}", handler_key);
        Ok(handler_arc)
    }

    /// Create a new handler instance (internal method)
    async fn create_new_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> OrchestrationResult<BaseTaskHandler> {
        // Load task template from configuration
        let template_path = format!("config/tasks/{namespace}/{name}/{version}.yaml");
        let task_template = self
            .config_manager
            .load_task_template(&template_path)
            .await
            .map_err(|e| OrchestrationError::ConfigurationError {
                source: "HandlerFactory".to_string(),
                reason: format!("Failed to load template {template_path}: {e}"),
            })?;

        // Create handler with pool if available
        let handler = if let Some(pool) = &self.pool {
            BaseTaskHandler::with_config_manager(
                task_template,
                pool.clone(),
                self.config_manager.clone(),
            )
        } else {
            return Err(OrchestrationError::ConfigurationError {
                source: "HandlerFactory".to_string(),
                reason: "No database pool configured".to_string(),
            });
        };

        Ok(handler)
    }

    /// Clear the handler cache
    pub async fn clear_cache(&self) {
        let mut cache = self.handler_cache.write().await;
        cache.clear();
        info!("Handler cache cleared");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> HandlerCacheStats {
        let cache = self.handler_cache.read().await;
        HandlerCacheStats {
            cached_handlers: cache.len(),
            cache_keys: cache.keys().cloned().collect(),
        }
    }
}

/// Statistics about the handler cache
#[derive(Debug, Clone)]
pub struct HandlerCacheStats {
    pub cached_handlers: usize,
    pub cache_keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_factory_creation() {
        let config_manager = Arc::new(ConfigurationManager::new());
        let factory = HandlerFactory::new(config_manager);

        // Verify factory is created with empty cache
        assert!(factory.pool.is_none());
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let config_manager = Arc::new(ConfigurationManager::new());
        let factory = HandlerFactory::new(config_manager);

        // Test cache stats
        let stats = factory.cache_stats().await;
        assert_eq!(stats.cached_handlers, 0);
        assert!(stats.cache_keys.is_empty());

        // Test cache clear
        factory.clear_cache().await;
        let stats = factory.cache_stats().await;
        assert_eq!(stats.cached_handlers, 0);
    }
}
