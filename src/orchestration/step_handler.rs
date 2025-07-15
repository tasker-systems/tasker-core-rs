//! # BaseStepHandler Framework
//!
//! ## Architecture: Configuration-Driven Step Execution
//!
//! The BaseStepHandler provides the foundational framework for step execution across
//! multiple language bindings (Rails, Python, Node.js). This implementation follows
//! the "step handler foundation" architecture where Rust implements the complete
//! base class that other frameworks extend through subclassing.
//!
//! ## Key Components:
//!
//! - **Configuration Integration**: Uses ConfigurationManager for step templates
//! - **Framework Hooks**: Provides `process()` and `process_results()` extension points
//! - **State Management**: Integrates with state machines for step lifecycle
//! - **Error Handling**: Comprehensive error classification and retry logic
//! - **Context Management**: Maintains execution context across step boundaries
//!
//! ## Usage Pattern:
//!
//! ```rust
//! use tasker_core::orchestration::step_handler::{BaseStepHandler, StepExecutionContext, StepResult, StepHandlerExecutor};
//! use tasker_core::orchestration::config::ConfigurationManager;
//! use std::collections::HashMap;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config_manager = ConfigurationManager::new();
//! let task_template = config_manager.load_task_template("config/tasks/payment_processing.yaml").await?;
//! let step_template = &task_template.step_templates[0];
//!
//! let step_handler = BaseStepHandler::new(step_template.clone());
//!
//! let context = StepExecutionContext {
//!     step_id: 123,
//!     task_id: 456,
//!     step_name: step_template.name.clone(),
//!     input_data: serde_json::json!({"amount": 100.0}),
//!     previous_steps: vec![], // Placeholder, will be populated by WorkflowStep objects
//!     step_config: HashMap::new(),
//!     attempt_number: 1,
//!     max_retry_attempts: 3,
//!     timeout_seconds: 30,
//!     is_retryable: true,
//!     environment: "test".to_string(),
//!     metadata: HashMap::new(),
//! };
//!
//! let result = step_handler.execute_step(context).await?;
//! // Default implementation passes through input data
//! assert!(result.success);
//! assert!(result.output_data.is_some());
//! # Ok(())
//! # }
//! ```

use crate::orchestration::config::{ConfigurationManager, StepTemplate};
use crate::orchestration::errors::{OrchestrationResult, StepExecutionError};
use crate::orchestration::step_execution_orchestrator::StepExecutionOrchestrator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

/// Step execution context containing all information needed for step processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionContext {
    /// Unique step identifier
    pub step_id: i64,

    /// Parent task identifier
    pub task_id: i64,

    /// Step name from the template
    pub step_name: String,

    /// Input data for the step
    pub input_data: serde_json::Value,

    /// Previous step results from dependency steps
    pub previous_steps: Vec<crate::models::WorkflowStep>,

    /// Step configuration from template
    pub step_config: HashMap<String, serde_json::Value>,

    /// Current attempt number (for retry logic)
    pub attempt_number: u32,

    /// Maximum retry attempts allowed
    pub max_retry_attempts: u32,

    /// Execution timeout in seconds
    pub timeout_seconds: u64,

    /// Whether this step is retryable
    pub is_retryable: bool,

    /// Execution environment (development, staging, production)
    pub environment: String,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Result of step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Whether the step completed successfully
    pub success: bool,

    /// Output data from the step
    pub output_data: Option<serde_json::Value>,

    /// Error information (if failed)
    pub error: Option<StepExecutionError>,

    /// Whether the step should be retried
    pub should_retry: bool,

    /// Delay before retry (in seconds)
    pub retry_delay: Option<u64>,

    /// Execution duration
    pub execution_duration: Duration,

    /// Additional result metadata
    pub metadata: HashMap<String, serde_json::Value>,

    /// Events to publish after step completion
    pub events_to_publish: Vec<StepExecutionEvent>,
}

/// Event to be published after step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepExecutionEvent {
    /// Event type name
    pub event_type: String,

    /// Event payload
    pub payload: serde_json::Value,

    /// Event metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Execution status for step tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Step is ready to execute
    Ready,

    /// Step is currently executing
    InProgress,

    /// Step completed successfully
    Completed,

    /// Step failed permanently
    Failed,

    /// Step failed but can be retried
    RetryableFailure,

    /// Step is blocked waiting for dependencies
    Blocked,

    /// Step execution timed out
    TimedOut,
}

