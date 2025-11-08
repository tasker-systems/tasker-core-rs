// TAS-50: Event systems configuration component
//
// TAS-61 Phase 6C/6D: Updated to use V2 configuration
// Note: EventSystemsConfig doesn't exist in V2 - orchestration and worker have separate configs

pub use crate::config::event_systems::{
    OrchestrationEventSystemConfig, TaskReadinessEventSystemConfig, WorkerEventSystemConfig,
};
// EventSystemsConfig removed - use OrchestrationEventSystemsConfig or WorkerEventSystemsConfig from tasker_v2
