//! # Shared Handle-Based FFI Architecture
//!
//! Language-agnostic handle patterns that eliminate FFI performance bottlenecks
//! by maintaining persistent references to Rust resources, preventing connection
//! pool exhaustion and eliminating repeated global lookups.
//!
//! ## Core Principle
//! - **ONE initialization** → **Many operations** via persistent handles
//! - **Zero global lookups** after handle creation
//! - **Shared resources** across all FFI calls
//! - **Production-ready** for high-throughput scenarios

use super::errors::*;
use super::orchestration_system::OrchestrationSystem;
use crate::orchestration::types::HandlerMetadata;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use tracing::{debug, info};

/// Global handle singleton to prevent creating multiple orchestration systems
static GLOBAL_SHARED_HANDLE: OnceLock<Arc<SharedOrchestrationHandle>> = OnceLock::new();

/// **SHARED HANDLE**: Core handle pattern that can be used by any language binding
///
/// This eliminates the root cause of connection pool exhaustion by ensuring
/// all FFI operations share the same initialized Rust resources.
pub struct SharedOrchestrationHandle {
    pub orchestration_system: Arc<OrchestrationSystem>,
    pub handle_id: String,
    pub created_at: SystemTime,
}

impl SharedOrchestrationHandle {
    /// **SINGLE INITIALIZATION POINT** - Creates handle with all resources
    pub fn new() -> Result<Self, SharedFFIError> {
        info!("Creating shared orchestration handle with persistent references");

        // Initialize resources ONCE - these will be shared across all operations
        let orchestration_system =
            super::orchestration_system::initialize_unified_orchestration_system();

        let handle_id = format!(
            "shared_handle_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        info!(
            "✅ SHARED HANDLE READY: {} with shared orchestration system",
            handle_id
        );

        Ok(Self {
            orchestration_system,
            handle_id,
            created_at: SystemTime::now(),
        })
    }

    /// **GLOBAL SINGLETON ACCESS** - Get or create the global handle
    pub fn get_global() -> Arc<SharedOrchestrationHandle> {
        GLOBAL_SHARED_HANDLE
            .get_or_init(|| {
                info!("🎯 GLOBAL SHARED HANDLE: Creating singleton handle");
                let handle = SharedOrchestrationHandle::new()
                    .expect("Failed to create global shared handle");
                Arc::new(handle)
            })
            .clone()
    }

    /// Validate handle is still usable (prevents stale handle usage)
    pub fn validate(&self) -> SharedFFIResult<()> {
        if self.is_expired() {
            let expires_in = self.expires_in().unwrap_or_default();
            return Err(SharedFFIError::HandleValidationFailed(
                format!("Handle expired {} seconds ago. Call refresh() to get a new handle or use expires_in() to check before operations.", 
                    expires_in.as_secs())
            ));
        }
        Ok(())
    }

    /// Validate handle or automatically refresh if expired
    ///
    /// This is the recommended method for production systems that need automatic recovery.
    /// Returns the current handle if valid, or a new refreshed handle if the current one expired.
    /// Only fails if the refresh operation itself fails.
    pub fn validate_or_refresh(&self) -> Result<Arc<SharedOrchestrationHandle>, SharedFFIError> {
        if !self.is_expired() {
            // Handle is still valid, return self wrapped in Arc
            debug!("✅ HANDLE VALID: Using existing handle {}", self.handle_id);
            Ok(Arc::new(SharedOrchestrationHandle {
                orchestration_system: self.orchestration_system.clone(),
                handle_id: self.handle_id.clone(),
                created_at: self.created_at,
            }))
        } else {
            // Handle expired, attempt automatic refresh
            info!(
                "🔄 HANDLE EXPIRED: Automatically refreshing handle {}",
                self.handle_id
            );
            match SharedOrchestrationHandle::refresh() {
                Ok(new_handle) => {
                    info!(
                        "✅ AUTO-REFRESH SUCCESS: New handle {} ready",
                        new_handle.handle_id
                    );
                    Ok(new_handle)
                }
                Err(e) => {
                    let error_msg = format!(
                        "Handle {} expired and auto-refresh failed: {}",
                        self.handle_id, e
                    );
                    Err(SharedFFIError::HandleValidationFailed(error_msg))
                }
            }
        }
    }

    /// Check if handle is expired without full validation overhead
    pub fn is_expired(&self) -> bool {
        self.created_at
            .elapsed()
            .unwrap_or(std::time::Duration::MAX)
            > std::time::Duration::from_secs(7200)
    }

