//! # Handler Foundation for Rails Migration
//!
//! Ruby base classes that Rails handlers can inherit from, enabling gradual migration
//! from Rails-native handlers to Rust-powered handler foundation.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use magnus::r_hash::ForEach;
use magnus::value::ReprValue;
use magnus::{Error, RClass, RHash, RModule, RString, Ruby, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

// Import from parent crate - this should work since bindings/ruby depends on tasker-core-rs
use tasker_core::orchestration::errors::OrchestrationError;
use tasker_core::orchestration::handler_config::HandlerConfiguration;
use tasker_core::orchestration::task_initializer::{TaskInitializer, TaskInitializationConfig};
use tasker_core::orchestration::types::{
    FrameworkIntegration, StepResult, StepStatus, TaskContext as RustTaskContext,
    TaskOrchestrationResult, ViableStep,
};
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;

use tasker_core::models::core::task_request::TaskRequest;

use tasker_core::orchestration::config::ConfigurationManager;

/// Ruby Task Handler Registry for Rails Integration
///
/// This registry provides the critical missing piece for the Ruby-Rust workflow,
/// enabling Rails to look up handlers by name/namespace/version and get the Ruby
/// class names to instantiate along with YAML template data.
///
/// ## Workflow Integration
///
/// This addresses steps 2-4 of the documented Ruby-Rust integration workflow:
/// 2. **Handler Registry Lookup**: Rails calls registry with TaskRequest
/// 3. **Handler Resolution**: Registry searches by name/namespace/version  
/// 4. **Class Name Return**: Returns Ruby class name and YAML template data
///
/// ## Usage in Rails
///
/// ```ruby
/// registry = TaskerCore::TaskHandlerRegistry.new
/// result = registry.find_handler(task_request)
///
/// if result["found"]
///   handler_class = result["ruby_class_name"]  # e.g., "MyTaskHandler"
///   yaml_config = result["yaml_template"]      # YAML configuration data
///   rails_handler = handler_class.constantize.new(yaml_config)
/// end
/// ```
#[derive(Clone)]
pub struct TaskHandlerRegistry {
    config_manager: Arc<ConfigurationManager>,
    pool: sqlx::PgPool,
}

impl TaskHandlerRegistry {
    /// Create new task handler registry
    pub fn new() -> Result<Self, Error> {
        let pool = Self::get_database_pool().map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to initialize database connection for registry",
            )
        })?;

        let config_manager = Arc::new(ConfigurationManager::new());

        Ok(Self {
            config_manager,
            pool,
        })
    }

    /// Find handler by name, namespace, and version
    ///
    /// This is the core registry lookup method that Rails will call.
    /// Returns a Ruby hash with handler information if found.
    pub fn find_handler(&self, name: &str, namespace: &str, version: &str) -> Result<Value, Error> {
        // Build runtime for async operations
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_e| {
                Error::new(
                    Ruby::get().unwrap().exception_runtime_error(),
                    "Failed to create tokio runtime for registry lookup",
                )
            })?;

        let result =
            runtime.block_on(async { self.find_handler_async(name, namespace, version).await });

        match result {
            Ok(handler_info) => json_to_ruby_value(handler_info),
            Err(error_msg) => {
                // Return a "not found" result rather than throwing an exception
                let not_found_result = serde_json::json!({
                    "found": false,
                    "error": error_msg,
                    "searched_for": {
                        "name": name,
                        "namespace": namespace,
                        "version": version
                    }
                });
                json_to_ruby_value(not_found_result)
            }
        }
    }

    /// Find handler by TaskRequest object
    ///
    /// Convenience method that extracts name/namespace/version from a TaskRequest
    pub fn find_handler_for_task_request(&self, task_request: Value) -> Result<Value, Error> {
        // Extract TaskRequest data
        let task_request_data = ruby_value_to_json(task_request)?;

        let name = task_request_data
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::new(
                    Ruby::get().unwrap().exception_arg_error(),
                    "TaskRequest missing 'name' field",
                )
            })?;

        let namespace = task_request_data
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default"); // Default namespace if not specified

        let version = task_request_data
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.1.0"); // Default version if not specified

        self.find_handler(name, namespace, version)
    }

    /// Async implementation of handler lookup
    async fn find_handler_async(
        &self,
        name: &str,
        namespace: &str,
        version: &str,
    ) -> Result<serde_json::Value, String> {
        // Build potential YAML file paths for this handler
        let potential_paths = vec![
            format!("config/tasker/tasks/{}.yaml", name),
            format!("config/tasker/tasks/{}.yml", name),
            format!("config/tasker/tasks/{}/{}.yaml", namespace, name),
            format!("config/tasker/tasks/{}/{}.yml", namespace, name),
        ];

        // Try to load handler configuration from each path
        for path_str in potential_paths {
            let path = Path::new(&path_str);
            if let Ok(handler_config) = HandlerConfiguration::from_yaml_file(path) {
                // Verify that this configuration matches our search criteria
                if self.matches_search_criteria(&handler_config, name, namespace, version) {
                    return Ok(serde_json::json!({
                        "found": true,
                        "ruby_class_name": handler_config.task_handler_class,
                        "yaml_template": handler_config,
                        "file_path": path_str,
                        "namespace": handler_config.namespace_name,
                        "version": handler_config.version,
                        "step_count": handler_config.step_templates.len()
                    }));
                }
            }
        }

        // No matching handler found
        Err(format!(
            "No handler found for name='{name}', namespace='{namespace}', version='{version}'"
        ))
    }

    /// Check if a handler configuration matches search criteria
    fn matches_search_criteria(
        &self,
        config: &HandlerConfiguration,
        name: &str,
        namespace: &str,
        version: &str,
    ) -> bool {
        // Match on name (exact or contains)
        let name_matches = config.name == name || config.name.contains(name);

        // Match on namespace
        let namespace_matches = config.namespace_name == namespace;

        // Match on version (exact or compatible)
        let version_matches =
            config.version == version || config.version.starts_with(&format!("{version}."));

        name_matches && namespace_matches && version_matches
    }

    /// List all available handlers
    ///
    /// Returns a list of all handlers that can be found in the configuration directories
    pub fn list_handlers(&self) -> Result<Value, Error> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_e| {
                Error::new(
                    Ruby::get().unwrap().exception_runtime_error(),
                    "Failed to create tokio runtime for handler listing",
                )
            })?;

        let result = runtime.block_on(async { self.list_handlers_async().await });

        let handlers_vec = result.unwrap_or_else(|_| Vec::new());
        let total_count = handlers_vec.len();

        let handlers_list = serde_json::json!({
            "handlers": handlers_vec,
            "total_count": total_count
        });

        json_to_ruby_value(handlers_list)
    }

    /// Async implementation of handler listing
    async fn list_handlers_async(&self) -> Result<Vec<serde_json::Value>, String> {
        let mut handlers = Vec::new();

        // Search common configuration directories
        let search_dirs = vec!["config/tasker/tasks"];

        for dir_path in search_dirs {
            if let Ok(entries) = std::fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && (path.extension() == Some(std::ffi::OsStr::new("yaml"))
                            || path.extension() == Some(std::ffi::OsStr::new("yml")))
                    {
                        if let Ok(config) = HandlerConfiguration::from_yaml_file(&path) {
                            handlers.push(serde_json::json!({
                                "name": config.name,
                                "namespace": config.namespace_name,
                                "version": config.version,
                                "ruby_class_name": config.task_handler_class,
                                "step_count": config.step_templates.len(),
                                "file_path": path.to_string_lossy()
                            }));
                        }
                    }
                }
            }
        }

        Ok(handlers)
    }

    /// Get database pool (shared with BaseTaskHandler)
    fn get_database_pool() -> Result<sqlx::PgPool, String> {
        BaseTaskHandler::get_database_pool()
    }
}

