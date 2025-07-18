//! # Base Task Handler Implementation
//!
//! Provides the foundation for task execution in Rust applications,
//! coordinating multiple steps and handling task-level concerns.

use crate::client::context::{StepContext, TaskContext};
use crate::client::step_handler::{BaseStepHandler, StepExecutionResult};
use crate::client::traits::{RustStepHandler, RustTaskHandler};
use crate::error::{Result, TaskerError};
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::handler_config::{HandlerConfiguration, ResolvedHandlerConfiguration};
// use crate::state_machine::events::TaskEvent;
use crate::state_machine::TaskStateMachine;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};

/// Base task handler that coordinates the execution of multiple steps
///
/// This struct serves as the foundation for task execution in Rust applications.
/// It manages the overall task lifecycle, coordinates step execution, and provides
/// hooks for custom task-level logic.
pub struct BaseTaskHandler {
    /// Resolved handler configuration with environment-specific overrides
    handler_config: ResolvedHandlerConfiguration,

    /// Step handlers for each step in the task
    step_handlers: HashMap<String, BaseStepHandler>,

    /// Custom task handler implementation
    custom_handler: Option<Arc<dyn RustTaskHandler>>,

    /// State machine for task lifecycle management
    state_machine: Option<Arc<TaskStateMachine>>,

    /// Whether to enable detailed performance metrics
    enable_metrics: bool,

    /// Configuration manager for system-level configuration
    config_manager: Arc<ConfigurationManager>,
}

/// Result of task execution
#[derive(Debug, Clone)]
pub struct TaskExecutionResult {
    /// Whether the task completed successfully
    pub success: bool,

    /// Results from each step execution
    pub step_results: HashMap<String, StepExecutionResult>,

    /// Final output data from the task
    pub output_data: Option<Value>,

    /// Error message if the task failed
    pub error_message: Option<String>,

    /// Error code for categorization
    pub error_code: Option<String>,

    /// Total execution duration
    pub duration: Duration,

    /// Number of steps executed
    pub steps_executed: usize,

    /// Whether the task should be retried (only relevant for failures)
    pub should_retry: bool,

    /// Task completion timestamp
    pub completed_at: DateTime<Utc>,

    /// Additional metadata about the execution
    pub metadata: HashMap<String, Value>,
}

impl BaseTaskHandler {
    /// Create a new base task handler from a resolved handler configuration
    pub fn new(handler_config: ResolvedHandlerConfiguration) -> Result<Self> {
        let config_manager = Arc::new(ConfigurationManager::new());
        Self::with_config_manager(handler_config, config_manager)
    }

    /// Create a new base task handler with a custom configuration manager
    pub fn with_config_manager(
        handler_config: ResolvedHandlerConfiguration,
        config_manager: Arc<ConfigurationManager>,
    ) -> Result<Self> {
        // Create step handlers for all steps in the configuration
        let mut step_handlers = HashMap::new();

        for step_template in &handler_config.resolved_step_templates {
            let step_handler = BaseStepHandler::new(step_template.clone());
            step_handlers.insert(step_template.name.clone(), step_handler);
        }

        Ok(Self {
            handler_config,
            step_handlers,
            custom_handler: None,
            state_machine: None,
            enable_metrics: true,
            config_manager,
        })
    }

    /// Create a new base task handler from a handler configuration and environment
    pub fn from_config(handler_config: HandlerConfiguration, environment: &str) -> Result<Self> {
        let resolved_config = handler_config
            .resolve_for_environment(environment)
            .map_err(|e| {
                TaskerError::ConfigurationError(format!(
                    "Failed to resolve handler configuration for environment '{environment}': {e}"
                ))
            })?;

        Self::new(resolved_config)
    }

    /// Set a custom task handler implementation
    pub fn with_handler(mut self, handler: Arc<dyn RustTaskHandler>) -> Self {
        self.custom_handler = Some(handler);
        self
    }

    /// Set custom step handlers for specific steps
    pub fn with_step_handler(
        mut self,
        step_name: String,
        handler: Arc<dyn RustStepHandler>,
    ) -> Result<Self> {
        if let Some(base_handler) = self.step_handlers.get_mut(&step_name) {
            *base_handler =
                BaseStepHandler::new(base_handler.step_template().clone()).with_handler(handler);
            Ok(self)
        } else {
            Err(TaskerError::ValidationError(format!(
                "Step '{step_name}' not found in task configuration"
            )))
        }
    }

