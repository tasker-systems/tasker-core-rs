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
//! use tasker_core::orchestration::step_handler::{BaseStepHandler, StepExecutionContext, StepResult};
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
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult, StepExecutionError};
use crate::orchestration::system_events::SystemEventsManager;
use crate::state_machine::events::StepEvent;
use crate::state_machine::StepStateMachine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

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

/// Base step handler implementation
pub struct BaseStepHandler {
    /// Step template configuration
    step_template: StepTemplate,

    /// Configuration manager for accessing system config
    config_manager: Arc<ConfigurationManager>,

    /// State machine for step lifecycle management
    state_machine: Option<Arc<StepStateMachine>>,

    /// Custom step handler implementation
    custom_handler: Option<Box<dyn StepHandler>>,

    /// System events manager for event publishing
    events_manager: Option<Arc<SystemEventsManager>>,
}

impl BaseStepHandler {
    /// Create a new base step handler from a step template
    pub fn new(step_template: StepTemplate) -> Self {
        Self {
            step_template,
            config_manager: Arc::new(ConfigurationManager::new()),
            state_machine: None,
            custom_handler: None,
            events_manager: None,
        }
    }

    /// Create a new base step handler with custom configuration manager
    pub fn with_config_manager(
        step_template: StepTemplate,
        config_manager: Arc<ConfigurationManager>,
    ) -> Self {
        Self {
            step_template,
            config_manager,
            state_machine: None,
            custom_handler: None,
            events_manager: None,
        }
    }

    /// Set the state machine for this step handler
    pub fn set_state_machine(&mut self, state_machine: Arc<StepStateMachine>) {
        self.state_machine = Some(state_machine);
    }

    /// Set custom step handler implementation
    pub fn set_custom_handler(&mut self, handler: Box<dyn StepHandler>) {
        self.custom_handler = Some(handler);
    }

    /// Set system events manager for event publishing
    pub fn set_events_manager(&mut self, events_manager: Arc<SystemEventsManager>) {
        self.events_manager = Some(events_manager);
    }

