//! # Embedded FFI Bridge for Testing
//!
//! Minimal FFI interface for embedded mode testing. Provides lifecycle management
//! for the orchestration system without complex state sharing.
//!
//! Design Principles:
//! - Lightweight: Only lifecycle management (start/stop/status)
//! - No complex state sharing between Ruby and Rust
//! - Same pgmq-based architecture, just running in-process
//! - Ruby tests can start embedded orchestrator, run tests, stop orchestrator

use magnus::{function, prelude::*, Error, RHash, RModule, Value};
use serde::{Deserialize, Serialize};
use serde_magnus::{deserialize, serialize};
use std::sync::{Arc, Mutex};
use tasker_core::ffi::shared::test_database_management::TestDatabaseManager;
use tasker_core::orchestration::{
    bootstrap::{OrchestrationBootstrap, OrchestrationSystemHandle},
    OrchestrationCore,
};
use tracing::{error, info};
use uuid::Uuid;

/// Global handle to the embedded orchestration system
static EMBEDDED_SYSTEM: Mutex<Option<EmbeddedOrchestrationHandle>> = Mutex::new(None);

/// Handle for managing embedded orchestration system lifecycle
struct EmbeddedOrchestrationHandle {
    system_handle: OrchestrationSystemHandle,
    /// Keep the runtime alive for the lifetime of the embedded system
    _runtime: tokio::runtime::Runtime,
}

impl EmbeddedOrchestrationHandle {
    /// Create new embedded handle with runtime instance
    fn new(system_handle: OrchestrationSystemHandle, runtime: tokio::runtime::Runtime) -> Self {
        Self {
            system_handle,
            _runtime: runtime,
        }
    }

    /// Check if system is running
    fn is_running(&self) -> bool {
        self.system_handle.is_running()
    }

    /// Stop the embedded system
    fn stop(&mut self) -> Result<(), String> {
        self.system_handle.stop().map_err(|e| e.to_string())
    }

    /// Get system status information
    fn status(&self) -> EmbeddedSystemStatus {
        let status = self.system_handle.status();
        EmbeddedSystemStatus {
            running: status.running,
            database_pool_size: status.database_pool_size,
            database_pool_idle: status.database_pool_idle,
        }
    }

    /// Get orchestration core for task operations
    fn orchestration_core(&self) -> &Arc<OrchestrationCore> {
        &self.system_handle.orchestration_core
    }

    /// Get runtime handle for async operations
    fn runtime_handle(&self) -> &tokio::runtime::Handle {
        &self.system_handle.runtime_handle
    }
}

/// System status information
#[derive(Debug)]
struct EmbeddedSystemStatus {
    running: bool,
    database_pool_size: u32,
    database_pool_idle: usize,
}

