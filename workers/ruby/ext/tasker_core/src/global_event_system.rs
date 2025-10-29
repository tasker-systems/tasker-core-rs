//! # Global Event System
//!
//! Provides a global singleton `WorkerEventSystem` that can be shared between
//! the `WorkerProcessor` and our Rust event handlers.

use std::sync::Arc;
use tasker_shared::events::WorkerEventSystem;

/// Global worker event system singleton
pub static GLOBAL_EVENT_SYSTEM: std::sync::LazyLock<Arc<WorkerEventSystem>> =
    std::sync::LazyLock::new(|| Arc::new(WorkerEventSystem::new()));

/// Get the global worker event system
pub fn get_global_event_system() -> Arc<WorkerEventSystem> {
    GLOBAL_EVENT_SYSTEM.clone()
}
