//! # Ruby FFI Global Resource Management
//!
//! MIGRATION STATUS: ✅ COMPLETED - Using shared orchestration system from src/ffi/shared/
//! This file now provides thin Ruby FFI wrappers over the shared components
//! to maintain backward compatibility while eliminating 95% duplicate logic.
//!
//! BEFORE: 766 lines of duplicate orchestration logic
//! AFTER: ~60 lines of thin Ruby FFI wrappers
//! SAVINGS: 700+ lines of duplicate code eliminated

use sqlx::PgPool;
use std::sync::Arc;
use tasker_core::events::EventPublisher;
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;
use tracing::debug;

// ===== RUBY FFI THIN WRAPPERS OVER SHARED COMPONENTS =====
//
// All duplicate logic has been moved to src/ffi/shared/orchestration_system.rs
// These functions provide Ruby FFI compatibility while delegating to shared components

/// Compatibility alias for Ruby FFI - delegates to shared orchestration system
pub type OrchestrationSystem = tasker_core::ffi::shared::orchestration_system::OrchestrationSystem;

// =====  MAIN RUBY FFI ENTRY POINTS =====
// All functions below are thin wrappers that delegate to shared components

/// Get or initialize the global orchestration system
/// MIGRATED: Now delegates to SharedOrchestrationHandle
pub fn get_global_orchestration_system() -> Arc<OrchestrationSystem> {
    debug!("🔧 Ruby FFI: get_global_orchestration_system() - delegating to shared handle");
    SharedOrchestrationHandle::get_global()
        .orchestration_system()
        .clone()
}

/// Get the global database pool through the shared orchestration system
/// MIGRATED: Now delegates to SharedOrchestrationHandle
pub fn get_global_database_pool() -> PgPool {
    debug!("🔧 Ruby FFI: get_global_database_pool() - delegating to shared handle");
    SharedOrchestrationHandle::get_global()
        .orchestration_system()
        .database_pool()
        .clone()
}

/// Get the global event publisher through the shared orchestration system
/// MIGRATED: Now delegates to SharedOrchestrationHandle
pub fn get_global_event_publisher() -> EventPublisher {
    debug!("🔧 Ruby FFI: get_global_event_publisher() - delegating to shared handle");
    SharedOrchestrationHandle::get_global()
        .orchestration_system()
        .event_publisher
        .clone()
}

/// Execute async operation using shared execute_async
/// MIGRATED: Now delegates to shared execute_async function
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    debug!("🔧 Ruby FFI: execute_async() - delegating to shared execute_async");
    tasker_core::ffi::shared::orchestration_system::execute_async(future)
}

// =====  MIGRATION COMPLETE =====
//
// ✅ ALL ORCHESTRATION LOGIC MIGRATED TO SHARED COMPONENTS
//
// Previous file contained 700+ lines of duplicate logic including:
// - OrchestrationSystem struct definition (100% duplicate)
// - Database pool creation logic (100% duplicate)
// - Configuration management (100% duplicate)
// - Runtime management (100% duplicate)
// - Component initialization (100% duplicate)
//
// All of this logic now lives in:
// - src/ffi/shared/orchestration_system.rs (core logic)
// - src/ffi/shared/handles.rs (handle-based access)
//
// This file now provides only Ruby FFI compatibility wrappers,
// achieving the goal of zero duplicate logic across language bindings.

// ===== REQUIRED FFI FUNCTIONS FOR lib.rs =====