    /// Set the state machine for task lifecycle management
    pub fn with_state_machine(mut self, state_machine: Arc<TaskStateMachine>) -> Self {
        self.state_machine = Some(state_machine);
        self
    }

    /// Enable or disable performance metrics collection
    pub fn with_metrics(mut self, enable: bool) -> Self {
        self.enable_metrics = enable;
        self
    }

    /// Execute the complete task workflow
    #[instrument(skip(self, context), fields(task_id = context.task_id, task_name = %context.task_name))]
    pub async fn execute_task(&self, context: TaskContext) -> Result<TaskExecutionResult> {
        let start_time = Instant::now();

        info!(
            task_id = context.task_id,
            task_name = %context.task_name,
            attempt = context.attempt_number,
            step_count = self.step_handlers.len(),
            "Starting task execution"
        );

        // Initialize custom handler if available
        if let Some(ref handler) = self.custom_handler {
            if let Err(init_error) = handler.initialize(&context).await {
                error!(
                    task_id = context.task_id,
                    error = %init_error,
                    "Task handler initialization failed"
                );
                return Err(init_error);
            }
        }

        // TODO: Transition to in_progress state when state machine integration is complete
        // if let Some(ref state_machine) = self.state_machine {
        //     self.transition_task_state(&context, "in_progress", state_machine).await?;
        // }

        // Execute steps in dependency order
        let execution_result = self.execute_steps_in_order(&context).await;
        let duration = start_time.elapsed();
        let completed_at = Utc::now();

        // Process the result and determine final state
        let task_result = match execution_result {
            Ok((step_results, final_output)) => {
                let steps_executed = step_results.len();

                info!(
                    task_id = context.task_id,
                    steps_executed = steps_executed,
                    duration_ms = duration.as_millis(),
                    "Task execution completed successfully"
                );

                // TODO: Transition to completed state when state machine integration is complete
                // if let Some(ref state_machine) = self.state_machine {
                //     self.transition_task_state(&context, "completed", state_machine).await?;
                // }

                TaskExecutionResult {
                    success: true,
                    step_results,
                    output_data: Some(final_output),
                    error_message: None,
                    error_code: None,
                    duration,
                    steps_executed,
                    should_retry: false,
                    completed_at,
                    metadata: self.create_success_metadata(&context, duration, steps_executed),
                }
            }
            Err((partial_results, error)) => {
                let steps_executed = partial_results.len();
                let should_retry = context.can_retry() && self.is_retryable_error(&error);

                error!(
                    task_id = context.task_id,
                    error = %error,
                    steps_executed = steps_executed,
                    should_retry = should_retry,
                    duration_ms = duration.as_millis(),
                    "Task execution failed"
                );

                // TODO: Transition to appropriate error state when state machine integration is complete
                // if let Some(ref state_machine) = self.state_machine {
                //     let target_state = if should_retry { "failed_retryable" } else { "failed" };
                //     self.transition_task_state(&context, target_state, state_machine).await?;
                // }

                TaskExecutionResult {
                    success: false,
                    step_results: partial_results,
                    output_data: None,
                    error_message: Some(error.to_string()),
                    error_code: self.classify_error(&error),
                    duration,
                    steps_executed,
                    should_retry,
                    completed_at,
                    metadata: self.create_error_metadata(
                        &context,
                        &error,
                        duration,
                        steps_executed,
                    ),
                }
            }
        };

        // Finalize custom handler if available
        if let Some(ref handler) = self.custom_handler {
            if let Err(finalize_error) = handler.finalize(&context, task_result.success).await {
                warn!(
                    task_id = context.task_id,
                    error = %finalize_error,
                    "Task handler finalization failed"
                );
            }
        }

        Ok(task_result)
    }

