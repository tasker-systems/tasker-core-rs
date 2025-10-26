// TAS-50: Event systems configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::event_systems::{
    OrchestrationEventSystemConfig, TaskReadinessEventSystemConfig, WorkerEventSystemConfig,
};
pub use crate::config::tasker::EventSystemsConfig;
