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

use magnus::{function, prelude::*, Error, IntoValue, RHash, RModule, TryConvert, Value};
use std::sync::{Arc, Mutex};
use tasker_core::ffi::shared::orchestration_system::OrchestrationSystem;
use tasker_core::ffi::shared::test_database_management::TestDatabaseManager;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// Global handle to the embedded orchestration system
static EMBEDDED_SYSTEM: Mutex<Option<EmbeddedOrchestrationHandle>> = Mutex::new(None);

/// Handle for managing embedded orchestration system lifecycle
struct EmbeddedOrchestrationHandle {
    system: Arc<OrchestrationSystem>,
    shutdown_sender: Option<oneshot::Sender<()>>,
    runtime_handle: tokio::runtime::Handle,
}

impl EmbeddedOrchestrationHandle {
    /// Create new embedded handle
    fn new(
        system: Arc<OrchestrationSystem>,
        shutdown_sender: oneshot::Sender<()>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            system,
            shutdown_sender: Some(shutdown_sender),
            runtime_handle,
        }
    }

    /// Check if system is running
    fn is_running(&self) -> bool {
        self.shutdown_sender.is_some()
    }

    /// Stop the embedded system
    fn stop(&mut self) -> Result<(), String> {
        if let Some(sender) = self.shutdown_sender.take() {
            sender
                .send(())
                .map_err(|_| "Failed to send shutdown signal")?;
            info!("🛑 Embedded orchestration system shutdown requested");
            Ok(())
        } else {
            warn!("Embedded orchestration system already stopped");
            Ok(())
        }
    }

    /// Get system status information
    fn status(&self) -> EmbeddedSystemStatus {
        EmbeddedSystemStatus {
            running: self.is_running(),
            database_pool_size: self.system.database_pool().size(),
            database_pool_idle: self.system.database_pool().num_idle(),
        }
    }
}

/// System status information
#[derive(Debug)]
struct EmbeddedSystemStatus {
    running: bool,
    database_pool_size: u32,
    database_pool_idle: usize,
}

