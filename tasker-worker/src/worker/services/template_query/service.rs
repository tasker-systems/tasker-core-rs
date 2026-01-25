//! # Template Query Service
//!
//! TAS-77: Template query logic extracted from web/handlers/templates.rs.
//! TAS-169: Simplified API - templates are immutable after worker bootstrap.
//!
//! This service encapsulates template query functionality, making it available
//! to both the HTTP API and FFI consumers without code duplication.
//!
//! Cache operations (clear, maintain, refresh) remain internal to TaskTemplateManager
//! and are not exposed via the API. Restart worker to refresh templates.
//! Distributed cache status is now available via /health/detailed.

use std::sync::Arc;

use thiserror::Error;
use tracing::{debug, error, warn};

use crate::worker::task_template_manager::{TaskTemplateManager, WorkerTaskTemplateOperations};
use tasker_shared::types::api::worker::{
    TemplateListResponse, TemplateResponse, TemplateValidationResponse,
};
use tasker_shared::types::base::CacheStats;

/// Errors that can occur during template query operations
#[derive(Error, Debug)]
pub enum TemplateQueryError {
    /// Template not found
    #[error("Template not found: {namespace}/{name}/{version}")]
    NotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Handler metadata not found
    #[error("Handler metadata not found: {namespace}/{name}/{version}")]
    HandlerMetadataNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Template Query Service
///
/// TAS-77: Provides template query functionality independent of the HTTP layer.
/// TAS-169: Simplified API - cache operations are internal-only.
///
/// This service can be used by:
/// - Web API handlers (via `WorkerWebState`)
/// - FFI consumers (Ruby, Python, etc.)
/// - Internal systems
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::services::template_query::TemplateQueryService;
/// use tasker_worker::worker::task_template_manager::TaskTemplateManager;
/// use std::sync::Arc;
///
/// async fn example(task_template_manager: Arc<TaskTemplateManager>) -> Result<(), Box<dyn std::error::Error>> {
///     let service = TemplateQueryService::new(task_template_manager);
///
///     // Get a specific template
///     let template = service.get_template("payments", "process_order", "1.0.0").await?;
///
///     // List all templates (with cache stats)
///     let list = service.list_templates(true).await;
///
///     // Validate a template for worker execution
///     let validation = service.validate_template("payments", "process_order", "1.0.0").await?;
///     Ok(())
/// }
/// ```
pub struct TemplateQueryService {
    /// Task template manager for template operations
    task_template_manager: Arc<TaskTemplateManager>,
}

impl std::fmt::Debug for TemplateQueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateQueryService")
            .field("task_template_manager", &"TaskTemplateManager")
            .finish()
    }
}

impl TemplateQueryService {
    /// Create a new TemplateQueryService
    pub fn new(task_template_manager: Arc<TaskTemplateManager>) -> Self {
        Self {
            task_template_manager,
        }
    }

    /// Get the underlying task template manager reference
    pub fn task_template_manager(&self) -> &Arc<TaskTemplateManager> {
        &self.task_template_manager
    }

    // =========================================================================
    // Template Query Methods
    // =========================================================================

