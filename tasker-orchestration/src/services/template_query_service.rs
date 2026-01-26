//! # Template Query Service (TAS-76)
//!
//! Read-only template discovery service for the orchestration API.
//! Provides query functionality for listing and retrieving task templates
//! independently of the HTTP layer.
//!
//! ## Design
//!
//! Unlike workers, orchestration does not execute templates - it only stores
//! and manages template definitions. This service provides visibility into
//! what templates are available for task creation.
//!
//! Uses `TaskHandlerRegistry` (via `SystemContext`) for individual template
//! resolution (leverages distributed caching), and direct database queries
//! for listing operations.
//!
//! ## Pattern
//!
//! ```text
//! Handler -> TemplateQueryService -> TaskHandlerRegistry (cached)
//!                                 -> Database (for listings)
//! ```

use sqlx::PgPool;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn};

use tasker_shared::models::{NamedTask, TaskNamespace};
use tasker_shared::registry::TaskHandlerRegistry;
use tasker_shared::system_context::SystemContext;
use tasker_shared::types::api::templates::{
    NamespaceSummary, StepDefinition, TemplateDetail, TemplateListResponse, TemplateSummary,
};

/// Errors that can occur during template query operations
#[derive(Error, Debug)]
pub enum TemplateQueryError {
    /// Template namespace not found
    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),

    /// Template not found
    #[error("Template not found: {namespace}/{name}/{version}")]
    TemplateNotFound {
        namespace: String,
        name: String,
        version: String,
    },

    /// Database error
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for template query operations
pub type TemplateQueryResult<T> = Result<T, TemplateQueryError>;

/// Template Query Service for Orchestration
///
/// TAS-76: Provides template discovery functionality independent of the HTTP layer.
///
/// This service can be used by:
/// - Web API handlers (via `AppState`)
/// - Future gRPC endpoints
/// - Internal systems
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_orchestration::services::TemplateQueryService;
/// use std::sync::Arc;
/// use tasker_shared::system_context::SystemContext;
///
/// async fn example(system_context: Arc<SystemContext>) -> Result<(), Box<dyn std::error::Error>> {
///     let service = TemplateQueryService::new(system_context);
///
///     // List all templates
///     let list = service.list_templates(None).await?;
///
///     // Get a specific template
///     let detail = service.get_template("payments", "process_order", "1.0.0").await?;
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct TemplateQueryService {
    /// Read-only database pool for listing queries
    db_pool: PgPool,
    /// Task handler registry for individual template resolution (with caching)
    task_handler_registry: TaskHandlerRegistry,
}

impl std::fmt::Debug for TemplateQueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateQueryService")
            .field("db_pool", &"PgPool")
            .field("task_handler_registry", &"TaskHandlerRegistry")
            .finish()
    }
}

impl TemplateQueryService {
    /// Create a new TemplateQueryService from SystemContext
    pub fn new(system_context: Arc<SystemContext>) -> Self {
        Self {
            db_pool: system_context.database_pool().clone(),
            task_handler_registry: TaskHandlerRegistry::with_system_context(system_context),
        }
    }

    /// Create with explicit database pool and registry (for testing)
    #[cfg(test)]
    pub fn with_pool_and_registry(db_pool: PgPool, registry: TaskHandlerRegistry) -> Self {
        Self {
            db_pool,
            task_handler_registry: registry,
        }
    }

    // =========================================================================
    // Template Query Methods
    // =========================================================================

    /// List available templates with namespace summaries
    ///
    /// Returns a list of all registered task templates, optionally filtered by namespace.
    pub async fn list_templates(
        &self,
        namespace_filter: Option<&str>,
    ) -> TemplateQueryResult<TemplateListResponse> {
        debug!(namespace = ?namespace_filter, "Listing available templates");

        // Get all namespaces with their template counts
        let namespace_infos =
            TaskNamespace::get_namespace_info_with_handler_count(&self.db_pool).await?;

        let namespaces: Vec<NamespaceSummary> = namespace_infos
            .iter()
            .map(|(ns, count)| NamespaceSummary {
                name: ns.name.clone(),
                description: ns.description.clone(),
                template_count: *count as usize,
            })
            .collect();

        // Pre-calculate total count for allocation
        let total_count: usize = namespace_infos.iter().map(|(_, c)| *c as usize).sum();

        // Get templates, optionally filtered by namespace
        let templates = if let Some(namespace_name) = namespace_filter {
            // Filter by specific namespace
            let tasks = NamedTask::list_by_namespace_name(&self.db_pool, namespace_name).await?;
            self.build_template_summaries(tasks, namespace_name).await
        } else {
            // Get all templates across all namespaces
            // Pre-allocate with known capacity to avoid reallocations
            let mut all_templates = Vec::with_capacity(total_count);
            for (ns, _) in &namespace_infos {
                let tasks = NamedTask::list_by_namespace_name(&self.db_pool, &ns.name).await?;
                all_templates.extend(self.build_template_summaries(tasks, &ns.name).await);
            }
            all_templates
        };

        Ok(TemplateListResponse {
            namespaces,
            templates,
            total_count,
        })
    }

