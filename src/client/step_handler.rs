//! # Base Step Handler Implementation
//!
//! Provides the foundation for step execution in Rust applications,
//! integrating with the orchestration system and providing hooks for
//! custom business logic implementation.

use crate::client::context::StepContext;
use crate::client::traits::RustStepHandler;
use crate::error::{Result, TaskerError};
use crate::orchestration::handler_config::{HandlerConfiguration, StepTemplate};
// use crate::state_machine::events::StepEvent;
use crate::state_machine::StepStateMachine;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Base step handler that coordinates step execution
///
/// This struct serves as the foundation for all step execution in Rust applications.
/// It handles the orchestration concerns (state management, timing, error handling)
/// while delegating the actual business logic to implementations of RustStepHandler.
pub struct BaseStepHandler {
    /// Step template configuration from the handler configuration
    step_template: StepTemplate,

    /// Custom step handler implementation
    custom_handler: Option<Arc<dyn RustStepHandler>>,

    /// State machine for step lifecycle management
    state_machine: Option<Arc<StepStateMachine>>,

    /// Whether to enable detailed performance metrics
    enable_metrics: bool,
}

/// Result of step execution
#[derive(Debug, Clone)]
pub struct StepExecutionResult {
    /// Whether the step succeeded
    pub success: bool,

    /// Output data from the step processing
    pub output_data: Option<Value>,

    /// Error message if the step failed
    pub error_message: Option<String>,

    /// Error code for categorization
    pub error_code: Option<String>,

    /// Execution duration
    pub duration: Duration,

    /// Whether the step should be retried (only relevant for failures)
    pub should_retry: bool,

    /// Additional metadata about the execution
    pub metadata: std::collections::HashMap<String, Value>,
}

impl BaseStepHandler {
    /// Create a new base step handler from a step template
    pub fn new(step_template: StepTemplate) -> Self {
        Self {
            step_template,
            custom_handler: None,
            state_machine: None,
            enable_metrics: true,
        }
    }

    /// Set a custom step handler implementation
    pub fn with_handler(mut self, handler: Arc<dyn RustStepHandler>) -> Self {
        self.custom_handler = Some(handler);
        self
    }

    /// Set the state machine for step lifecycle management
    pub fn with_state_machine(mut self, state_machine: Arc<StepStateMachine>) -> Self {
        self.state_machine = Some(state_machine);
        self
    }