    /// Execute steps in dependency order
    async fn execute_steps_in_order(
        &self,
        context: &TaskContext,
    ) -> std::result::Result<
        (HashMap<String, StepExecutionResult>, Value),
        (HashMap<String, StepExecutionResult>, TaskerError),
    > {
        let mut step_results = HashMap::new();
        let mut accumulated_results = HashMap::new();
        let execution_order = self.get_step_execution_order();

        for step_name in execution_order {
            // Create step context from task context
            let step_context =
                match self.create_step_context(context, &step_name, &accumulated_results) {
                    Ok(ctx) => ctx,
                    Err(e) => return Err((step_results, e)),
                };

            // Get the step handler
            let Some(step_handler) = self.step_handlers.get(&step_name) else {
                let error = TaskerError::ValidationError(format!(
                    "Step handler not found for step '{step_name}'"
                ));
                return Err((step_results, error));
            };

            // Execute the step
            debug!(
                task_id = context.task_id,
                step_name = %step_name,
                "Executing step"
            );

            match step_handler.execute_step(step_context).await {
                Ok(step_result) => {
                    // Store the result
                    if let Some(ref output) = step_result.output_data {
                        accumulated_results.insert(step_name.clone(), output.clone());
                    }
                    step_results.insert(step_name.clone(), step_result);

                    debug!(
                        task_id = context.task_id,
                        step_name = %step_name,
                        "Step completed successfully"
                    );
                }
                Err(step_error) => {
                    error!(
                        task_id = context.task_id,
                        step_name = %step_name,
                        error = %step_error,
                        "Step execution failed"
                    );

                    let step_result = StepExecutionResult {
                        success: false,
                        output_data: None,
                        error_message: Some(step_error.to_string()),
                        error_code: Some("STEP_EXECUTION_FAILED".to_string()),
                        duration: Duration::from_secs(0),
                        should_retry: false,
                        metadata: HashMap::new(),
                    };
                    step_results.insert(step_name, step_result);

                    return Err((step_results, step_error));
                }
            }
        }

        // Create final output from all step results
        let final_output = serde_json::json!({
            "task_completed": true,
            "step_results": accumulated_results,
            "task_context": {
                "task_id": context.task_id,
                "task_name": &context.task_name,
                "namespace": &context.namespace
            }
        });

        Ok((step_results, final_output))
    }

    /// Get the execution order for steps based on dependencies
    fn get_step_execution_order(&self) -> Vec<String> {
        // For now, use the order from the resolved configuration
        // TODO: Implement proper dependency resolution algorithm
        self.handler_config
            .resolved_step_templates
            .iter()
            .map(|template| template.name.clone())
            .collect()
    }

    /// Create a step context from the task context
    fn create_step_context(
        &self,
        task_context: &TaskContext,
        step_name: &str,
        previous_results: &HashMap<String, Value>,
    ) -> std::result::Result<StepContext, TaskerError> {
        // Find the step template
        let step_template = self
            .handler_config
            .resolved_step_templates
            .iter()
            .find(|template| template.name == step_name)
            .ok_or_else(|| {
                TaskerError::ValidationError(format!("Step template '{step_name}' not found"))
            })?;

        // Create step configuration from template and task config
        let mut step_config = HashMap::new();
        if let Some(ref handler_config) = step_template.handler_config {
            if let Some(config_map) = handler_config.as_object() {
                for (key, value) in config_map {
                    step_config.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(StepContext {
            step_id: 0, // Will be set by the orchestration system
            task_id: task_context.task_id,
            step_name: step_name.to_string(),
            input_data: task_context.input_data.clone(),
            step_config,
            previous_results: previous_results.clone(),
            attempt_number: 1, // Individual step attempts
            max_attempts: step_template.default_retry_limit.unwrap_or(3) as u32,
            is_retryable: step_template.default_retryable.unwrap_or(true),
            timeout_seconds: self
                .config_manager
                .system_config()
                .execution
                .step_execution_timeout_seconds,
            environment: task_context.environment.clone(),
            metadata: task_context.metadata.clone(),
            tags: task_context.tags.clone(),
            dependencies: step_template.all_dependencies(),
        })
    }

    /// Transition task state through the state machine
    /// TODO: Complete implementation once state machine API is finalized
    #[allow(dead_code)]
    async fn transition_task_state(
        &self,
        context: &TaskContext,
        target_state: &str,
        _state_machine: &TaskStateMachine,
    ) -> Result<()> {
        // let event = match target_state {
        //     "in_progress" => TaskEvent::Start,
        //     "completed" => TaskEvent::Complete,
        //     "failed" | "failed_retryable" => TaskEvent::Fail(
        //         format!("Task {} failed during execution", context.task_name)
        //     ),
        //     _ => {
        //         warn!("Unknown target state: {}, using Start event", target_state);
        //         TaskEvent::Start
        //     }
        // };

        // state_machine.handle_event(context.task_id, event).await
        //     .map_err(|e| TaskerError::StateTransitionError(format!(
        //         "Failed to transition task {} to state {}: {}",
        //         context.task_id, target_state, e
        //     )))
        debug!(
            task_id = context.task_id,
            target_state = target_state,
            "State transition placeholder - implementation pending"
        );
        Ok(())
    }

    /// Check if an error is retryable at the task level
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
            TaskerError::DatabaseError(_) => "TASK_DATABASE_ERROR",
            TaskerError::OrchestrationError(_) => "TASK_ORCHESTRATION_ERROR",
            TaskerError::EventError(_) => "TASK_EVENT_ERROR",
            TaskerError::ValidationError(_) => "TASK_VALIDATION_ERROR",
            TaskerError::InvalidInput(_) => "TASK_INVALID_INPUT",
            TaskerError::ConfigurationError(_) => "TASK_CONFIGURATION_ERROR",
            TaskerError::FFIError(_) => "TASK_FFI_ERROR",
            TaskerError::StateTransitionError(_) => "TASK_STATE_TRANSITION_ERROR",
        };
        Some(code.to_string())
    }

    /// Create success metadata
    fn create_success_metadata(
        &self,
        context: &TaskContext,
        duration: Duration,
        steps_executed: usize,
    ) -> HashMap<String, Value> {
        let mut metadata = HashMap::new();

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
                "task_name".to_string(),
                Value::from(context.task_name.clone()),
            );
            metadata.insert(
                "namespace".to_string(),
                Value::from(context.namespace.clone()),
            );
            metadata.insert("steps_executed".to_string(), Value::from(steps_executed));
            metadata.insert(
                "handler_class".to_string(),
                Value::from(self.handler_config.base_config.task_handler_class.clone()),
            );
        }