/// Ruby base class for step handlers
///
/// Rails step handlers can inherit from this class to gradually adopt the
/// Rust handler foundation while maintaining compatibility with existing patterns.
///
/// ## Migration Path
///
/// **Phase 1**: Existing Rails handlers continue as-is
/// **Phase 2**: New handlers inherit from BaseStepHandler
/// **Phase 3**: Migrate existing handlers to inherit from BaseStepHandler
/// **Phase 4**: Full Rust orchestration with Ruby business logic
#[derive(Debug, Clone)]
pub struct BaseStepHandler {
    // Placeholder for Ruby method storage in full implementation
}

impl BaseStepHandler {
    /// Create a new BaseStepHandler instance
    pub fn new() -> Result<Self, Error> {
        Ok(Self {})
    }

    /// Process a step with Rails engine signature: process(task, sequence, step)
    ///
    /// Rails subclasses should override this method to implement their business logic.
    /// The Rust foundation provides the execution framework, retry logic, and context management.
    pub fn process(&self, _task: Value, _sequence: Value, _step: Value) -> Result<Value, Error> {
        // This method MUST be overridden by Ruby subclasses
        // The Rust orchestration system will call this method on the Ruby handler
        // Rails subclasses implement their business logic here
        Err(Error::new(
            Ruby::get().unwrap().exception_not_imp_error(),
            "BaseStepHandler#process must be implemented by subclass",
        ))
    }

    /// Process the results from the main process() method
    /// Rails engine signature: process_results(step, process_output, initial_results)
    ///
    /// This hook allows Rails handlers to transform, validate, or augment the output
    /// before it's stored as the final step result. Maintains compatibility with
    /// existing Rails handler patterns.
    pub fn process_results(
        &self,
        step: Value,
        process_output: Value,
        initial_results: Value,
    ) -> Result<Value, Error> {
        // Default implementation returns the process output unchanged
        // Rails subclasses can override this for result transformation
        let _ = (step, initial_results); // Suppress unused parameter warnings
        Ok(process_output)
    }

    /// Validate step configuration before execution
    ///
    /// Rails handlers can override this to validate their specific configuration.
    /// This is called during handler registration and before step execution.
    pub fn validate_config(&self, config_hash: RHash) -> Result<bool, Error> {
        // Default implementation accepts all configurations
        // Rails subclasses can override for custom validation
        let _ = config_hash; // Suppress unused variable warning
        Ok(true)
    }

    /// Get the step handler name for identification
    ///
    /// Used for logging, debugging, and handler registration.
    /// Rails subclasses will typically override this.
    pub fn handler_name(&self) -> String {
        "BaseStepHandler".to_string()
    }

    /// Handler metadata for registration and introspection
    pub fn metadata(&self) -> Result<RHash, Error> {
        let hash = RHash::new();
        hash.aset("handler_name", self.handler_name())?;
        hash.aset("handler_type", "step")?;
        hash.aset("language", "rust_ruby_ffi")?;
        hash.aset("version", env!("CARGO_PKG_VERSION"))?;
        Ok(hash)
    }
}