    /// Enable or disable performance metrics collection
    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }

    /// Execute a step with the given context
    #[instrument(skip(self, context), fields(step_id = context.step_id, step_name = %context.step_name))]
    pub async fn execute_step(&self, context: StepContext) -> Result<StepExecutionResult> {
        let start_time = Instant::now();

        info!(
            step_id = context.step_id,
            step_name = %context.step_name,
            attempt = context.attempt_number,
            "Starting step execution"
        );

        // TODO: Transition to in_progress state when state machine integration is complete
        // if let Some(ref state_machine) = self.state_machine {
        //     self.transition_step_state(&context, "in_progress", state_machine).await?;
        // }

        // Execute the step with timeout
        let execution_result = self.execute_with_timeout(&context).await;
        let duration = start_time.elapsed();

        // Process the result and determine final state
        let step_result = match execution_result {
            Ok(output_data) => {
                info!(
                    step_id = context.step_id,
                    duration_ms = duration.as_millis(),
                    "Step execution completed successfully"
                );

                // TODO: Transition to completed state when state machine integration is complete
                // if let Some(ref state_machine) = self.state_machine {
                //     self.transition_step_state(&context, "completed", state_machine).await?;
                // }

                StepExecutionResult {
                    success: true,
                    output_data: Some(output_data),
                    error_message: None,
                    error_code: None,
                    duration,
                    should_retry: false,
                    metadata: self.create_success_metadata(&context, duration),
                }
            }
            Err(error) => {
                let should_retry = context.can_retry() && self.is_retryable_error(&error);

                error!(
                    step_id = context.step_id,
                    error = %error,
                    should_retry = should_retry,
                    duration_ms = duration.as_millis(),
                    "Step execution failed"
                );

                // TODO: Transition to appropriate error state when state machine integration is complete
                // if let Some(ref state_machine) = self.state_machine {
                //     let target_state = if should_retry { "failed_retryable" } else { "failed" };
                //     self.transition_step_state(&context, target_state, state_machine).await?;
                // }

                StepExecutionResult {
                    success: false,
                    output_data: None,
                    error_message: Some(error.to_string()),
                    error_code: self.classify_error(&error),
                    duration,
                    should_retry,
                    metadata: self.create_error_metadata(&context, &error, duration),
                }
            }
        };

        // Call result processing hook if available and step succeeded
        if let Some(ref handler) = self.custom_handler {
            if let Some(ref output_data) = step_result.output_data {
                match handler.process_results(&context, output_data, None).await {
                    Ok(processed_result) => {
                        // Update the step result with the processed output
                        let mut updated_result = step_result.clone();
                        updated_result.output_data = Some(processed_result);
                        return Ok(updated_result);
                    }
                    Err(process_error) => {
                        warn!(
                            step_id = context.step_id,
                            error = %process_error,
                            "Result processing hook failed"
                        );
                        // Return error result instead of continuing with unprocessed output
                        return Ok(StepExecutionResult {
                            success: false,
                            output_data: None,
                            error_message: Some(format!(
                                "Result processing failed: {process_error}"
                            )),
                            error_code: Some("RESULT_PROCESSING_ERROR".to_string()),
                            duration: step_result.duration,
                            should_retry: context.can_retry(),
                            metadata: self.create_error_metadata(
                                &context,
                                &TaskerError::OrchestrationError(process_error.to_string()),
                                step_result.duration,
                            ),
                        });
                    }
                }
            }
        }

        Ok(step_result)
    }

    /// Execute the step with timeout protection
    async fn execute_with_timeout(&self, context: &StepContext) -> Result<Value> {
        let timeout_duration = Duration::from_secs(context.timeout_seconds);

        match timeout(timeout_duration, self.execute_step_internal(context)).await {
            Ok(result) => result,
            Err(_) => {
                error!(
                    step_id = context.step_id,
                    timeout_seconds = context.timeout_seconds,
                    "Step execution timed out"
                );
                Err(TaskerError::OrchestrationError(format!(
                    "Step execution timed out after {} seconds",
                    context.timeout_seconds
                )))
            }
        }
    }

    /// Internal step execution logic
    async fn execute_step_internal(&self, context: &StepContext) -> Result<Value> {
        // If no custom handler is provided, return input data as default behavior
        let Some(ref handler) = self.custom_handler else {
            debug!(
                step_id = context.step_id,
                "No custom handler provided, returning input data as output"
            );
            return Ok(context.input_data.clone());
        };

        // Validate configuration if the handler supports it
        if let Err(validation_error) = handler.validate_config(&context.step_config) {
            return Err(TaskerError::ValidationError(format!(
                "Step configuration validation failed: {validation_error}"
            )));
        }

        // Execute the custom handler logic
        handler
            .process(context)
            .await
            .map_err(|e| TaskerError::OrchestrationError(format!("Step handler failed: {e}")))
    }

    /// Transition step state through the state machine
    /// TODO: Complete implementation once state machine API is finalized
    #[allow(dead_code)]
    async fn transition_step_state(
        &self,
        context: &StepContext,
        target_state: &str,
        _state_machine: &StepStateMachine,
    ) -> Result<()> {
        // let event = match target_state {
        //     "in_progress" => StepEvent::Start,
        //     "completed" => StepEvent::Complete(None),
        //     "failed" | "failed_retryable" => StepEvent::Fail(
        //         format!("Step {} failed during execution", context.step_name)
        //     ),
        //     _ => {
        //         warn!("Unknown target state: {}, using Start event", target_state);
        //         StepEvent::Start
        //     }
        // };

        // state_machine.handle_event(context.step_id, event).await
        //     .map_err(|e| TaskerError::StateTransitionError(format!(
        //         "Failed to transition step {} to state {}: {}",
        //         context.step_id, target_state, e
        //     )))
        debug!(
            step_id = context.step_id,
            target_state = target_state,
            "State transition placeholder - implementation pending"
        );
        Ok(())
    }

    /// Check if an error is retryable
    fn is_retryable_error(&self, error: &TaskerError) -> bool {
        match error {
            TaskerError::DatabaseError(_) => true,
            TaskerError::OrchestrationError(_) => true,
            TaskerError::EventError(_) => true,
            TaskerError::ValidationError(_) => false,
            TaskerError::InvalidInput(_) => false,
            TaskerError::ConfigurationError(_) => false,
            TaskerError::FFIError(_) => true,
            TaskerError::StateTransitionError(_) => true,
        }
    }

    /// Classify error for reporting purposes
    fn classify_error(&self, error: &TaskerError) -> Option<String> {
        let code = match error {
            TaskerError::DatabaseError(_) => "DATABASE_ERROR",
            TaskerError::OrchestrationError(_) => "ORCHESTRATION_ERROR",
            TaskerError::EventError(_) => "EVENT_ERROR",
            TaskerError::ValidationError(_) => "VALIDATION_ERROR",
            TaskerError::InvalidInput(_) => "INVALID_INPUT",
            TaskerError::ConfigurationError(_) => "CONFIGURATION_ERROR",
            TaskerError::FFIError(_) => "FFI_ERROR",
            TaskerError::StateTransitionError(_) => "STATE_TRANSITION_ERROR",
        };
        Some(code.to_string())
    }

    /// Create success metadata
    fn create_success_metadata(
        &self,
        context: &StepContext,
        duration: Duration,
    ) -> std::collections::HashMap<String, Value> {
        let mut metadata = std::collections::HashMap::new();

        if self.enable_metrics {
            metadata.insert(
                "execution_time_ms".to_string(),
                Value::from(duration.as_millis() as u64),
            );
            metadata.insert(
                "attempt_number".to_string(),
                Value::from(context.attempt_number),
            );
            metadata.insert(
                "step_name".to_string(),
                Value::from(context.step_name.clone()),
            );
            metadata.insert(
                "handler_class".to_string(),
                Value::from(self.step_template.handler_class.clone()),
            );
        }

        metadata
    }

    /// Create error metadata
    fn create_error_metadata(
        &self,
        context: &StepContext,
        error: &TaskerError,
        duration: Duration,
    ) -> std::collections::HashMap<String, Value> {
        let mut metadata = self.create_success_metadata(context, duration);

        metadata.insert("error_type".to_string(), Value::from(format!("{error:?}")));
        metadata.insert(
            "is_retryable".to_string(),
            Value::from(self.is_retryable_error(error)),
        );

        metadata
    }

    /// Get the step template configuration
    pub fn step_template(&self) -> &StepTemplate {
        &self.step_template
    }

    /// Check if a custom handler is configured
    pub fn has_custom_handler(&self) -> bool {
        self.custom_handler.is_some()
    }
}