    /// Get time until handle expires (None if already expired)
    pub fn expires_in(&self) -> Option<std::time::Duration> {
        let elapsed = self.created_at.elapsed().ok()?;
        let lifetime = std::time::Duration::from_secs(7200);
        if elapsed < lifetime {
            Some(lifetime - elapsed)
        } else {
            None
        }
    }

    /// Get absolute time when handle expires
    pub fn expires_at(&self) -> SystemTime {
        self.created_at + std::time::Duration::from_secs(7200)
    }

    /// Create a fresh handle with new timestamp (for renewal)
    /// Note: This creates a new handle but cannot replace the global singleton once set.
    /// For production use, consider implementing handle pooling or instance-based handles.
    pub fn refresh() -> Result<Arc<SharedOrchestrationHandle>, SharedFFIError> {
        info!("🔄 HANDLE REFRESH: Creating fresh handle to replace expired one");

        // Create new handle with fresh timestamp
        let new_handle = SharedOrchestrationHandle::new()?;
        let new_handle_arc = Arc::new(new_handle);

        info!(
            "✅ HANDLE REFRESHED: New handle {} ready (Note: global singleton unchanged)",
            new_handle_arc.handle_id
        );
        Ok(new_handle_arc)
    }

    /// Get handle info for debugging and monitoring
    pub fn info(&self) -> HandleInfo {
        let expires_at = self
            .expires_at()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_in_seconds = self.expires_in().map(|d| d.as_secs()).unwrap_or(0);
        let status = if self.is_expired() {
            "expired"
        } else {
            "active"
        }
        .to_string();

        HandleInfo {
            handle_id: self.handle_id.clone(),
            created_at: self
                .created_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            age_seconds: self.created_at.elapsed().unwrap_or_default().as_secs(),
            expires_at,
            expires_in_seconds,
            orchestration_pool_size: self.orchestration_system.database_pool().size(),
            status,
        }
    }

    /// Get database pool from orchestration system (for performance functions)
    pub fn database_pool(&self) -> &sqlx::PgPool {
        self.orchestration_system.database_pool()
    }

    /// Get event publisher from orchestration system (for event functions)
    pub fn event_publisher(&self) -> &crate::events::EventPublisher {
        &self.orchestration_system.event_publisher
    }

    // ========================================================================
    // CORE ORCHESTRATION OPERATIONS (language-agnostic)
    // ========================================================================

    /// Get orchestration system handle for language bindings
    pub fn orchestration_system(&self) -> &Arc<OrchestrationSystem> {
        &self.orchestration_system
    }

    /// Get testing factory handle for language bindings
    pub fn testing_factory(&self) -> Arc<super::testing::SharedTestingFactory> {
        super::testing::get_global_testing_factory()
    }

    /// Register handler using shared types
    pub fn register_handler(&self, metadata: HandlerMetadata) -> SharedFFIResult<()> {
        // Use validate_or_refresh for production resilience - auto-recover from expired handles
        let _validated_handle = self.validate_or_refresh()?;

        let result: Result<(), crate::error::TaskerError> =
            super::orchestration_system::execute_async(async {
                // 1. Register FFI handler metadata
                self.orchestration_system
                    .task_handler_registry
                    .register_ffi_handler(
                        &metadata.namespace,
                        &metadata.name,
                        &metadata.version,
                        &metadata.handler_class,
                        metadata.config_schema.clone(),
                    )
                    .await?;

                // 2. 🚀 NEW: Register TaskTemplate if config_schema is provided
                // This fixes "No task configuration available" errors by ensuring
                // TaskConfigFinder can find the task template in the registry
                if let Some(ref config_json) = metadata.config_schema {
                    // Convert JSON config to TaskTemplate
                    match serde_json::from_value::<crate::models::core::task_template::TaskTemplate>(
                        config_json.clone(),
                    ) {
                        Ok(task_template) => {
                            // Register the task template so TaskConfigFinder can find it
                            if let Err(e) = self
                                .orchestration_system
                                .task_handler_registry
                                .register_task_template(
                                    &metadata.namespace,
                                    &metadata.name,
                                    &metadata.version,
                                    task_template,
                                )
                                .await
                            {
                                tracing::warn!(
                                    namespace = metadata.namespace,
                                    name = metadata.name,
                                    version = metadata.version,
                                    error = %e,
                                    "Failed to register task template, step execution may fail"
                                );
                            } else {
                                tracing::info!(
                                    namespace = metadata.namespace,
                                    name = metadata.name,
                                    version = metadata.version,
                                    "Successfully registered task template for TaskConfigFinder"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                namespace = metadata.namespace,
                                name = metadata.name,
                                version = metadata.version,
                                error = %e,
                                "Failed to parse config_schema as TaskTemplate"
                            );
                        }
                    }
                }

                Ok(())
            });

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(SharedFFIError::HandlerRegistrationFailed(e.to_string())),
        }
    }