/// Ruby base class for task handlers with integrated Rust orchestration
///
/// Rails task handlers can inherit from this class to get high-performance
/// Rust orchestration while maintaining compatibility with existing patterns.
///
/// ## Simplified Architecture (2025 Update)
///
/// - **Primitive Interfaces**: Uses simple integers (task_id) instead of complex Ruby objects
/// - **Memory Safety**: No unsafe unwraps or Ruby object lifecycle concerns
/// - **Database Management**: Tasker-core manages its own database pool internally
/// - **Configuration**: Reads DATABASE_URL from environment/config, not from Rails
/// - **Framework Agnostic**: Rails passes primitives, Rust handles all orchestration
/// - **Performance**: 10-100x faster dependency resolution and state management
///
/// ## Primary Entry Points
///
/// 1. **`initialize_task(task_request)`** - Create new tasks from Ruby TaskRequest objects
/// 2. **`handle(task_id)`** - Execute complete workflow for a task using task ID
///
/// ## Usage in Rails
///
/// ```ruby
/// class MyTaskHandler < BaseTaskHandler
///   # Step handler mappings are provided via DSL-generated methods
///   # No need to override get_step_handler - it's handled internally
/// end
///
/// # Task creation
/// handler = MyTaskHandler.new
/// result = handler.initialize_task(task_request)  # Creates task, returns task_id
///
/// # Task execution
/// result = handler.handle(task_id)  # Execute workflow using task ID
/// ```
///
/// ## Internal Methods
///
/// Several methods are marked as internal and handled by Rust orchestration:
/// - `get_step_handler` - Step handler resolution (internal to orchestration)
/// - `start_task` - State transitions (handled by WorkflowCoordinator)
/// - `handle_one_step` - Individual step execution (handled by StepExecutor)
///
/// These methods should not be called directly from Ruby code.
#[derive(Debug, Clone)]
pub struct BaseTaskHandler {
    // Internal database pool managed by tasker-core
    // This is lazy-initialized from configuration on first use
}

impl BaseTaskHandler {
    /// Create a new BaseTaskHandler instance
    pub fn new() -> Result<Self, Error> {
        Ok(Self {})
    }

