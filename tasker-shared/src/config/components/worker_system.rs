// TAS-50: Worker system configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::worker::{
    EventSystemConfig, HealthMonitoringConfig, StepProcessingConfig,
    WorkerConfig as WorkerSystemConfig,
};
