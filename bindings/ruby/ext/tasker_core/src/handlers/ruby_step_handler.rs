//! # Ruby Step Handler Integration
//!
//! This module provides the `RubyStepHandler` struct that implements the Rust `StepHandler` trait,
//! allowing Ruby step handlers to be seamlessly integrated into the orchestration system.
//!
//! ## Architecture
//!
//! The `RubyStepHandler` acts as a bridge between the Rust orchestration core and Ruby business logic:
//! - Implements the `StepHandler` trait for orchestration integration
//! - Stores Ruby handler class name and configuration
//! - Converts data between Rust and Ruby formats
//! - Calls Ruby methods through Magnus FFI
//!
//! ## Usage
//!
//! Ruby handlers are registered with the orchestration system and can be discovered by the
//! StepExecutor during workflow execution.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::models::{RubyTask, RubyStep, RubyStepSequence};
use crate::globals::get_global_database_pool;
use magnus::{Ruby, Value};
use magnus::value::ReprValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tasker_core::orchestration::config::ConfigurationManager;
use tasker_core::orchestration::step_execution_orchestrator::StepExecutionOrchestrator;
use tasker_core::orchestration::step_handler::{StepHandler, StepHandlerExecutor, StepExecutionContext, StepResult};
use tasker_core::orchestration::errors::OrchestrationResult;
use tasker_core::models::core::workflow_step::WorkflowStep;
use tasker_core::models::core::task::Task;
use tracing::{debug, warn};

/// Ruby step handler that implements the Rust StepHandler trait
///
/// This struct bridges Ruby step handlers with the Rust orchestration system,
/// allowing Ruby business logic to be executed within the orchestration workflow.
#[derive(Debug, Clone)]
pub struct RubyStepHandler {
    /// Ruby handler class name (e.g., "PaymentProcessingStepHandler")
    handler_class: String,

    /// Step handler configuration
    config: HashMap<String, serde_json::Value>,

    /// Step template name for identification
    step_name: String,

    /// Orchestrator for shared execution logic
    orchestrator: Option<StepExecutionOrchestrator>,
}

impl RubyStepHandler {
    /// Create a new Ruby step handler
    pub fn new(handler_class: String, step_name: String, config: HashMap<String, serde_json::Value>) -> Self {
        Self {
            handler_class,
            config,
            step_name,
            orchestrator: None,
        }
    }

    /// Create a new Ruby step handler with orchestrator
    pub fn with_orchestrator(
        handler_class: String,
        step_name: String,
        config: HashMap<String, serde_json::Value>,
        orchestrator: StepExecutionOrchestrator,
    ) -> Self {
        Self {
            handler_class,
            config,
            step_name,
            orchestrator: Some(orchestrator),
        }
    }

    /// Set up the orchestrator for this handler
    pub fn setup_orchestrator(&mut self, config_manager: Arc<ConfigurationManager>) {
        self.orchestrator = Some(StepExecutionOrchestrator::new(config_manager));
    }

    /// Get a reference to the orchestrator
    pub fn orchestrator(&self) -> Option<&StepExecutionOrchestrator> {
        self.orchestrator.as_ref()
    }

    /// Get a mutable reference to the orchestrator
    pub fn orchestrator_mut(&mut self) -> Option<&mut StepExecutionOrchestrator> {
        self.orchestrator.as_mut()
    }

    /// Get the handler class name
    pub fn handler_class(&self) -> &str {
        &self.handler_class
    }

    /// Get the step name
    pub fn step_name(&self) -> &str {
        &self.step_name
    }

