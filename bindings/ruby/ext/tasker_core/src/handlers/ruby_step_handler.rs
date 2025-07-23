//! # Ruby Step Handler Integration - Migrated to Shared Components
//!
//! MIGRATION STATUS: ✅ COMPLETED - Now using shared handle-based database access
//! This module provides the `RubyStepHandler` struct that implements the Rust `StepHandler` trait,
//! allowing Ruby step handlers to be seamlessly integrated into the orchestration system.
//!
//! BEFORE: Direct global database pool access (2 instances)
//! AFTER: Shared handle-based database access pattern
//! MIGRATION: Eliminated global database pool lookups, now uses persistent shared handles
//!
//! ## Architecture
//!
//! The `RubyStepHandler` acts as a bridge between the Rust orchestration core and Ruby business logic:
//! - Implements the `StepHandler` trait for orchestration integration
//! - ✅ **Uses shared handle-based database access** for consistent resource management
//! - Stores Ruby handler class name and configuration
//! - Converts data between Rust and Ruby formats
//! - Calls Ruby methods through Magnus FFI
//!
//! ## Usage
//!
//! Ruby handlers are registered with the orchestration system and can be discovered by the
//! StepExecutor during workflow execution.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::models::ruby_task::RubyTask;
use crate::models::ruby_step::RubyStep;
use crate::models::ruby_step_sequence::RubyStepSequence;
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;
use magnus::{Ruby, Value, Error, RModule, Module, Object, function, method};
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
use tasker_core::logging::{log_step_operation, log_ffi_operation, log_error};

