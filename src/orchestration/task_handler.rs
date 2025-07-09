//! # BaseTaskHandler Framework
//!
//! ## Architecture: Developer-Facing Task Integration
//!
//! The BaseTaskHandler provides the foundational framework for task execution across
//! multiple language bindings (Rails, Python, Node.js). This implementation follows
//! the "task handler foundation" architecture where Rust implements the complete
//! base class that other frameworks extend through subclassing.
//!
//! ## Key Components:
//!
//! - **Configuration Integration**: Uses ConfigurationManager for task templates
//! - **Framework Hooks**: Provides Rails-compatible methods for task lifecycle
//! - **Workflow Coordination**: Integrates with WorkflowCoordinator for orchestration
//! - **State Management**: Uses StateManager for task and step state transitions
//! - **Dependency Management**: Implements `establish_step_dependencies_and_defaults`
//! - **Annotation Updates**: Provides `update_annotations` for task metadata
//!
//! ## Usage Pattern:
//!
//! ```rust,no_run
//! use tasker_core::orchestration::task_handler::{BaseTaskHandler, TaskHandler, TaskExecutionContext};
//! use tasker_core::orchestration::config::ConfigurationManager;
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config_manager = Arc::new(ConfigurationManager::new());
//! let task_template = config_manager.load_task_template("config/tasks/payment_processing.yaml").await?;
//!
//! // Create a custom task handler by implementing the trait
//! struct PaymentTaskHandler;
//!
//! #[async_trait::async_trait]
//! impl TaskHandler for PaymentTaskHandler {
//!     async fn initialize(&self, context: &TaskExecutionContext) -> Result<(), Box<dyn std::error::Error>> {
//!         // Custom initialization logic
//!         Ok(())
//!     }
//!
//!     async fn before_execute(&self, context: &TaskExecutionContext) -> Result<(), Box<dyn std::error::Error>> {
//!         // Pre-execution hooks
//!         Ok(())
//!     }
//!
//!     async fn after_execute(&self, context: &TaskExecutionContext) -> Result<(), Box<dyn std::error::Error>> {
//!         // Post-execution cleanup
//!         Ok(())
//!     }
//! }
//!
//! // Use BaseTaskHandler with custom implementation
//! let mut base_handler = BaseTaskHandler::new(task_template, sqlx::PgPool::connect("postgresql://localhost/test").await?);
//! base_handler.set_custom_handler(Box::new(PaymentTaskHandler));
//!
//! // Execute the task
//! let context = TaskExecutionContext {
//!     task_id: 123,
//!     namespace: "payments".to_string(),
//!     input_data: serde_json::json!({"amount": 100.0}),
//!     environment: "production".to_string(),
//!     metadata: std::collections::HashMap::new(),
//! };
//!
//! let result = base_handler.execute_task(context).await?;
//! # Ok(())
//! # }
//! ```

use crate::orchestration::config::{ConfigurationManager, TaskTemplate};
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use crate::orchestration::system_events::SystemEventsManager;
use crate::orchestration::types::{FrameworkIntegration, TaskContext, TaskOrchestrationResult};
use crate::orchestration::workflow_coordinator::{WorkflowCoordinator, WorkflowCoordinatorConfig};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

