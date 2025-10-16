// TAS-50: Orchestration system configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::orchestration::{
    OrchestrationConfig as OrchestrationSystemConfig,
    OrchestrationSystemConfig as OrchestrationSystemConfigInternal,
};