/// Base step handler trait for framework extension
#[async_trait::async_trait]
pub trait StepHandler: Send + Sync {
    /// Process the step - this is the main extension point for frameworks
    async fn process(
        &self,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value>;

    /// Process results after step completion - optional extension point
    async fn process_results(
        &self,
        context: &StepExecutionContext,
        result: &StepResult,
    ) -> OrchestrationResult<()> {
        // Default implementation does nothing
        let _ = (context, result);
        Ok(())
    }

    /// Validate step configuration - optional validation hook
    fn validate_config(
        &self,
        config: &HashMap<String, serde_json::Value>,
    ) -> OrchestrationResult<()> {
        // Default implementation accepts all configurations
        let _ = config;
        Ok(())
    }

    /// Get step timeout override - optional timeout customization
    fn get_timeout_override(&self) -> Option<Duration> {
        None
    }
}

/// Extended trait for step handlers that provide full orchestrated execution
///
/// This trait allows step handlers to provide the complete execution experience
/// including state transitions, event publishing, retry logic, etc.
#[async_trait::async_trait]
pub trait StepHandlerExecutor: StepHandler {
    /// Execute a step with full lifecycle management
    ///
    /// This method provides the complete orchestration experience including:
    /// - State machine transitions
    /// - Event publishing
    /// - Timeout management
    /// - Retry logic and backoff
    /// - Error handling
    async fn execute_step(&self, context: StepExecutionContext) -> OrchestrationResult<StepResult>;
}

/// Base step handler implementation using composition
pub struct BaseStepHandler {
    /// Step template configuration
    step_template: StepTemplate,

    /// Orchestrator for shared execution logic
    orchestrator: StepExecutionOrchestrator,

    /// Custom step handler implementation
    custom_handler: Option<Box<dyn StepHandler>>,
}

impl BaseStepHandler {
    /// Create a new base step handler from a step template
    pub fn new(step_template: StepTemplate) -> Self {
        let config_manager = Arc::new(ConfigurationManager::new());
        let orchestrator = StepExecutionOrchestrator::new(config_manager);

        Self {
            step_template,
            orchestrator,
            custom_handler: None,
        }
    }

    /// Create a new base step handler with custom configuration manager
    pub fn with_config_manager(
        step_template: StepTemplate,
        config_manager: Arc<ConfigurationManager>,
    ) -> Self {
        let orchestrator = StepExecutionOrchestrator::new(config_manager);

        Self {
            step_template,
            orchestrator,
            custom_handler: None,
        }
    }

    /// Create a new base step handler with custom orchestrator
    pub fn with_orchestrator(
        step_template: StepTemplate,
        orchestrator: StepExecutionOrchestrator,
    ) -> Self {
        Self {
            step_template,
            orchestrator,
            custom_handler: None,
        }
    }

    /// Set custom step handler implementation
    pub fn set_custom_handler(&mut self, handler: Box<dyn StepHandler>) {
        self.custom_handler = Some(handler);
    }

    /// Get a reference to the orchestrator for additional configuration
    pub fn orchestrator(&self) -> &StepExecutionOrchestrator {
        &self.orchestrator
    }

    /// Get a mutable reference to the orchestrator for configuration
    pub fn orchestrator_mut(&mut self) -> &mut StepExecutionOrchestrator {
        &mut self.orchestrator
    }

    /// Apply step template configuration to execution context
    fn apply_template_config(&self, context: &mut StepExecutionContext) {
        // Apply retry configuration
        if let Some(retry_limit) = self.step_template.default_retry_limit {
            context.max_retry_attempts = retry_limit as u32;
        }

        if let Some(retryable) = self.step_template.default_retryable {
            context.is_retryable = retryable;
        }

        // Apply timeout configuration
        if let Some(timeout) = self.step_template.timeout_seconds {
            context.timeout_seconds = timeout as u64;
        }

        // Apply handler configuration
        if let Some(handler_config) = &self.step_template.handler_config {
            context.step_config.extend(handler_config.clone());
        }
    }

    /// Get step template information
    pub fn step_template(&self) -> &StepTemplate {
        &self.step_template
    }

    /// Get step name
    pub fn step_name(&self) -> &str {
        &self.step_template.name
    }

