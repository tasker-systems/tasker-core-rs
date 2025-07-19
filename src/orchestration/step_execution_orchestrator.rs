//! # Step Execution Orchestrator
//!
//! ## Purpose
//!
//! The `StepExecutionOrchestrator` provides shared orchestration logic for all step handlers,
//! enabling consistent execution patterns across different handler implementations (Ruby, Python, etc.).
//!
//! ## Architecture
//!
//! This follows the composition pattern, allowing step handlers to "have-a" orchestrator
//! rather than inheriting from a base class. This provides flexibility while maintaining
//! consistency in execution behavior.
//!
//! ## Key Responsibilities
//!
//! - State machine transitions during step lifecycle
//! - Event publishing for step execution phases
//! - Timeout management and enforcement
//! - Retry logic with backoff calculation
//! - Error handling and result processing
//! - Metrics collection and monitoring

use crate::events::publisher::EventPublisher;
use crate::orchestration::backoff_calculator::{BackoffCalculator, BackoffContext};
use crate::orchestration::config::ConfigurationManager;
use crate::orchestration::errors::{OrchestrationError, OrchestrationResult, StepExecutionError};
use crate::orchestration::step_handler::{StepExecutionContext, StepHandler, StepResult};
use crate::orchestration::system_events::SystemEventsManager;
use crate::state_machine::events::StepEvent;
use crate::state_machine::StepStateMachine;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// Orchestrates step execution with consistent behavior across all handler types
pub struct StepExecutionOrchestrator {
    /// Configuration manager for accessing system config
    config_manager: Arc<ConfigurationManager>,

    /// Backoff calculator for retry logic
    backoff_calculator: Option<BackoffCalculator>,

    /// State machine for step lifecycle management
    state_machine: Option<Arc<StepStateMachine>>,

    /// System events manager for event metadata and validation
    events_manager: Option<Arc<SystemEventsManager>>,

    /// Event publisher for actual event publishing
    event_publisher: Option<EventPublisher>,
}