/// Start the embedded orchestration system for testing
///
/// This starts the orchestration system in a background thread using the same
/// pgmq architecture, but running in the same process as Ruby for testing.
///
/// # Arguments
/// * `namespaces` - Array of namespace strings to initialize queues for
///
/// # Returns
/// * Success message or error string
fn start_embedded_orchestration(namespaces: Vec<String>) -> Result<String, Error> {
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

    info!("🚀 Starting embedded orchestration system for testing");

    // Create tokio runtime for orchestration system
    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        error!("Failed to create tokio runtime: {}", e);
        Error::new(
            magnus::exception::runtime_error(),
            "Runtime creation failed",
        )
    })?;

    let runtime_handle = rt.handle().clone();

    // Initialize orchestration system
    let system = rt
        .block_on(async {
            OrchestrationSystem::new().await.map_err(|e| {
                error!("Failed to initialize orchestration system: {}", e);
                format!("System initialization failed: {e}")
            })
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Initialize namespace queues
    let namespaces_refs: Vec<&str> = namespaces.iter().map(|s| s.as_str()).collect();
    rt.block_on(async {
        system
            .initialize_queues(&namespaces_refs)
            .await
            .map_err(|e| {
                error!("Failed to initialize queues: {}", e);
                format!("Queue initialization failed: {e}")
            })
    })
    .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Create shutdown channel
    let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

    // Spawn background task to run orchestration loop
    let system_clone = system.clone();
    std::thread::spawn(move || {
        rt.block_on(async {
            info!("✅ Embedded orchestration system started - starting orchestration loop");

            // Start all three processors concurrently without waiting for them to complete
            // They are designed to run indefinitely, so we spawn them and let them run
            let orchestration_loop = Arc::clone(&system_clone.orchestration_loop);
            let task_request_processor = Arc::clone(&system_clone.task_request_processor);
            let step_result_processor = Arc::clone(&system_clone.step_result_processor);

            info!("🚀 EMBEDDED: Starting orchestration processors");

            // Spawn tasks and store abort handles for clean shutdown
            let orchestration_task = tokio::spawn(async move {
                info!(
                    "🔄 EMBEDDED: Orchestration loop task started - beginning continuous operation"
                );
                if let Err(e) = orchestration_loop.run_continuous().await {
                    error!("❌ EMBEDDED: Orchestration loop failed: {}", e);
                } else {
                    info!("✅ EMBEDDED: Orchestration loop completed successfully");
                }
            });
            let orchestration_abort_handle = orchestration_task.abort_handle();

            let task_request_task = tokio::spawn(async move {
                info!("🔄 EMBEDDED: Task request processor started");
                if let Err(e) = task_request_processor.start_processing_loop().await {
                    error!("❌ EMBEDDED: Task request processor failed: {}", e);
                }
            });
            let task_request_abort_handle = task_request_task.abort_handle();

            let step_result_task = tokio::spawn(async move {
                info!("🔄 EMBEDDED: Step result processor started");
                if let Err(e) = step_result_processor.start_processing_loop().await {
                    error!("❌ EMBEDDED: Step result processor failed: {}", e);
                }
            });
            let step_result_abort_handle = step_result_task.abort_handle();

            info!("✅ EMBEDDED: All orchestration processors spawned successfully");

            // Wait for shutdown signal - the processors run indefinitely in the background
            tokio::select! {
                _ = shutdown_receiver => {
                    info!("🛑 EMBEDDED: Shutdown signal received, stopping processors");
                    orchestration_abort_handle.abort();
                    task_request_abort_handle.abort();
                    step_result_abort_handle.abort();
                }
                result = orchestration_task => {
                    match result {
                        Ok(_) => info!("✅ EMBEDDED: Orchestration loop completed normally"),
                        Err(e) => error!("❌ EMBEDDED: Orchestration loop task failed: {}", e),
                    }
                }
                result = task_request_task => {
                    match result {
                        Ok(_) => info!("✅ EMBEDDED: Task request processor completed normally"),
                        Err(e) => error!("❌ EMBEDDED: Task request processor task failed: {}", e),
                    }
                }
                result = step_result_task => {
                    match result {
                        Ok(_) => info!("✅ EMBEDDED: Step result processor completed normally"),
                        Err(e) => error!("❌ EMBEDDED: Step result processor task failed: {}", e),
                    }
                }
            }

            info!("🛑 EMBEDDED: Orchestration system shutting down");
        });

        info!("✅ EMBEDDED: Orchestration system background thread completed");
    });

    // Store handle
    *handle_guard = Some(EmbeddedOrchestrationHandle::new(
        system,
        shutdown_sender,
        runtime_handle,
    ));

    Ok("Embedded orchestration system started successfully".to_string())
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

    // Extract values from Ruby hash and convert to Rust types
    // Note: Ruby to_ffi_hash creates symbol keys, so we need to access them as symbols
    let namespace: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("namespace"))
            .map_err(|e| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("Missing namespace: {e}"),
                )
            })?,
    )?;

    let name: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("name"))
            .map_err(|e| {
                Error::new(magnus::exception::arg_error(), format!("Missing name: {e}"))
            })?,
    )?;

    let version: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("version"))
            .map_err(|e| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("Missing version: {e}"),
                )
            })?,
    )?;

    let context_value: Value = task_request_hash
        .aref(magnus::Symbol::new("context"))
        .map_err(|e| {
            Error::new(
                magnus::exception::arg_error(),
                format!("Missing context: {e}"),
            )
        })?;
    let context_json = crate::context::ruby_value_to_json(context_value)?;

    let status: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("status"))
            .unwrap_or_else(|_| "PENDING".into_value()),
    )?;

    let initiator: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("initiator"))
            .map_err(|e| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("Missing initiator: {e}"),
                )
            })?,
    )?;

    let source_system: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("source_system"))
            .map_err(|e| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("Missing source_system: {e}"),
                )
            })?,
    )?;

    let reason: String = String::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("reason"))
            .map_err(|e| {
                Error::new(
                    magnus::exception::arg_error(),
                    format!("Missing reason: {e}"),
                )
            })?,
    )?;

    let complete: bool = bool::try_convert(
        task_request_hash
            .aref(magnus::Symbol::new("complete"))
            .unwrap_or_else(|_| false.into_value()),
    )?;

    // Extract tags array (optional)
    let tags: Vec<String> = match task_request_hash.aref(magnus::Symbol::new("tags")) {
        Ok(tags_value) => {
            let tags_array: magnus::RArray = magnus::RArray::try_convert(tags_value)?;
            let mut tags_vec = Vec::new();
            for i in 0..tags_array.len() {
                if let Ok(tag_value) = tags_array.entry(i as isize) {
                    if let Ok(tag_str) = String::try_convert(tag_value) {
                        tags_vec.push(tag_str);
                    }
                }
            }
            tags_vec
        }
        Err(_) => vec![],
    };

    // Extract bypass_steps array (optional)
    let bypass_steps: Vec<String> =
        match task_request_hash.aref(magnus::Symbol::new("bypass_steps")) {
            Ok(bypass_value) => {
                let bypass_array: magnus::RArray = magnus::RArray::try_convert(bypass_value)?;
                let mut bypass_vec = Vec::new();
                for i in 0..bypass_array.len() {
                    if let Ok(step_value) = bypass_array.entry(i as isize) {
                        if let Ok(step_str) = String::try_convert(step_value) {
                            bypass_vec.push(step_str);
                        }
                    }
                }
                bypass_vec
            }
            Err(_) => vec![],
        };

    // Extract requested_at (optional, default to now)
    let requested_at = match task_request_hash.aref(magnus::Symbol::new("requested_at")) {
        Ok(time_value) => {
            let time_str: String = String::try_convert(time_value)?;
            chrono::NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%dT%H:%M:%S")
                .unwrap_or_else(|_| chrono::Utc::now().naive_utc())
        }
        Err(_) => chrono::Utc::now().naive_utc(),
    };

    // Extract priority (optional)
    let priority: Option<i32> = task_request_hash
        .aref(magnus::Symbol::new("priority"))
        .ok()
        .and_then(|v| i32::try_convert(v).ok());

    // Extract claim_timeout_seconds (optional)
    let claim_timeout_seconds: Option<i32> = task_request_hash
        .aref(magnus::Symbol::new("claim_timeout_seconds"))
        .ok()
        .and_then(|v| i32::try_convert(v).ok());

    // Extract options hash (optional)
    let options: Option<std::collections::HashMap<String, serde_json::Value>> =
        match task_request_hash.aref(magnus::Symbol::new("options")) {
            Ok(options_value) => {
                // Convert options_value to JSON first, then to HashMap
                let options_json = crate::context::ruby_value_to_json(options_value)?;
                if let Some(options_obj) = options_json.as_object() {
                    let mut options_map = std::collections::HashMap::new();
                    for (key, value) in options_obj {
                        options_map.insert(key.clone(), value.clone());
                    }
                    if options_map.is_empty() {
                        None
                    } else {
                        Some(options_map)
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        };

    // Create task request using the same structure as the system expects
    let task_request = tasker_core::models::core::task_request::TaskRequest {
        namespace,
        name,
        version,
        context: context_json,
        status,
        initiator,
        source_system,
        reason,
        complete,
        tags,
        bypass_steps,
        requested_at,
        options,
        priority,
        claim_timeout_seconds,
    };

    // Use the embedded orchestration system's task initializer
    let system = handle.system.clone();
    let runtime_handle = handle.runtime_handle.clone();

    let result = runtime_handle
        .block_on(async {
            system.initialize_task(task_request).await.map_err(|e| {
                error!("Failed to initialize task via orchestration system: {}", e);
                format!("Task initialization failed: {e}")
            })
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    // Convert to Ruby hash
    let hash = RHash::new();
    hash.aset("task_id", result.task_id)?;
    hash.aset("success", true)?;
    hash.aset(
        "message",
        format!(
            "Task initialized via orchestration system with {} steps",
            result.step_count
        ),
    )?;

    // Convert metadata
    let metadata_hash = RHash::new();
    metadata_hash.aset("step_count", result.step_count)?;
    if let Some(config_name) = result.handler_config_name {
        metadata_hash.aset("handler_config_name", config_name)?;
    }

    // Convert step_mapping to Ruby
    let step_mapping_json =
        serde_json::to_value(&result.step_mapping).unwrap_or(serde_json::Value::Null);
    let step_mapping_ruby = crate::context::json_to_ruby_value(step_mapping_json)?;
    metadata_hash.aset("step_mapping", step_mapping_ruby)?;

    hash.aset("metadata", metadata_hash)?;

    info!(
        "✅ FFI: Task initialized via orchestration system - ID: {}, Steps: {}",
        result.task_id, result.step_count
    );

    Ok(hash.as_value())
}

/// Enqueue ready steps for a task (testing helper)
///
/// This provides a simple way for Ruby tests to trigger step enqueueing
/// without complex orchestration setup.
fn enqueue_task_steps(task_id: i64) -> Result<String, Error> {
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
    let system = handle.system.clone();
    let runtime_handle = handle.runtime_handle.clone();

    let result = runtime_handle
        .block_on(async {
            system.enqueue_ready_steps(task_id).await.map_err(|e| {
                error!("Failed to enqueue steps for task {}: {}", task_id, e);
                format!("Step enqueueing failed: {e}")
            })
        })
        .map_err(|e| Error::new(magnus::exception::runtime_error(), e))?;

    Ok(format!("Enqueued steps for task {task_id}: {result:?}"))
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
                .map_err(|e| format!("Data cleanup failed: {e}"))
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