/// FFI-compatible TaskRequest that can be deserialized from Ruby Hash using serde_magnus
/// This provides a clean interface for converting Ruby hashes to Rust structs
#[derive(Debug, Clone, Deserialize)]
struct RubyTaskRequest {
    /// The namespace of the task to be performed (required)
    pub namespace: String,
    /// The name of the task to be performed (required)
    pub name: String,
    /// The version of the task to be performed (required)
    pub version: String,
    /// Context data required for task execution (required)
    pub context: serde_json::Value,
    /// Current status of the task (optional, defaults to "PENDING")
    #[serde(default = "default_status")]
    pub status: String,
    /// The entity or system that initiated this task request (required)
    pub initiator: String,
    /// The system from which this task originated (required)
    pub source_system: String,
    /// The reason why this task was requested (required)
    pub reason: String,
    /// Indicates whether the task has been completed (optional, defaults to false)
    #[serde(default)]
    pub complete: bool,
    /// Tags associated with this task (optional, defaults to empty vec)
    #[serde(default)]
    pub tags: Vec<String>,
    /// List of step names that should be bypassed (optional, defaults to empty vec)
    #[serde(default)]
    pub bypass_steps: Vec<String>,
    /// Timestamp when the task was requested (optional, defaults to now)
    #[serde(default = "default_requested_at")]
    pub requested_at: String,
    /// Priority for task execution (optional)
    pub priority: Option<i32>,
    /// Configurable timeout for task claims (optional)
    pub claim_timeout_seconds: Option<i32>,
    /// Custom options that override task template defaults (optional)
    pub options: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Default status for task requests
fn default_status() -> String {
    "PENDING".to_string()
}

/// Default requested_at timestamp (now)
fn default_requested_at() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

impl RubyTaskRequest {
    /// Convert RubyTaskRequest to core TaskRequest
    fn into_task_request(
        self,
    ) -> Result<tasker_core::models::core::task_request::TaskRequest, Error> {
        // Parse requested_at timestamp
        let requested_at =
            chrono::NaiveDateTime::parse_from_str(&self.requested_at, "%Y-%m-%dT%H:%M:%S")
                .unwrap_or_else(|_| chrono::Utc::now().naive_utc());

        Ok(tasker_core::models::core::task_request::TaskRequest {
            namespace: self.namespace,
            name: self.name,
            version: self.version,
            context: self.context,
            status: self.status,
            initiator: self.initiator,
            source_system: self.source_system,
            reason: self.reason,
            complete: self.complete,
            tags: self.tags,
            bypass_steps: self.bypass_steps,
            requested_at,
            options: self.options,
            priority: self.priority,
            claim_timeout_seconds: self.claim_timeout_seconds,
        })
    }
}

/// Response struct for task initialization that can be serialized to Ruby using serde_magnus
#[derive(Debug, Clone, Serialize)]
struct TaskInitializationResponse {
    /// Task ID of the created task
    pub task_uuid: Uuid,
    /// Success indicator
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// Task initialization metadata
    pub metadata: TaskMetadata,
}

/// Task metadata for the initialization response
#[derive(Debug, Clone, Serialize)]
struct TaskMetadata {
    /// Number of workflow steps created
    pub step_count: usize,
    /// Handler configuration name used (if any)
    pub handler_config_name: Option<String>,
    /// Step mapping information
    pub step_mapping: serde_json::Value,
}

impl TaskInitializationResponse {
    /// Create response from orchestration result
    fn from_orchestration_result(
        result: tasker_core::orchestration::task_initializer::TaskInitializationResult,
    ) -> Self {
        Self {
            task_uuid: result.task_uuid,
            success: true,
            message: format!(
                "Task initialized via orchestration system with {} steps",
                result.step_count
            ),
            metadata: TaskMetadata {
                step_count: result.step_count,
                handler_config_name: result.handler_config_name,
                step_mapping: serde_json::to_value(&result.step_mapping)
                    .unwrap_or(serde_json::Value::Null),
            },
        }
    }
}

/// Start the embedded orchestration system for testing
///
/// UNIFIED ARCHITECTURE: This starts the orchestration system using OrchestrationLoopCoordinator
/// with configuration-driven initialization. The system now provides:
/// - Dynamic executor pool management with auto-scaling
/// - Health monitoring and resource management
/// - Circuit breaker integration for resilience
/// - Configuration-driven scaling policies
///
/// # Arguments
/// * `namespaces` - Array of namespace strings to initialize queues for
///
/// # Returns
/// * Success message or error string
fn start_embedded_orchestration(namespaces: Vec<String>) -> Result<String, Error> {
    // Load environment variables from .env files FIRST - this ensures DATABASE_URL and other
    // environment variables are available before any configuration loading begins
    // Note: dotenvy is loaded in the parent process, so .env should already be available

    let mut handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    if handle_guard.is_some() {
        return Ok("Embedded orchestration system already running".to_string());
    }

    info!("🚀 EMBEDDED: Starting orchestration system using unified OrchestrationLoopCoordinator architecture");

    // Create tokio runtime for orchestration system
    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    // Use standalone bootstrap with orchestration domain config directory
    // When running embedded from Ruby bindings, we point directly to the orchestration
    // domain which contains all the required component configurations
    let system_handle = rt
        .block_on(async {
            OrchestrationBootstrap::bootstrap_embedded(namespaces)
                .await
                .map_err(|e| {
                    error!(
                        "Failed to bootstrap orchestration system with coordinator: {}",
                        e
                    );
                    format!("Orchestration coordinator bootstrap failed: {e}")
                })
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    info!(
        "✅ EMBEDDED: Orchestration system bootstrapped successfully using OrchestrationLoopCoordinator"
    );

    // The OrchestrationLoopCoordinator handles all executor lifecycle management automatically,
    // providing dynamic scaling, health monitoring, and resource management

    // Store the handle with the runtime instance to prevent shutdown
    *handle_guard = Some(EmbeddedOrchestrationHandle::new(system_handle, rt));

    Ok(
        "Embedded orchestration system started successfully with OrchestrationLoopCoordinator"
            .to_string(),
    )
}

/// Stop the embedded orchestration system
fn stop_embedded_orchestration() -> Result<String, Error> {
    let mut handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    match handle_guard.as_mut() {
        Some(handle) => {
            handle.stop().map_err(|e| {
                error!("Failed to stop embedded system: {}", e);
                Error::new(magnus::exception::runtime_error(), e)
            })?;
            *handle_guard = None;
            Ok("Embedded orchestration system stopped".to_string())
        }
        None => Ok("Embedded orchestration system not running".to_string()),
    }
}

/// Get status of the embedded orchestration system
fn get_embedded_orchestration_status() -> Result<Value, Error> {
    let handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let status = match handle_guard.as_ref() {
        Some(handle) => handle.status(),
        None => EmbeddedSystemStatus {
            running: false,
            database_pool_size: 0,
            database_pool_idle: 0,
        },
    };

    // Convert to Ruby hash
    let hash = RHash::new();
    hash.aset("running", status.running)?;
    hash.aset("database_pool_size", status.database_pool_size)?;
    hash.aset("database_pool_idle", status.database_pool_idle)?;

    Ok(hash.as_value())
}

/// Initialize a task directly using the embedded orchestration system
///
/// This uses the embedded orchestrator's handle to initialize tasks directly,
/// ensuring we use the same system that processes workflows rather than
/// creating parallel infrastructure.
///
/// # Arguments
/// * `task_request_hash` - Ruby hash from TaskRequest#to_ffi_hash containing all task fields
fn initialize_task_embedded(task_request_hash: RHash) -> Result<Value, Error> {
    let handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Embedded system not running",
        )
    })?;