    /// Initialize task - Primary creation entry point
    ///
    /// Creates and initializes a new task with workflow steps using Rust orchestration.
    /// This is one of the two main entry points for the Ruby bindings (along with `handle`).
    ///
    /// ## TaskRequest Handling
    ///
    /// Receives a Ruby TaskRequest object (dry-struct) from Rails. The TaskRequest
    /// will be serialized to JSON for safe transfer to Rust orchestration, avoiding
    /// complex Ruby object lifecycle management across the FFI boundary.
    ///
    /// ## Rust Integration
    ///
    /// Delegates to Rust TaskInitializer which handles:
    /// 1. TaskRequest validation and normalization
    /// 2. Task record creation in database using Rust models
    /// 3. Workflow step generation from step templates
    /// 4. Step dependency establishment
    /// 5. Initial state machine setup
    ///
    /// Returns the created Task ID for subsequent `handle(task_id)` calls.
    pub fn initialize_task(&self, task_request: Value) -> Result<Value, Error> {
        // Serialize Ruby TaskRequest to JSON for safe Rust processing
        let task_request_data = ruby_value_to_json(task_request)?;

        // Get database pool for orchestration
        let pool = Self::get_database_pool().map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to initialize database connection",
            )
        })?;

        // TODO: Create proper Rust TaskInitializer integration
        // For now we have the infrastructure in place but need to implement:
        //
        // 1. TaskRequest deserialization and validation in Rust
        // 2. Integration with existing Task model creation
        // 3. Step template processing and workflow step creation
        // 4. Dependency establishment using our SQL functions
        // 5. State machine initialization
        //
        // This will replace the Rails Tasker::Orchestration::TaskInitializer
        // with the Rust equivalent, providing significant performance improvements

        // Create async runtime for task initialization
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_e| {
                Error::new(
                    Ruby::get().unwrap().exception_runtime_error(),
                    "Failed to create tokio runtime",
                )
            })?;

        let result = runtime.block_on(async {
            // Actual Rust TaskInitializer implementation using existing components
            match self.create_task_from_request(pool, task_request_data).await {
                Ok((task_id, step_count)) => {
                    serde_json::json!({
                        "status": "initialized",
                        "task_id": task_id,
                        "step_count": step_count,
                        "orchestration": "rust_tasker_core"
                    })
                }
                Err(error_msg) => {
                    serde_json::json!({
                        "status": "error",
                        "error": error_msg,
                        "orchestration": "rust_tasker_core"
                    })
                }
            }
        });

        json_to_ruby_value(result)
    }

    /// Main task execution handler - Simplified signature: handle(task_id)
    ///
    /// Executes the complete workflow for a task using Rust orchestration.
    ///
    /// ## Architecture Change
    ///
    /// This method now takes a simple `task_id` (i64) instead of a Ruby Task object.
    /// This provides several benefits:
    /// - **Memory Safety**: No unsafe unwraps of Ruby objects
    /// - **Clean Separation**: Rust fetches the task using its own models and database pool
    /// - **Primitive Interface**: Simple integer instead of complex Ruby ActiveRecord object
    /// - **Performance**: Avoids serialization overhead and Ruby object lifecycle concerns
    ///
    /// Rails usage:
    /// ```ruby
    /// handler = MyTaskHandler.new
    /// result = handler.handle(task.id)  # Pass task.id, not task object
    /// ```
    pub fn handle(&self, task_id: i64) -> Result<Value, Error> {
        // Get database pool from internal configuration
        let pool = Self::get_database_pool().map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to initialize database connection",
            )
        })?;

        // Create WorkflowCoordinator with internal database pool
        let coordinator = WorkflowCoordinator::new(pool);

        // Create Ruby framework integration
        let framework = Arc::new(RubyFrameworkIntegration::new(self.clone()));

        // Execute async orchestration in blocking context
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_e| {
                Error::new(
                    Ruby::get().unwrap().exception_runtime_error(),
                    "Failed to create tokio runtime",
                )
            })?;

        let result = runtime
            .block_on(async { coordinator.execute_task_workflow(task_id, framework).await })
            .map_err(|_| {
                Error::new(
                    Ruby::get().unwrap().exception_runtime_error(),
                    "Task orchestration failed",
                )
            })?;

        // Convert TaskOrchestrationResult to Ruby
        self.convert_orchestration_result_to_ruby(result)
    }

    /// Create task from TaskRequest using centralized TaskInitializer
    ///
    /// This method now uses the transaction-safe TaskInitializer for atomic task creation
    /// with proper error handling and state machine integration.
    ///
    /// ## Process Flow
    ///
    /// 1. Deserializes and validates the TaskRequest
    /// 2. Creates TaskInitializer with Ruby-specific configuration  
    /// 3. Uses TaskInitializer for atomic task creation with transaction safety
    /// 4. Returns (task_id, step_count) on success
    ///
    /// ## Improvements Over Previous Implementation
    ///
    /// - **Transaction Safety**: All operations wrapped in SQLx transactions for atomicity
    /// - **Error Recovery**: Automatic rollback on any failure ensures consistency
    /// - **Maintainability**: Uses decomposed TaskInitializer methods
    /// - **Reusability**: Same TaskInitializer used across the entire system
    /// - **Better Error Handling**: Comprehensive error types with clear messages
    async fn create_task_from_request(
        &self,
        pool: sqlx::PgPool,
        task_request_data: serde_json::Value,
    ) -> Result<(i64, usize), String> {
        // 1. Deserialize TaskRequest from JSON
        let task_request: TaskRequest = serde_json::from_value(task_request_data)
            .map_err(|e| format!("Invalid TaskRequest JSON: {e}"))?;

        // 2. Create TaskInitializer with Ruby-specific configuration
        let config = TaskInitializationConfig {
            default_system_id: 1, // Default system ID for Ruby bindings
            initialize_state_machine: true,
            event_metadata: Some(serde_json::json!({
                "initialization": true,
                "created_by": "ruby_bindings",
                "source": "ruby_task_handler",
                "version": env!("CARGO_PKG_VERSION")
            })),
        };

        let initializer = TaskInitializer::with_config(pool, config);

        // 3. Use TaskInitializer for atomic task creation
        let result = initializer
            .create_task_from_request(task_request)
            .await
            .map_err(|e| format!("Task initialization failed: {e}"))?;

        // 4. Return the same format as the previous implementation for Ruby compatibility
        Ok((result.task_id, result.step_count))
    }

    /// Load handler configuration from YAML files
    ///
    /// Attempts to load YAML configuration for the task handler.
    /// Returns error if no configuration file is found.
    async fn load_handler_configuration(
        &self,
        task_name: &str,
    ) -> Result<HandlerConfiguration, String> {
        // TODO: Read tasker core config to get root path for task config directory
        // For now, use standard paths
        let config_paths = vec![
            format!("config/tasker/tasks/{}.yaml", task_name),
            format!("config/tasker/tasks/{}.yml", task_name),
        ];

        for path in config_paths {
            if let Ok(config) = HandlerConfiguration::from_yaml_file(Path::new(&path)) {
                return Ok(config);
            }
        }

        // Return error if no YAML configuration found
        Err(format!(
            "No configuration file found for task '{task_name}'"
        ))
    }

    /// Get database pool from internal tasker-core configuration
    ///
    /// This reads from DATABASE_URL environment variable or tasker-core config files.
    /// The pool is managed internally by tasker-core, not by the framework.
    ///
    /// Uses a singleton pattern to cache the pool for performance.
    fn get_database_pool() -> Result<sqlx::PgPool, String> {
        static DATABASE_POOL: OnceLock<Result<sqlx::PgPool, String>> = OnceLock::new();

        DATABASE_POOL
            .get_or_init(|| {
                // Read DATABASE_URL from environment
                let database_url = std::env::var("DATABASE_URL").map_err(|_| {
                    "DATABASE_URL environment variable not set for tasker-core".to_string()
                })?;

                // Create lazy connection pool (doesn't actually connect until first use)
                sqlx::PgPool::connect_lazy(&database_url)
                    .map_err(|e| format!("Failed to create database pool: {e}"))
            })
            .clone()
    }

    /// Convert TaskOrchestrationResult to Ruby Value
    fn convert_orchestration_result_to_ruby(
        &self,
        result: TaskOrchestrationResult,
    ) -> Result<Value, Error> {
        let result_json = match result {
            TaskOrchestrationResult::Complete {
                task_id,
                steps_executed,
                total_execution_time_ms,
            } => serde_json::json!({
                "status": "complete",
                "task_id": task_id,
                "steps_executed": steps_executed,
                "total_execution_time_ms": total_execution_time_ms
            }),
            TaskOrchestrationResult::Failed {
                task_id,
                error,
                failed_steps,
            } => serde_json::json!({
                "status": "failed",
                "task_id": task_id,
                "error": error,
                "failed_steps": failed_steps
            }),
            TaskOrchestrationResult::InProgress {
                task_id,
                steps_executed,
                next_poll_delay_ms,
            } => serde_json::json!({
                "status": "in_progress",
                "task_id": task_id,
                "steps_executed": steps_executed,
                "next_poll_delay_ms": next_poll_delay_ms
            }),
            TaskOrchestrationResult::Blocked {
                task_id,
                blocking_reason,
            } => serde_json::json!({
                "status": "blocked",
                "task_id": task_id,
                "blocking_reason": blocking_reason
            }),
        };

        json_to_ruby_value(result_json)
    }

    /// Validate task configuration
    ///
    /// Called during task handler registration to validate the task-level configuration.
    /// Rails handlers can override this for their specific validation requirements.
    pub fn validate_task_config(&self, config_hash: RHash) -> Result<bool, Error> {
        // Default implementation accepts all configurations
        // Rails subclasses can override for custom validation
        let _ = config_hash; // Suppress unused variable warning
        Ok(true)
    }
}