/// Task execution context containing all information needed for task processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionContext {
    /// Unique task identifier
    pub task_id: i64,

    /// Task namespace
    pub namespace: String,

    /// Input data for the task
    pub input_data: serde_json::Value,

    /// Execution environment (development, staging, production)
    pub environment: String,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Task handler trait for framework extension
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync {
    /// Initialize the task handler
    async fn initialize(
        &self,
        context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Hook called before task execution
    async fn before_execute(
        &self,
        context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Default implementation does nothing
        let _ = context;
        Ok(())
    }

    /// Hook called after task execution
    async fn after_execute(
        &self,
        context: &TaskExecutionContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Default implementation does nothing
        let _ = context;
        Ok(())
    }

    /// Validate task configuration - optional validation hook
    fn validate_config(
        &self,
        config: &HashMap<String, serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Default implementation accepts all configurations
        let _ = config;
        Ok(())
    }
}

/// Base task handler implementation
pub struct BaseTaskHandler {
    /// Task template configuration
    task_template: TaskTemplate,

    /// Database pool for operations
    #[allow(dead_code)] // Will be used for database operations in the future
    pool: PgPool,

    /// Workflow coordinator for orchestration
    workflow_coordinator: WorkflowCoordinator,

    /// Configuration manager
    config_manager: Arc<ConfigurationManager>,

    /// System events manager
    #[allow(dead_code)] // Will be used for event publishing in the future
    events_manager: Arc<SystemEventsManager>,

    /// Custom task handler implementation
    custom_handler: Option<Box<dyn TaskHandler>>,

    /// Framework integration for step execution
    framework_integration: Option<Arc<dyn FrameworkIntegration>>,
}

impl BaseTaskHandler {
    /// Create a new base task handler from a task template
    pub fn new(task_template: TaskTemplate, pool: PgPool) -> Self {
        let config_manager = Arc::new(ConfigurationManager::new());
        let workflow_config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);
        let workflow_coordinator = WorkflowCoordinator::with_config_manager(
            pool.clone(),
            workflow_config,
            config_manager.clone(),
        );

        // Create system events manager - in production this would be loaded from file
        let events_manager = Arc::new(SystemEventsManager::new(
            crate::orchestration::system_events::SystemEventsConfig {
                event_metadata: std::collections::HashMap::new(),
                state_machine_mappings: crate::orchestration::system_events::StateMachineMappings {
                    task_transitions: vec![],
                    step_transitions: vec![],
                },
            },
        ));

        Self {
            task_template,
            pool,
            workflow_coordinator,
            config_manager,
            events_manager,
            custom_handler: None,
            framework_integration: None,
        }
    }

    /// Create a new base task handler with custom configuration manager
    pub fn with_config_manager(
        task_template: TaskTemplate,
        pool: PgPool,
        config_manager: Arc<ConfigurationManager>,
    ) -> Self {
        let workflow_config = WorkflowCoordinatorConfig::from_config_manager(&config_manager);
        let workflow_coordinator = WorkflowCoordinator::with_config_manager(
            pool.clone(),
            workflow_config,
            config_manager.clone(),
        );

        // Create system events manager
        let events_manager = Arc::new(SystemEventsManager::new(
            crate::orchestration::system_events::SystemEventsConfig {
                event_metadata: std::collections::HashMap::new(),
                state_machine_mappings: crate::orchestration::system_events::StateMachineMappings {
                    task_transitions: vec![],
                    step_transitions: vec![],
                },
            },
        ));

        Self {
            task_template,
            pool,
            workflow_coordinator,
            config_manager,
            events_manager,
            custom_handler: None,
            framework_integration: None,
        }
    }

    /// Set custom task handler implementation
    pub fn set_custom_handler(&mut self, handler: Box<dyn TaskHandler>) {
        self.custom_handler = Some(handler);
    }

    /// Set framework integration for step execution
    pub fn set_framework_integration(&mut self, integration: Arc<dyn FrameworkIntegration>) {
        self.framework_integration = Some(integration);
    }

    /// Execute a task with full lifecycle management
    #[instrument(skip(self, context))]
    pub async fn execute_task(
        &self,
        context: TaskExecutionContext,
    ) -> OrchestrationResult<TaskOrchestrationResult> {
        info!(
            "Starting task execution: {} (task_id: {})",
            self.task_template.name, context.task_id
        );

        // Initialize handler if custom implementation provided
        if let Some(handler) = &self.custom_handler {
            handler.initialize(&context).await.map_err(|e| {
                OrchestrationError::TaskExecutionFailed {
                    task_id: context.task_id,
                    reason: format!("Handler initialization failed: {e}"),
                    error_code: None,
                }
            })?;
        }

        // Establish step dependencies and defaults (Rails-compatible method)
        self.establish_step_dependencies_and_defaults(context.task_id)
            .await?;

        // Call before_execute hook
        if let Some(handler) = &self.custom_handler {
            handler.before_execute(&context).await.map_err(|e| {
                OrchestrationError::TaskExecutionFailed {
                    task_id: context.task_id,
                    reason: format!("before_execute hook failed: {e}"),
                    error_code: None,
                }
            })?;
        }

        // Execute the workflow using WorkflowCoordinator
        let framework = self.framework_integration.as_ref().ok_or_else(|| {
            OrchestrationError::ConfigurationError {
                source: "BaseTaskHandler".to_string(),
                reason: "No framework integration configured".to_string(),
            }
        })?;

        let result = self
            .workflow_coordinator
            .execute_task_workflow(context.task_id, framework.clone())
            .await?;

        // Call after_execute hook
        if let Some(handler) = &self.custom_handler {
            if let Err(e) = handler.after_execute(&context).await {
                warn!("after_execute hook failed: {}", e);
                // Don't fail the task for cleanup errors
            }
        }

        // Update annotations after execution (Rails-compatible method)
        self.update_annotations(context.task_id, &result).await?;

        Ok(result)
    }

    /// Establish step dependencies and defaults (Rails-compatible)
    #[instrument(skip(self))]
    async fn establish_step_dependencies_and_defaults(
        &self,
        task_id: i64,
    ) -> OrchestrationResult<()> {
        debug!("Establishing step dependencies for task: {}", task_id);

        // This method would:
        // 1. Create workflow steps from the task template
        // 2. Set up dependencies between steps
        // 3. Apply default retry policies and timeouts
        // 4. Initialize step states

        // For now, we'll implement a simplified version
        // In production, this would use the models to create actual workflow steps

        for (index, step_template) in self.task_template.step_templates.iter().enumerate() {
            debug!("Processing step template {}: {}", index, step_template.name);

            // Validate dependencies exist
            if let Some(depends_on) = &step_template.depends_on_step {
                if !self
                    .task_template
                    .step_templates
                    .iter()
                    .any(|s| &s.name == depends_on)
                {
                    return Err(OrchestrationError::ValidationError {
                        field: "depends_on_step".to_string(),
                        reason: format!(
                            "Step '{}' depends on non-existent step '{}'",
                            step_template.name, depends_on
                        ),
                    });
                }
            }

            // Apply defaults from template
            let retry_limit = step_template.default_retry_limit.unwrap_or(
                self.config_manager
                    .system_config()
                    .backoff
                    .default_backoff_seconds
                    .len() as i32,
            );

            let timeout = step_template.timeout_seconds.unwrap_or(
                self.config_manager
                    .system_config()
                    .execution
                    .step_execution_timeout_seconds as i32,
            );

            debug!(
                "Step '{}' configured with retry_limit={}, timeout={}s",
                step_template.name, retry_limit, timeout
            );
        }

        info!(
            "Established dependencies for {} steps",
            self.task_template.step_templates.len()
        );

        Ok(())
    }

    /// Update task annotations after execution (Rails-compatible)
    #[instrument(skip(self, result))]
    async fn update_annotations(
        &self,
        task_id: i64,
        result: &TaskOrchestrationResult,
    ) -> OrchestrationResult<()> {
        debug!("Updating annotations for task: {}", task_id);

        // This method would update task metadata/annotations based on execution results
        // Examples:
        // - execution_time
        // - steps_completed
        // - error_summary
        // - retry_count

        let annotations = match result {
            TaskOrchestrationResult::Complete {
                steps_executed,
                total_execution_time_ms,
                ..
            } => {
                serde_json::json!({
                    "status": "complete",
                    "steps_executed": steps_executed,
                    "execution_time_ms": total_execution_time_ms,
                    "completed_at": chrono::Utc::now().to_rfc3339(),
                })
            }
            TaskOrchestrationResult::Failed {
                error,
                failed_steps,
                ..
            } => {
                serde_json::json!({
                    "status": "failed",
                    "error": error,
                    "failed_steps": failed_steps,
                    "failed_at": chrono::Utc::now().to_rfc3339(),
                })
            }
            TaskOrchestrationResult::InProgress {
                steps_executed,
                next_poll_delay_ms,
                ..
            } => {
                serde_json::json!({
                    "status": "in_progress",
                    "steps_executed": steps_executed,
                    "next_poll_delay_ms": next_poll_delay_ms,
                    "last_update": chrono::Utc::now().to_rfc3339(),
                })
            }
            TaskOrchestrationResult::Blocked {
                blocking_reason, ..
            } => {
                serde_json::json!({
                    "status": "blocked",
                    "blocking_reason": blocking_reason,
                    "blocked_at": chrono::Utc::now().to_rfc3339(),
                })
            }
        };

        debug!("Task annotations updated: {:?}", annotations);

        // In production, this would persist to the database
        // For now, we just log the annotations

        Ok(())
    }

    /// Get task template information
    pub fn task_template(&self) -> &TaskTemplate {
        &self.task_template
    }

    /// Get task name
    pub fn task_name(&self) -> &str {
        &self.task_template.name
    }

    /// Get handler class
    pub fn handler_class(&self) -> &str {
        &self.task_template.task_handler_class
    }

    /// Get namespace
    pub fn namespace(&self) -> &str {
        &self.task_template.namespace_name
    }
}