    /// Get a specific template by namespace, name, and version
    ///
    /// Uses `TaskHandlerRegistry` for resolution which leverages distributed caching.
    pub async fn get_template(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TemplateQueryResult<TemplateDetail> {
        debug!(
            namespace = %namespace,
            name = %name,
            version = %version,
            "Getting template details"
        );

        // Use TaskHandlerRegistry to get the template (with caching)
        let task_template = self
            .task_handler_registry
            .get_task_template(namespace, name, version)
            .await
            .map_err(|e| {
                warn!(
                    namespace = %namespace,
                    name = %name,
                    version = %version,
                    error = %e,
                    "Template not found via registry"
                );
                TemplateQueryError::TemplateNotFound {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                    version: version.to_string(),
                }
            })?;

        // Convert TaskTemplate steps to StepDefinition for the API response
        let steps: Vec<StepDefinition> = task_template
            .steps
            .into_iter()
            .map(|step| StepDefinition {
                name: step.name,
                description: step.description,
                default_retryable: step.retry.retryable,
                default_max_attempts: step.retry.max_attempts as i32,
            })
            .collect();

        Ok(TemplateDetail {
            name: task_template.name,
            namespace: task_template.namespace_name,
            version: task_template.version,
            description: task_template.description,
            configuration: None, // Don't expose full config via API
            steps,
        })
    }

    /// Check if a template exists
    pub async fn template_exists(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> TemplateQueryResult<bool> {
        self.task_handler_registry
            .is_template_registered(namespace, name, version)
            .await
            .map_err(|e| TemplateQueryError::Internal(e.to_string()))
    }

    /// Get namespace information
    pub async fn get_namespace(&self, namespace: &str) -> TemplateQueryResult<NamespaceSummary> {
        let namespace_obj = TaskNamespace::find_by_name(&self.db_pool, namespace)
            .await?
            .ok_or_else(|| TemplateQueryError::NamespaceNotFound(namespace.to_string()))?;

        // Get template count for this namespace
        let templates = NamedTask::list_by_namespace_name(&self.db_pool, namespace).await?;

        Ok(NamespaceSummary {
            name: namespace_obj.name,
            description: namespace_obj.description,
            template_count: templates.len(),
        })
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Build template summaries from NamedTask records
    async fn build_template_summaries(
        &self,
        tasks: Vec<NamedTask>,
        namespace: &str,
    ) -> Vec<TemplateSummary> {
        let mut summaries = Vec::with_capacity(tasks.len());

        for task in tasks {
            // Try to get step count from the template configuration
            let step_count = self.get_step_count_for_task(&task).await.unwrap_or(0);

            summaries.push(TemplateSummary {
                name: task.name,
                namespace: namespace.to_string(),
                version: task.version,
                description: task.description,
                step_count,
            });
        }

        summaries
    }

    /// Get step count from task configuration
    async fn get_step_count_for_task(&self, task: &NamedTask) -> Option<usize> {
        // Try to parse the configuration as a TaskTemplate to get the step count
        if let Some(ref config) = task.configuration {
            // The configuration is the full TaskTemplate serialized as JSON
            if let Ok(template) = serde_json::from_value::<
                tasker_shared::models::core::task_template::TaskTemplate,
            >(config.clone())
            {
                return Some(template.steps.len());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_query_error_display() {
        let error = TemplateQueryError::TemplateNotFound {
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
    fn test_namespace_not_found_error() {
        let error = TemplateQueryError::NamespaceNotFound("unknown".to_string());
        let display = format!("{}", error);
        assert!(display.contains("Namespace not found"));
        assert!(display.contains("unknown"));
    }

    #[test]
    fn test_internal_error() {
        let error = TemplateQueryError::Internal("Something went wrong".to_string());
        let display = format!("{}", error);
        assert!(display.contains("Internal error"));
        assert!(display.contains("Something went wrong"));
    }
}
