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
use crate::orchestration::OrchestrationCore;
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
    pub orchestration_core: Arc<OrchestrationCore>,
    pub handle_id: String,
    pub created_at: SystemTime,
}

impl SharedOrchestrationHandle {
    /// **SINGLE INITIALIZATION POINT** - Creates handle with all resources
    pub fn new() -> Result<Self, SharedFFIError> {
        info!("Creating shared orchestration handle with persistent references");

        // Initialize resources ONCE - these will be shared across all operations
        let _database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());
        
        // Create a synchronous runtime for initialization
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| SharedFFIError::RuntimeError(format!("Failed to create runtime: {}", e)))?;
        
        let orchestration_core = rt.block_on(async {
            OrchestrationCore::new().await
        }).map_err(|e| SharedFFIError::InitializationError(format!("Failed to initialize OrchestrationCore: {}", e)))?;

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
            orchestration_core: Arc::new(orchestration_core),
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
                orchestration_core: self.orchestration_core.clone(),
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
            orchestration_pool_size: self.orchestration_core.database_pool().size(),
            status,
        }
    }

    /// Get database pool from orchestration core (for performance functions)
    pub fn database_pool(&self) -> &sqlx::PgPool {
        self.orchestration_core.database_pool()
    }

    // ========================================================================
    // CORE ORCHESTRATION OPERATIONS (language-agnostic)
    // ========================================================================

    /// Get orchestration core handle for language bindings
    pub fn orchestration_core(&self) -> &Arc<OrchestrationCore> {
        &self.orchestration_core
    }

    // ========================================================================
    // ORCHESTRATION SYSTEM ACCESS (for language bindings)
    // ========================================================================

    // ========================================================================
    // ZEROMQ BATCH PROCESSING (for Ruby orchestration integration)
    // ========================================================================
}

/// Task metadata for handler lookup
#[derive(Debug, Clone)]
pub struct TaskMetadata {
    pub task_id: i64,
    pub namespace: String,
    pub name: String,
    pub version: String,
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

// ===== SHARED CORE HANDLE LOGIC ENDS HERE =====
// Language bindings should implement their own wrapper types that
// delegate to SharedOrchestrationHandle and convert between language-specific
// types and the shared types defined above.
//
// Testing factory operations (create_task, create_workflow_step, etc.) should
// be accessed through the testing_factory() method which returns an
// Arc<SharedTestingFactory> with all testing operations properly implemented.