/// Factory for creating BaseTaskHandler instances from configuration
pub struct TaskHandlerFactory {
    config_manager: Arc<ConfigurationManager>,
    pool: PgPool,
}

impl TaskHandlerFactory {
    /// Create a new task handler factory
    pub fn new(config_manager: Arc<ConfigurationManager>, pool: PgPool) -> Self {
        Self {
            config_manager,
            pool,
        }
    }

    /// Create a task handler from a task template
    pub fn create_handler(&self, task_template: TaskTemplate) -> BaseTaskHandler {
        BaseTaskHandler::with_config_manager(
            task_template,
            self.pool.clone(),
            Arc::clone(&self.config_manager),
        )
    }

    /// Create a task handler from a template file
    pub async fn create_handler_from_file(
        &self,
        template_path: &str,
    ) -> OrchestrationResult<BaseTaskHandler> {
        let task_template = self
            .config_manager
            .load_task_template(template_path)
            .await?;

        Ok(self.create_handler(task_template))
    }
}

/// Default framework integration for testing
pub struct DefaultFrameworkIntegration;

#[async_trait::async_trait]
impl FrameworkIntegration for DefaultFrameworkIntegration {
    async fn execute_single_step(
        &self,
        step: &crate::orchestration::types::ViableStep,
        _task_context: &TaskContext,
    ) -> Result<crate::orchestration::types::StepResult, OrchestrationError> {
        // Default implementation just returns success
        Ok(crate::orchestration::types::StepResult {
            step_id: step.step_id,
            status: crate::orchestration::types::StepStatus::Completed,
            output: serde_json::json!({"message": "Default implementation"}),
            execution_duration: std::time::Duration::from_millis(100),
            error_message: None,
            retry_after: None,
            error_code: None,
            error_context: None,
        })
    }