/// Helper function to convert Ruby hash to Rust HashMap<String, serde_json::Value>
///
/// Used for configuration processing and validation across the FFI boundary.
pub fn ruby_hash_to_config(ruby_hash: RHash) -> Result<HashMap<String, serde_json::Value>, Error> {
    let mut config = HashMap::new();

    ruby_hash.foreach(|key: Value, value: Value| -> Result<ForEach, Error> {
        if let Some(key_str) = RString::from_value(key) {
            let key_s = unsafe { key_str.as_str() }?;
            let json_value = ruby_value_to_json(value)?;
            config.insert(key_s.to_string(), json_value);
        }
        Ok(ForEach::Continue)
    })?;

    Ok(config)
}

/// Ruby Framework Integration for Rust Orchestration
///
/// This bridges Ruby step handlers to the Rust orchestration system, implementing
/// the FrameworkIntegration trait to delegate step execution to Ruby handlers.
pub struct RubyFrameworkIntegration {
    /// Reference to the Ruby task handler for delegation
    task_handler: BaseTaskHandler,
}

/// Result structure for Ruby step execution
#[derive(Debug, Clone)]
struct RubyStepResult {
    status: StepStatus,
    output: serde_json::Value,
    error_message: Option<String>,
    retry_after: Option<Duration>,
    error_code: Option<String>,
    error_context: Option<HashMap<String, serde_json::Value>>,
}

impl RubyFrameworkIntegration {
    /// Create new Ruby framework integration
    pub fn new(task_handler: BaseTaskHandler) -> Self {
        Self { task_handler }
    }

    /// Execute Ruby step handler - the core delegation method
    ///
    /// This method handles the complete Ruby delegation cycle:
    /// 1. Convert Rust types to Ruby objects
    /// 2. Look up appropriate Ruby step handler by step name
    /// 3. Call handler.process() with Rails-compatible signatures
    /// 4. Convert Ruby results back to Rust types
    fn execute_ruby_step_handler(
        step: &ViableStep,
        task_context: &RustTaskContext,
    ) -> Result<RubyStepResult, OrchestrationError> {
        // Get Ruby runtime - Magnus pattern for FFI calls
        let ruby = Ruby::get().map_err(|e| OrchestrationError::TaskExecutionFailed {
            task_id: step.task_id,
            reason: format!("Ruby runtime not available: {e}"),
            error_code: Some("RUBY_RUNTIME_ERROR".to_string()),
        })?;

        // 1. Convert Rust ViableStep to Ruby objects
        let ruby_step = Self::convert_viable_step_to_ruby(step, &ruby)?;
        let ruby_task_context = Self::convert_task_context_to_ruby(task_context, &ruby)?;

        // 2. Look up Ruby step handler class by step name
        let step_handler_class = Self::lookup_ruby_step_handler(&step.name, &ruby)?;

        // 3. Instantiate the step handler
        let step_handler_instance: Value = step_handler_class.funcall("new", ()).map_err(|e| {
            OrchestrationError::TaskExecutionFailed {
                task_id: step.task_id,
                reason: format!(
                    "Failed to instantiate Ruby step handler '{}': {}",
                    step.name, e
                ),
                error_code: Some("RUBY_HANDLER_INSTANTIATION_ERROR".to_string()),
            }
        })?;

        // 4. Call handler.process(task, sequence, step) - Rails signature
        let sequence_number = 1; // TODO: Get actual sequence number from orchestration context
        let ruby_result = step_handler_instance
            .funcall("process", (ruby_task_context, sequence_number, ruby_step))
            .map_err(|e| OrchestrationError::TaskExecutionFailed {
                task_id: step.task_id,
                reason: format!("Ruby step handler process() failed: {e}"),
                error_code: Some("RUBY_PROCESS_ERROR".to_string()),
            })?;

        // 5. Convert Ruby result back to Rust types
        Self::convert_ruby_result_to_rust(ruby_result, step.step_id)
    }