/// Factory for creating BaseStepHandler instances from handler configurations
pub struct StepHandlerFactory;

impl StepHandlerFactory {
    /// Create a step handler from a handler configuration and step name
    pub fn create_step_handler(
        handler_config: &HandlerConfiguration,
        step_name: &str,
    ) -> Result<BaseStepHandler> {
        // Find the step template for the given step name
        let step_template = handler_config
            .step_templates
            .iter()
            .find(|template| template.name == step_name)
            .ok_or_else(|| {
                TaskerError::ValidationError(format!(
                    "Step template '{step_name}' not found in handler configuration"
                ))
            })?;

        Ok(BaseStepHandler::new(step_template.clone()))
    }

    /// Create step handlers for all steps in a handler configuration
    pub fn create_all_step_handlers(
        handler_config: &HandlerConfiguration,
    ) -> Result<std::collections::HashMap<String, BaseStepHandler>> {
        let mut handlers = std::collections::HashMap::new();

        for step_template in &handler_config.step_templates {
            let handler = BaseStepHandler::new(step_template.clone());
            handlers.insert(step_template.name.clone(), handler);
        }

        Ok(handlers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::context::ExecutionMetadata;
    use async_trait::async_trait;
    use std::collections::HashMap;

    struct TestStepHandler;

    #[async_trait]
    impl RustStepHandler for TestStepHandler {
        async fn process(&self, context: &StepContext) -> Result<Value> {
            Ok(serde_json::json!({
                "processed": true,
                "input_was": context.input_data
            }))
        }
    }

    fn create_test_step_template() -> StepTemplate {
        StepTemplate {
            name: "test_step".to_string(),
            description: Some("Test step".to_string()),
            dependent_system: None,
            default_retryable: Some(true),
            default_retry_limit: Some(3),
            skippable: Some(false),
            handler_class: "TestStepHandler".to_string(),
            handler_config: None,
            depends_on_step: None,
            depends_on_steps: None,
            custom_events: None,
        }
    }

    fn create_test_context() -> StepContext {
        StepContext {
            step_id: 1,
            task_id: 100,
            step_name: "test_step".to_string(),
            input_data: serde_json::json!({"test": "data"}),
            step_config: HashMap::new(),
            previous_results: HashMap::new(),
            attempt_number: 1,
            max_attempts: 3,
            is_retryable: true,
            timeout_seconds: 30,
            environment: "test".to_string(),
            metadata: ExecutionMetadata::default(),
            tags: vec![],
            dependencies: vec![],
        }
    }

    #[tokio::test]
    async fn test_base_step_handler_default_behavior() {
        let step_template = create_test_step_template();
        let handler = BaseStepHandler::new(step_template);
        let context = create_test_context();

        let result = handler.execute_step(context.clone()).await.unwrap();

        assert!(result.success);
        assert_eq!(result.output_data.unwrap(), context.input_data);
        assert!(result.error_message.is_none());
    }

    #[tokio::test]
    async fn test_base_step_handler_with_custom_handler() {
        let step_template = create_test_step_template();
        let custom_handler = Arc::new(TestStepHandler);
        let handler = BaseStepHandler::new(step_template).with_handler(custom_handler);
        let context = create_test_context();

        let result = handler.execute_step(context).await.unwrap();

        assert!(result.success);
        let output = result.output_data.unwrap();
        assert_eq!(output["processed"], true);
        assert_eq!(output["input_was"], serde_json::json!({"test": "data"}));
    }

    #[tokio::test]
    async fn test_step_handler_factory() {
        let handler_config = HandlerConfiguration {
            name: "test_task".to_string(),
            module_namespace: None,
            task_handler_class: "TestTaskHandler".to_string(),
            namespace_name: "test".to_string(),
            version: "1.0.0".to_string(),
            default_dependent_system: None,
            named_steps: vec!["step1".to_string(), "step2".to_string()],
            schema: None,
            step_templates: vec![
                StepTemplate {
                    name: "step1".to_string(),
                    description: Some("First step".to_string()),
                    dependent_system: None,
                    default_retryable: Some(true),
                    default_retry_limit: Some(3),
                    skippable: Some(false),
                    handler_class: "Step1Handler".to_string(),
                    handler_config: None,
                    depends_on_step: None,
                    depends_on_steps: None,
                    custom_events: None,
                },
                StepTemplate {
                    name: "step2".to_string(),
                    description: Some("Second step".to_string()),
                    dependent_system: None,
                    default_retryable: Some(true),
                    default_retry_limit: Some(3),
                    skippable: Some(false),
                    handler_class: "Step2Handler".to_string(),
                    handler_config: None,
                    depends_on_step: Some("step1".to_string()),
                    depends_on_steps: None,
                    custom_events: None,
                },
            ],
            environments: None,
            default_context: None,
            default_options: None,
        };

        let handlers = StepHandlerFactory::create_all_step_handlers(&handler_config).unwrap();

        assert_eq!(handlers.len(), 2);
        assert!(handlers.contains_key("step1"));
        assert!(handlers.contains_key("step2"));

        let step1_handler =
            StepHandlerFactory::create_step_handler(&handler_config, "step1").unwrap();
        assert_eq!(step1_handler.step_template().name, "step1");
    }
}