    // Use serde_magnus to deserialize Ruby hash directly to Rust struct - much cleaner!
    let ruby_task_request: RubyTaskRequest =
        deserialize(task_request_hash.as_value()).map_err(|e| {
            error!("Failed to deserialize Ruby task request: {}", e);
            Error::new(
                magnus::exception::arg_error(),
                format!("Invalid task request format: {e}"),
            )
        })?;

    info!(
        "🔄 EMBEDDED: Processing task request - namespace: '{}', name: '{}', version: '{}'",
        ruby_task_request.namespace, ruby_task_request.name, ruby_task_request.version
    );

    // Convert to core TaskRequest type
    let task_request = ruby_task_request.into_task_request()?;

    // Use the embedded orchestration system's task initializer
    let system = handle.orchestration_core().clone();
    let runtime_handle = handle.runtime_handle().clone();

    // Check if the system is still running before proceeding
    if !handle.is_running() {
        return Err(Error::new(
            magnus::exception::runtime_error(),
            "Embedded orchestration system is shutting down - cannot initialize task",
        ));
    }

    let result = runtime_handle
        .block_on(async {
            system.initialize_task(task_request).await.map_err(|e| {
                error!("Failed to initialize task via orchestration system: {}", e);
                format!("Task initialization failed: {e}")
            })
        })
        .map_err(|e| {
            // Check if this is a runtime shutdown error
            let error_msg = e.to_string();
            if error_msg.contains("shutdown") || error_msg.contains("being shutdown") {
                Error::new(
                    magnus::exception::runtime_error(),
                    "Orchestration runtime is shutting down - task initialization aborted",
                )
            } else {
                Error::new(magnus::exception::runtime_error(), e)
            }
        })?;

    info!(
        "✅ EMBEDDED: Task initialized via orchestration system - ID: {}, Steps: {}",
        result.task_uuid, result.step_count
    );

    // Use serde_magnus to serialize response to Ruby hash - much cleaner!
    let response = TaskInitializationResponse::from_orchestration_result(result);
    serialize(&response).map_err(|e| {
        error!("Failed to serialize task initialization response: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            format!("Response serialization failed: {e}"),
        )
    })
}