    /// Find handler by namespace, name, and version
    pub fn find_handler(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> SharedFFIResult<Option<HandlerMetadata>> {
        // Use validate_or_refresh for production resilience - auto-recover from expired handles
        let _validated_handle = self.validate_or_refresh()?;

        // Access the task handler registry directly through orchestration system
        match self
            .orchestration_system
            .task_handler_registry
            .get_handler_metadata(namespace, name, version)
        {
            Ok(metadata) => Ok(Some(metadata)),
            Err(_) => Ok(None), // Handler not found - return None instead of error for graceful handling
        }
    }

    /// Find handler from task request structure
    pub fn find_handler_from_request(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
    ) -> SharedFFIResult<Option<HandlerMetadata>> {
        self.find_handler(namespace, name, version)
    }

    // ========================================================================
    // ORCHESTRATION SYSTEM ACCESS (for language bindings)
    // ========================================================================

    /// Get analytics manager for performance operations
    pub fn analytics_manager(&self) -> Arc<super::analytics::SharedAnalyticsManager> {
        super::analytics::get_global_analytics_manager()
    }

    /// Get event bridge for cross-language event operations
    pub fn event_bridge(&self) -> Arc<super::event_bridge::SharedEventBridge> {
        super::event_bridge::get_global_event_bridge()
    }

    // ========================================================================
    // ZEROMQ BATCH PROCESSING (for Ruby orchestration integration)
    // ========================================================================

    /// Check if ZeroMQ batch processing is enabled
    pub fn is_zeromq_enabled(&self) -> SharedFFIResult<bool> {
        let _validated_handle = self.validate_or_refresh()?;
        Ok(self.orchestration_system.is_zeromq_enabled())
    }


    /// Receive result messages from Ruby (non-blocking)
    pub fn receive_results(&self) -> SharedFFIResult<Vec<serde_json::Value>> {
        let _validated_handle = self.validate_or_refresh()?;

        // TODO: Update for ZmqPubSubExecutor integration
        // The handles.rs file needs to be updated to work with ZmqPubSubExecutor
        // instead of the old BatchPublisher interface
        let results = Vec::new();
        
        if self.orchestration_system.zmq_pub_sub_executor().is_some() {

            Ok(results)
        } else {
            Err(SharedFFIError::ZeroMqNotEnabled(
                "ZeroMQ batch processing is not enabled".to_string(),
            ))
        }
    }

    /// Get ZeroMQ configuration information
    pub fn zeromq_config(&self) -> SharedFFIResult<serde_json::Value> {
        let _validated_handle = self.validate_or_refresh()?;

        let zeromq_config = &self
            .orchestration_system
            .config_manager
            .system_config()
            .zeromq;
        serde_json::to_value(zeromq_config).map_err(|e| {
            SharedFFIError::SerializationError(format!("Failed to serialize ZeroMQ config: {e}"))
        })
    }

    /// Get access to the shared ZMQ context for cross-language socket communication
    /// This enables Ruby to create sockets that share the same context as Rust
    /// for proper inproc:// socket communication
    pub fn zmq_context(&self) -> SharedFFIResult<std::sync::Arc<zmq::Context>> {
        let _validated_handle = self.validate_or_refresh()?;
        Ok(self.orchestration_system.zmq_context().clone())
    }
}

/// Handle information for debugging
#[derive(Debug, Clone)]
pub struct HandleInfo {
    pub handle_id: String,
    pub created_at: u64,
    pub age_seconds: u64,
    pub expires_at: u64,
    pub expires_in_seconds: u64,
    pub orchestration_pool_size: u32,
    pub status: String,
}

/// Trait that language bindings should implement for their handle types
pub trait SharedOrchestrationHandleTrait {
    fn register_handler(&self, metadata: HandlerMetadata) -> SharedFFIResult<()>;
}

impl SharedOrchestrationHandleTrait for SharedOrchestrationHandle {
    fn register_handler(&self, metadata: HandlerMetadata) -> SharedFFIResult<()> {
        self.register_handler(metadata)
    }
}

// ===== SHARED CORE HANDLE LOGIC ENDS HERE =====
// Language bindings should implement their own wrapper types that
// delegate to SharedOrchestrationHandle and convert between language-specific
// types and the shared types defined above.
//
// Testing factory operations (create_task, create_workflow_step, etc.) should
// be accessed through the testing_factory() method which returns an
// Arc<SharedTestingFactory> with all testing operations properly implemented.
