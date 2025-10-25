// TAS-50: MPSC channels configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::mpsc_channels::{
    MpscChannelsConfig, OrchestrationChannelsConfig, SharedChannelsConfig, WorkerChannelsConfig,
};