/// Ruby step handler that implements the Rust StepHandler trait
///
/// This struct bridges Ruby step handlers with the Rust orchestration system,
/// allowing Ruby business logic to be executed within the orchestration workflow.
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

    /// ✅ HANDLE-BASED: Convert StepExecutionContext to Ruby-compatible data using provided pool
    async fn convert_context_to_ruby(&self, context: &StepExecutionContext, pool: &sqlx::PgPool) -> OrchestrationResult<(RubyTask, RubyStepSequence, RubyStep)> {
        log_step_operation(
            "CONVERT_CONTEXT_TO_RUBY",
            Some(context.task_id),
            Some(context.step_id),
            Some(&context.step_name),
            "STARTING",
            Some(&format!("handler_class={}", self.handler_class))
        );

        // Load Task from database
        debug!("📂 RUBY_STEP_HANDLER: Loading task from database - task_id={}", context.task_id);
        let task = Task::find_by_id(&pool, context.task_id).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_task".to_string(),
                reason: format!("Failed to load task {}: {}", context.task_id, e),
            })?
            .ok_or_else(|| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_task".to_string(),
                reason: format!("Task {} not found", context.task_id),
            })?;
        debug!("✅ RUBY_STEP_HANDLER: Task loaded successfully - task_id={}, complete={}", task.task_id, task.complete);

        // Load WorkflowStep from database
        debug!("📂 RUBY_STEP_HANDLER: Loading workflow step from database - step_id={}", context.step_id);
        let workflow_step = WorkflowStep::find_by_id(&pool, context.step_id).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_workflow_step".to_string(),
                reason: format!("Failed to load workflow step {}: {}", context.step_id, e),
            })?
            .ok_or_else(|| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "load_workflow_step".to_string(),
                reason: format!("WorkflowStep {} not found", context.step_id),
            })?;
        debug!("✅ RUBY_STEP_HANDLER: WorkflowStep loaded successfully - step_id={}, named_step_id={}, processed={}", 
               workflow_step.workflow_step_id, workflow_step.named_step_id, workflow_step.processed);

        // Get step dependencies
        debug!("📂 RUBY_STEP_HANDLER: Loading step dependencies - step_id={}", context.step_id);
        let dependencies = workflow_step.get_dependencies(&pool).await
            .map_err(|e| tasker_core::orchestration::errors::OrchestrationError::DatabaseError {
                operation: "get_dependencies".to_string(),
                reason: format!("Failed to get dependencies for step {}: {}", context.step_id, e),
            })?;
        debug!("✅ RUBY_STEP_HANDLER: Dependencies loaded - count={}", dependencies.len());

        // Convert to Ruby objects
        debug!("🔄 RUBY_STEP_HANDLER: Converting to Ruby objects");
        let ruby_task = RubyTask::from_task(&task);
        let ruby_step = RubyStep::from_workflow_step_with_name(&workflow_step, &context.step_name);
        debug!("✅ RUBY_STEP_HANDLER: Ruby task and step created - task_id={}, step_name={}", ruby_task.task_id, ruby_step.name);

        // Convert dependencies to RubyStep objects
        debug!("🔄 RUBY_STEP_HANDLER: Converting {} dependencies to Ruby objects", dependencies.len());
        let ruby_dependencies: Vec<RubyStep> = dependencies
            .iter()
            .map(|dep| RubyStep::from_workflow_step(dep))
            .collect();
        debug!("✅ RUBY_STEP_HANDLER: Ruby dependencies created - count={}", ruby_dependencies.len());

        // Create all_steps including dependencies + current step
        let mut all_steps = ruby_dependencies.clone();
        all_steps.push(ruby_step.clone());
        debug!("🔄 RUBY_STEP_HANDLER: Creating step sequence - total_steps={}, current_position={}", 
               dependencies.len() + 1, dependencies.len());

        let ruby_sequence = RubyStepSequence::new_with_all_steps(
            dependencies.len() + 1, // total steps including current
            dependencies.len(), // current_position - current step is last
            ruby_dependencies,
            context.step_id,
            all_steps,
        );

        debug!("✅ RUBY_STEP_HANDLER: Context conversion complete - ready for Ruby handler call");
        Ok((ruby_task, ruby_sequence, ruby_step))
    }

    /// Call Ruby process method
    async fn call_ruby_process(&self, ruby_task: &RubyTask, ruby_sequence: &RubyStepSequence, ruby_step: &RubyStep) -> OrchestrationResult<serde_json::Value> {
        log_ffi_operation(
            "CALL_RUBY_PROCESS_METHOD",
            "RubyStepHandler",
            "STARTING",
            Some(&format!("handler_class={}, step_name={}", self.handler_class, self.step_name)),
            Some(&format!("task_id={}, step_id={}", ruby_task.task_id, ruby_step.workflow_step_id))
        );

        debug!("📞 RUBY_STEP_HANDLER: Getting Ruby interpreter");
        let ruby = Ruby::get().map_err(|e| {
            debug!("❌ RUBY_STEP_HANDLER: Failed to get Ruby interpreter - error={}", e);
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "get_ruby".to_string(),
                reason: format!("Failed to get Ruby interpreter: {}", e),
            }
        })?;
        debug!("✅ RUBY_STEP_HANDLER: Ruby interpreter acquired");

        // Get the handler class
        debug!("🏗️  RUBY_STEP_HANDLER: Instantiating Ruby handler - class_name={}", self.handler_class);
        let handler_class: Value = ruby.eval(&format!("{}::new", self.handler_class))
            .map_err(|e| {
                debug!("❌ RUBY_STEP_HANDLER: Failed to instantiate Ruby handler - class={}, error={}", self.handler_class, e);
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "instantiate_handler".to_string(),
                    reason: format!("Failed to instantiate Ruby handler {}: {}", self.handler_class, e),
                }
            })?;
        debug!("✅ RUBY_STEP_HANDLER: Ruby handler instantiated successfully - class={}", self.handler_class);

        // Convert Ruby objects to Values for method call
        // Note: These types implement IntoValue automatically via #[magnus::wrap]
        // but we need to clone them since we have references and IntoValue requires owned values
        debug!("🔄 RUBY_STEP_HANDLER: Preparing method arguments - cloning Ruby objects");
        let task_clone = ruby_task.clone();
        let sequence_clone = ruby_sequence.clone();
        let step_clone = ruby_step.clone();
        debug!("✅ RUBY_STEP_HANDLER: Method arguments prepared");

        // Call the process method
        log_ffi_operation(
            "RUBY_PROCESS_METHOD_CALL",
            "RubyStepHandler",
            "CALLING",
            Some(&format!("handler_class={}, method=process", self.handler_class)),
            Some(&format!("task_id={}, step_id={}", ruby_task.task_id, ruby_step.workflow_step_id))
        );
        let result: Value = handler_class.funcall("process", (task_clone, sequence_clone, step_clone))
            .map_err(|e| {
                log_error(
                    "RubyStepHandler",
                    "call_ruby_process",
                    &format!("Ruby process method failed - handler_class={}, error={}", self.handler_class, e),
                    Some(&format!("task_id={}, step_id={}", ruby_task.task_id, ruby_step.workflow_step_id))
                );
                tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                    operation: "call_process".to_string(),
                    reason: format!("Failed to call process method on {}: {}", self.handler_class, e),
                }
            })?;
        log_ffi_operation(
            "RUBY_PROCESS_METHOD_CALL",
            "RubyStepHandler",
            "SUCCESS",
            Some(&format!("handler_class={}", self.handler_class)),
            Some(&format!("task_id={}, step_id={}", ruby_task.task_id, ruby_step.workflow_step_id))
        );

        // Convert result back to JSON
        debug!("🔄 RUBY_STEP_HANDLER: Converting Ruby result to JSON - handler_class={}", self.handler_class);
        let json_result = ruby_value_to_json(result).map_err(|e| {
            debug!("❌ RUBY_STEP_HANDLER: Failed to convert result to JSON - handler_class={}, error={}", self.handler_class, e);
            tasker_core::orchestration::errors::OrchestrationError::FfiBridgeError {
                operation: "convert_result".to_string(),
                reason: format!("Failed to convert Ruby result to JSON: {}", e),
            }
        })?;
        debug!("✅ RUBY_STEP_HANDLER: Ruby result converted to JSON successfully - handler_class={}", self.handler_class);
        
        Ok(json_result)
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
        log_step_operation(
            "RUBY_STEP_HANDLER_PROCESS",
            Some(context.task_id),
            Some(context.step_id),
            Some(&context.step_name),
            "STARTING",
            Some(&format!("handler_class={}", self.handler_class))
        );

        // Convert context to Ruby objects
        // ✅ MIGRATED: Now using shared handle-based pool access
        debug!("📡 STEP_HANDLER_TRAIT: Getting shared orchestration handle");
        let shared_handle = SharedOrchestrationHandle::get_global();
        let pool = shared_handle.database_pool();
        debug!("✅ STEP_HANDLER_TRAIT: Shared handle and pool acquired");
        
        debug!("🔄 STEP_HANDLER_TRAIT: Converting execution context to Ruby objects");
        let (ruby_task, ruby_sequence, ruby_step) = self.convert_context_to_ruby(context, pool).await?;
        debug!("✅ STEP_HANDLER_TRAIT: Context converted to Ruby objects successfully");

        // Call Ruby process method
        debug!("🚀 STEP_HANDLER_TRAIT: Calling Ruby process method");
        let result = self.call_ruby_process(&ruby_task, &ruby_sequence, &ruby_step).await?;
        debug!("✅ STEP_HANDLER_TRAIT: Ruby process method completed successfully");

        log_step_operation(
            "RUBY_STEP_HANDLER_PROCESS",
            Some(context.task_id),
            Some(context.step_id),
            Some(&context.step_name),
            "SUCCESS",
            Some(&format!("handler_class={}", self.handler_class))
        );

        Ok(result)
    }

    /// Process results after step completion
    async fn process_results(&self, context: &StepExecutionContext, result: &StepResult) -> OrchestrationResult<()> {
        debug!(
            "🔄 STEP_HANDLER_TRAIT: Processing results for step - handler_class={}, step_name={}, success={}",
            self.handler_class,
            context.step_name,
            result.success
        );

        // Convert context to Ruby objects  
        // ✅ MIGRATED: Now using shared handle-based pool access
        debug!("📡 STEP_HANDLER_TRAIT: Getting shared handle for process_results");
        let shared_handle = SharedOrchestrationHandle::get_global();
        let pool = shared_handle.database_pool();
        let (_, _, ruby_step) = self.convert_context_to_ruby(context, pool).await?;
        debug!("✅ STEP_HANDLER_TRAIT: Context converted for process_results");

        // Call Ruby process_results method if it exists
        debug!("📞 STEP_HANDLER_TRAIT: Calling Ruby process_results method if available");
        self.call_ruby_process_results(&ruby_step, result.output_data.as_ref().unwrap_or(&serde_json::json!({})), None).await?;
        debug!("✅ STEP_HANDLER_TRAIT: Ruby process_results method completed");

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

/// Ruby wrapper for RubyStepHandler 
#[magnus::wrap(class = "TaskerCore::RubyStepHandler")]
pub struct RubyStepHandlerWrapper {
    inner: RubyStepHandler,
}

impl RubyStepHandlerWrapper {
    /// Create new RubyStepHandler from Ruby
    pub fn new(handler_class: String, step_name: String, config: Value) -> Result<Self, Error> {
        let config_json = ruby_value_to_json(config)?;
        let config_map: HashMap<String, serde_json::Value> = if let serde_json::Value::Object(map) = config_json {
            map.into_iter().collect()
        } else {
            HashMap::new()
        };
        
        let inner = RubyStepHandler::new(handler_class, step_name, config_map);
        Ok(Self { inner })
    }
    
    /// Get handler class name
    pub fn handler_class(&self) -> String {
        self.inner.handler_class().to_string()
    }
    
    /// Get step name
    pub fn step_name(&self) -> String {
        self.inner.step_name().to_string()
    }
}

/// Register RubyStepHandler class with Ruby
pub fn register_ruby_step_handler_class(ruby: &Ruby, module: &RModule) -> Result<(), Error> {
    let class = module.define_class("RubyStepHandler", ruby.class_object())?;
    
    class.define_singleton_method("new", function!(RubyStepHandlerWrapper::new, 3))?;
    class.define_method("handler_class", method!(RubyStepHandlerWrapper::handler_class, 0))?;
    class.define_method("step_name", method!(RubyStepHandlerWrapper::step_name, 0))?;
    
    Ok(())
}
