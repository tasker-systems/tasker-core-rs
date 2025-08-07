//! # Task Initialization FFI
//!
//! Provides FFI methods for direct task initialization in embedded mode,
//! bypassing the task request queue while still using the orchestrator.

use chrono;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::events::EventPublisher;
use crate::ffi::shared::errors::{SharedFFIError, SharedFFIResult};
use crate::models::task_request::TaskRequest;
use crate::orchestration::{TaskInitializationResult, TaskInitializer};
use crate::registry::TaskHandlerRegistry;
use sqlx::PgPool;
use std::sync::Arc;

/// FFI-compatible task initialization result
#[derive(Debug, Clone)]
pub struct FFITaskInitializationResult {
    pub task_id: i64,
    pub success: bool,
    pub message: String,
    pub metadata: HashMap<String, Value>,
}

impl From<TaskInitializationResult> for FFITaskInitializationResult {
    fn from(result: TaskInitializationResult) -> Self {
        Self {
            task_id: result.task_id,
            success: true,
            message: format!(
                "Task initialized successfully with {} steps",
                result.step_count
            ),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert(
                    "step_count".to_string(),
                    Value::Number(result.step_count.into()),
                );
                meta.insert(
                    "handler_config_name".to_string(),
                    result
                        .handler_config_name
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );

                // Convert step_mapping to metadata
                let step_mapping_json =
                    serde_json::to_value(&result.step_mapping).unwrap_or(Value::Null);
                meta.insert("step_mapping".to_string(), step_mapping_json);

                meta
            },
        }
    }
}

/// Initialize a task directly through FFI, bypassing the task request queue
///
/// This is intended for embedded mode testing where we need immediate task_id
/// feedback while still using the orchestrator for workflow processing.
///
/// # Arguments
/// * `database_url` - PostgreSQL connection string
/// * `namespace` - Task namespace (e.g., "linear_workflow")
/// * `name` - Task name (e.g., "mathematical_sequence")  
/// * `version` - Task version (e.g., "1.0.0")
/// * `context` - Task context as JSON Value
/// * `initiator` - Who initiated the task
/// * `source_system` - Source system identifier
/// * `reason` - Reason for task creation
/// * `priority` - Task priority (1-10)
/// * `claim_timeout_seconds` - How long steps can be claimed
///
/// # Returns
/// * `FFIResult<FFITaskInitializationResult>` - Task ID and initialization details
pub async fn initialize_task_direct(
    database_url: &str,
    namespace: String,
    name: String,
    version: String,
    context: Value,
    initiator: String,
    source_system: String,
    reason: String,
    priority: i32,
    claim_timeout_seconds: i32,
) -> SharedFFIResult<FFITaskInitializationResult> {
    info!(
        "🚀 FFI: Direct task initialization - {}::{}/{}",
        namespace, name, version
    );

    // Create database pool
    let pool = match PgPool::connect(database_url).await {
        Ok(pool) => pool,
        Err(e) => {
            warn!("Failed to create database pool: {}", e);
            return Err(SharedFFIError::DatabaseError(format!(
                "Database connection failed: {}",
                e
            )));
        }
    };

    // Create task request with all required fields
    let task_request = TaskRequest {
        namespace: namespace.clone(),
        name: name.clone(),
        version: version.clone(),
        context,
        status: "PENDING".to_string(),
        initiator,
        source_system,
        reason,
        complete: false,
        tags: vec![],
        bypass_steps: vec![],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(priority),
        claim_timeout_seconds: Some(claim_timeout_seconds),
    };

    // Initialize task directly using TaskInitializer with registry support
    // The database should already have task templates loaded by Ruby infrastructure
    // so we can use registry-based configuration loading instead of filesystem
    let registry = Arc::new(crate::registry::TaskHandlerRegistry::new(pool.clone()));
    let task_initializer = TaskInitializer::with_state_manager_and_registry(
        pool.clone(),
        crate::orchestration::TaskInitializationConfig::default(),
        crate::events::EventPublisher::new(),
        registry,
    );

    match task_initializer
        .create_task_from_request(task_request)
        .await
    {
        Ok(initialization_result) => {
            let task_id = initialization_result.task_id;
            info!(
                "✅ FFI: Task initialized successfully - ID: {}, Steps: {}",
                task_id, initialization_result.step_count
            );

            Ok(initialization_result.into())
        }
        Err(e) => {
            warn!("Failed to initialize task: {}", e);
            Err(SharedFFIError::TaskCreationFailed(format!(
                "Task initialization failed for {}::{}/{}: {}",
                namespace, name, version, e
            )))
        }
    }
}

/// Synchronous wrapper for initialize_task_direct using tokio runtime
///
/// This provides a sync interface for FFI bindings that can't handle async.
pub fn initialize_task_direct_sync(
    database_url: &str,
    namespace: String,
    name: String,
    version: String,
    context: Value,
    initiator: String,
    source_system: String,
    reason: String,
    priority: i32,
    claim_timeout_seconds: i32,
) -> SharedFFIResult<FFITaskInitializationResult> {
    // Create a new tokio runtime for this operation
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            return Err(SharedFFIError::Internal(format!(
                "Failed to create async runtime: {}",
                e
            )));
        }
    };

    // Run the async function
    rt.block_on(initialize_task_direct(
        database_url,
        namespace,
        name,
        version,
        context,
        initiator,
        source_system,
        reason,
        priority,
        claim_timeout_seconds,
    ))
}