    /// Convert StepExecutionContext to Ruby-compatible data
    async fn convert_context_to_ruby(&self, context: &StepExecutionContext) -> OrchestrationResult<(RubyTask, RubyStepSequence, RubyStep)> {
        let pool = get_global_database_pool();

        // Load Task from database
        let task = Task::find_by_id(&pool, context.task_id).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_task".to_string(),
                reason: format!("Failed to load task {}: {}", context.task_id, e),
            })?
            .ok_or_else(|| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_task".to_string(),
                reason: format!("Task {} not found", context.task_id),
            })?;

        // Load WorkflowStep from database
        let workflow_step = WorkflowStep::find_by_id(&pool, context.step_id).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_workflow_step".to_string(),
                reason: format!("Failed to load workflow step {}: {}", context.step_id, e),
            })?
            .ok_or_else(|| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_workflow_step".to_string(),
                reason: format!("WorkflowStep {} not found", context.step_id),
            })?;

        // Get step dependencies
        let dependencies = workflow_step.get_dependencies(&pool).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "get_dependencies".to_string(),
                reason: format!("Failed to get dependencies for step {}: {}", context.step_id, e),
            })?;

        // Convert to Ruby objects
        let ruby_task = RubyTask::from_task(&task);
        let ruby_step = RubyStep::from_workflow_step(&workflow_step);

        // Convert dependencies to RubyStep objects
        let ruby_dependencies: Vec<RubyStep> = dependencies
            .iter()
            .map(|dep| RubyStep::from_workflow_step(dep))
            .collect();

        let ruby_sequence = RubyStepSequence::new(
            dependencies.len(),
            0, // current_position - could be calculated from step state
            ruby_dependencies,
            context.step_id,
        );

        Ok((ruby_task, ruby_sequence, ruby_step))
    }

    /// Call Ruby process method
    async fn call_ruby_process(&self, ruby_task: &RubyTask, ruby_sequence: &RubyStepSequence, ruby_step: &RubyStep) -> OrchestrationResult<serde_json::Value> {
        let ruby = Ruby::get().map_err(|e| {
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "get_ruby".to_string(),
                reason: format!("Failed to get Ruby interpreter: {}", e),
            }
        })?;

        // Get the handler class
        let handler_class: Value = ruby.eval(&format!("{}::new", self.handler_class))
            .map_err(|e| {
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "instantiate_handler".to_string(),
                    reason: format!("Failed to instantiate Ruby handler {}: {}", self.handler_class, e),
                }
            })?;

        // Convert Ruby objects to Values for method call
        // Note: These types implement IntoValue automatically via #[magnus::wrap]
        // but we need to clone them since we have references and IntoValue requires owned values
        let task_clone = ruby_task.clone();
        let sequence_clone = ruby_sequence.clone();
        let step_clone = ruby_step.clone();

        // Call the process method
        let result: Value = handler_class.funcall("process", (task_clone, sequence_clone, step_clone))
            .map_err(|e| {
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "call_process".to_string(),
                    reason: format!("Failed to call process method on {}: {}", self.handler_class, e),
                }
            })?;

        // Convert result back to JSON
        ruby_value_to_json(result).map_err(|e| {
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "convert_result".to_string(),
                reason: format!("Failed to convert Ruby result to JSON: {}", e),
            }
        })
    }

    /// Call Ruby process_results method (optional)
    async fn call_ruby_process_results(&self, ruby_step: &RubyStep, process_output: &serde_json::Value, initial_results: Option<&serde_json::Value>) -> OrchestrationResult<()> {
        let ruby = Ruby::get().map_err(|e| {
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "get_ruby".to_string(),
                reason: format!("Failed to get Ruby interpreter: {}", e),
            }
        })?;

        // Get the handler class
        let handler_class: Value = ruby.eval(&format!("{}::new", self.handler_class))
            .map_err(|e| {
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "instantiate_handler".to_string(),
                    reason: format!("Failed to instantiate Ruby handler {}: {}", self.handler_class, e),
                }
            })?;

        // Convert arguments to Ruby values
        let step_value = ruby_step.clone();
        let process_output_value = json_to_ruby_value(process_output.clone()).map_err(|e| {
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "convert_process_output".to_string(),
                reason: format!("Failed to convert process output to Ruby: {}", e),
            }
        })?;
        let initial_results_value = if let Some(results) = initial_results {
            json_to_ruby_value(results.clone()).map_err(|e| {
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "convert_initial_results".to_string(),
                    reason: format!("Failed to convert initial results to Ruby: {}", e),
                }
            })?
        } else {
            ruby.qnil().as_value()
        };

        // Call the process_results method (if it exists)
        let method_exists: bool = handler_class.funcall("respond_to?", ("process_results",))
            .unwrap_or(false);

        if method_exists {
            let _result: Value = handler_class.funcall("process_results", (step_value, process_output_value, initial_results_value))
                .map_err(|e| {
                    // Don't fail the step for process_results errors, just log them
                    warn!("process_results method failed on {}: {}", self.handler_class, e);
                    e
                }).unwrap_or_else(|_| ruby.qnil().as_value());
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl StepHandler for RubyStepHandler {
    /// Process the step by calling the Ruby handler
    async fn process(&self, context: &StepExecutionContext) -> OrchestrationResult<serde_json::Value> {
        debug!(
            "Processing step {} with Ruby handler {}",
            context.step_name,
            self.handler_class
        );

        // Convert context to Ruby objects
        let (ruby_task, ruby_sequence, ruby_step) = self.convert_context_to_ruby(context).await?;

        // Call Ruby process method
        let result = self.call_ruby_process(&ruby_task, &ruby_sequence, &ruby_step).await?;

        debug!(
            "Ruby handler {} completed processing step {}",
            self.handler_class,
            context.step_name
        );

        Ok(result)
    }

    /// Process results after step completion
    async fn process_results(&self, context: &StepExecutionContext, result: &StepResult) -> OrchestrationResult<()> {
        debug!(
            "Processing results for step {} with Ruby handler {}",
            context.step_name,
            self.handler_class
        );

        // Convert context to Ruby objects
        let (_, _, ruby_step) = self.convert_context_to_ruby(context).await?;

        // Call Ruby process_results method if it exists
        self.call_ruby_process_results(&ruby_step, result.output_data.as_ref().unwrap_or(&serde_json::json!({})), None).await?;

        Ok(())
    }

    /// Validate step configuration
    fn validate_config(&self, config: &HashMap<String, serde_json::Value>) -> OrchestrationResult<()> {
        // For now, accept all configurations
        // In the future, we could call a Ruby validation method
        let _ = config;
        Ok(())
    }

    /// Get step timeout override
    fn get_timeout_override(&self) -> Option<Duration> {
        // Check if timeout is specified in config
        self.config.get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .map(Duration::from_secs)
    }
}

// Implement StepHandlerExecutor trait for RubyStepHandler
#[async_trait::async_trait]
impl StepHandlerExecutor for RubyStepHandler {
    async fn execute_step(
        &self,
        context: StepExecutionContext,
    ) -> OrchestrationResult<StepResult> {
        if let Some(orchestrator) = &self.orchestrator {
            // Use the orchestrator for full lifecycle management
            orchestrator.execute_step(self, context).await
        } else {
            // Fallback to basic execution without orchestration
            debug!(
                "No orchestrator available for Ruby handler {}, using basic execution",
                self.handler_class
            );
            
            let output_data = self.process(&context).await?;
            
            let result = StepResult {
                success: true,
                output_data: Some(output_data),
                error: None,
                should_retry: false,
                retry_delay: None,
                execution_duration: Duration::from_millis(0), // TODO: Track actual duration
                metadata: HashMap::new(),
                events_to_publish: vec![],
            };
            
            // Call process_results hook
            self.process_results(&context, &result).await?;
            
            Ok(result)
        }
    }
}