    /// Get a specific task template
    ///
    /// GET /templates/{namespace}/{name}/{version}
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<TemplateResponse, TemplateQueryError> {
        debug!(
            namespace = %namespace,
            name = %name,
            version = %version,
            "Getting task template"
        );

        // Get the template from the task template manager
        let template = self
            .task_template_manager
            .get_task_template(namespace, name, version)
            .await
            .map_err(|e| {
                warn!(
                    namespace = %namespace,
                    name = %name,
                    version = %version,
                    error = %e,
                    "Template not found or access error"
                );
                TemplateQueryError::NotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?;

        // Get handler metadata as well
        let handler_metadata = self
            .task_template_manager
            .get_handler_metadata(namespace, name, version)
            .await
            .map_err(|e| {
                error!(
                    namespace = %namespace,
                    name = %name,
                    version = %version,
                    error = %e,
                    "Failed to get handler metadata for template"
                );
                TemplateQueryError::HandlerMetadataNotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?;

        // Check if it was cached
        let cache_stats = self.task_template_manager.cache_stats().await;
        let cached = cache_stats.total_cached > 0;

        Ok(TemplateResponse {
            template,
            handler_metadata,
            cached,
            cache_age_seconds: None, // TODO: Calculate from cached entry
            access_count: None,      // TODO: Get from cached entry
        })
    }

    /// List supported templates and namespaces
    ///
    /// GET /templates
    pub async fn list_templates(&self, include_cache_stats: bool) -> TemplateListResponse {
        debug!("Listing supported templates and namespaces");

        let supported_namespaces = self.task_template_manager.supported_namespaces().await;

        // Get cache stats if requested
        let cache_stats = if include_cache_stats {
            Some(self.task_template_manager.cache_stats().await)
        } else {
            None
        };

        let template_count = self.task_template_manager.cache_stats().await.total_cached;

        // Worker capabilities (static for now)
        let worker_capabilities = vec![
            "command_processing".to_string(),
            "database_access".to_string(),
            "message_queues".to_string(),
            "ffi_integration".to_string(),
        ];

        TemplateListResponse {
            supported_namespaces,
            template_count,
            cache_stats,
            worker_capabilities,
        }
    }

    /// Validate a template for worker execution
    ///
    /// POST /templates/{namespace}/{name}/{version}/validate
    pub async fn validate_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> Result<TemplateValidationResponse, TemplateQueryError> {
        debug!(
            namespace = %namespace,
            name = %name,
            version = %version,
            "Validating template for worker execution"
        );

        // Get the template
        let template = self
            .task_template_manager
            .get_task_template(namespace, name, version)
            .await
            .map_err(|e| {
                warn!(
                    namespace = %namespace,
                    name = %name,
                    version = %version,
                    error = %e,
                    "Template not found for validation"
                );
                TemplateQueryError::NotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?;

        let mut errors = Vec::new();

        // Validate template for worker execution
        if let Err(validation_error) = self
            .task_template_manager
            .validate_for_worker(&template)
            .await
        {
            errors.push(validation_error.to_string());
        }

        // Extract required capabilities and step handlers
        let required_capabilities = self.task_template_manager.requires_capabilities(&template);
        let step_handlers = self.task_template_manager.extract_step_handlers(&template);

        Ok(TemplateValidationResponse {
            valid: errors.is_empty(),
            errors,
            required_capabilities,
            step_handlers,
        })
    }

    // =========================================================================
    // Internal Cache Access (for list_templates with include_cache_stats)
    // =========================================================================

    /// Get cache statistics (internal use)
    ///
    /// Used by list_templates when include_cache_stats=true.
    /// TAS-169: Cache operations are no longer exposed via API.
    pub async fn cache_stats(&self) -> CacheStats {
        debug!("Getting template cache statistics");
        self.task_template_manager.cache_stats().await
    }

    /// Get supported namespaces
    pub async fn supported_namespaces(&self) -> Vec<String> {
        self.task_template_manager.supported_namespaces().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_query_error_display() {
        let error = TemplateQueryError::NotFound {
            namespace: "payments".to_string(),
            name: "process_order".to_string(),
            version: "1.0.0".to_string(),
        };

        let display = format!("{}", error);
        assert!(display.contains("payments"));
        assert!(display.contains("process_order"));
        assert!(display.contains("1.0.0"));
    }

    #[test]
    fn test_handler_metadata_not_found_error() {
        let error = TemplateQueryError::HandlerMetadataNotFound {
            namespace: "test".to_string(),
            name: "task".to_string(),
            version: "1.0".to_string(),
        };

        let display = format!("{}", error);
        assert!(display.contains("Handler metadata not found"));
    }
}