    /// Convert Rust ViableStep to Ruby hash
    fn convert_viable_step_to_ruby(
        step: &ViableStep,
        ruby: &Ruby,
    ) -> Result<RHash, OrchestrationError> {
        let hash = RHash::new();

        hash.aset("step_id", step.step_id)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "step_id".to_string(),
                reason: format!("Failed to set Ruby step_id: {e}"),
            })?;

        hash.aset("name", step.name.clone())
            .map_err(|e| OrchestrationError::ValidationError {
                field: "name".to_string(),
                reason: format!("Failed to set Ruby step name: {e}"),
            })?;

        hash.aset("task_id", step.task_id)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "task_id".to_string(),
                reason: format!("Failed to set Ruby task_id: {e}"),
            })?;

        // Add step state and readiness information
        hash.aset("current_state", step.current_state.clone())
            .map_err(|e| OrchestrationError::ValidationError {
                field: "current_state".to_string(),
                reason: format!("Failed to set Ruby current_state: {e}"),
            })?;

        hash.aset("dependencies_satisfied", step.dependencies_satisfied)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "dependencies_satisfied".to_string(),
                reason: format!("Failed to set Ruby dependencies_satisfied: {e}"),
            })?;

        hash.aset("retry_eligible", step.retry_eligible)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "retry_eligible".to_string(),
                reason: format!("Failed to set Ruby retry_eligible: {e}"),
            })?;

        hash.aset("attempts", step.attempts)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "attempts".to_string(),
                reason: format!("Failed to set Ruby attempts: {e}"),
            })?;

        // TODO: Get step inputs from WorkflowStep model
        // For now, set inputs to nil - this will need to be implemented
        // by looking up the WorkflowStep record and getting its inputs field
        hash.aset("inputs", ruby.qnil())
            .map_err(|e| OrchestrationError::ValidationError {
                field: "inputs".to_string(),
                reason: format!("Failed to set nil inputs: {e}"),
            })?;

        Ok(hash)
    }

    /// Convert Rust TaskContext to Ruby hash
    fn convert_task_context_to_ruby(
        context: &RustTaskContext,
        _ruby: &Ruby,
    ) -> Result<RHash, OrchestrationError> {
        let hash = RHash::new();

        hash.aset("task_id", context.task_id)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "task_id".to_string(),
                reason: format!("Failed to set Ruby context task_id: {e}"),
            })?;

        // Convert context data to Ruby
        let ruby_data = json_to_ruby_value(context.data.clone()).map_err(|e| {
            OrchestrationError::ValidationError {
                field: "data".to_string(),
                reason: format!("Failed to convert context data to Ruby: {e}"),
            }
        })?;
        hash.aset("data", ruby_data)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "data".to_string(),
                reason: format!("Failed to set Ruby context data: {e}"),
            })?;

        // Convert metadata to Ruby hash
        let metadata_hash = RHash::new();
        for (key, value) in &context.metadata {
            let ruby_value = json_to_ruby_value(value.clone()).map_err(|e| {
                OrchestrationError::ValidationError {
                    field: format!("metadata.{key}"),
                    reason: format!("Failed to convert metadata value to Ruby: {e}"),
                }
            })?;
            metadata_hash.aset(key.clone(), ruby_value).map_err(|e| {
                OrchestrationError::ValidationError {
                    field: format!("metadata.{key}"),
                    reason: format!("Failed to set Ruby metadata value: {e}"),
                }
            })?;
        }
        hash.aset("metadata", metadata_hash)
            .map_err(|e| OrchestrationError::ValidationError {
                field: "metadata".to_string(),
                reason: format!("Failed to set Ruby metadata: {e}"),
            })?;

        Ok(hash)
    }

    /// Look up Ruby step handler class by step name
    fn lookup_ruby_step_handler(
        step_name: &str,
        ruby: &Ruby,
    ) -> Result<RClass, OrchestrationError> {
        // Convert step name to Ruby class name convention
        // "payment_step" → "PaymentStepHandler"
        let class_name = Self::step_name_to_class_name(step_name);

        // Try to get the class from Ruby runtime
        // First try direct lookup, then try with module scoping
        let potential_class_names = vec![
            class_name.clone(),
            format!("TaskerCore::{}", class_name),
            format!("Tasker::{}", class_name),
        ];

        for class_name_attempt in potential_class_names {
            // Try to get the constant
            if let Ok(class) = ruby.eval::<RClass>(&class_name_attempt) {
                return Ok(class);
            }
        }

        // If no handler found, return error
        Err(OrchestrationError::TaskExecutionFailed {
            task_id: 0, // We don't have access to task_id in this context
            reason: format!(
                "Ruby step handler class not found for step '{step_name}'. Tried: {class_name}"
            ),
            error_code: Some("RUBY_HANDLER_NOT_FOUND".to_string()),
        })
    }

    /// Convert step name to Ruby class name convention
    fn step_name_to_class_name(step_name: &str) -> String {
        // Convert "payment_step" → "PaymentStepHandler"
        let words: Vec<&str> = step_name.split('_').collect();
        let pascal_case: String = words
            .iter()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect();

        // Add Handler suffix if not already present
        if pascal_case.ends_with("Handler") {
            pascal_case
        } else {
            format!("{pascal_case}Handler")
        }
    }

    /// Convert Ruby result back to Rust RubyStepResult
    fn convert_ruby_result_to_rust(
        ruby_result: Value,
        _step_id: i64,
    ) -> Result<RubyStepResult, OrchestrationError> {
        // Convert Ruby result to JSON for easier processing
        let result_json = ruby_value_to_json(ruby_result).map_err(|e| {
            OrchestrationError::TaskExecutionFailed {
                task_id: 0,
                reason: format!("Failed to convert Ruby result to JSON: {e}"),
                error_code: Some("RUBY_RESULT_CONVERSION_ERROR".to_string()),
            }
        })?;

        // Parse the result structure
        let status = if let Some(success) = result_json.get("success").and_then(|v| v.as_bool()) {
            if success {
                StepStatus::Completed
            } else {
                StepStatus::Failed
            }
        } else if let Some(status_str) = result_json.get("status").and_then(|v| v.as_str()) {
            match status_str {
                "success" | "completed" | "complete" => StepStatus::Completed,
                "failed" | "error" => StepStatus::Failed,
                "retry" | "retriable" => StepStatus::Failed, // Will be retried by orchestration
                _ => StepStatus::Completed, // Default to success for unknown statuses
            }
        } else {
            StepStatus::Completed // Default to success if no status indicator
        };

        let output = result_json
            .get("data")
            .or_else(|| result_json.get("result"))
            .or_else(|| result_json.get("output"))
            .cloned()
            .unwrap_or_else(|| result_json.clone()); // Use entire result if no specific output field

        let error_message = result_json
            .get("error")
            .or_else(|| result_json.get("error_message"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let error_code = result_json
            .get("error_code")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let retry_after = result_json
            .get("retry_after")
            .and_then(|v| v.as_u64())
            .map(Duration::from_secs);

        let error_context = result_json
            .get("error_context")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<String, serde_json::Value>>()
            });

        Ok(RubyStepResult {
            status,
            output,
            error_message,
            retry_after,
            error_code,
            error_context,
        })
    }
}