impl StepExecutionOrchestrator {
    /// Create a new orchestrator with minimal configuration
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self {
            config_manager,
            backoff_calculator: None,
            state_machine: None,
            events_manager: None,
            event_publisher: None,
        }
    }

    /// Set the state machine for this orchestrator
    pub fn with_state_machine(mut self, state_machine: Arc<StepStateMachine>) -> Self {
        self.state_machine = Some(state_machine);
        self
    }

    /// Set backoff calculator for retry logic
    pub fn with_backoff_calculator(mut self, backoff_calculator: BackoffCalculator) -> Self {
        self.backoff_calculator = Some(backoff_calculator);
        self
    }

    /// Set system events manager for event publishing
    pub fn with_events_manager(mut self, events_manager: Arc<SystemEventsManager>) -> Self {
        self.events_manager = Some(events_manager);
        self
    }

    /// Set system events manager for event publishing (mutating version)
    pub fn set_events_manager(&mut self, events_manager: Arc<SystemEventsManager>) {
        self.events_manager = Some(events_manager);
    }

    /// Set event publisher for actual event publishing
    pub fn with_event_publisher(mut self, event_publisher: EventPublisher) -> Self {
        self.event_publisher = Some(event_publisher);
        self
    }

    /// Set event publisher for actual event publishing (mutating version)
    pub fn set_event_publisher(&mut self, event_publisher: EventPublisher) {
        self.event_publisher = Some(event_publisher);
    }

    /// Execute a step with full lifecycle management
    #[instrument(skip(self, handler, context))]
    pub async fn execute_step(
        &self,
        handler: &dyn StepHandler,
        mut context: StepExecutionContext,
    ) -> OrchestrationResult<StepResult> {
        let start_time = SystemTime::now();
        info!(
            "Starting orchestrated step execution: {} (step_id: {})",
            context.step_name, context.step_id
        );

        // Validate step configuration
        handler.validate_config(&context.step_config)?;

        // Apply configuration defaults
        self.apply_configuration_defaults(&mut context);

        // Publish before_handle event
        self.publish_before_handle_event(&context).await;

        // Transition to executing state
        if let Err(e) = self.transition_to_executing(&context).await {
            warn!("Failed to transition step to executing state: {}", e);
        }

        // Execute the step with timeout
        let execution_result = self.execute_with_timeout(handler, &context).await;

        // Calculate execution duration
        let execution_duration = start_time.elapsed().unwrap_or(Duration::from_secs(0));

        // Process execution result
        let result = self
            .process_execution_result(execution_result, execution_duration, &context)
            .await;

        // Call process_results hook if available
        if let Err(e) = handler.process_results(&context, &result).await {
            warn!("process_results hook failed: {}", e);
            // Don't fail the step for hook errors, just log them
        }

        // Update state based on result
        self.update_step_state(&result).await;

        // Publish completion/failure events
        self.publish_completion_events(&context, &result, execution_duration)
            .await;

        info!(
            "Step execution completed: {} (success: {}, duration: {:?})",
            context.step_name, result.success, execution_duration
        );

        Ok(result)
    }

    /// Apply configuration defaults to the execution context
    fn apply_configuration_defaults(&self, context: &mut StepExecutionContext) {
        let system_config = self.config_manager.system_config();

        // Apply timeout defaults if not set
        if context.timeout_seconds == 0 {
            context.timeout_seconds = system_config.execution.step_execution_timeout_seconds;
        }

        // Apply retry defaults if not set
        if context.max_retry_attempts == 0 {
            // Use a reasonable default for now, could be moved to config later
            context.max_retry_attempts = 3;
        }
    }

    /// Publish before_handle event
    async fn publish_before_handle_event(&self, context: &StepExecutionContext) {
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

            // Validate the payload first
            if let Err(e) = events_manager.config().validate_event_payload(
                "step",
                "before_handle",
                &before_handle_payload,
            ) {
                warn!("Event payload validation failed: {}", e);
                return;
            }

            // Publish the event if we have a publisher
            if let Some(ref publisher) = self.event_publisher {
                if let Err(e) = publisher
                    .publish("step.before_handle", before_handle_payload)
                    .await
                {
                    warn!("Failed to publish before_handle event: {}", e);
                }
            }
        }
    }

    /// Transition step to executing state
    async fn transition_to_executing(
        &self,
        _context: &StepExecutionContext,
    ) -> OrchestrationResult<()> {
        if let Some(state_machine) = &self.state_machine {
            let mut state_machine = state_machine.as_ref().clone();
            state_machine.transition(StepEvent::Start).await?;
        }
        Ok(())
    }

    /// Execute step with timeout protection
    async fn execute_with_timeout(
        &self,
        handler: &dyn StepHandler,
        context: &StepExecutionContext,
    ) -> OrchestrationResult<serde_json::Value> {
        let timeout_duration = handler
            .get_timeout_override()
            .unwrap_or_else(|| Duration::from_secs(context.timeout_seconds));

        debug!("Executing step with timeout: {:?}", timeout_duration);

        match timeout(timeout_duration, handler.process(context)).await {
            Ok(result) => result,
            Err(_) => {
                error!("Step execution timed out after {:?}", timeout_duration);
                Err(OrchestrationError::TimeoutError {
                    operation: format!("step_execution_{}", context.step_id),
                    timeout_duration,
                })
            }
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

                // Clear backoff for successful step completion
                if let Some(backoff_calculator) = &self.backoff_calculator {
                    if let Err(e) = backoff_calculator.clear_backoff(context.step_id).await {
                        warn!(
                            "Failed to clear backoff for successful step {}: {}",
                            context.step_id, e
                        );
                    }
                }

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
                    self.calculate_retry_delay_with_backoff(
                        context.step_id,
                        context.attempt_number,
                        Some(error.to_string()),
                    )
                    .await
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

    /// Update state machine based on execution result
    async fn update_step_state(&self, result: &StepResult) {
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
    }

    /// Publish step completion/failure events
    async fn publish_completion_events(
        &self,
        context: &StepExecutionContext,
        result: &StepResult,
        execution_duration: Duration,
    ) {
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

                // Validate the payload first
                if let Err(e) = events_manager.config().validate_event_payload(
                    "step",
                    "completed",
                    &completed_payload,
                ) {
                    warn!("Step completed event validation failed: {}", e);
                    return;
                }

                // Publish the event if we have a publisher
                if let Some(ref publisher) = self.event_publisher {
                    if let Err(e) = publisher.publish("step.completed", completed_payload).await {
                        warn!("Failed to publish step completed event: {}", e);
                    }
                }
            } else if let Some(error) = &result.error {
                let failed_payload = events_manager.create_step_failed_payload(
                    context.task_id,
                    context.step_id,
                    &context.step_name,
                    &error.to_string(),
                    error.error_class(),
                    context.attempt_number,
                );

                debug!("Publishing step failed event for: {}", context.step_name);

                // Validate the payload first
                if let Err(e) = events_manager.config().validate_event_payload(
                    "step",
                    "failed",
                    &failed_payload,
                ) {
                    warn!("Step failed event validation failed: {}", e);
                    return;
                }

                // Publish the event if we have a publisher
                if let Some(ref publisher) = self.event_publisher {
                    if let Err(e) = publisher.publish("step.failed", failed_payload).await {
                        warn!("Failed to publish step failed event: {}", e);
                    }
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

    /// Calculate retry delay using BackoffCalculator if available
    async fn calculate_retry_delay_with_backoff(
        &self,
        step_id: i64,
        attempt_number: u32,
        error_message: Option<String>,
    ) -> Option<u64> {
        if let Some(backoff_calculator) = &self.backoff_calculator {
            let context = BackoffContext::new()
                .with_error(error_message.unwrap_or_else(|| "Step execution failed".to_string()))
                .with_metadata("attempt".to_string(), serde_json::json!(attempt_number))
                .with_metadata("step_id".to_string(), serde_json::json!(step_id));

            match backoff_calculator
                .calculate_and_apply_backoff(step_id, context)
                .await
            {
                Ok(result) => Some(result.delay_seconds as u64),
                Err(e) => {
                    warn!("Failed to calculate backoff for step {}: {}", step_id, e);
                    // Fallback to simple exponential backoff
                    self.calculate_simple_retry_delay(attempt_number)
                }
            }
        } else {
            // No BackoffCalculator available, use simple backoff
            self.calculate_simple_retry_delay(attempt_number)
        }
    }

    /// Simple retry delay calculation as fallback
    fn calculate_simple_retry_delay(&self, attempt_number: u32) -> Option<u64> {
        let system_config = self.config_manager.system_config();
        let backoff_config = &system_config.backoff;

        // Use configured backoff times if available
        if let Some(delay) = backoff_config
            .default_backoff_seconds
            .get(attempt_number as usize)
        {
            Some(*delay as u64)
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
            Some(delay.min(backoff_config.max_backoff_seconds as f64) as u64)
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
}

/// Builder pattern for creating orchestrators with fluent API
pub struct StepExecutionOrchestratorBuilder {
    config_manager: Arc<ConfigurationManager>,
    backoff_calculator: Option<BackoffCalculator>,
    state_machine: Option<Arc<StepStateMachine>>,
    events_manager: Option<Arc<SystemEventsManager>>,
}

impl StepExecutionOrchestratorBuilder {
    /// Create a new builder
    pub fn new(config_manager: Arc<ConfigurationManager>) -> Self {
        Self {
            config_manager,
            backoff_calculator: None,
            state_machine: None,
            events_manager: None,
        }
    }

    /// Set the state machine
    pub fn with_state_machine(mut self, state_machine: Arc<StepStateMachine>) -> Self {
        self.state_machine = Some(state_machine);
        self
    }

    /// Set the backoff calculator
    pub fn with_backoff_calculator(mut self, backoff_calculator: BackoffCalculator) -> Self {
        self.backoff_calculator = Some(backoff_calculator);
        self
    }

    /// Set the events manager
    pub fn with_events_manager(mut self, events_manager: Arc<SystemEventsManager>) -> Self {
        self.events_manager = Some(events_manager);
        self
    }

    /// Build the orchestrator
    pub fn build(self) -> StepExecutionOrchestrator {
        StepExecutionOrchestrator {
            config_manager: self.config_manager,
            backoff_calculator: self.backoff_calculator,
            state_machine: self.state_machine,
            events_manager: self.events_manager,
            event_publisher: None, // Can be set later using set_event_publisher
        }
    }
}