/// Enqueue ready steps for a task (testing helper)
///
/// This provides a simple way for Ruby tests to trigger step enqueueing
/// without complex orchestration setup.
fn enqueue_task_steps(task_uuid_str: String) -> Result<String, Error> {
    let task_uuid = Uuid::parse_str(&task_uuid_str)
        .map_err(|e| Error::new(magnus::exception::arg_error(), format!("Invalid UUID: {e}")))?;
    let handle_guard = EMBEDDED_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire embedded system lock: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Lock acquisition failed",
        )
    })?;

    let handle = handle_guard.as_ref().ok_or_else(|| {
        Error::new(
            magnus::exception::runtime_error(),
            "Embedded system not running",
        )
    })?;

    // Execute step enqueueing on the runtime
    let system = handle.orchestration_core().clone();
    let runtime_handle = handle.runtime_handle().clone();

    let result = runtime_handle
        .block_on(async {
            system.enqueue_ready_steps(task_uuid).await.map_err(|e| {
                error!("Failed to enqueue steps for task {}: {}", task_uuid, e);
                format!("Step enqueueing failed: {e}")
            })
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    Ok(format!("Enqueued steps for task {task_uuid}: {result:?}"))
}

/// Setup test database (comprehensive setup with migrations and cleanup)
///
/// Environment-aware database setup that only operates in test environments.
/// Provides complete database initialization for testing.
///
/// This function creates its own database connection to be independent of the orchestration system,
/// since it's designed to fix database issues that prevent the orchestrator from starting.
fn setup_test_database(database_url: String) -> Result<Value, Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    // Create our own runtime and database pool for independence
    let rt = Runtime::new().map_err(|e| {
        error!("Failed to create async runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let result = rt
        .block_on(async {
            // Create independent database pool
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .map_err(|e| format!("Failed to connect to database: {e}"))?;

            // Create test database manager with independent pool
            let test_manager = TestDatabaseManager::new(pool)
                .map_err(|e| format!("Failed to create test database manager: {e}"))?;

            test_manager
                .setup_test_database(&database_url)
                .await
                .map_err(|e| format!("Database setup failed: {e}"))
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert JSON result to Ruby value
    crate::context::json_to_ruby_value(result).map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Result conversion failed: {e}"),
        )
    })
}

/// Teardown test database (comprehensive cleanup of data and queues)
///
/// Environment-aware database teardown that only operates in test environments.
/// Provides complete cleanup of test data and message queues.
fn teardown_test_database(database_url: String) -> Result<Value, Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    // Create our own runtime and database pool for independence
    let rt = Runtime::new().map_err(|e| {
        error!("Failed to create async runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let result = rt
        .block_on(async {
            // Create independent database pool
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .map_err(|e| format!("Failed to connect to database: {e}"))?;

            // Create test database manager with independent pool
            let test_manager = TestDatabaseManager::new(pool)
                .map_err(|e| format!("Failed to create test database manager: {e}"))?;

            test_manager
                .teardown_test_database(&database_url)
                .await
                .map_err(|e| format!("Database teardown failed: {e}"))
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert JSON result to Ruby value
    crate::context::json_to_ruby_value(result).map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Result conversion failed: {e}"),
        )
    })
}

/// Cleanup test data (tables only, preserves schema)
///
/// Environment-aware data cleanup that removes all test data while preserving
/// database schema and structure.
fn cleanup_test_data() -> Result<Value, Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    // Create our own runtime and database pool for independence
    let rt = Runtime::new().map_err(|e| {
        error!("Failed to create async runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let result = rt
        .block_on(async {
            let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
            });

            // Create independent database pool
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .map_err(|e| format!("Failed to connect to database: {e}"))?;

            // Create test database manager with independent pool
            let test_manager = TestDatabaseManager::new(pool)
                .map_err(|e| format!("Failed to create test database manager: {e}"))?;

            test_manager
                .cleanup_test_data()
                .await
                .map_err(|e| format!("Data cleanup failed: {e}"))?;
            test_manager
                .cleanup_test_queues()
                .await
                .map_err(|e| format!("Queue cleanup failed: {e}"))
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert JSON result to Ruby value
    crate::context::json_to_ruby_value(result).map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Result conversion failed: {e}"),
        )
    })
}

/// Cleanup test queues (pgmq queues only)
///
/// Environment-aware queue cleanup that removes all pgmq queues used in testing.
fn cleanup_test_queues() -> Result<Value, Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    // Create our own runtime and database pool for independence
    let rt = Runtime::new().map_err(|e| {
        error!("Failed to create async runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let result = rt
        .block_on(async {
            let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
            });

            // Create independent database pool
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .map_err(|e| format!("Failed to connect to database: {e}"))?;

            // Create test database manager with independent pool
            let test_manager = TestDatabaseManager::new(pool)
                .map_err(|e| format!("Failed to create test database manager: {e}"))?;

            test_manager
                .cleanup_test_queues()
                .await
                .map_err(|e| format!("Queue cleanup failed: {e}"))
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert JSON result to Ruby value
    crate::context::json_to_ruby_value(result).map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Result conversion failed: {e}"),
        )
    })
}

/// Get test database statistics
///
/// Returns comprehensive statistics about the test database including
/// table counts, queue information, and environment details.
fn get_test_database_stats() -> Result<Value, Error> {
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    // Create our own runtime and database pool for independence
    let rt = Runtime::new().map_err(|e| {
        error!("Failed to create async runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let result = rt
        .block_on(async {
            let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
            });

            // Create independent database pool
            let pool = PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&database_url)
                .await
                .map_err(|e| format!("Failed to connect to database: {e}"))?;

            // Create test database manager with independent pool
            let test_manager = TestDatabaseManager::new(pool)
                .map_err(|e| format!("Failed to create test database manager: {e}"))?;

            test_manager
                .get_test_database_stats()
                .await
                .map_err(|e| format!("Stats retrieval failed: {e}"))
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert JSON result to Ruby value
    crate::context::json_to_ruby_value(result).map_err(|e| {
        Error::new(
            magnus::exception::runtime_error(),
            format!("Result conversion failed: {e}"),
        )
    })
}

/// Initialize embedded FFI bridge
pub fn init_embedded_bridge(tasker_core_module: &RModule) -> Result<(), Error> {
    info!("🔌 Initializing embedded FFI bridge");

    // Define module functions for lifecycle management
    tasker_core_module.define_singleton_method(
        "start_embedded_orchestration",
        function!(start_embedded_orchestration, 1),
    )?;
    tasker_core_module.define_singleton_method(
        "stop_embedded_orchestration",
        function!(stop_embedded_orchestration, 0),
    )?;
    tasker_core_module.define_singleton_method(
        "embedded_orchestration_status",
        function!(get_embedded_orchestration_status, 0),
    )?;

    // Define task initialization function for embedded mode
    tasker_core_module.define_singleton_method(
        "initialize_task_embedded",
        function!(initialize_task_embedded, 1),
    )?;

    // Define step enqueueing function for testing
    tasker_core_module
        .define_singleton_method("enqueue_task_steps", function!(enqueue_task_steps, 1))?;

    // Define test database management functions
    tasker_core_module
        .define_singleton_method("setup_test_database", function!(setup_test_database, 1))?;
    tasker_core_module.define_singleton_method(
        "teardown_test_database",
        function!(teardown_test_database, 1),
    )?;
    tasker_core_module
        .define_singleton_method("cleanup_test_data", function!(cleanup_test_data, 0))?;
    tasker_core_module
        .define_singleton_method("cleanup_test_queues", function!(cleanup_test_queues, 0))?;
    tasker_core_module.define_singleton_method(
        "get_test_database_stats",
        function!(get_test_database_stats, 0),
    )?;

    info!("✅ Embedded FFI bridge initialized with test database management");
    Ok(())
}

/// Cleanup embedded system on process exit
pub fn cleanup_embedded_system() {
    if let Ok(mut handle_guard) = EMBEDDED_SYSTEM.lock() {
        if let Some(mut handle) = handle_guard.take() {
            let _ = handle.stop();
            info!("🧹 Embedded orchestration system cleaned up");
        }
    }
}