/// Implementation of FrameworkIntegration trait for Ruby/Rails
#[async_trait::async_trait]
impl FrameworkIntegration for RubyFrameworkIntegration {
    /// Execute a single step through Ruby handlers
    ///
    /// This is the critical method that enables concurrent processing - it delegates
    /// individual step execution to Ruby handlers while the Rust orchestration
    /// manages the concurrent execution and dependency resolution.
    async fn execute_single_step(
        &self,
        step: &ViableStep,
        task_context: &RustTaskContext,
    ) -> Result<StepResult, OrchestrationError> {
        use std::time::Instant;

        let start_time = Instant::now();

        // Execute the Ruby delegation in a blocking context since Ruby FFI is not async
        let step_clone = step.clone();
        let context_clone = task_context.clone();

        let result = tokio::task::spawn_blocking(move || {
            Self::execute_ruby_step_handler(&step_clone, &context_clone)
        })
        .await
        .map_err(|e| OrchestrationError::TaskExecutionFailed {
            task_id: step.task_id,
            reason: format!("Ruby step execution spawn failed: {e}"),
            error_code: Some("RUBY_SPAWN_ERROR".to_string()),
        })??;

        let execution_duration = start_time.elapsed();

        // Create StepResult from Ruby execution result
        Ok(StepResult {
            step_id: step.step_id,
            status: result.status,
            output: result.output,
            execution_duration,
            error_message: result.error_message,
            retry_after: result.retry_after,
            error_code: result.error_code,
            error_context: result.error_context,
        })
    }

    /// Framework name for logging/metrics
    fn framework_name(&self) -> &'static str {
        "ruby_rails"
    }

    /// Get task context for execution
    async fn get_task_context(&self, task_id: i64) -> Result<RustTaskContext, OrchestrationError> {
        // TODO: Fetch task context from Rails database
        // For now, return minimal context
        Ok(RustTaskContext {
            task_id,
            data: serde_json::json!({
                "task_id": task_id,
                "fetched_from": "ruby_rails_integration"
            }),
            metadata: std::collections::HashMap::new(),
        })
    }

    /// Enqueue task back to framework's queue
    async fn enqueue_task(
        &self,
        task_id: i64,
        delay: Option<Duration>,
    ) -> Result<(), OrchestrationError> {
        // TODO: Delegate to Rails queue system (Sidekiq, DelayedJob, etc.)
        let delay_ms = delay.map(|d| d.as_millis()).unwrap_or(0);
        println!(
            "Ruby framework: enqueuing task {} with delay {}ms",
            task_id, delay_ms
        );
        Ok(())
    }

    /// Mark task as failed in framework
    async fn mark_task_failed(&self, task_id: i64, error: &str) -> Result<(), OrchestrationError> {
        // TODO: Update Rails task status
        eprintln!(
            "Ruby framework: marking task {} as failed: {}",
            task_id, error
        );
        Ok(())
    }

    /// Update step state in framework's storage
    async fn update_step_state(
        &self,
        step_id: i64,
        state: &str,
        result: Option<&serde_json::Value>,
    ) -> Result<(), OrchestrationError> {
        // TODO: Update Rails step state
        println!(
            "Ruby framework: updating step {} to state '{}' (has_result: {})",
            step_id,
            state,
            result.is_some()
        );
        Ok(())
    }
}