    /// Get handler class
    pub fn handler_class(&self) -> &str {
        &self.step_template.handler_class
    }
}

// Implement StepHandler trait for BaseStepHandler
#[async_trait::async_trait]
impl StepHandler for BaseStepHandler {
    async fn process(
        &self,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value> {
        if let Some(handler) = &self.custom_handler {
            // Use custom handler implementation
            handler.process(context).await
        } else {
            // Default implementation - just pass through input data
            debug!(
                "Using default step implementation for: {}",
                context.step_name
            );
            Ok(context.input_data.clone())
        }
    }

    async fn process_results(
        &self,
        context: &StepExecutionContext,
        result: &StepResult,
    ) -> OrchestrationResult<()> {
        if let Some(handler) = &self.custom_handler {
            handler.process_results(context, result).await
        } else {
            Ok(())
        }
    }

    fn validate_config(
        &self,
        config: &HashMap<String, serde_json::Value>,
    ) -> OrchestrationResult<()> {
        if let Some(handler) = &self.custom_handler {
            handler.validate_config(config)
        } else {
            Ok(())
        }
    }

    fn get_timeout_override(&self) -> Option<Duration> {
        if let Some(handler) = &self.custom_handler {
            handler.get_timeout_override()
        } else {
            self.step_template
                .timeout_seconds
                .map(|timeout| Duration::from_secs(timeout as u64))
        }
    }
}

// Implement StepHandlerExecutor trait for BaseStepHandler
#[async_trait::async_trait]
impl StepHandlerExecutor for BaseStepHandler {
    async fn execute_step(
        &self,
        mut context: StepExecutionContext,
    ) -> OrchestrationResult<StepResult> {
        // Apply step template configuration to context
        self.apply_template_config(&mut context);

        // Delegate to the orchestrator for full lifecycle management
        self.orchestrator.execute_step(self, context).await
    }
}

/// Factory for creating BaseStepHandler instances from configuration
pub struct StepHandlerFactory {
    config_manager: Arc<ConfigurationManager>,
}

impl StepHandlerFactory {
    /// Create a new step handler factory
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self { config_manager }
    }

    /// Create a step handler from a step template
    pub fn create_handler(&self, step_template: StepTemplate) -> BaseStepHandler {
        BaseStepHandler::with_config_manager(step_template, Arc::clone(&self.config_manager))
    }

    /// Create multiple step handlers from a task template
    pub async fn create_handlers_for_task(
        &self,
        task_template_path: &str,
    ) -> OrchestrationResult<Vec<BaseStepHandler>> {
        let task_template = self
            .config_manager
            .load_task_template(task_template_path)
            .await?;

        Ok(task_template
            .step_templates
            .into_iter()
            .map(|step_template| self.create_handler(step_template))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_step_execution_context_creation() {
        let context = StepExecutionContext {
            step_id: 123,
            task_id: 456,
            step_name: "test_step".to_string(),
            input_data: serde_json::json!({"test": "value"}),
            previous_steps: vec![], // Placeholder, will be populated by WorkflowStep objects
            step_config: HashMap::new(),
            attempt_number: 1,
            max_retry_attempts: 3,
            timeout_seconds: 300,
            is_retryable: true,
            environment: "test".to_string(),
            metadata: HashMap::new(),
        };

        assert_eq!(context.step_id, 123);
        assert_eq!(context.task_id, 456);
        assert_eq!(context.step_name, "test_step");
        assert!(context.is_retryable);
    }

    #[test]
    fn test_step_result_creation() {
        let result = StepResult {
            success: true,
            output_data: Some(serde_json::json!({"result": "success"})),
            error: None,
            should_retry: false,
            retry_delay: None,
            execution_duration: Duration::from_millis(500),
            metadata: HashMap::new(),
            events_to_publish: vec![],
        };

        assert!(result.success);
        assert!(result.output_data.is_some());
        assert!(result.error.is_none());
        assert!(!result.should_retry);
    }

    #[test]
    fn test_base_step_handler_creation() {
        let step_template = StepTemplate {
            name: "test_step".to_string(),
            description: Some("Test step".to_string()),
            handler_class: "TestHandler".to_string(),
            handler_config: None,
            depends_on_step: None,
            depends_on_steps: None,
            default_retryable: Some(true),
            default_retry_limit: Some(3),
            timeout_seconds: Some(300),
            retry_backoff: None,
        };

        let handler = BaseStepHandler::new(step_template);
        assert_eq!(handler.step_name(), "test_step");
        assert_eq!(handler.handler_class(), "TestHandler");
    }

    #[test]
    fn test_step_handler_factory() {
        let config_manager = Arc::new(ConfigurationManager::new());
        let factory = StepHandlerFactory::new(config_manager);

        let step_template = StepTemplate {
            name: "test_step".to_string(),
            description: Some("Test step".to_string()),
            handler_class: "TestHandler".to_string(),
            handler_config: None,
            depends_on_step: None,
            depends_on_steps: None,
            default_retryable: Some(true),
            default_retry_limit: Some(3),
            timeout_seconds: Some(300),
            retry_backoff: None,
        };

        let handler = factory.create_handler(step_template);
        assert_eq!(handler.step_name(), "test_step");
    }
}