        metadata
    }

    /// Create error metadata
    fn create_error_metadata(
        &self,
        context: &TaskContext,
        error: &TaskerError,
        duration: Duration,
        steps_executed: usize,
    ) -> HashMap<String, Value> {
        let mut metadata = self.create_success_metadata(context, duration, steps_executed);

        metadata.insert("error_type".to_string(), Value::from(format!("{error:?}")));
        metadata.insert(
            "is_retryable".to_string(),
            Value::from(self.is_retryable_error(error)),
        );

        metadata
    }

    /// Get the handler configuration
    pub fn handler_config(&self) -> &ResolvedHandlerConfiguration {
        &self.handler_config
    }

    /// Get the step handlers
    pub fn step_handlers(&self) -> &HashMap<String, BaseStepHandler> {
        &self.step_handlers
    }

    /// Check if a custom handler is configured
    pub fn has_custom_handler(&self) -> bool {
        self.custom_handler.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::context::ExecutionMetadata;
    use crate::orchestration::handler_config::{HandlerConfiguration, StepTemplate};

    fn create_test_handler_config() -> HandlerConfiguration {
        HandlerConfiguration {
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
        }
    }

    fn create_test_task_context() -> TaskContext {
        TaskContext {
            task_id: 100,
            task_name: "test_task".to_string(),
            namespace: "test".to_string(),
            version: "1.0.0".to_string(),
            input_data: serde_json::json!({"test": "data"}),
            task_config: HashMap::new(),
            environment: "test".to_string(),
            metadata: ExecutionMetadata::default(),
            tags: vec![],
            status: "pending".to_string(),
            is_retryable: true,
            max_attempts: 3,
            attempt_number: 1,
            step_names: vec!["step1".to_string(), "step2".to_string()],
        }
    }

    #[tokio::test]
    async fn test_base_task_handler_creation() {
        let handler_config = create_test_handler_config();
        let task_handler = BaseTaskHandler::from_config(handler_config, "test").unwrap();

        assert_eq!(task_handler.step_handlers().len(), 2);
        assert!(task_handler.step_handlers().contains_key("step1"));
        assert!(task_handler.step_handlers().contains_key("step2"));
    }

    #[tokio::test]
    async fn test_task_execution_default_behavior() {
        let handler_config = create_test_handler_config();
        let task_handler = BaseTaskHandler::from_config(handler_config, "test").unwrap();
        let context = create_test_task_context();

        let result = task_handler.execute_task(context).await.unwrap();

        assert!(result.success);
        assert_eq!(result.steps_executed, 2);
        assert!(result.output_data.is_some());
        assert!(result.error_message.is_none());
    }
}