    /// Execute a step with full lifecycle management
    #[instrument(skip(self, context))]
    pub async fn execute_step(
        &self,
        mut context: StepExecutionContext,
    ) -> OrchestrationResult<StepResult> {
        let start_time = SystemTime::now();
        info!(
            "Starting step execution: {} (step_id: {})",
            context.step_name, context.step_id
        );

        // Validate step configuration
        if let Some(handler) = &self.custom_handler {
            handler.validate_config(&context.step_config)?;
        }

        // Apply step template configuration to context
        self.apply_template_config(&mut context);

        // Publish before_handle event
        if let Some(events_manager) = &self.events_manager {
            let before_handle_payload = serde_json::json!({
                "task_id": context.task_id.to_string(),
                "step_id": context.step_id.to_string(),
                "step_name": context.step_name,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            debug!(
                "Publishing before_handle event for step: {}",
                context.step_name
            );
            // TODO: Integrate with EventPublisher to actually publish the event
            // For now, we just create the payload for validation
            if let Err(e) = events_manager.config().validate_event_payload(
                "step",
                "before_handle",
                &before_handle_payload,
            ) {
                warn!("Event payload validation failed: {}", e);
            }
        }

        // Transition to executing state
        if let Some(state_machine) = &self.state_machine {
            let mut state_machine = state_machine.as_ref().clone();
            state_machine.transition(StepEvent::Start).await?;
        }

        // Execute the step with timeout
        let execution_result = self.execute_with_timeout(&context).await;

        // Calculate execution duration
        let execution_duration = start_time.elapsed().unwrap_or(Duration::from_secs(0));

        // Process execution result
        let result = self
            .process_execution_result(execution_result, execution_duration, &context)
            .await;

        // Call process_results hook if available
        if let Some(handler) = &self.custom_handler {
            if let Err(e) = handler.process_results(&context, &result).await {
                warn!("process_results hook failed: {}", e);
                // Don't fail the step for hook errors, just log them
            }
        }

        // Update state based on result
        if let Some(state_machine) = &self.state_machine {
            let event = if result.success {
                StepEvent::Complete(result.output_data.clone())
            } else if let Some(error) = &result.error {
                StepEvent::Fail(error.to_string())
            } else {
                StepEvent::Fail("Unknown error".to_string())
            };

            let mut state_machine = state_machine.as_ref().clone();
            if let Err(e) = state_machine.transition(event).await {
                error!("Failed to transition step state: {}", e);
                // Don't fail the step for state transition errors in post-processing
            }
        }

        // Publish step completion/failure events
        if let Some(events_manager) = &self.events_manager {
            if result.success {
                let completed_payload = events_manager.create_step_completed_payload(
                    context.task_id,
                    context.step_id,
                    &context.step_name,
                    execution_duration.as_secs_f64(),
                    context.attempt_number,
                );

                debug!("Publishing step completed event for: {}", context.step_name);
                // TODO: Integrate with EventPublisher to actually publish the event
                if let Err(e) = events_manager.config().validate_event_payload(
                    "step",
                    "completed",
                    &completed_payload,
                ) {
                    warn!("Step completed event validation failed: {}", e);
                }
            } else if let Some(error) = &result.error {
                let failed_payload = events_manager.create_step_failed_payload(
                    context.task_id,
                    context.step_id,
                    &context.step_name,
                    &error.to_string(),
                    "StepExecutionError", // TODO: Get actual error class
                    context.attempt_number,
                );

                debug!("Publishing step failed event for: {}", context.step_name);
                // TODO: Integrate with EventPublisher to actually publish the event
                if let Err(e) = events_manager.config().validate_event_payload(
                    "step",
                    "failed",
                    &failed_payload,
                ) {
                    warn!("Step failed event validation failed: {}", e);
                }
            }
        }

        info!(
            "Step execution completed: {} (success: {}, duration: {:?})",
            context.step_name, result.success, execution_duration
        );

        Ok(result)
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

    /// Execute step with timeout protection
    async fn execute_with_timeout(
        &self,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value> {
        let timeout_duration = Duration::from_secs(context.timeout_seconds);

        // Get timeout override from custom handler
        let final_timeout = if let Some(handler) = &self.custom_handler {
            handler.get_timeout_override().unwrap_or(timeout_duration)
        } else {
            timeout_duration
        };

        debug!("Executing step with timeout: {:?}", final_timeout);

        match timeout(final_timeout, self.execute_step_logic(context)).await {
            Ok(result) => result,
            Err(_) => {
                error!("Step execution timed out after {:?}", final_timeout);
                Err(OrchestrationError::TimeoutError {
                    operation: format!("step_execution_{}", context.step_id),
                    timeout_duration: final_timeout,
                })
            }
        }
    }

    /// Core step execution logic
    async fn execute_step_logic(
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

    /// Process execution result and create StepResult
    async fn process_execution_result(
        &self,
        execution_result: OrchestrationResult<serde_json::Value>,
        execution_duration: Duration,
        context: &StepExecutionContext,
    ) -> StepResult {
        match execution_result {
            Ok(output_data) => {
                debug!("Step executed successfully: {}", context.step_name);
                StepResult {
                    success: true,
                    output_data: Some(output_data),
                    error: None,
                    should_retry: false,
                    retry_delay: None,
                    execution_duration,
                    metadata: HashMap::new(),
                    events_to_publish: vec![],
                }
            }
            Err(error) => {
                error!("Step execution failed: {} - {}", context.step_name, error);

                let should_retry = context.is_retryable
                    && context.attempt_number < context.max_retry_attempts
                    && self.is_retryable_error(&error);

                let retry_delay = if should_retry {
                    Some(self.calculate_retry_delay(context.attempt_number))
                } else {
                    None
                };

                // Convert OrchestrationError to StepExecutionError
                let step_error = self.convert_to_step_execution_error(error);

                StepResult {
                    success: false,
                    output_data: None,
                    error: Some(step_error),
                    should_retry,
                    retry_delay,
                    execution_duration,
                    metadata: HashMap::new(),
                    events_to_publish: vec![],
                }
            }
        }
    }

    /// Determine if an error is retryable
    fn is_retryable_error(&self, error: &OrchestrationError) -> bool {
        match error {
            OrchestrationError::TimeoutError { .. } => true,
            OrchestrationError::DatabaseError { .. } => true,
            OrchestrationError::SqlFunctionError { .. } => true,
            OrchestrationError::EventPublishingError { .. } => true,
            OrchestrationError::StepExecutionFailed { .. } => true,
            OrchestrationError::StateTransitionFailed { .. } => false,
            OrchestrationError::ValidationError { .. } => false,
            OrchestrationError::ConfigurationError { .. } => false,
            _ => false,
        }
    }

    /// Calculate retry delay using exponential backoff
    fn calculate_retry_delay(&self, attempt_number: u32) -> u64 {
        let system_config = self.config_manager.system_config();
        let backoff_config = &system_config.backoff;

        // Use configured backoff times if available
        if let Some(delay) = backoff_config
            .default_backoff_seconds
            .get(attempt_number as usize)
        {
            *delay as u64
        } else {
            // Fallback to exponential backoff
            let base_delay = backoff_config
                .default_backoff_seconds
                .last()
                .map(|&d| d as f64)
                .unwrap_or(32.0);

            let delay = base_delay
                * backoff_config
                    .backoff_multiplier
                    .powi(attempt_number as i32);
            delay.min(backoff_config.max_backoff_seconds as f64) as u64
        }
    }

    /// Convert OrchestrationError to StepExecutionError
    fn convert_to_step_execution_error(&self, error: OrchestrationError) -> StepExecutionError {
        match error {
            OrchestrationError::TimeoutError {
                timeout_duration, ..
            } => StepExecutionError::Timeout {
                message: format!("Step execution timed out after {timeout_duration:?}"),
                timeout_duration,
            },
            OrchestrationError::ValidationError { field, reason } => {
                StepExecutionError::Permanent {
                    message: format!("Validation error in field '{field}': {reason}"),
                    error_code: Some("VALIDATION_ERROR".to_string()),
                }
            }
            OrchestrationError::ConfigurationError { reason, .. } => {
                StepExecutionError::Permanent {
                    message: format!("Configuration error: {reason}"),
                    error_code: Some("CONFIG_ERROR".to_string()),
                }
            }
            _ => StepExecutionError::Retryable {
                message: error.to_string(),
                retry_after: None,
                skip_retry: false,
                context: None,
            },
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