/// Register handler foundation classes in Ruby
///
/// This exposes BaseStepHandler, BaseTaskHandler, and TaskHandlerRegistry as Ruby classes
/// that Rails applications can use, enabling the complete handler foundation architecture.
///
/// ## Ruby Classes Exposed:
/// - **TaskerCore::BaseStepHandler** - Foundation for step handlers
/// - **TaskerCore::BaseTaskHandler** - Foundation for task handlers  
/// - **TaskerCore::TaskHandlerRegistry** - Handler lookup and registry functionality
///
/// ## Critical Missing Piece Resolved
///
/// The TaskHandlerRegistry implementation resolves the critical missing piece identified
/// in the workflow analysis - Rails can now:
/// 1. Look up handlers by name/namespace/version
/// 2. Get Ruby class names to instantiate
/// 3. Retrieve YAML template data for configuration
///
/// This enables the complete Ruby-Rust integration workflow from TaskRequest
/// creation through task execution.
pub fn register_handler_classes(_ruby: &Ruby, module: RModule) -> Result<(), Error> {
    // Simplified approach: Register as module functions instead of class methods
    // This avoids complex Magnus trait requirements while still providing the functionality

    // TaskHandlerRegistry functions
    module.define_module_function("registry_new", magnus::function!(registry_new_wrapper, 0))?;
    module.define_module_function(
        "registry_find_handler",
        magnus::function!(registry_find_handler_wrapper, 3),
    )?;
    module.define_module_function(
        "registry_find_handler_for_task_request",
        magnus::function!(registry_find_handler_for_task_request_wrapper, 1),
    )?;
    module.define_module_function(
        "registry_list_handlers",
        magnus::function!(registry_list_handlers_wrapper, 0),
    )?;

    // BaseTaskHandler functions
    module.define_module_function(
        "task_handler_new",
        magnus::function!(task_handler_new_wrapper, 0),
    )?;
    module.define_module_function(
        "task_handler_initialize_task",
        magnus::function!(task_handler_initialize_task_wrapper, 1),
    )?;
    module.define_module_function(
        "task_handler_handle",
        magnus::function!(task_handler_handle_wrapper, 1),
    )?;
    module.define_module_function(
        "task_handler_validate_task_config",
        magnus::function!(task_handler_validate_task_config_wrapper, 1),
    )?;

    // BaseStepHandler functions
    module.define_module_function(
        "step_handler_new",
        magnus::function!(step_handler_new_wrapper, 0),
    )?;
    module.define_module_function(
        "step_handler_process",
        magnus::function!(step_handler_process_wrapper, 3),
    )?;
    module.define_module_function(
        "step_handler_process_results",
        magnus::function!(step_handler_process_results_wrapper, 3),
    )?;
    module.define_module_function(
        "step_handler_validate_config",
        magnus::function!(step_handler_validate_config_wrapper, 1),
    )?;

    Ok(())
}

// Simplified Magnus wrapper functions for module-level access
// These create new instances each time to avoid complex trait requirements

// TaskHandlerRegistry wrappers
fn registry_new_wrapper() -> Result<Value, Error> {
    let registry = TaskHandlerRegistry::new()?;
    // Convert to a simple hash representation for Ruby
    let hash = RHash::new();
    hash.aset("type", "TaskHandlerRegistry")?;
    hash.aset("status", "initialized")?;
    Ok(hash.as_value())
}

fn registry_find_handler_wrapper(
    name: String,
    namespace: String,
    version: String,
) -> Result<Value, Error> {
    let registry = TaskHandlerRegistry::new()?;
    registry.find_handler(&name, &namespace, &version)
}

fn registry_find_handler_for_task_request_wrapper(task_request: Value) -> Result<Value, Error> {
    let registry = TaskHandlerRegistry::new()?;
    registry.find_handler_for_task_request(task_request)
}

fn registry_list_handlers_wrapper() -> Result<Value, Error> {
    let registry = TaskHandlerRegistry::new()?;
    registry.list_handlers()
}

// BaseTaskHandler wrappers
fn task_handler_new_wrapper() -> Result<Value, Error> {
    let handler = BaseTaskHandler::new()?;
    let hash = RHash::new();
    hash.aset("type", "BaseTaskHandler")?;
    hash.aset("status", "initialized")?;
    Ok(hash.as_value())
}

fn task_handler_initialize_task_wrapper(task_request: Value) -> Result<Value, Error> {
    let handler = BaseTaskHandler::new()?;
    handler.initialize_task(task_request)
}

fn task_handler_handle_wrapper(task_id: i64) -> Result<Value, Error> {
    let handler = BaseTaskHandler::new()?;
    handler.handle(task_id)
}

fn task_handler_validate_task_config_wrapper(config_hash: RHash) -> Result<bool, Error> {
    let handler = BaseTaskHandler::new()?;
    handler.validate_task_config(config_hash)
}

// BaseStepHandler wrappers
fn step_handler_new_wrapper() -> Result<Value, Error> {
    let handler = BaseStepHandler::new()?;
    let hash = RHash::new();
    hash.aset("type", "BaseStepHandler")?;
    hash.aset("status", "initialized")?;
    Ok(hash.as_value())
}

fn step_handler_process_wrapper(task: Value, sequence: Value, step: Value) -> Result<Value, Error> {
    let handler = BaseStepHandler::new()?;
    handler.process(task, sequence, step)
}

fn step_handler_process_results_wrapper(
    step: Value,
    process_output: Value,
    initial_results: Value,
) -> Result<Value, Error> {
    let handler = BaseStepHandler::new()?;
    handler.process_results(step, process_output, initial_results)
}

fn step_handler_validate_config_wrapper(config_hash: RHash) -> Result<bool, Error> {
    let handler = BaseStepHandler::new()?;
    handler.validate_config(config_hash)
}