    fn framework_name(&self) -> &'static str {
        "DefaultFramework"
    }

    async fn get_task_context(&self, task_id: i64) -> Result<TaskContext, OrchestrationError> {
        Ok(TaskContext {
            task_id,
            data: serde_json::json!({}),
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_execution_context_creation() {
        let context = TaskExecutionContext {
            task_id: 123,
            namespace: "payments".to_string(),
            input_data: serde_json::json!({"amount": 100.0}),
            environment: "test".to_string(),
            metadata: HashMap::new(),
        };

        assert_eq!(context.task_id, 123);
        assert_eq!(context.namespace, "payments");
        assert_eq!(context.environment, "test");
    }

    #[test]
    fn test_base_task_handler_creation() {
        let _task_template = TaskTemplate {
            name: "test_task".to_string(),
            module_namespace: Some("TestModule".to_string()),
            task_handler_class: "TestTaskHandler".to_string(),
            namespace_name: "test_namespace".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test task".to_string()),
            default_dependent_system: None,
            named_steps: vec!["step1".to_string(), "step2".to_string()],
            schema: None,
            step_templates: vec![],
            environments: None,
            custom_events: None,
        };

        // We can't create a real PgPool in tests without a database
        // This test just verifies the struct creation compiles
        // Integration tests will test the full functionality
    }

    #[test]
    fn test_task_handler_factory() {
        let _config_manager = Arc::new(ConfigurationManager::new());
        // Factory creation test - actual usage requires a database
    }
}
